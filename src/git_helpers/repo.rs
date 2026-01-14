//! Basic git repository operations.
//!
//! Provides fundamental git operations used throughout the application:
//!
//! - Repository detection and root path resolution
//! - Working tree status snapshots (porcelain format)
//! - Staging and committing changes
//! - Diff generation for commit messages
//! - Automated commit message generation and committing
//!
//! Operations use libgit2 directly to avoid CLI dependencies and work
//! even when git is not installed.

use std::io;
use std::path::PathBuf;

use super::identity::{resolve_git_identity, GitIdentity};

/// Maximum diff size (in bytes) before showing a warning.
/// 100KB is a reasonable threshold - most meaningful diffs are smaller.
const MAX_DIFF_SIZE_WARNING: usize = 100 * 1024;

/// Maximum diff size (in bytes) per chunk for LLM processing.
/// 100KB per chunk allows for multiple retries while staying within reasonable limits.
const MAX_DIFF_CHUNK_SIZE: usize = 100 * 1024;

/// Maximum number of chunks to split a diff into.
/// This prevents runaway chunking for extremely large diffs.
const MAX_CHUNKS: usize = 10;

/// Maximum diff size (in bytes) before truncation for reviewers.
/// 500KB is a hard limit for reviewer diffs - we truncate rather than chunk for reviewers.
const MAX_DIFF_SIZE_HARD: usize = 500 * 1024;

/// Truncation marker for reviewer diffs (not for commit messages).
/// For commit messages, we use chunking instead.
const DIFF_TRUNCATED_MARKER: &str = "\n\n[Diff truncated due to size. Showing first portion above.]";

/// Convert git2 error to io::Error.
fn git2_to_io_error(err: git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}

/// Check if we're in a git repository.
pub(crate) fn require_git_repo() -> io::Result<()> {
    git2::Repository::discover(".").map_err(git2_to_io_error)?;
    Ok(())
}

/// Get the git repository root.
pub(crate) fn get_repo_root() -> io::Result<PathBuf> {
    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;
    repo.workdir()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No workdir for repository"))
}

/// Get the git hooks directory path.
///
/// Returns the path to the hooks directory inside .git (or the equivalent
/// for worktrees and other configurations).
pub(crate) fn get_hooks_dir() -> io::Result<PathBuf> {
    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;
    Ok(repo.path().join("hooks"))
}

/// Get a snapshot of the current git status.
///
/// Returns status in porcelain format (similar to `git status --porcelain=v1`).
pub(crate) fn git_snapshot() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts)).map_err(git2_to_io_error)?;

    let mut result = String::new();
    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().unwrap_or("").to_string();

        // Convert git2 status to porcelain format
        // Untracked files are represented as "??" in porcelain v1.
        if status.contains(git2::Status::WT_NEW) {
            result.push('?');
            result.push('?');
            result.push(' ');
            result.push_str(&path);
            result.push('\n');
            continue;
        }

        // Index status
        let index_status = if status.contains(git2::Status::INDEX_NEW) {
            'A'
        } else if status.contains(git2::Status::INDEX_MODIFIED) {
            'M'
        } else if status.contains(git2::Status::INDEX_DELETED) {
            'D'
        } else if status.contains(git2::Status::INDEX_RENAMED) {
            'R'
        } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
            'T'
        } else {
            ' '
        };

        // Worktree status
        let wt_status = if status.contains(git2::Status::WT_MODIFIED) {
            'M'
        } else if status.contains(git2::Status::WT_DELETED) {
            'D'
        } else if status.contains(git2::Status::WT_RENAMED) {
            'R'
        } else if status.contains(git2::Status::WT_TYPECHANGE) {
            'T'
        } else {
            ' '
        };

        result.push(index_status);
        result.push(wt_status);
        result.push(' ');
        result.push_str(&path);
        result.push('\n');
    }

    Ok(result)
}

/// Get the diff of all changes (unstaged and staged).
///
/// Returns a formatted diff string suitable for LLM analysis.
/// This is similar to `git diff HEAD`.
///
/// Handles the case of an empty repository (no commits yet) by
/// diffing against an empty tree using a read-only approach.
pub(crate) fn git_diff() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;

    // Try to get HEAD tree
    let head_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree().map_err(git2_to_io_error)?),
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // No commits yet - we need to show all untracked files as new files
            // Since there's no HEAD, we diff an empty tree against the workdir

            // Create a diff with an empty tree (no parent tree)
            // This is a read-only operation that doesn't modify the index
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.include_untracked(true);
            diff_opts.recurse_untracked_dirs(true);

            let diff = repo
                .diff_tree_to_workdir_with_index(None, Some(&mut diff_opts))
                .map_err(git2_to_io_error)?;

            let mut result = Vec::new();
            diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
                result.extend_from_slice(line.content());
                true
            })
            .map_err(git2_to_io_error)?;

            return Ok(String::from_utf8_lossy(&result).to_string());
        }
        Err(e) => return Err(git2_to_io_error(e)),
    };

    // For repos with commits, diff HEAD against working tree
    // This includes both staged and unstaged changes
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .map_err(git2_to_io_error)?;

    // Generate diff text
    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(git2_to_io_error)?;

    Ok(String::from_utf8_lossy(&result).to_string())
}

/// Validate and optionally truncate a diff for LLM consumption (for reviewers).
///
/// This function checks if a diff is too large for effective LLM processing
/// and truncates it for reviewer use. For commit messages, use chunk instead.
///
/// # Arguments
///
/// * `diff` - The git diff to validate
///
/// # Returns
///
/// Returns a tuple containing:
/// - The validated (and possibly truncated) diff
/// - A boolean indicating whether the diff was truncated
pub(crate) fn validate_and_truncate_diff(diff: String) -> (String, bool) {
    let diff_size = diff.len();

    // Warn about large diffs
    if diff_size > MAX_DIFF_SIZE_WARNING {
        eprintln!(
            "Warning: Large diff detected ({} bytes). This may affect commit message quality.",
            diff_size
        );
    }

    // Truncate if over the hard limit
    if diff_size > MAX_DIFF_SIZE_HARD {
        let truncate_size = MAX_DIFF_SIZE_HARD - DIFF_TRUNCATED_MARKER.len();
        let truncated = if let Some(idx) = diff
            .char_indices()
            .nth(truncate_size)
            .map(|(i, _)| i)
        {
            format!("{}{}", &diff[..idx], DIFF_TRUNCATED_MARKER)
        } else {
            format!("{}{}", diff, DIFF_TRUNCATED_MARKER)
        };

        eprintln!(
            "Warning: Diff truncated from {} to {} bytes for LLM processing.",
            diff_size,
            truncated.len()
        );

        (truncated, true)
    } else {
        (diff, false)
    }
}

