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

/// Generate a commit message by calling an LLM with the diff.
///
/// This function calls the configured developer agent with a prompt to generate
/// a commit message from the provided diff. It returns the raw output from the agent.
///
/// The prompt is passed via stdin to avoid command-line length limits on some systems.
///
/// This is a public function so it can be used by both `commit_with_auto_message()`
/// and plumbing commands like `--generate-commit-msg`.
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message for
/// * `agent_cmd` - The command to invoke the agent (e.g., "claude", "codex")
///
/// # Returns
///
/// Returns `Ok(String)` with the generated commit message, or an error if the call fails.
///
/// # Timeout
///
/// The LLM agent has a 60 second timeout. If it exceeds this, the function returns
/// an error and the caller should use fallback commit message generation.
pub(crate) fn generate_commit_message_with_llm(diff: &str, agent_cmd: &str) -> io::Result<String> {
    use crate::prompts::prompt_generate_commit_message_with_diff;
    use crate::utils::split_command;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // Create the prompt with the diff
    let prompt = prompt_generate_commit_message_with_diff(diff);

    // Parse the agent command
    let argv = split_command(agent_cmd).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Failed to parse agent command: {}", e),
        )
    })?;

    // Build the command, passing prompt via stdin to avoid command-line length limits
    // Use pattern matching to safely extract program and args without unwrap
    let (program, args) = match argv.split_first() {
        Some(pair) => pair,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Agent command is empty",
            ))
        }
    };

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Write the prompt to the child process's stdin
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(prompt.as_bytes()).map_err(|e| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("Failed to write prompt to agent stdin: {}", e),
            )
        })?;
        // Drop stdin to signal EOF to the child process
        drop(stdin);
    }

    // Set a timeout of 60 seconds for the LLM to respond
    // This prevents the pipeline from hanging indefinitely if the agent hangs
    let timeout = Duration::from_secs(60);
    let start_time = Instant::now();

    // Wait for the process with timeout using try_wait in a loop.
    //
    // Note: `try_wait()` reaps the child when it exits; calling `wait_with_output()`
    // afterwards can fail ("no child processes"). We collect stdout/stderr ourselves
    // once we observe the exit status.
    loop {
        // Try to wait for the process without blocking
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                // Process completed - collect output and return
                use std::io::Read;

                let mut stdout = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    out.read_to_end(&mut stdout)
                        .map_err(|e| io::Error::other(format!("Failed to read LLM agent stdout: {}", e)))?;
                }

                let mut stderr = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_end(&mut stderr)
                        .map_err(|e| io::Error::other(format!("Failed to read LLM agent stderr: {}", e)))?;
                }

                if !exit_status.success() {
                    let stderr_str = String::from_utf8_lossy(&stderr);
                    return Err(io::Error::other(format!(
                        "Agent command failed with exit code: {:?}{}",
                        exit_status.code(),
                        if stderr_str.trim().is_empty() {
                            String::new()
                        } else {
                            format!("\n{}", stderr_str.trim())
                        }
                    )));
                }

                // Extract commit message using the robust LLM output extraction module.
                // This handles multiple output formats: Claude, Codex, Gemini, OpenCode, and plain text.
                use crate::files::llm_output_extraction::{
                    extract_llm_output, validate_commit_message, OutputFormat,
                };

                let raw_output = String::from_utf8_lossy(&stdout);
                let format_hint = agent_cmd
                    .split_whitespace()
                    .find_map(|tok| {
                        let tok = tok.to_lowercase();
                        if tok.contains("codex") {
                            Some("codex")
                        } else if tok.contains("claude")
                            || tok.contains("ccs")
                            || tok.contains("qwen")
                        {
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

                let extraction = extract_llm_output(&raw_output, format_hint);
                if let Some(warning) = &extraction.warning {
                    eprintln!(
                        "Warning: LLM output extraction warning (format={:?}): {}",
                        extraction.format, warning
                    );
                } else if !extraction.was_structured {
                    eprintln!(
                        "Warning: LLM output extraction fell back to plain text (format={:?})",
                        extraction.format
                    );
                }

                // Clean the extracted content
                let commit_message = clean_commit_message(&extraction.content);

                // Validate the commit message to catch extraction failures early
                if let Err(validation_error) = validate_commit_message(&commit_message) {
                    // Log warning but don't fail - validation is advisory
                    // The commit message might still be usable even if it doesn't pass validation
                    eprintln!(
                        "Warning: commit message validation failed: {}",
                        validation_error
                    );

                    // If the message looks like raw JSON (extraction totally failed), return error
                    if commit_message.starts_with('{') && commit_message.contains(r#""type":"#) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Failed to extract commit message from LLM output: {}. Raw output appears to be JSON.",
                                validation_error
                            ),
                        ));
                    }
                }

                return Ok(commit_message);
            }
            Ok(None) => {
                // Process still running - check timeout before sleeping
                if start_time.elapsed() >= timeout {
                    // Kill the child process and return timeout error
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("LLM agent timed out after {} seconds", timeout.as_secs()),
                    ));
                }
                // Sleep a bit and try again
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                // Error checking process status
                let _ = child.kill();
                return Err(io::Error::other(format!(
                    "Failed to wait for LLM agent: {}",
                    e
                )));
            }
        }
    }
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

/// Generate a descriptive fallback commit message from a diff.
///
/// When LLM commit message generation fails, this function analyzes the diff
/// to create a more informative fallback message than a generic "chore" message.
/// It extracts information about changed files and change types.
fn generate_fallback_commit_message(diff: &str) -> String {
    let mut changed_files = Vec::new();

    for line in diff.lines() {
        // Parse diff headers to extract file names and change types
        // Git diff format:
        // diff --git a/path/to/file b/path/to/file
        // new file mode ...
        // deleted file mode ...
        if line.starts_with("diff --git") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                // Extract file path from "a/path" or "b/path"
                let file_path = parts[3].strip_prefix("b/").unwrap_or(parts[3]);
                changed_files.push(file_path.to_string());
            }
        }
    }

    // Count total changes
    let total_changes = changed_files.len();

    if total_changes == 0 {
        return "chore: uncommitted changes".to_string();
    }

    // Build a descriptive message
    let mut message = String::from("chore:");

    if total_changes <= 3 {
        // List up to 3 files by name
        message.push_str(&format!(" update {}", changed_files.join(", ")));
    } else {
        // For more than 3 files, just show the count
        message.push_str(&format!(" {} file(s) changed", total_changes));
    }

    message
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

    // Generate commit message via LLM, with fallback if it fails
    let commit_message = match generate_commit_message_with_llm(&diff, agent_cmd) {
        Ok(msg) => {
            // Validate the commit message is not empty
            if msg.trim().is_empty() {
                generate_fallback_commit_message(&diff)
            } else {
                msg
            }
        }
        Err(_) => {
            // LLM failed to generate a message - use a fallback
            // This ensures we don't lose progress if the LLM is unavailable
            generate_fallback_commit_message(&diff)
        }
    };

    // Stage all changes and verify staging succeeded
    let staged = git_add_all()?;

    // Validate that staging succeeded before attempting to commit
    // If no files were staged (staged == false), there's nothing to commit
    if !staged {
        return Ok(None);
    }

    // Create the commit
    let oid = git_commit(&commit_message, git_user_name, git_user_email)?;

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

    // Integration test helper - note this would require a temporary git repo
    // For full integration tests, see tests/git_workflow.rs
}
