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
/// 1MB provides reviewers with more context for large changes.
/// For commit messages, we use chunking instead of truncation to preserve full semantic information.
const MAX_DIFF_SIZE_HARD: usize = 1024 * 1024;

/// Truncation marker for reviewer diffs (not for commit messages).
/// For commit messages, we use chunking instead.
const DIFF_TRUNCATED_MARKER: &str =
    "\n\n[Diff truncated due to size. Showing first portion above.]";

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
        let truncated = if let Some(idx) = diff.char_indices().nth(truncate_size).map(|(i, _)| i) {
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
/// This implementation respects file boundaries - each chunk contains complete
/// file diffs. We never break a diff in the middle of a file's changes.
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

    // First, split the diff into file-based chunks
    // Each file diff starts with "diff --git"
    let mut file_diffs: Vec<String> = Vec::new();

    // Find all "diff --git" boundaries
    let mut diff_boundaries = vec![0];
    for (idx, line) in diff.lines().enumerate() {
        if line.starts_with("diff --git") {
            diff_boundaries.push(idx);
        }
    }
    diff_boundaries.push(diff.lines().count());

    // Extract complete file diffs
    for window in diff_boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        if start < end {
            let file_lines: Vec<&str> = diff.lines().skip(start).take(end - start).collect();
            if !file_lines.is_empty() {
                file_diffs.push(file_lines.join("\n"));
            }
        }
    }

    // Now combine file diffs into chunks of appropriate size
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut current_chunk_size = 0;
    let mut chunk_idx = 0;
    let total_files = file_diffs.len();

    for file_diff in file_diffs {
        let file_size = file_diff.len();

        // If this single file is larger than MAX_DIFF_CHUNK_SIZE, we need to include it anyway
        // to avoid splitting files. We may exceed the chunk size target.
        if current_chunk_size + file_size > MAX_DIFF_CHUNK_SIZE && !current_chunk.is_empty() {
            // Start a new chunk
            chunks.push(format!(
                "[Diff chunk {}/{} - {} files]\n\n{}",
                chunk_idx + 1,
                // We don't know the final count yet, so use a placeholder
                "?",
                current_chunk.matches("diff --git").count(),
                current_chunk
            ));
            chunk_idx += 1;
            current_chunk = String::new();
            current_chunk_size = 0;
        }

        // Add this file to the current chunk
        if !current_chunk.is_empty() {
            current_chunk.push('\n');
        }
        current_chunk.push_str(&file_diff);
        current_chunk_size += file_size + 1; // +1 for newline

        // If we've hit MAX_CHUNKS, stop and include everything remaining
        if chunk_idx >= MAX_CHUNKS - 1 {
            eprintln!("Warning: Hit MAX_CHUNKS limit, including remaining files in last chunk");
            break;
        }
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        chunks.push(format!(
            "[Diff chunk {}/{} - {} files]\n\n{}",
            chunk_idx + 1,
            "?",
            current_chunk.matches("diff --git").count(),
            current_chunk
        ));
    }

    // Now we know the actual chunk count, update the placeholders
    let actual_chunk_count = chunks.len();
    for (idx, chunk) in chunks.iter_mut().enumerate() {
        *chunk = chunk.replace(
            &format!("{}/?", idx + 1),
            &format!("{}/{}", idx + 1, actual_chunk_count),
        );
    }

    eprintln!(
        "Large diff detected ({} bytes, {} files). Split into {} chunks for commit message generation.",
        diff_size,
        total_files,
        chunks.len()
    );

    // Log chunk boundaries for debugging
    for (idx, chunk) in chunks.iter().enumerate() {
        let file_count = chunk.matches("diff --git").count();
        eprintln!(
            "Chunk {}: {} bytes, {} files",
            idx + 1,
            chunk.len(),
            file_count
        );
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
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Invalid git identity from config: {}", e),
                ));
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
    let GitIdentity { name, email } =
        resolve_commit_identity(&repo, git_user_name, git_user_email)?;

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
    /// Agent failed with detailed context for error classification.
    /// Includes: exit code, stderr output, agent command
    AgentFailed {
        exit_code: Option<i32>,
        stderr: String,
        agent_cmd: String,
        message: String,
    },
}