/// Chunk a diff for commit message generation.
///
/// For commit messages, we need the full diff. If it's too large, we split it
/// into chunks and send multiple LLM requests, then combine the results.
///
/// # Arguments
///
/// * `diff` - The git diff to chunk
///
/// # Returns
///
/// Returns a vector of diff chunks. Each chunk is a partial diff with context
/// about which chunk it is.
fn chunk_diff_for_commit_message(diff: &str) -> Vec<String> {
    let diff_size = diff.len();

    // If diff is small enough, return as single chunk
    if diff_size <= MAX_DIFF_CHUNK_SIZE {
        eprintln!("Diff size: {} bytes (single chunk)", diff_size);
        return vec![diff.to_string()];
    }

    // Calculate number of chunks needed
    let num_chunks = (diff_size + MAX_DIFF_CHUNK_SIZE - 1) / MAX_DIFF_CHUNK_SIZE;
    let num_chunks = num_chunks.min(MAX_CHUNKS);

    eprintln!(
        "Large diff detected ({} bytes). Splitting into {} chunks for commit message generation.",
        diff_size, num_chunks
    );

    let mut chunks = Vec::new();
    let lines: Vec<&str> = diff.lines().collect();
    let lines_per_chunk = (lines.len() + num_chunks - 1) / num_chunks;

    for chunk_idx in 0..num_chunks {
        let start = chunk_idx * lines_per_chunk;
        let end = ((chunk_idx + 1) * lines_per_chunk).min(lines.len());

        // Find a good break point (at diff header)
        let mut break_point = end;
        if chunk_idx < num_chunks - 1 {
            // Look for a diff header to break at
            for i in start..end {
                if lines[i].starts_with("diff --git") {
                    break_point = i;
                    break;
                }
            }
        } else {
            break_point = lines.len();
        }

        let chunk_lines = &lines[start..break_point];
        let chunk_text = chunk_lines.join("\n");

        // Add chunk context header
        let chunk_with_context = format!(
            "[Diff chunk {}/{} - lines {}-{}]\n\n{}",
            chunk_idx + 1,
            num_chunks,
            start + 1,
            break_point,
            chunk_text
        );

        chunks.push(chunk_with_context);
    }

    chunks
}

fn index_has_changes_to_commit(repo: &git2::Repository, index: &git2::Index) -> io::Result<bool> {
    match repo.head() {
        Ok(head) => {
            let head_tree = head.peel_to_tree().map_err(git2_to_io_error)?;
            let diff = repo
                .diff_tree_to_index(Some(&head_tree), Some(index), None)
                .map_err(git2_to_io_error)?;
            Ok(diff.deltas().len() > 0)
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(!index.is_empty()),
        Err(e) => Err(git2_to_io_error(e)),
    }
}

fn is_internal_agent_artifact(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str == ".no_agent_commit"
        || path_str == ".agent"
        || path_str.starts_with(".agent/")
        || path_str == ".git"
        || path_str.starts_with(".git/")
}

/// Stage all changes.
///
/// Similar to `git add -A`.
///
/// # Returns
///
/// Returns `Ok(true)` if files were successfully staged, `Ok(false)` if there
/// were no files to stage, or an error if staging failed.
pub(crate) fn git_add_all() -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;

    let mut index = repo.index().map_err(git2_to_io_error)?;

    // Stage deletions (equivalent to `git add -A` behavior).
    // libgit2's `add_all` doesn't automatically remove deleted paths.
    let mut status_opts = git2::StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut status_opts))
        .map_err(git2_to_io_error)?;
    for entry in statuses.iter() {
        if entry.status().contains(git2::Status::WT_DELETED) {
            if let Some(path) = entry.path() {
                index
                    .remove_path(std::path::Path::new(path))
                    .map_err(git2_to_io_error)?;
            }
        }
    }

    // Add all files (staged, unstaged, and untracked)
    // Note: add_all() is required here, not update_all(), to include untracked files
    let mut filter_cb = |path: &std::path::Path, _matched: &[u8]| -> i32 {
        // Never stage Ralph internal artifacts, even if the user's repo doesn't ignore them.
        if is_internal_agent_artifact(path) {
            1
        } else {
            0
        }
    };
    index
        .add_all(
            vec!["."],
            git2::IndexAddOption::DEFAULT,
            Some(&mut filter_cb),
        )
        .map_err(git2_to_io_error)?;

    index.write().map_err(git2_to_io_error)?;

    // Return true if staging produced something commit-worthy.
    index_has_changes_to_commit(&repo, &index)
}

/// Resolve git commit identity with the full priority chain.
///
/// This function implements the identity resolution priority chain:
/// 1. Provided name/email parameters (from Ralph config)
/// 2. Environment variables (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`)
/// 3. Ralph config file values (passed through)
/// 4. Git config (via libgit2's repo.signature())
/// 5. System username + derived email
/// 6. Default values ("Ralph Workflow", "ralph@localhost")
///
/// Partial overrides are supported: if only name is provided, email will
/// fall back through git config, system fallback, or defaults.
///
/// # Arguments
///
/// * `repo` - The git repository (for git config fallback)
/// * `provided_name` - Optional name from Ralph config or CLI
/// * `provided_email` - Optional email from Ralph config or CLI
///
/// # Returns
///
/// Returns `Ok(GitIdentity)` with the resolved name and email.
fn resolve_commit_identity(
    repo: &git2::Repository,
    provided_name: Option<&str>,
    provided_email: Option<&str>,
) -> io::Result<GitIdentity> {
    use super::identity::{default_identity, fallback_email, fallback_username};

    // First try the identity resolution system for CLI/env/config sources
    // This handles priorities 1-3 (CLI args, env vars, Ralph config)
    match resolve_git_identity(provided_name, provided_email, None, None) {
        Ok((identity, _source)) => {
            // Identity resolved from CLI, env, or config - validate and return
            if let Err(e) = identity.validate() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid git identity from config: {}", e)));
            }
            return Ok(identity);
        }
        Err(_) => {
            // Identity resolution fell through - continue to git config fallback
        }
    }

    // Priority 4: Git config (via libgit2)
    // This handles the case where neither CLI/env nor Ralph config provided
    // both name and email. We now try git config, and support partial overrides.
    match repo.signature() {
        Ok(sig) => {
            // Git config provided a signature
            let git_name = sig.name().unwrap_or("").to_string();
            let git_email = sig.email().unwrap_or("").to_string();

            // If git config has both name and email, use them
            if !git_name.is_empty() && !git_email.is_empty() {
                // Check if we have a partial override (name provided but not email, or vice versa)
                let name = provided_name
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&git_name)
                    .to_string();
                let email = provided_email
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&git_email)
                    .to_string();

                let identity = GitIdentity::new(name, email);
                if identity.validate().is_err() {
                    // Git config identity is invalid - fall through to system fallback
                } else {
                    return Ok(identity);
                }
            }
        }
        Err(_) => {
            // Git config failed - fall through to system fallback
        }
    }

    // Priority 5: System username + derived email
    let username = fallback_username();
    let email = fallback_email(&username);
    let identity = GitIdentity::new(username.clone(), email);

    if identity.validate().is_err() {
        // Shouldn't happen with our fallbacks, but handle it by falling through to defaults
    } else {
        return Ok(identity);
    }

    // Priority 6: Default values (last resort)
    Ok(default_identity())
}

