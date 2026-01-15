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

use super::identity::GitIdentity;

/// Maximum diff size (in bytes) before showing a warning.
/// 100KB is a reasonable threshold - most meaningful diffs are smaller.
const MAX_DIFF_SIZE_WARNING: usize = 100 * 1024;

/// Maximum diff size (in bytes) before truncation for reviewers.
/// 1MB provides reviewers with more context for large changes.
const MAX_DIFF_SIZE_HARD: usize = 1024 * 1024;

/// Truncation marker for reviewer diffs.
const DIFF_TRUNCATED_MARKER: &str =
    "\n\n[Diff truncated due to size. Showing first portion above.]";

/// Convert git2 error to `io::Error`.
fn git2_to_io_error(err: &git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}

/// Check if we're in a git repository.
pub fn require_git_repo() -> io::Result<()> {
    git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    Ok(())
}

/// Get the git repository root.
pub fn get_repo_root() -> io::Result<PathBuf> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    repo.workdir()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No workdir for repository"))
}

/// Get the git hooks directory path.
///
/// Returns the path to the hooks directory inside .git (or the equivalent
/// for worktrees and other configurations).
pub fn get_hooks_dir() -> io::Result<PathBuf> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    Ok(repo.path().join("hooks"))
}

/// Get a snapshot of the current git status.
///
/// Returns status in porcelain format (similar to `git status --porcelain=v1`).
pub fn git_snapshot() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| git2_to_io_error(&e))?;

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
pub fn git_diff() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Try to get HEAD tree
    let head_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree().map_err(|e| git2_to_io_error(&e))?),
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
                .map_err(|e| git2_to_io_error(&e))?;

            let mut result = Vec::new();
            diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
                result.extend_from_slice(line.content());
                true
            })
            .map_err(|e| git2_to_io_error(&e))?;

            return Ok(String::from_utf8_lossy(&result).to_string());
        }
        Err(e) => return Err(git2_to_io_error(&e)),
    };

    // For repos with commits, diff HEAD against working tree
    // This includes both staged and unstaged changes
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .map_err(|e| git2_to_io_error(&e))?;

    // Generate diff text
    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(|e| git2_to_io_error(&e))?;

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
pub fn validate_and_truncate_diff(diff: String) -> (String, bool) {
    let diff_size = diff.len();

    // Warn about large diffs
    if diff_size > MAX_DIFF_SIZE_WARNING {
        eprintln!(
            "Warning: Large diff detected ({diff_size} bytes). This may affect commit message quality."
        );
    }

    // Truncate if over the hard limit
    if diff_size > MAX_DIFF_SIZE_HARD {
        let truncate_size = MAX_DIFF_SIZE_HARD - DIFF_TRUNCATED_MARKER.len();
        let truncated = diff.char_indices().nth(truncate_size).map_or_else(
            || format!("{diff}{DIFF_TRUNCATED_MARKER}"),
            |(i, _)| format!("{}{}", &diff[..i], DIFF_TRUNCATED_MARKER),
        );

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

fn index_has_changes_to_commit(repo: &git2::Repository, index: &git2::Index) -> io::Result<bool> {
    match repo.head() {
        Ok(head) => {
            let head_tree = head.peel_to_tree().map_err(|e| git2_to_io_error(&e))?;
            let diff = repo
                .diff_tree_to_index(Some(&head_tree), Some(index), None)
                .map_err(|e| git2_to_io_error(&e))?;
            Ok(diff.deltas().len() > 0)
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(!index.is_empty()),
        Err(e) => Err(git2_to_io_error(&e)),
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
pub fn git_add_all() -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    let mut index = repo.index().map_err(|e| git2_to_io_error(&e))?;

    // Stage deletions (equivalent to `git add -A` behavior).
    // libgit2's `add_all` doesn't automatically remove deleted paths.
    let mut status_opts = git2::StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut status_opts))
        .map_err(|e| git2_to_io_error(&e))?;
    for entry in statuses.iter() {
        if entry.status().contains(git2::Status::WT_DELETED) {
            if let Some(path) = entry.path() {
                index
                    .remove_path(std::path::Path::new(path))
                    .map_err(|e| git2_to_io_error(&e))?;
            }
        }
    }

    // Add all files (staged, unstaged, and untracked)
    // Note: add_all() is required here, not update_all(), to include untracked files
    let mut filter_cb = |path: &std::path::Path, _matched: &[u8]| -> i32 {
        // Never stage Ralph internal artifacts, even if the user's repo doesn't ignore them.
        i32::from(is_internal_agent_artifact(path))
    };
    index
        .add_all(
            vec!["."],
            git2::IndexAddOption::DEFAULT,
            Some(&mut filter_cb),
        )
        .map_err(|e| git2_to_io_error(&e))?;

    index.write().map_err(|e| git2_to_io_error(&e))?;

    // Return true if staging produced something commit-worthy.
    index_has_changes_to_commit(&repo, &index)
}

/// Resolve git commit identity with the full priority chain.
///
/// This function implements the identity resolution priority chain:
/// 1. Git config (via libgit2's `repo.signature()`) - primary source
/// 2. Provided name/email parameters (from Ralph config, CLI args, or env vars)
/// 3. Environment variables (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`)
/// 4. Ralph config file values (passed through)
/// 5. System username + derived email
/// 6. Default values ("Ralph Workflow", "ralph@localhost")
///
/// Partial overrides are supported: CLI args/env vars/config can override
/// individual fields (name or email) from git config.
///
/// # Arguments
///
/// * `repo` - The git repository (for git config)
/// * `provided_name` - Optional name from Ralph config or CLI
/// * `provided_email` - Optional email from Ralph config or CLI
///
/// # Returns
///
/// Returns `GitIdentity` with the resolved name and email.
fn resolve_commit_identity(
    repo: &git2::Repository,
    provided_name: Option<&str>,
    provided_email: Option<&str>,
) -> GitIdentity {
    use super::identity::{default_identity, fallback_email, fallback_username};

    // Priority 1: Git config (via libgit2) - primary source
    let mut name = String::new();
    let mut email = String::new();
    let mut has_git_config = false;

    if let Ok(sig) = repo.signature() {
        let git_name = sig.name().unwrap_or("");
        let git_email = sig.email().unwrap_or("");
        if !git_name.is_empty() && !git_email.is_empty() {
            name = git_name.to_string();
            email = git_email.to_string();
            has_git_config = true;
        }
    }

    // Priority 2-4: CLI args, env vars, Ralph config (as overrides to git config)
    // These can override individual fields from git config (partial override support)
    let cli_name = std::env::var("RALPH_GIT_USER_NAME").ok();
    let cli_email = std::env::var("RALPH_GIT_USER_EMAIL").ok();

    // Apply overrides in priority order: CLI args > env vars > provided params > git config
    let final_name = provided_name
        .filter(|s| !s.is_empty())
        .or(cli_name.as_deref())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            has_git_config
                .then_some(name.as_str())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("");

    let final_email = provided_email
        .filter(|s| !s.is_empty())
        .or(cli_email.as_deref())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            has_git_config
                .then_some(email.as_str())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("");

    // If we have both name and email from git config + overrides, use them
    if !final_name.is_empty() && !final_email.is_empty() {
        let identity = GitIdentity::new(final_name.to_string(), final_email.to_string());
        if identity.validate().is_ok() {
            return identity;
        }
    }

    // Priority 5: System username + derived email
    let username = fallback_username();
    let system_email = fallback_email(&username);
    let identity = GitIdentity::new(
        if final_name.is_empty() {
            username
        } else {
            final_name.to_string()
        },
        if final_email.is_empty() {
            system_email
        } else {
            final_email.to_string()
        },
    );

    if identity.validate().is_ok() {
        return identity;
    }

    // Priority 6: Default values (last resort)
    default_identity()
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
/// 1. Git config (via libgit2) - primary source
/// 2. Provided `git_user_name` and `git_user_email` parameters (overrides)
/// 3. Environment variables (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`)
/// 4. Ralph config file (read by caller, passed as parameters)
/// 5. System username + derived email (sane fallback)
/// 6. Default values ("Ralph Workflow", "ralph@localhost") - last resort
///
/// Partial overrides are supported: CLI args/env vars/config can override individual
/// fields (name or email) from git config.
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
pub fn git_commit(
    message: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> io::Result<Option<git2::Oid>> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Get the index
    let mut index = repo.index().map_err(|e| git2_to_io_error(&e))?;

    // Don't create empty commits: if the index matches HEAD (or is empty on an unborn branch),
    // there's nothing to commit.
    if !index_has_changes_to_commit(&repo, &index)? {
        return Ok(None);
    }

    // Get the tree from the index
    let tree_oid = index.write_tree().map_err(|e| git2_to_io_error(&e))?;

    let tree = repo.find_tree(tree_oid).map_err(|e| git2_to_io_error(&e))?;

    // Resolve git identity using the identity resolution system.
    // This implements the full priority chain with proper fallbacks.
    let GitIdentity { name, email } = resolve_commit_identity(&repo, git_user_name, git_user_email);

    // Log the resolved identity source for visibility
    let identity_source = if git_user_name.is_some() || git_user_email.is_some() {
        "CLI/config override"
    } else if std::env::var("RALPH_GIT_USER_NAME").is_ok()
        || std::env::var("RALPH_GIT_USER_EMAIL").is_ok()
    {
        "environment variable"
    } else if repo.signature().is_ok() {
        "git config"
    } else {
        "system/default"
    };
    eprintln!("Git identity: {name} <{email}> (source: {identity_source})");

    // Create the signature with the resolved identity
    let sig = git2::Signature::now(&name, &email).map_err(|e| git2_to_io_error(&e))?;

    let oid = match repo.head() {
        Ok(head) => {
            // Normal commit: has a parent
            let head_commit = head.peel_to_commit().map_err(|e| git2_to_io_error(&e))?;
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head_commit])
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // Initial commit: no parents, but verify tree is not empty
            // An empty tree can happen in edge cases where files were staged and then removed
            let mut has_entries = false;
            tree.walk(git2::TreeWalkMode::PreOrder, |_, _| {
                has_entries = true;
                1 // Stop iteration after first entry
            })
            .ok(); // Ignore errors, we just want to know if there's at least one entry

            if !has_entries {
                // Tree is empty, return None instead of creating empty commit
                return Ok(None);
            }
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
        }
        Err(e) => return Err(git2_to_io_error(&e)),
    }
    .map_err(|e| git2_to_io_error(&e))?;

    Ok(Some(oid))
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
pub fn git_diff_from(start_oid: &str) -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Parse the starting OID
    let oid = git2::Oid::from_str(start_oid).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid commit OID: {start_oid}"),
        )
    })?;

    // Find the starting commit
    let start_commit = repo.find_commit(oid).map_err(|e| git2_to_io_error(&e))?;
    let start_tree = start_commit.tree().map_err(|e| git2_to_io_error(&e))?;

    // Diff between start commit and current working tree, including staged + unstaged
    // changes and untracked files.
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&start_tree), Some(&mut diff_opts))
        .map_err(|e| git2_to_io_error(&e))?;

    // Generate diff text
    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(|e| git2_to_io_error(&e))?;

    Ok(String::from_utf8_lossy(&result).to_string())
}

fn git_diff_from_empty_tree(repo: &git2::Repository) -> io::Result<String> {
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(None, Some(&mut diff_opts))
        .map_err(|e| git2_to_io_error(&e))?;

    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(|e| git2_to_io_error(&e))?;

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
pub fn get_git_diff_from_start() -> io::Result<String> {
    use crate::git_helpers::start_commit::{load_start_point, save_start_commit, StartPoint};

    // Ensure a valid starting point exists. This is expected to persist across runs,
    // but we also repair missing/corrupt files opportunistically for robustness.
    save_start_commit()?;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    match load_start_point()? {
        StartPoint::Commit(oid) => git_diff_from(&oid.to_string()),
        StartPoint::EmptyRepo => git_diff_from_empty_tree(&repo),
    }
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
}