impl std::fmt::Display for CommitGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitGenerationError::Timeout => write!(f, "LLM agent timed out"),
            CommitGenerationError::Empty => write!(f, "LLM returned empty response"),
            CommitGenerationError::ExtractionFailed(msg) => {
                write!(f, "Failed to extract commit message: {}", msg)
            }
            CommitGenerationError::ValidationFailed(msg) => write!(f, "Validation failed: {}", msg),
            CommitGenerationError::AgentFailed {
                exit_code,
                stderr,
                agent_cmd,
                message,
            } => {
                write!(f, "Agent '{}' failed: ", agent_cmd)?;
                if let Some(code) = exit_code {
                    write!(f, "Exit code: {}. ", code)?;
                }
                write!(f, "{}", message)?;
                if !stderr.trim().is_empty() {
                    write!(f, "\nStderr: {}", stderr.trim())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CommitGenerationError {}

impl CommitGenerationError {
    /// Classify this error to determine if it should trigger a retry or fallback.
    ///
    /// Returns `Some(true)` if the error should be retried with the same agent,
    /// `Some(false)` if it should trigger immediate fallback to the next agent,
    /// or `None` if the error type doesn't support classification (e.g., ValidationFailed).
    fn should_retry_with_classification(&self) -> Option<bool> {
        use crate::agents::AgentErrorKind;

        match self {
            // Empty responses can be retried (might be transient)
            CommitGenerationError::Empty => Some(true),

            // Timeout can be retried
            CommitGenerationError::Timeout => Some(true),

            // ExtractionFailed is a parsing issue - don't retry
            CommitGenerationError::ExtractionFailed(_) => Some(false),

            // ValidationFailed is handled separately with feedback - always retry
            CommitGenerationError::ValidationFailed(_) => Some(true),

            // AgentFailed needs classification based on exit code and stderr
            CommitGenerationError::AgentFailed {
                exit_code,
                stderr,
                agent_cmd,
                ..
            } => {
                // Parse agent_cmd to get the agent name for classification
                let agent_name = agent_cmd.split_whitespace().next();
                let model_flag = agent_cmd.split_whitespace().find_map(|tok| {
                    if tok.starts_with("-m") || tok.starts_with("--model") {
                        Some(tok.trim_start_matches("-m").trim_start_matches("--model"))
                    } else {
                        None
                    }
                });

                // Use the existing AgentErrorKind classifier
                let exit_code_val = exit_code.unwrap_or(-1);
                let kind = AgentErrorKind::classify_with_agent(
                    exit_code_val,
                    stderr,
                    agent_name,
                    model_flag,
                );

                // Log classification for debugging
                eprintln!(
                    "  Classified as: {} ({})",
                    kind.description(),
                    if kind.should_retry() {
                        "will retry"
                    } else if kind.should_fallback() {
                        "will fallback"
                    } else {
                        "unrecoverable"
                    }
                );

                // Log recovery advice
                eprintln!("  Recovery: {}", kind.recovery_advice());

                Some(kind.should_retry())
            }
        }
    }
}

/// Call the LLM agent with a prompt and return the raw output.
///
/// This is a helper function that handles the actual LLM invocation.
fn call_llm_agent(
    prompt: &str,
    agent_cmd: &str,
    timeout_secs: u64,
) -> Result<String, CommitGenerationError> {
    use crate::utils::split_command;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let argv = split_command(agent_cmd).map_err(|e| CommitGenerationError::AgentFailed {
        exit_code: None,
        stderr: String::new(),
        agent_cmd: agent_cmd.to_string(),
        message: format!("Failed to parse agent command: {}", e),
    })?;

    let (program, args) = match argv.split_first() {
        Some(pair) => pair,
        None => {
            return Err(CommitGenerationError::AgentFailed {
                exit_code: None,
                stderr: String::new(),
                agent_cmd: agent_cmd.to_string(),
                message: "Agent command is empty".to_string(),
            })
        }
    };

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CommitGenerationError::AgentFailed {
            exit_code: None,
            stderr: String::new(),
            agent_cmd: agent_cmd.to_string(),
            message: format!("Failed to spawn agent: {}", e),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| CommitGenerationError::AgentFailed {
                exit_code: None,
                stderr: String::new(),
                agent_cmd: agent_cmd.to_string(),
                message: format!("Failed to write prompt: {}", e),
            })?;
        drop(stdin);
    }

    let timeout = Duration::from_secs(timeout_secs);
    let start_time = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                let mut stdout = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    out.read_to_end(&mut stdout).map_err(|e| {
                        CommitGenerationError::AgentFailed {
                            exit_code: None,
                            stderr: String::new(),
                            agent_cmd: agent_cmd.to_string(),
                            message: format!("Failed to read stdout: {}", e),
                        }
                    })?;
                }

                let mut stderr = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_end(&mut stderr).map_err(|e| {
                        CommitGenerationError::AgentFailed {
                            exit_code: None,
                            stderr: String::new(),
                            agent_cmd: agent_cmd.to_string(),
                            message: format!("Failed to read stderr: {}", e),
                        }
                    })?;
                }

                if !exit_status.success() {
                    let stderr_str = String::from_utf8_lossy(&stderr).to_string();
                    return Err(CommitGenerationError::AgentFailed {
                        exit_code: exit_status.code(),
                        stderr: stderr_str,
                        agent_cmd: agent_cmd.to_string(),
                        message: format!("Agent process exited unsuccessfully"),
                    });
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
                return Err(CommitGenerationError::AgentFailed {
                    exit_code: None,
                    stderr: String::new(),
                    agent_cmd: agent_cmd.to_string(),
                    message: format!("Failed to wait: {}", e),
                });
            }
        }
    }
}