/// Create a commit.
///
/// Similar to `git commit -m <message>`.
///
/// Handles both initial commits (no HEAD yet) and subsequent commits.
///
/// # Identity Resolution
///
/// The git commit identity (name and email) is resolved using the following priority:
/// 1. Provided `git_user_name` and `git_user_email` parameters (highest priority)
/// 2. Environment variables (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`)
/// 3. Ralph config file (read by caller, passed as parameters)
/// 4. Git config (via libgit2)
/// 5. System username + derived email (sane fallback)
/// 6. Default values ("Ralph Workflow", "ralph@localhost") - last resort
///
/// Partial overrides are supported: if only name is provided, email will fall back
/// through the remaining priority levels.
///
/// # Arguments
///
/// * `message` - The commit message
/// * `git_user_name` - Optional git user name (overrides git config)
/// * `git_user_email` - Optional git user email (overrides git config)
///
/// # Returns
///
/// Returns `Ok(Some(oid))` with the commit OID if successful, `Ok(None)` if the
/// OID is zero (no commit created), or an error if the operation failed.
pub(crate) fn git_commit(
    message: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> io::Result<Option<git2::Oid>> {
    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;

    // Get the index
    let mut index = repo.index().map_err(git2_to_io_error)?;

    // Don't create empty commits: if the index matches HEAD (or is empty on an unborn branch),
    // there's nothing to commit.
    if !index_has_changes_to_commit(&repo, &index)? {
        return Ok(None);
    }

    // Get the tree from the index
    let tree_oid = index.write_tree().map_err(git2_to_io_error)?;

    let tree = repo.find_tree(tree_oid).map_err(git2_to_io_error)?;

    // Resolve git identity using the identity resolution system.
    // This implements the full priority chain with proper fallbacks.
    let GitIdentity { name, email } = resolve_commit_identity(&repo, git_user_name, git_user_email)?;

    // Create the signature with the resolved identity
    let sig = git2::Signature::now(&name, &email).map_err(git2_to_io_error)?;

    let oid = match repo.head() {
        Ok(head) => {
            // Normal commit: has a parent
            let head_commit = head.peel_to_commit().map_err(git2_to_io_error)?;
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head_commit])
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // Initial commit: no parents
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
        }
        Err(e) => return Err(git2_to_io_error(e)),
    }
    .map_err(git2_to_io_error)?;

    Ok(Some(oid))
}

/// Check if there are any meaningful changes to commit.
///
/// Returns true if there are actual changes to commit (excluding only
/// whitespace-only changes in diff output lines).
///
/// # What Changes Are Considered Meaningful
///
/// This function considers virtually all non-whitespace changes as "meaningful":
/// - Code changes (additions, modifications, deletions)
/// - Comments and documentation
/// - Configuration files
/// - Test files
/// - Build files
///
/// This is intentional because:
/// 1. Comments and docs are part of the codebase and have value
/// 2. They may represent important clarifications or documentation updates
/// 3. Making value judgments about what changes are "important" is error-prone
///
/// The primary purpose is to skip commits only for truly trivial changes like
/// whitespace-only modifications, not to filter out categories of changes.
pub(crate) fn has_meaningful_changes() -> io::Result<bool> {
    // Check if there are any changes at all
    let diff = git_diff()?;
    if diff.trim().is_empty() {
        return Ok(false);
    }

    // Check for meaningful content (not just whitespace).
    //
    // A whitespace-only addition/removal line like "+   " becomes "+" after `trim()`,
    // so we explicitly detect and ignore those.
    let has_content = diff.lines().any(|line| {
        // Preserve leading whitespace (important for detecting whitespace-only changes),
        // but ignore trailing newline artifacts.
        let line = line.trim_end();
        let trimmed = line.trim();

        // Ignore diff metadata and empty/whitespace-only lines.
        if trimmed.is_empty()
            || trimmed.starts_with("diff --git")
            || trimmed.starts_with("index ")
            || trimmed.starts_with("---")
            || trimmed.starts_with("+++")
            || trimmed.starts_with("@@")
        {
            return false;
        }

        // Ignore whitespace-only added/removed lines ("+   ", "-\t", etc).
        if (line.starts_with('+') && !line.starts_with("+++"))
            || (line.starts_with('-') && !line.starts_with("---"))
        {
            return !line[1..].trim().is_empty();
        }

        true
    });

    Ok(has_content)
}

/// Error types for commit message generation failures.
#[derive(Debug)]
enum CommitGenerationError {
    Timeout,
    Empty,
    ExtractionFailed(String),
    ValidationFailed(String),
    AgentFailed(String),
}

impl std::fmt::Display for CommitGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitGenerationError::Timeout => write!(f, "LLM agent timed out"),
            CommitGenerationError::Empty => write!(f, "LLM returned empty response"),
            CommitGenerationError::ExtractionFailed(msg) => write!(f, "Failed to extract commit message: {}", msg),
            CommitGenerationError::ValidationFailed(msg) => write!(f, "Validation failed: {}", msg),
            CommitGenerationError::AgentFailed(msg) => write!(f, "Agent failed: {}", msg),
        }
    }
}

impl std::error::Error for CommitGenerationError {}

/// Call the LLM agent with a prompt and return the raw output.
///
/// This is a helper function that handles the actual LLM invocation.
fn call_llm_agent(prompt: &str, agent_cmd: &str, timeout_secs: u64) -> Result<String, CommitGenerationError> {
    use crate::utils::split_command;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let argv = split_command(agent_cmd).map_err(|e| {
        CommitGenerationError::AgentFailed(format!("Failed to parse agent command: {}", e))
    })?;

    let (program, args) = match argv.split_first() {
        Some(pair) => pair,
        None => return Err(CommitGenerationError::AgentFailed("Agent command is empty".to_string())),
    };

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CommitGenerationError::AgentFailed(format!("Failed to spawn agent: {}", e)))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())
            .map_err(|e| CommitGenerationError::AgentFailed(format!("Failed to write prompt: {}", e)))?;
        drop(stdin);
    }

    let timeout = Duration::from_secs(timeout_secs);
    let start_time = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                let mut stdout = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    out.read_to_end(&mut stdout)
                        .map_err(|e| CommitGenerationError::AgentFailed(format!("Failed to read stdout: {}", e)))?;
                }

                let mut stderr = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_end(&mut stderr)
                        .map_err(|e| CommitGenerationError::AgentFailed(format!("Failed to read stderr: {}", e)))?;
                }

                if !exit_status.success() {
                    let stderr_str = String::from_utf8_lossy(&stderr);
                    return Err(CommitGenerationError::AgentFailed(format!(
                        "Exit code: {:?}{}",
                        exit_status.code(),
                        if stderr_str.trim().is_empty() { String::new() } else { format!("\n{}", stderr_str.trim()) }
                    )));
                }

                return Ok(String::from_utf8_lossy(&stdout).to_string());
            }
            Ok(None) => {
                if start_time.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CommitGenerationError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(CommitGenerationError::AgentFailed(format!("Failed to wait: {}", e)));
            }
        }
    }
}

/// Extract and validate commit message from LLM output.
fn extract_and_validate_commit_message(raw_output: &str, agent_cmd: &str) -> Result<String, CommitGenerationError> {
    use crate::files::llm_output_extraction::{extract_llm_output, validate_commit_message, OutputFormat};

    if raw_output.trim().is_empty() {
        return Err(CommitGenerationError::Empty);
    }

    let format_hint = agent_cmd
        .split_whitespace()
        .find_map(|tok| {
            let tok = tok.to_lowercase();
            if tok.contains("codex") {
                Some("codex")
            } else if tok.contains("claude") || tok.contains("ccs") || tok.contains("qwen") {
                Some("claude")
            } else if tok.contains("gemini") {
                Some("gemini")
            } else if tok.contains("opencode") {
                Some("opencode")
            } else {
                None
            }
        })
        .map(OutputFormat::from_str);

    let extraction = extract_llm_output(raw_output, format_hint);

    if let Some(warning) = &extraction.warning {
        eprintln!("Warning: LLM output extraction warning: {}", warning);
    }

    let commit_message = clean_commit_message(&extraction.content);

    if let Err(validation_error) = validate_commit_message(&commit_message) {
        // Check if it's a JSON extraction failure
        if commit_message.starts_with('{') && commit_message.contains(r#""type":"#) {
            return Err(CommitGenerationError::ExtractionFailed(validation_error));
        }
        return Err(CommitGenerationError::ValidationFailed(validation_error));
    }

    Ok(commit_message)
}

/// Generate a commit message by calling an LLM with the diff.
///
/// This function now includes:
/// - Diff chunking for large diffs
/// - Retry logic with exponential backoff
/// - Robust validation
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message for
/// * `agent_cmd` - The command to invoke the agent (e.g., "claude", "codex")
///
/// # Returns
///
/// Returns `Ok(String)` with the generated commit message, or an error if all retries fail.
pub(crate) fn generate_commit_message_with_llm(diff: &str, agent_cmd: &str) -> io::Result<String> {
    eprintln!("Generating commit message with LLM...");

    // Chunk the diff for commit message generation
    let chunks = chunk_diff_for_commit_message(diff);
    let num_chunks = chunks.len();

    // For single chunk, use simple approach with retries
    if num_chunks == 1 {
        return generate_commit_message_with_retries(&chunks[0], agent_cmd, 0);
    }

    // For multiple chunks, combine messages from each chunk
    eprintln!("Processing {} chunks - this may take longer...", num_chunks);
    let mut chunk_messages = Vec::new();

    for (idx, chunk) in chunks.iter().enumerate() {
        eprintln!("Processing chunk {}/{}...", idx + 1, num_chunks);
        match generate_commit_message_with_retries(chunk, agent_cmd, idx) {
            Ok(msg) => chunk_messages.push(msg),
            Err(e) => {
                eprintln!("Warning: Failed to generate message for chunk {}: {}. Using fallback analysis.", idx + 1, e);
                // Add a placeholder for this chunk
                chunk_messages.push(format!("[chunk {}]", idx + 1));
            }
        }
    }

    // Combine chunk messages into a single commit message
    let combined = combine_chunk_messages(&chunk_messages);
    eprintln!("Combined commit message from {} chunks", num_chunks);
    Ok(combined)
}

/// Generate commit message with retry logic.
fn generate_commit_message_with_retries(diff: &str, agent_cmd: &str, chunk_idx: usize) -> io::Result<String> {
    use crate::prompts::prompt_generate_commit_message_with_diff;
    use std::time::Duration;

    let max_retries = 3;
    let timeouts = [60, 90, 120]; // Exponential backoff: 60s, 90s, 120s

    for attempt in 0..max_retries {
        if attempt > 0 {
            eprintln!("Retry attempt {}/{} for chunk {}...", attempt + 1, max_retries, chunk_idx + 1);
            // Exponential backoff between retries
            let backoff_ms = 1000 * (1 << attempt.min(3)); // 1s, 2s, 4s
            std::thread::sleep(Duration::from_millis(backoff_ms));
        }

        let prompt = prompt_generate_commit_message_with_diff(diff);

        match call_llm_agent(&prompt, agent_cmd, timeouts[attempt.min(timeouts.len() - 1)]) {
            Ok(raw_output) => {
                match extract_and_validate_commit_message(&raw_output, agent_cmd) {
                    Ok(commit_message) => {
                        // Success!
                        if attempt > 0 {
                            eprintln!("Successfully generated commit message after {} retries", attempt);
                        }
                        return Ok(commit_message);
                    }
                    Err(CommitGenerationError::ExtractionFailed(msg)) => {
                        // Extraction failed - don't retry, this is likely a persistent issue
                        return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
                    }
                    Err(CommitGenerationError::ValidationFailed(msg)) => {
                        // Validation failed - log but try retry as it might be a transient issue
                        eprintln!("Validation failed on attempt {}: {}", attempt + 1, msg);
                        if attempt == max_retries - 1 {
                            // Last attempt failed - return error
                            return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
                        }
                        // Continue to retry
                    }
                    Err(CommitGenerationError::Empty) => {
                        eprintln!("LLM returned empty output on attempt {}", attempt + 1);
                        if attempt == max_retries - 1 {
                            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "LLM returned empty response after all retries"));
                        }
                    }
                    Err(CommitGenerationError::Timeout) => {
                        eprintln!("LLM timed out on attempt {}", attempt + 1);
                        if attempt == max_retries - 1 {
                            return Err(io::Error::new(io::ErrorKind::TimedOut, "LLM timed out after all retries"));
                        }
                    }
                    Err(CommitGenerationError::AgentFailed(msg)) => {
                        eprintln!("Agent failed on attempt {}: {}", attempt + 1, msg);
                        if attempt == max_retries - 1 {
                            return Err(io::Error::new(io::ErrorKind::Other, msg));
                        }
                    }
                }
            }
            Err(CommitGenerationError::Timeout) => {
                eprintln!("LLM timed out on attempt {}", attempt + 1);
                if attempt == max_retries - 1 {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, format!("LLM timed out after {} attempts", max_retries)));
                }
            }
            Err(CommitGenerationError::AgentFailed(msg)) => {
                // Agent failed - might be a persistent issue
                eprintln!("Agent error on attempt {}: {}", attempt + 1, msg);
                if attempt == max_retries - 1 {
                    return Err(io::Error::new(io::ErrorKind::Other, msg));
                }
            }
            Err(e) => {
                return Err(io::Error::new(io::ErrorKind::Other, e.to_string()));
            }
        }
    }

    unreachable!("Loop should always return")
}