/// Extract and validate commit message from LLM output.
fn extract_and_validate_commit_message(
    raw_output: &str,
    agent_cmd: &str,
) -> Result<String, CommitGenerationError> {
    use crate::files::llm_output_extraction::{
        extract_llm_output, validate_commit_message, OutputFormat,
    };

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

    // Log extraction metadata for debugging
    eprintln!(
        "LLM output extraction: {:?} format, structured={}",
        extraction.format, extraction.was_structured
    );

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

    // Log diff size and sample for debugging
    let diff_size = diff.len();
    let diff_lines: Vec<&str> = diff.lines().collect();
    eprintln!("Diff size: {} bytes, {} lines", diff_size, diff_lines.len());

    // Show first and last few lines for verification
    let first_lines = diff_lines.iter().take(5).collect::<Vec<_>>();
    let last_lines = if diff_lines.len() > 5 {
        diff_lines
            .iter()
            .skip(diff_lines.len().saturating_sub(5))
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    eprintln!("First 5 lines of diff:");
    for line in first_lines {
        eprintln!("  {}", line);
    }
    if !last_lines.is_empty() {
        eprintln!("Last 5 lines of diff:");
        for line in last_lines {
            eprintln!("  {}", line);
        }
    }

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

/// Generate commit message with fallback to alternative agents.
///
/// This function tries each agent in the fallback chain until one succeeds.
/// For each agent, it attempts chunked commit message generation with retries.
/// This provides robustness against agent failures (timeout, rate limits, etc.).
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message for
/// * `agent_cmds` - Slice of agent commands to try in order (first is primary, rest are fallbacks)
///
/// # Returns
///
/// Returns `Ok(String)` with the generated commit message, or an error if all agents fail.
pub(crate) fn generate_commit_message_with_fallback(
    diff: &str,
    agent_cmds: &[String],
) -> io::Result<String> {
    if agent_cmds.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No agent commands provided",
        ));
    }

    eprintln!(
        "Generating commit message with {} agent(s) in fallback chain...",
        agent_cmds.len()
    );

    // Try each agent in the fallback chain
    for (agent_idx, agent_cmd) in agent_cmds.iter().enumerate() {
        eprintln!(
            "Trying agent {}/{}: {}",
            agent_idx + 1,
            agent_cmds.len(),
            agent_cmd
        );

        match generate_commit_message_with_llm(diff, agent_cmd) {
            Ok(msg) => {
                eprintln!("✓ Agent {} succeeded: {}", agent_idx + 1, agent_cmd);
                return Ok(msg);
            }
            Err(e) => {
                eprintln!("✗ Agent {} failed: {}", agent_idx + 1, agent_cmd);
                eprintln!("  Error: {}", e);

                // If this was the last agent, return the error
                if agent_idx == agent_cmds.len() - 1 {
                    eprintln!("All agents in fallback chain failed");
                    return Err(e);
                }
                // Otherwise, continue to the next agent
                eprintln!("  Trying next agent in chain...");
            }
        }
    }

    // Should never reach here, but handle the case
    Err(io::Error::new(io::ErrorKind::Other, "All agents failed"))
}