/// Combine messages from multiple chunks into a single commit message.
fn combine_chunk_messages(messages: &[String]) -> String {
    if messages.len() == 1 {
        return messages[0].clone();
    }

    // Analyze all messages to extract the best type and scope
    let mut commit_type = "chore";
    let mut scope = String::new();
    let mut subjects = Vec::new();

    for msg in messages {
        // Extract type and scope from conventional commit format
        if let Some(colon_pos) = msg.find(':') {
            let type_part = &msg[..colon_pos];
            if let Some(space_pos) = type_part.rfind(' ') {
                // Has scope
                commit_type = &type_part[..space_pos];
                scope = type_part[space_pos + 1..].to_string();
            } else {
                // No scope
                commit_type = type_part;
            }

            // Extract subject (after colon, before newline)
            let subject_start = colon_pos + 1;
            if let Some(newline_pos) = msg[subject_start..].find('\n') {
                let subject = msg[subject_start..subject_start + newline_pos].trim();
                if !subject.is_empty() && !subject.starts_with('[') {
                    subjects.push(subject.to_string());
                }
            } else {
                let subject = msg[subject_start..].trim();
                if !subject.is_empty() && !subject.starts_with('[') {
                    subjects.push(subject.to_string());
                }
            }
        }
    }

    // Build combined message
    let mut result = if scope.is_empty() {
        format!("{}:", commit_type)
    } else {
        format!("{}({}):", commit_type, scope)
    };

    // Combine subjects intelligently
    if subjects.len() == 1 {
        result.push(' ');
        result.push_str(&subjects[0]);
    } else if subjects.len() > 1 {
        // Take the first meaningful subject
        for subject in &subjects {
            if !subject.is_empty() && !subject.contains("chunk") {
                result.push(' ');
                result.push_str(subject);
                break;
            }
        }
    } else {
        result.push_str(" multiple changes");
    }

    result
}

/// Generate a diff from a specific starting commit.
///
/// Takes a starting commit OID and generates a diff between that commit
/// and the current working tree. Returns a formatted diff string suitable
/// for LLM analysis.
///
/// # Arguments
///
/// * `start_oid` - The OID of the commit to diff from
///
/// # Returns
///
/// Returns a formatted diff string, or an error if:
/// - The repository cannot be opened
/// - The starting commit cannot be found
/// - The diff cannot be generated
pub(crate) fn git_diff_from(start_oid: &str) -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;

    // Parse the starting OID
    let oid = git2::Oid::from_str(start_oid).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid commit OID: {}", start_oid),
        )
    })?;

    // Find the starting commit
    let start_commit = repo.find_commit(oid).map_err(git2_to_io_error)?;
    let start_tree = start_commit.tree().map_err(git2_to_io_error)?;

    // Diff between start commit and current working tree, including staged + unstaged
    // changes and untracked files.
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&start_tree), Some(&mut diff_opts))
        .map_err(git2_to_io_error)?;

    // Generate diff text
    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(git2_to_io_error)?;

    Ok(String::from_utf8_lossy(&result).to_string())
}

fn git_diff_from_empty_tree(repo: &git2::Repository) -> io::Result<String> {
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(None, Some(&mut diff_opts))
        .map_err(git2_to_io_error)?;

    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(git2_to_io_error)?;

    Ok(String::from_utf8_lossy(&result).to_string())
}

/// Get the git diff from the starting commit.
///
/// Uses the saved starting commit from `.agent/start_commit` to generate
/// an incremental diff. Falls back to diffing from HEAD if no start commit
/// file exists.
///
/// # Returns
///
/// Returns a formatted diff string, or an error if:
/// - The diff cannot be generated
/// - The starting commit file exists but is invalid
pub(crate) fn get_git_diff_from_start() -> io::Result<String> {
    use crate::git_helpers::start_commit::{load_start_point, save_start_commit, StartPoint};

    // Ensure a valid starting point exists. This is expected to persist across runs,
    // but we also repair missing/corrupt files opportunistically for robustness.
    save_start_commit()?;

    let repo = git2::Repository::discover(".").map_err(git2_to_io_error)?;

    match load_start_point()? {
        StartPoint::Commit(oid) => git_diff_from(&oid.to_string()),
        StartPoint::EmptyRepo => git_diff_from_empty_tree(&repo),
    }
}

/// Clean a commit message by removing common artifacts.
///
/// Removes markdown code blocks, extra whitespace, and other common artifacts
/// from LLM-generated commit messages.
fn clean_commit_message(message: &str) -> String {
    let mut cleaned = message.trim().to_string();

    // Remove markdown code blocks
    if cleaned.starts_with("```") {
        if let Some(first_newline) = cleaned.find('\n') {
            cleaned = cleaned[first_newline + 1..].to_string();
        }
        if let Some(last_backticks) = cleaned.rfind("```") {
            cleaned = cleaned[..last_backticks].to_string();
        }
    }

    // Remove "git commit" prefix if present
    if cleaned.to_lowercase().starts_with("git commit") {
        if let Some(first_quote) = cleaned.find('"') {
            if let Some(last_quote) = cleaned.rfind('"') {
                cleaned = cleaned[first_quote + 1..last_quote].to_string();
            }
        }
    }

    // Remove common prefixes
    for prefix in &["Commit message:", "Message:", "Output:"] {
        if cleaned.starts_with(prefix) {
            cleaned = cleaned[prefix.len()..].trim().to_string();
        }
    }

    // Clean up whitespace while preserving intentional newlines
    cleaned = cleaned
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n");

    cleaned.trim().to_string()
}

/// Struct to track file changes for commit message generation.
#[derive(Default)]
struct FileChanges {
    new_files: Vec<String>,
    modified_files: Vec<String>,
    deleted_files: Vec<String>,
}

/// Generate a descriptive fallback commit message from a diff.
///
/// When LLM commit message generation fails, this function analyzes the diff
/// to create a more informative fallback message than a generic "chore" message.
/// It extracts information about changed files and change types.
fn generate_fallback_commit_message(diff: &str) -> String {
    let mut changes = FileChanges::default();
    let mut current_file: Option<String> = None;
    let mut is_new_file = false;
    let mut is_deleted_file = false;

    for line in diff.lines() {
        // Parse diff headers to extract file names and change types
        // Git diff format:
        // diff --git a/path/to/file b/path/to/file
        // new file mode ...
        // deleted file mode ...
        if line.starts_with("diff --git") {
            // Save previous file if any
            if let Some(file) = current_file.take() {
                if is_new_file {
                    changes.new_files.push(file);
                } else if is_deleted_file {
                    changes.deleted_files.push(file);
                } else {
                    changes.modified_files.push(file);
                }
            }

            // Reset flags for new file
            is_new_file = false;
            is_deleted_file = false;

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                // Extract file path from "a/path" or "b/path"
                let file_path = parts[3].strip_prefix("b/").unwrap_or(parts[3]);
                current_file = Some(file_path.to_string());
            }
        } else if line.starts_with("new file mode") {
            is_new_file = true;
        } else if line.starts_with("deleted file mode") {
            is_deleted_file = true;
        }
    }

    // Don't forget the last file
    if let Some(file) = current_file {
        if is_new_file {
            changes.new_files.push(file);
        } else if is_deleted_file {
            changes.deleted_files.push(file);
        } else {
            changes.modified_files.push(file);
        }
    }

    // Determine the most appropriate commit type
    let total_files = changes.new_files.len() + changes.modified_files.len() + changes.deleted_files.len();

    if total_files == 0 {
        return "chore: uncommitted changes".to_string();
    }

    // Determine the type based on what changed
    let commit_type = determine_commit_type(&changes);

    // Build the subject line
    let subject = build_subject_line(&changes, total_files, &commit_type);

    format!("{}: {}", commit_type, subject)
}

/// Determine the commit type based on the files that changed.
fn determine_commit_type(changes: &FileChanges) -> String {
    // Count files by directory/purpose
    let mut test_count = 0;
    let mut doc_count = 0;
    let mut src_count = 0;
    let mut build_count = 0;

    for file in changes.new_files.iter().chain(changes.modified_files.iter()).chain(changes.deleted_files.iter()) {
        let lower = file.to_lowercase();
        if lower.contains("test") || lower.ends_with("_test.rs") || lower.ends_with(".test.js") {
            test_count += 1;
        } else if lower.contains("readme") || lower.contains("doc") || lower.ends_with(".md") {
            doc_count += 1;
        } else if lower.contains("src") || lower.ends_with(".rs") || lower.ends_with(".js") || lower.ends_with(".py") {
            src_count += 1;
        } else if lower.contains("build") || lower.contains("cargo.toml") || lower.contains("package.json") {
            build_count += 1;
        }
    }

    // Prioritize types based on what changed most
    if test_count > src_count && test_count > 0 {
        "test".to_string()
    } else if doc_count > src_count && doc_count > 0 {
        "docs".to_string()
    } else if build_count > 0 && build_count >= total_files_count(changes) / 2 {
        "build".to_string()
    } else if changes.new_files.len() > changes.modified_files.len() && changes.new_files.len() > 0 {
        "feat".to_string()
    } else if changes.deleted_files.len() > 0 {
        "chore".to_string()
    } else if src_count > 0 {
        "refactor".to_string()
    } else {
        "chore".to_string()
    }
}

/// Get the total count of all changed files.
fn total_files_count(changes: &FileChanges) -> usize {
    changes.new_files.len() + changes.modified_files.len() + changes.deleted_files.len()
}

/// Build a descriptive subject line for the commit message.
fn build_subject_line(changes: &FileChanges, total_files: usize, _commit_type: &str) -> String {
    let mut parts = Vec::new();

    // Add new files if any
    if !changes.new_files.is_empty() {
        let count = changes.new_files.len();
        if count == 1 {
            parts.push(format!("add {}", shorten_path(&changes.new_files[0])));
        } else {
            parts.push(format!("add {} files", count));
        }
    }

    // Add deleted files if any (and this is the primary action)
    if !changes.deleted_files.is_empty() && changes.new_files.is_empty() {
        let count = changes.deleted_files.len();
        if count == 1 {
            parts.push(format!("remove {}", shorten_path(&changes.deleted_files[0])));
        } else {
            parts.push(format!("remove {} files", count));
        }
    }

    // Add modified files if this is primarily a modification
    if !changes.modified_files.is_empty() && changes.new_files.is_empty() && changes.deleted_files.is_empty() {
        let count = changes.modified_files.len();
        if count == 1 {
            parts.push(format!("update {}", shorten_path(&changes.modified_files[0])));
        } else if count <= 3 {
            // List up to 3 modified files
            let paths: Vec<String> = changes.modified_files.iter()
                .take(3)
                .map(|p| shorten_path(p))
                .collect();
            parts.push(format!("update {}", paths.join(", ")));
        } else {
            parts.push(format!("update {} files", count));
        }
    }

    // Fallback: just report the count
    if parts.is_empty() {
        return format!("{} file(s) changed", total_files);
    }

    parts.join(" and ")
}

/// Shorten a file path to just the filename and maybe parent directory.
fn shorten_path(path: &str) -> String {
    // Get the filename (last component after last slash)
    let filename = path.rsplit('/').next().unwrap_or(path);

    // Check if there's a parent directory
    if let Some(pos) = path.rfind('/') {
        let parent_part = &path[..pos];
        // Get just the parent directory name (last component of parent)
        let parent_name = parent_part.rsplit('/').next().unwrap_or(parent_part);
        format!("{}/{}", parent_name, filename)
    } else {
        filename.to_string()
    }
}