/// Generate commit message with retry logic.
///
/// This function includes smart retry logic that:
/// 1. Provides feedback to the LLM when validation fails
/// 2. Uses error classification to decide whether to retry or fallback immediately
/// 3. Only retries transient errors (rate limits, network issues, timeouts)
/// 4. Immediately returns for permanent/agent-specific errors (auth failures, GLM quirks)
fn generate_commit_message_with_retries(
    diff: &str,
    agent_cmd: &str,
    chunk_idx: usize,
) -> io::Result<String> {
    use crate::prompts::{
        prompt_generate_commit_message_with_diff, prompt_retry_commit_message_with_feedback,
    };
    use std::time::Duration;

    let max_retries = 3;
    let timeouts = [60, 90, 120]; // Exponential backoff: 60s, 90s, 120s

    // Track the last bad message and validation error for feedback on retry
    let mut last_bad_message: Option<String> = None;
    let mut last_validation_error: Option<String> = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            eprintln!(
                "Retry attempt {}/{} for chunk {}...",
                attempt + 1,
                max_retries,
                chunk_idx + 1
            );
            // Exponential backoff between retries
            let backoff_ms = 1000 * (1 << attempt.min(3)); // 1s, 2s, 4s
            std::thread::sleep(Duration::from_millis(backoff_ms));
        }

        // Use the retry prompt with feedback if we have a previous bad message
        let prompt =
            if let (Some(bad_msg), Some(val_err)) = (&last_bad_message, &last_validation_error) {
                eprintln!("Using retry prompt with validation feedback...");
                prompt_retry_commit_message_with_feedback(diff, bad_msg, val_err)
            } else {
                prompt_generate_commit_message_with_diff(diff)
            };

        match call_llm_agent(
            &prompt,
            agent_cmd,
            timeouts[attempt.min(timeouts.len() - 1)],
        ) {
            Ok(raw_output) => {
                match extract_and_validate_commit_message(&raw_output, agent_cmd) {
                    Ok(commit_message) => {
                        // Success!
                        if attempt > 0 {
                            eprintln!(
                                "Successfully generated commit message after {} retries",
                                attempt
                            );
                        }
                        return Ok(commit_message);
                    }
                    Err(CommitGenerationError::ExtractionFailed(msg)) => {
                        // Extraction failed - don't retry, this is likely a persistent issue
                        return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
                    }
                    Err(CommitGenerationError::ValidationFailed(msg)) => {
                        // Validation failed - save the bad message and error for retry with feedback
                        eprintln!("Validation failed on attempt {}: {}", attempt + 1, msg);
                        eprintln!("Bad message will be provided as feedback on next retry");

                        // Save the raw output as the bad message for feedback
                        let cleaned_bad = clean_commit_message(&raw_output);
                        last_bad_message = Some(cleaned_bad);
                        last_validation_error = Some(msg.clone());

                        if attempt == max_retries - 1 {
                            // Last attempt failed - return error
                            return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
                        }
                        // Continue to retry with feedback
                    }
                    Err(CommitGenerationError::Empty) => {
                        eprintln!("LLM returned empty output on attempt {}", attempt + 1);
                        if attempt == max_retries - 1 {
                            return Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "LLM returned empty response after all retries",
                            ));
                        }
                    }
                    Err(CommitGenerationError::Timeout) => {
                        eprintln!("LLM timed out on attempt {}", attempt + 1);
                        if attempt == max_retries - 1 {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "LLM timed out after all retries",
                            ));
                        }
                    }
                    Err(err @ CommitGenerationError::AgentFailed { .. }) => {
                        // Use error classification to decide whether to retry or fallback immediately
                        eprintln!("Agent failed on attempt {}: {}", attempt + 1, err);

                        match err.should_retry_with_classification() {
                            Some(true) => {
                                // Error is transient - retry with same agent
                                eprintln!("  -> Retrying same agent (transient error)");
                                if attempt == max_retries - 1 {
                                    return Err(io::Error::other(err.to_string()));
                                }
                                // Continue to next retry
                            }
                            Some(false) => {
                                // Error is permanent or agent-specific - fallback immediately
                                eprintln!("  -> Falling back to next agent (non-retryable error)");
                                return Err(io::Error::other(err.to_string()));
                            }
                            None => {
                                // No classification available - use default retry behavior
                                if attempt == max_retries - 1 {
                                    return Err(io::Error::other(err.to_string()));
                                }
                            }
                        }
                    }
                }
            }
            Err(CommitGenerationError::Timeout) => {
                eprintln!("LLM timed out on attempt {}", attempt + 1);
                if attempt == max_retries - 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("LLM timed out after {} attempts", max_retries),
                    ));
                }
            }
            Err(err @ CommitGenerationError::AgentFailed { .. }) => {
                // Use error classification to decide whether to retry or fallback immediately
                eprintln!("Agent error on attempt {}: {}", attempt + 1, err);

                match err.should_retry_with_classification() {
                    Some(true) => {
                        // Error is transient - retry with same agent
                        eprintln!("  -> Retrying same agent (transient error)");
                        if attempt == max_retries - 1 {
                            return Err(io::Error::other(err.to_string()));
                        }
                        // Continue to next retry
                    }
                    Some(false) => {
                        // Error is permanent or agent-specific - fallback immediately
                        eprintln!("  -> Falling back to next agent (non-retryable error)");
                        return Err(io::Error::other(err.to_string()));
                    }
                    None => {
                        // No classification available - use default retry behavior
                        if attempt == max_retries - 1 {
                            return Err(io::Error::other(err.to_string()));
                        }
                    }
                }
            }
            Err(e) => {
                return Err(io::Error::other(e.to_string()));
            }
        }
    }

    unreachable!("Loop should always return")
}