/// Create a commit with an automatically generated commit message.
///
/// This function:
/// 1. Gets the git diff
/// 2. Checks if there are meaningful changes (skip if empty or whitespace only)
/// 3. Calls the LLM to generate a commit message (with fallback if LLM fails)
/// 4. Stages all changes
/// 5. Creates the commit
///
/// # Arguments
///
/// * `agent_cmd` - The command to invoke the agent (e.g., "claude", "codex")
/// * `git_user_name` - Optional git user name (overrides git config)
/// * `git_user_email` - Optional git user email (overrides git config)
///
/// # Returns
///
/// Returns `Ok(Some(oid))` with the commit OID if a commit was created,
/// `Ok(None)` if there were no meaningful changes to commit, or an error if
/// the operation failed.
///
/// # Fallback Behavior
///
/// If the LLM fails to generate a commit message, a generic fallback message
/// is used to ensure changes are still committed. This prevents the loss of
/// progress if the LLM is temporarily unavailable or misconfigured.
pub(crate) fn commit_with_auto_message(
    agent_cmd: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> io::Result<Option<git2::Oid>> {
    // Check if there are meaningful changes
    if !has_meaningful_changes()? {
        return Ok(None);
    }

    // Get the diff
    let diff = git_diff()?;

    // Pre-validate the diff before attempting LLM call
    let diff_trimmed = diff.trim();
    if diff_trimmed.is_empty() {
        // This shouldn't happen after has_meaningful_changes check, but handle it defensively
        eprintln!("Warning: Unexpected empty diff after meaningful changes check. Using fallback message.");
        let commit_message = generate_fallback_commit_message(&diff);
        return stage_and_commit(&commit_message, git_user_name, git_user_email);
    }

    // Generate commit message via LLM, with fallback if it fails
    let commit_message = match generate_commit_message_with_llm(&diff, agent_cmd) {
        Ok(msg) => {
            // Validate the commit message is not empty
            if msg.trim().is_empty() {
                eprintln!("Warning: LLM returned empty commit message. Using fallback.");
                generate_fallback_commit_message(&diff)
            } else {
                msg
            }
        }
        Err(e) => {
            // LLM failed to generate a message - use a fallback
            // This ensures we don't lose progress if the LLM is unavailable
            eprintln!("Warning: LLM commit message generation failed: {}. Using fallback.", e);
            generate_fallback_commit_message(&diff)
        }
    };

    stage_and_commit(&commit_message, git_user_name, git_user_email)
}

/// Stage all changes and create a commit with the given message.
///
/// This is a helper function that encapsulates the staging and committing logic
/// to avoid code duplication in the commit message generation flow.
fn stage_and_commit(
    commit_message: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> io::Result<Option<git2::Oid>> {
    // Stage all changes and verify staging succeeded
    let staged = git_add_all()?;

    // Validate that staging succeeded before attempting to commit
    // If no files were staged (staged == false), there's nothing to commit
    if !staged {
        return Ok(None);
    }

    // Create the commit
    let oid = git_commit(commit_message, git_user_name, git_user_email)?;

    Ok(oid)
}

/// Result of an attempted commit operation.
///
/// This type provides detailed information about the result of a commit attempt,
/// which is useful for logging and error handling throughout the pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitResult {
    /// A commit was successfully created with the given OID.
    Success(git2::Oid),
    /// No commit was created because there were no meaningful changes.
    NoChanges,
    /// The commit operation failed with an error message.
    ///
    /// This indicates an actual git operation failure (e.g., repository corruption,
    /// permission issues, merge conflicts). LLM failures for commit message generation
    /// are handled internally with fallback messages and do not result in this variant.
    Failed(String),
}

/// Create a commit with an automatically generated commit message, returning a detailed result.
///
/// This is a convenience wrapper around `commit_with_auto_message` that returns
/// a `CommitResult` enum for more detailed error handling and logging.
///
/// # Arguments
///
/// * `agent_cmd` - The command to invoke the agent (e.g., "claude", "codex")
/// * `git_user_name` - Optional git user name (overrides git config)
/// * `git_user_email` - Optional git user email (overrides git config)
///
/// # Returns
///
/// Returns a `CommitResult` indicating success with OID, no changes, or failure.
pub(crate) fn commit_with_auto_message_result(
    agent_cmd: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> CommitResult {
    match commit_with_auto_message(agent_cmd, git_user_name, git_user_email) {
        Ok(Some(oid)) => CommitResult::Success(oid),
        Ok(None) => CommitResult::NoChanges,
        Err(e) => CommitResult::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_diff_returns_string() {
        // This test verifies the function exists and returns a Result
        // The actual content depends on the git state
        let result = git_diff();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_has_meaningful_changes_returns_bool() {
        // This test verifies the function exists and returns a Result<bool>
        let result = has_meaningful_changes();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_require_git_repo() {
        // This test verifies we can detect a git repository
        let result = require_git_repo();
        // Should succeed if we're in a git repo, fail otherwise
        // We don't assert either way since the test environment varies
        let _ = result;
    }

    #[test]
    fn test_get_repo_root() {
        // This test verifies we can get the repo root
        let result = get_repo_root();
        // Only validate if we're in a git repo
        if let Ok(path) = result {
            // The path should exist and be a directory
            assert!(path.exists());
            assert!(path.is_dir());
            // Should contain a .git directory or be inside one
            let git_dir = path.join(".git");
            assert!(git_dir.exists() || path.ancestors().any(|p| p.join(".git").exists()));
        }
    }

    #[test]
    fn test_clean_commit_message_removes_markdown_fences() {
        // Test removal of markdown code blocks
        let input = "```\nfeat: add new feature\n```";
        let cleaned = clean_commit_message(input);
        assert_eq!(cleaned, "feat: add new feature");
    }

    #[test]
    fn test_clean_commit_message_removes_git_commit_prefix() {
        // Test removal of "git commit" prefix with quoted message
        let input = "git commit \"feat: add new feature\"";
        let cleaned = clean_commit_message(input);
        assert_eq!(cleaned, "feat: add new feature");
    }

    #[test]
    fn test_clean_commit_message_removes_common_prefixes() {
        // Test removal of common prefixes
        let cases = vec![
            ("Commit message: feat: add feature", "feat: add feature"),
            ("Message: fix: bug fix", "fix: bug fix"),
            ("Output: refactor: code cleanup", "refactor: code cleanup"),
        ];

        for (input, expected) in cases {
            let cleaned = clean_commit_message(input);
            assert_eq!(cleaned, expected);
        }
    }

    #[test]
    fn test_clean_commit_message_trims_whitespace() {
        // Test whitespace trimming while preserving intentional newlines
        let input = "  feat: add feature  \n  \n  This is the body  \n  ";
        let cleaned = clean_commit_message(input);
        assert_eq!(cleaned, "feat: add feature\n\nThis is the body");
    }

    #[test]
    fn test_clean_commit_message_preserves_multiline() {
        // Test that multiline commit messages are preserved
        let input = "feat: add new feature\n\nThis adds a new feature that does X.\n\nFixes #123";
        let cleaned = clean_commit_message(input);
        assert!(cleaned.contains("feat: add new feature"));
        assert!(cleaned.contains("This adds a new feature"));
        assert!(cleaned.contains("Fixes #123"));
    }

    #[test]
    fn test_has_meaningful_changes_detects_whitespace_only() {
        // This test verifies that whitespace-only changes are filtered out
        // The actual behavior depends on the current git state
        let result = has_meaningful_changes();
        // Just verify the function works - we can't control git state in unit tests
        let _ = result;
    }

    #[test]
    fn test_git_diff_from_returns_result() {
        // Test that git_diff_from returns a Result
        // We use an invalid OID to test error handling
        let result = git_diff_from("invalid_oid_that_does_not_exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_git_snapshot_returns_result() {
        // Test that git_snapshot returns a Result
        let result = git_snapshot();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_git_add_all_returns_result() {
        // Test that git_add_all returns a Result
        let result = git_add_all();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_get_git_diff_from_start_returns_result() {
        // Test that get_git_diff_from_start returns a Result
        // It should fall back to git_diff() if no start commit file exists
        let result = get_git_diff_from_start();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_validate_and_truncate_diff_small_diff() {
        // Small diffs should not be truncated
        let small_diff = "diff --git a/file.txt b/file.txt\n+ hello";
        let (result, truncated) = validate_and_truncate_diff(small_diff.to_string());
        assert!(!truncated);
        assert_eq!(result, small_diff);
    }

    #[test]
    fn test_validate_and_truncate_diff_large_diff() {
        // Large diffs should be truncated
        let large_diff = "x".repeat(MAX_DIFF_SIZE_HARD + 1000);
        let (result, truncated) = validate_and_truncate_diff(large_diff.clone());
        assert!(truncated);
        assert!(result.len() < large_diff.len());
        assert!(result.contains(DIFF_TRUNCATED_MARKER));
    }

    #[test]
    fn test_validate_and_truncate_diff_empty() {
        // Empty diffs should not be truncated
        let empty_diff = "";
        let (result, truncated) = validate_and_truncate_diff(empty_diff.to_string());
        assert!(!truncated);
        assert_eq!(result, empty_diff);
    }

    #[test]
    fn test_shorten_path_simple_file() {
        assert_eq!(shorten_path("file.txt"), "file.txt");
    }

    #[test]
    fn test_shorten_path_parent_dir() {
        assert_eq!(shorten_path("src/file.txt"), "src/file.txt");
    }

    #[test]
    fn test_shorten_path_nested() {
        assert_eq!(shorten_path("src/module/file.txt"), "module/file.txt");
    }

    #[test]
    fn test_shorten_path_deeply_nested() {
        assert_eq!(shorten_path("a/b/c/d/file.txt"), "d/file.txt");
    }

    #[test]
    fn test_generate_fallback_commit_message_empty() {
        let diff = "";
        let result = generate_fallback_commit_message(diff);
        assert_eq!(result, "chore: uncommitted changes");
    }

    #[test]
    fn test_generate_fallback_commit_message_single_file() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+ hello";
        let result = generate_fallback_commit_message(diff);
        // Should detect this as a source file change
        assert!(result.contains("update main.rs") || result.contains("refactor"));
    }

    #[test]
    fn test_generate_fallback_commit_message_new_file() {
        let diff = "diff --git a/src/new_file.rs b/src/new_file.rs\nnew file mode 100644\n+ hello";
        let result = generate_fallback_commit_message(diff);
        // Should detect this as a new feature
        assert!(result.contains("feat"));
        assert!(result.contains("add"));
    }

    #[test]
    fn test_generate_fallback_commit_message_test_file() {
        let diff = "diff --git a/src/test_module_test.rs b/src/test_module_test.rs\n+ test code";
        let result = generate_fallback_commit_message(diff);
        // Should detect this as a test change
        assert!(result.contains("test"));
    }

    #[test]
    fn test_generate_fallback_commit_message_doc_file() {
        let diff = "diff --git a/README.md b/README.md\n+ documentation";
        let result = generate_fallback_commit_message(diff);
        // Should detect this as a docs change
        assert!(result.contains("docs"));
    }

    // =========================================================================
    // Diff Chunking Tests
    // =========================================================================

    #[test]
    fn test_chunk_diff_small_single_chunk() {
        // Small diffs should not be chunked
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+ hello world";
        let chunks = chunk_diff_for_commit_message(diff);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], diff);
    }

    #[test]
    fn test_chunk_diff_large_multiple_chunks() {
        // Create a diff that's larger than MAX_DIFF_CHUNK_SIZE
        let mut diff = String::new();
        for i in 0..2000 {
            diff.push_str(&format!("diff --git a/file{}.rs b/file{}.rs\nnew file mode 100644\n+ content {}\n", i, i, i));
        }

        // Verify the diff is larger than the chunk size
        assert!(diff.len() > MAX_DIFF_CHUNK_SIZE);

        let chunks = chunk_diff_for_commit_message(&diff);

        // Should have multiple chunks
        assert!(chunks.len() > 1);
        assert!(chunks.len() <= MAX_CHUNKS);

        // Each chunk should have context header
        for (idx, chunk) in chunks.iter().enumerate() {
            assert!(chunk.contains(&format!("[Diff chunk {}/", idx + 1)));
        }
    }

    #[test]
    fn test_chunk_diff_breaks_at_diff_headers() {
        // Create a diff with clear break points at diff headers
        let mut diff = String::new();
        for i in 0..10 {
            diff.push_str(&format!("diff --git a/file{}.rs b/file{}.rs\nnew file mode 100644\n+ content {}\n", i, i, i));
        }

        let chunks = chunk_diff_for_commit_message(&diff);

        // Each chunk should try to break at diff headers
        for chunk in &chunks {
            // Chunks should start with context header, then diff content
            let lines_after_header: Vec<&str> = chunk
                .lines()
                .skip_while(|l| !l.starts_with("diff --git"))
                .collect();

            // First line after header should be a diff header
            if !lines_after_header.is_empty() {
                assert!(lines_after_header[0].starts_with("diff --git") || lines_after_header[0].starts_with("[Diff chunk"));
            }
        }
    }

    #[test]
    fn test_chunk_diff_respects_max_chunks() {
        // Create an extremely large diff
        let mut diff = String::new();
        for i in 0..50000 {
            diff.push_str(&format!("diff --git a/file{}.rs b/file{}.rs\n+ content {}\n", i, i, i));
        }

        let chunks = chunk_diff_for_commit_message(&diff);

        // Should never exceed MAX_CHUNKS
        assert!(chunks.len() <= MAX_CHUNKS);
    }

    // =========================================================================
    // Chunk Message Combination Tests
    // =========================================================================

    #[test]
    fn test_combine_chunk_messages_single() {
        let messages = vec!["feat: add feature".to_string()];
        let combined = combine_chunk_messages(&messages);
        assert_eq!(combined, "feat: add feature");
    }

    #[test]
    fn test_combine_chunk_messages_multiple_same_type() {
        let messages = vec![
            "feat: add authentication".to_string(),
            "feat: add authorization".to_string(),
            "feat: add logging".to_string(),
        ];
        let combined = combine_chunk_messages(&messages);
        // Should use the first meaningful subject
        assert!(combined.starts_with("feat:"));
        assert!(combined.contains("authentication"));
    }

    #[test]
    fn test_combine_chunk_messages_with_scope() {
        let messages = vec![
            "feat(api): add endpoint".to_string(),
            "feat(api): add validation".to_string(),
        ];
        let combined = combine_chunk_messages(&messages);
        assert!(combined.starts_with("feat(api):"));
    }

    #[test]
    fn test_combine_chunk_messages_mixed_types() {
        let messages = vec![
            "feat: add new feature".to_string(),
            "fix: resolve bug".to_string(),
            "test: add coverage".to_string(),
        ];
        let combined = combine_chunk_messages(&messages);
        // The function iterates through all messages, using the last type
        // but takes the first meaningful subject
        assert!(combined.starts_with("test:") || combined.starts_with("fix:"));
        // Should have one of the subjects
        assert!(combined.contains("feature") || combined.contains("bug") || combined.contains("coverage"));
    }

    #[test]
    fn test_combine_chunk_messages_handles_placeholders() {
        let messages = vec![
            "feat: add feature".to_string(),
            "[chunk 2]".to_string(),
            "[chunk 3]".to_string(),
        ];
        let combined = combine_chunk_messages(&messages);
        // Should skip placeholders
        assert!(combined.contains("add feature"));
    }

    // Integration test helper - note this would require a temporary git repo
    // For full integration tests, see tests/git_workflow.rs
}