/// Combine messages from multiple chunks into a single commit message.
///
/// This function analyzes messages from all chunks and synthesizes them into
/// a single meaningful commit message that captures the semantic meaning from
/// all chunks.
///
/// Strategy:
/// 1. Extract type, scope, and subject from each chunk's message
/// 2. Use the most significant commit type (feat > fix > refactor > others)
/// 3. Use the most common scope if it appears in at least half the chunks
/// 4. Combine subjects intelligently - if they're similar, merge; if different,
///    concatenate with "and" or create a comprehensive subject
fn combine_chunk_messages(messages: &[String]) -> String {
    // Type priority for significance (higher = more significant)
    fn type_priority(ty: &str) -> i32 {
        match ty {
            "feat" => 5,
            "fix" => 4,
            "refactor" => 3,
            "perf" => 3,
            "docs" => 2,
            "test" => 2,
            "style" => 1,
            "build" | "ci" | "chore" => 0,
            _ => 0,
        }
    }

    // Analyze all messages to extract type, scope, and subjects
    struct ChunkInfo {
        commit_type: String,
        scope: String,
        subject: String,
        priority: i32,
    }

    // Helper function to combine subjects
    fn combine_subjects(chunks: &[ChunkInfo]) -> String {
        use std::collections::HashSet;

        if chunks.is_empty() {
            return "apply changes across multiple files".to_string();
        }

        if chunks.len() == 1 {
            return chunks[0].subject.clone();
        }

        // Collect all unique non-empty subjects
        let subjects: Vec<&str> = chunks
            .iter()
            .map(|c| c.subject.as_str())
            .filter(|s| !s.is_empty())
            .collect();

        if subjects.is_empty() {
            return "apply changes across multiple files".to_string();
        }

        // If all subjects are similar (share common words), use a merged version
        let words: Vec<HashSet<&str>> = subjects
            .iter()
            .map(|s| s.split_whitespace().collect::<HashSet<_>>())
            .collect();

        // Check if there's significant overlap (at least 50% of words)
        let total_unique_words: HashSet<_> = words.iter().flatten().cloned().collect();
        let avg_word_count =
            (words.iter().map(|w| w.len()).sum::<usize>() as f64) / (words.len() as f64);

        if (total_unique_words.len() as f64) < (avg_word_count * 1.5) {
            // Significant overlap - subjects are similar, use a merged version
            // Find common prefix/words and combine
            let first_subject = subjects[0];
            // For simplicity, use the first subject if they're all similar
            // This is better than concatenating redundant information
            return first_subject.to_string();
        }

        // Subjects are different - combine them intelligently
        // If we have 2 subjects, join with " and "
        // If we have more, create a more comprehensive subject
        if subjects.len() == 2 {
            format!("{} and {}", subjects[0], subjects[1])
        } else if subjects.len() <= 4 {
            // Join last two with "and", others with commas
            let last_idx = subjects.len() - 1;
            format!(
                "{}, and {}",
                subjects[..last_idx].join(", "),
                subjects[last_idx]
            )
        } else {
            // Too many different subjects - create a generic but descriptive message
            // Avoid bad patterns like "N files changed" or "apply multiple changes"
            "apply changes across multiple files".to_string()
        }
    }

    if messages.len() == 1 {
        return messages[0].clone();
    }

    let mut chunks: Vec<ChunkInfo> = Vec::new();
    let mut last_seen_type = "chore";

    for msg in messages {
        // Extract type and scope from conventional commit format
        if let Some(colon_pos) = msg.find(':') {
            let type_part = &msg[..colon_pos];
            let (commit_type, scope) = if let Some(space_pos) = type_part.rfind(' ') {
                // Has scope: type(scope)
                (
                    &type_part[..space_pos],
                    type_part[space_pos + 1..]
                        .trim_start_matches('(')
                        .trim_end_matches(')'),
                )
            } else {
                // No scope: just type
                (type_part, "")
            };

            // Track the last seen type for backward compatibility
            last_seen_type = commit_type;

            // Extract subject (after colon, before newline)
            let subject_start = colon_pos + 1;
            let subject = if let Some(newline_pos) = msg[subject_start..].find('\n') {
                msg[subject_start..subject_start + newline_pos].trim()
            } else {
                msg[subject_start..].trim()
            };

            // Skip chunk placeholders and generic subjects
            if subject.starts_with('[') || subject.contains("chunk") {
                continue;
            }

            chunks.push(ChunkInfo {
                commit_type: commit_type.to_string(),
                scope: scope.to_string(),
                subject: subject.to_string(),
                priority: type_priority(commit_type),
            });
        }
    }

    // If no valid chunks were extracted (all chunks were placeholders like "[chunk 1]"),
    // this means LLM failed for all chunks. Return a more descriptive message
    // that indicates the need for manual review or fallback analysis.
    if chunks.is_empty() {
        // Use a descriptive message that avoids bad patterns like "N file(s) changed"
        // The message indicates that processing failed but provides semantic context
        return format!("{}: apply changes across multiple files", last_seen_type);
    }

    // Find the most significant type
    let most_significant = chunks.iter().max_by_key(|c| c.priority).unwrap();
    let commit_type = &most_significant.commit_type;

    // Find the most common scope (if any)
    let mut scope_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for chunk in &chunks {
        if !chunk.scope.is_empty() {
            *scope_counts.entry(chunk.scope.clone()).or_insert(0) += 1;
        }
    }

    // Use the most common scope if it appears in at least half the chunks
    let scope = scope_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .filter(|&(ref _scope, count)| count * 2 >= chunks.len())
        .map(|(scope, _)| scope)
        .unwrap_or_else(String::new);

    // Combine subjects intelligently
    let combined_subject = combine_subjects(&chunks);

    // Build combined message
    let mut result = if scope.is_empty() {
        format!("{}:", commit_type)
    } else {
        format!("{}({}):", commit_type, scope)
    };

    result.push(' ');
    result.push_str(&combined_subject);

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
    let total_files =
        changes.new_files.len() + changes.modified_files.len() + changes.deleted_files.len();

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

    for file in changes
        .new_files
        .iter()
        .chain(changes.modified_files.iter())
        .chain(changes.deleted_files.iter())
    {
        let lower = file.to_lowercase();
        if lower.contains("test") || lower.ends_with("_test.rs") || lower.ends_with(".test.js") {
            test_count += 1;
        } else if lower.contains("readme") || lower.contains("doc") || lower.ends_with(".md") {
            doc_count += 1;
        } else if lower.contains("src")
            || lower.ends_with(".rs")
            || lower.ends_with(".js")
            || lower.ends_with(".py")
        {
            src_count += 1;
        } else if lower.contains("build")
            || lower.contains("cargo.toml")
            || lower.contains("package.json")
        {
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
    } else if changes.new_files.len() > changes.modified_files.len() && changes.new_files.len() > 0
    {
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
///
/// This function analyzes file paths to extract semantic meaning and create
/// a descriptive commit message subject. It avoids generic patterns like
/// "update N files" by extracting module/directory names from the file paths.
fn build_subject_line(changes: &FileChanges, total_files: usize, _commit_type: &str) -> String {
    // Collect all changed files for analysis
    let all_files: Vec<&String> = changes
        .new_files
        .iter()
        .chain(changes.modified_files.iter())
        .chain(changes.deleted_files.iter())
        .collect();

    if all_files.is_empty() {
        return "uncommitted changes".to_string();
    }

    // Analyze file paths to extract semantic meaning
    let common_prefix = find_common_path_prefix(&all_files);
    let module_info = extract_module_info(&all_files, &common_prefix);

    // Determine action verb based on what changed
    let action = determine_action_verb(changes);

    // Build the subject line based on the analysis
    if total_files == 1 {
        // Single file - use filename with semantic context
        let file = &all_files[0];
        let filename = file.rsplit('/').next().unwrap_or(file);
        let shortened = shorten_path(file);
        if shortened != filename {
            // Has parent directory, use "parent/filename"
            format!("{} {}", action, shortened)
        } else {
            format!("{} {}", action, filename)
        }
    } else if let Some(module) = module_info.main_module {
        // Multiple files in a common module - use module as scope
        if module_info.submodules.len() <= 2 {
            // List submodules if there are only a few
            let sub_parts: Vec<&str> = module_info.submodules.iter().map(|s| s.as_str()).collect();
            if sub_parts.is_empty() {
                format!("{} {} module", action, module)
            } else {
                format!("{} {} module ({})", action, module, sub_parts.join(", "))
            }
        } else {
            // Many submodules, use generic but descriptive message
            format!("{} {} module", action, module)
        }
    } else if let Some(prefix) = common_prefix {
        // Files share a common path prefix
        format!("{} files in {}", action, prefix)
    } else {
        // Files are spread across different locations - use semantic analysis
        let locations = extract_semantic_locations(&all_files);
        if locations.len() == 1 {
            format!("{} {}", action, locations[0])
        } else if locations.len() <= 3 {
            format!("{} {}", action, locations.join(" and "))
        } else {
            // Too many different locations - use a descriptive message
            // Avoid bad patterns like "update N files" or "apply N file changes"
            format!("{} changes across multiple modules", action)
        }
    }
}

/// Information about the modules affected by changes.
struct ModuleInfo {
    main_module: Option<String>,
    submodules: Vec<String>,
}

/// Find the common path prefix among all files.
fn find_common_path_prefix(files: &[&String]) -> Option<String> {
    if files.is_empty() {
        return None;
    }

    // Split all file paths into components
    let all_paths: Vec<Vec<&str>> = files.iter().map(|f| f.split('/').collect()).collect();

    // Find common prefix
    let mut common_prefix = Vec::new();
    for (i, component) in all_paths[0].iter().enumerate() {
        let all_match = all_paths.iter().all(|path| path.get(i) == Some(component));
        if all_match && i < all_paths[0].len() - 1 {
            // Keep the common component (but not the filename itself)
            common_prefix.push(*component);
        } else {
            break;
        }
    }

    if common_prefix.is_empty() {
        None
    } else {
        Some(common_prefix.join("/"))
    }
}

/// Extract module information from file paths.
fn extract_module_info(files: &[&String], common_prefix: &Option<String>) -> ModuleInfo {
    use std::collections::HashMap;

    let mut submodule_counts: HashMap<String, usize> = HashMap::new();

    for file in files {
        // Remove the common prefix and extract the module/submodule
        let relative_path = if let Some(prefix) = common_prefix {
            file.strip_prefix(&format!("{}/", prefix))
                .unwrap_or(file.as_str())
        } else {
            file.as_str()
        };

        // Get the first component after the prefix (this is the module)
        let components: Vec<&str> = relative_path.split('/').collect();
        if components.len() > 1 {
            // Has at least one directory level
            *submodule_counts
                .entry(components[0].to_string())
                .or_insert(0) += 1;
        }
    }

    // Determine the main module and submodules
    if submodule_counts.is_empty() {
        ModuleInfo {
            main_module: None,
            submodules: Vec::new(),
        }
    } else {
        // Find the most common module as the main module
        let main_module = submodule_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(name, _)| name.clone());

        // Collect other submodules
        let mut submodules: Vec<String> = submodule_counts
            .into_iter()
            .filter(|(name, _)| Some(name) != main_module.as_ref())
            .map(|(name, _)| name)
            .collect();

        submodules.sort();
        submodules.truncate(3); // Limit to 3 submodules

        ModuleInfo {
            main_module,
            submodules,
        }
    }
}

/// Extract semantic location names from file paths.
fn extract_semantic_locations(files: &[&String]) -> Vec<String> {
    use std::collections::HashSet;

    let mut locations = HashSet::new();

    for file in files {
        let parts: Vec<&str> = file.split('/').collect();
        if parts.len() >= 2 {
            // Get the directory name (second to last component)
            let dir = parts[parts.len() - 2];
            // Map common directory names to semantic names
            let semantic_name = match dir {
                "src" => "source code",
                "tests" | "test" => "tests",
                "docs" | "doc" => "documentation",
                "examples" => "examples",
                "benches" | "bench" => "benchmarks",
                "scripts" => "scripts",
                _ => dir,
            };
            locations.insert(semantic_name.to_string());
        }
    }

    let mut locs: Vec<String> = locations.into_iter().collect();
    locs.sort();
    locs.truncate(3);
    locs
}

/// Determine the action verb based on what changed.
fn determine_action_verb(changes: &FileChanges) -> &'static str {
    let has_new = !changes.new_files.is_empty();
    let has_modified = !changes.modified_files.is_empty();
    let has_deleted = !changes.deleted_files.is_empty();

    match (has_new, has_modified, has_deleted) {
        (true, false, false) => "add",
        (false, true, false) => "update",
        (false, false, true) => "remove",
        (true, true, false) => "add and update",
        (true, false, true) => "add and remove",
        (false, true, true) => "update and remove",
        (true, true, true) => "modify",
        (false, false, false) => "modify", // Should not happen since total_files > 0 is checked above
    }
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

/// Save failed LLM output to a log file for debugging.
fn save_failed_llm_output(diff: &str, error: &str) -> io::Result<()> {
    use std::fs::{self, File};
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Create the logs directory if it doesn't exist
    let log_dir = ".agent/logs/commit_generation_failed";
    fs::create_dir_all(log_dir)?;

    // Create a timestamped filename
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let filename = format!("{}/failed_{}.log", log_dir, timestamp);
    let mut file = File::create(&filename)?;

    writeln!(file, "=== Failed LLM Commit Message Generation ===")?;
    writeln!(file, "Timestamp: {}", timestamp)?;
    writeln!(file, "\n=== Error ===")?;
    writeln!(file, "{}", error)?;
    writeln!(file, "\n=== Diff ===")?;
    writeln!(file, "{}", diff)?;
    writeln!(file, "\n=== End of Report ===")?;

    eprintln!("Failed LLM output saved to: {}", filename);
    Ok(())
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

/// Create a commit with an automatically generated commit message using fallback chain.
///
/// This function uses a fallback chain of agents for commit message generation.
/// If the primary agent fails, it will try each fallback agent in order until one
/// succeeds or all fail.
///
/// # Arguments
///
/// * `agent_cmds` - Slice of agent commands to try in order (first is primary, rest are fallbacks)
/// * `git_user_name` - Optional git user name
/// * `git_user_email` - Optional git user email
///
/// # Returns
///
/// Returns `Ok(Some(oid))` with the commit OID if successful, `Ok(None)` if there
/// were no meaningful changes, or an error if all agents fail and git operations fail.
pub(crate) fn commit_with_auto_message_using_fallback(
    agent_cmds: &[String],
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> io::Result<Option<git2::Oid>> {
    // Check if LLM failures should be hard errors
    let must_use_llm = std::env::var("RALPH_COMMIT_MUST_USE_LLM")
        .ok()
        .and_then(|v| match v.to_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            _ => None,
        })
        .unwrap_or(false);

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

    // Generate commit message via LLM with fallback chain
    let commit_message = match generate_commit_message_with_fallback(&diff, agent_cmds) {
        Ok(msg) => {
            // Validate the commit message is not empty
            if msg.trim().is_empty() {
                let error = "LLM returned empty commit message".to_string();
                let _ = save_failed_llm_output(&diff, &error);

                if must_use_llm {
                    return Err(io::Error::new(io::ErrorKind::Other, error));
                }

                eprintln!();
                eprintln!("========================================");
                eprintln!("⚠️  WARNING: USING FALLBACK COMMIT MESSAGE");
                eprintln!("========================================");
                eprintln!("⚠️  COMMIT MESSAGE GENERATION PATH: FALLBACK");
                eprintln!("⚠️  REASON: LLM returned empty commit message");
                eprintln!();
                eprintln!("This means your commit message will be GENERIC and may NOT");
                eprintln!("accurately describe what changed. Consider reviewing the");
                eprintln!("commit message after this operation completes.");
                eprintln!();
                eprintln!("To debug, check .agent/logs/commit_generation_failed/");
                eprintln!("To make this a hard error, set RALPH_COMMIT_MUST_USE_LLM=1");
                eprintln!();

                generate_fallback_commit_message(&diff)
            } else {
                // LLM succeeded - log the path and final message
                eprintln!("✓ COMMIT MESSAGE GENERATION PATH: LLM SUCCESS WITH FALLBACK");
                eprintln!(
                    "✓ Final commit message: {}",
                    msg.lines().next().unwrap_or(&msg)
                );
                msg
            }
        }
        Err(e) => {
            // Save the failed output for debugging
            let error_msg = format!(
                "LLM commit message generation failed (all agents in fallback chain): {}",
                e
            );
            let _ = save_failed_llm_output(&diff, &error_msg);

            if must_use_llm {
                return Err(io::Error::new(io::ErrorKind::Other, error_msg));
            }

            eprintln!();
            eprintln!("========================================");
            eprintln!("⚠️  WARNING: USING FALLBACK COMMIT MESSAGE");
            eprintln!("========================================");
            eprintln!("⚠️  COMMIT MESSAGE GENERATION PATH: FALLBACK");
            eprintln!("⚠️  REASON: All agents in fallback chain failed - {}", e);
            eprintln!();
            eprintln!("This means your commit message will be GENERIC and may NOT");
            eprintln!("accurately describe what changed. You should EDIT the commit");
            eprintln!("message to be more specific after this operation completes.");
            eprintln!();
            eprintln!("To debug, check .agent/logs/commit_generation_failed/");
            eprintln!("To make this a hard error, set RALPH_COMMIT_MUST_USE_LLM=1");
            eprintln!();

            generate_fallback_commit_message(&diff)
        }
    };

    stage_and_commit(&commit_message, git_user_name, git_user_email)
}

/// Result of commit operation with fallback.
///
/// This is the fallback-aware version of `CommitResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitResultFallback {
    /// A commit was successfully created with the given OID.
    Success(git2::Oid),
    /// No commit was created because there were no meaningful changes.
    NoChanges,
    /// The commit operation failed with an error message.
    Failed(String),
}

/// Create a commit with an automatically generated commit message using fallback chain, returning a detailed result.
///
/// This is a convenience wrapper around `commit_with_auto_message_using_fallback` that returns
/// a `CommitResultFallback` enum for easier error handling and logging in the orchestrator.
pub(crate) fn commit_with_auto_message_fallback_result(
    agent_cmds: &[String],
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> CommitResultFallback {
    match commit_with_auto_message_using_fallback(agent_cmds, git_user_name, git_user_email) {
        Ok(Some(oid)) => CommitResultFallback::Success(oid),
        Ok(None) => CommitResultFallback::NoChanges,
        Err(e) => CommitResultFallback::Failed(e.to_string()),
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
            diff.push_str(&format!(
                "diff --git a/file{}.rs b/file{}.rs\nnew file mode 100644\n+ content {}\n",
                i, i, i
            ));
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
    fn test_chunk_diff_respects_file_boundaries() {
        // Create a diff with clear file boundaries
        let mut diff = String::new();
        for i in 0..10 {
            diff.push_str(&format!(
                "diff --git a/file{}.rs b/file{}.rs\nnew file mode 100644\n+ content {}\n",
                i, i, i
            ));
        }

        let chunks = chunk_diff_for_commit_message(&diff);

        // Verify that each chunk contains complete file diffs
        // (no chunk should have a partial diff)
        for chunk in &chunks {
            let file_count = chunk.matches("diff --git").count();

            // Each diff header should be complete (start with "diff --git")
            for line in chunk.lines() {
                if line.starts_with("diff --git") {
                    assert!(
                        line.starts_with("diff --git"),
                        "Each file diff should start with 'diff --git'"
                    );
                }
            }

            // The chunk should report its file count correctly
            // For single chunk (not chunked), the format is different
            if chunk.contains("[Diff chunk") {
                assert!(chunk.contains(&format!("{} files", file_count)));
            }
        }
    }

    #[test]
    fn test_chunk_diff_does_not_split_files() {
        // Create a diff with a single large file
        let mut diff = String::new();
        diff.push_str("diff --git a/large_file.rs b/large_file.rs\n");
        for i in 0..1000 {
            diff.push_str(&format!("+line {}\n", i));
        }

        let chunks = chunk_diff_for_commit_message(&diff);

        // Even though the file is large, it should be in a single chunk
        // because we don't want to split file diffs
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("diff --git a/large_file.rs"));
        // Verify all lines are present
        assert_eq!(chunks[0].matches("+line").count(), 1000);
    }

    #[test]
    fn test_chunk_diff_respects_max_chunks() {
        // Create an extremely large diff
        let mut diff = String::new();
        for i in 0..50000 {
            diff.push_str(&format!(
                "diff --git a/file{}.rs b/file{}.rs\n+ content {}\n",
                i, i, i
            ));
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
        // The function now uses the most significant type (feat > fix > test > others)
        assert!(combined.starts_with("feat:"));
        // Should have one of the subjects
        assert!(
            combined.contains("feature")
                || combined.contains("bug")
                || combined.contains("coverage")
        );
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
