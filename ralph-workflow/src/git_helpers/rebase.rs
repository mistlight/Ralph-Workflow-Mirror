//! Git rebase operations using libgit2 with Git CLI fallback.
//!
//! This module provides functionality to:
//! - Perform rebase operations onto a specified upstream branch
//! - Detect and report conflicts during rebase
//! - Abort an in-progress rebase
//! - Continue a rebase after conflict resolution
//! - Get lists of conflicted files
//! - Handle all rebase failure modes with fault tolerance
//!
//! # Architecture
//!
//! This module uses a hybrid approach:
//! - **libgit2**: For repository state detection, validation, and queries
//! - **Git CLI**: For the actual rebase operation (more reliable)
//! - **Fallback patterns**: For operations that may fail with libgit2
//!
//! The Git CLI is used for rebase operations because:
//! 1. Better error messages for classification
//! 2. More robust edge case handling
//! 3. Better tested across Git versions
//! 4. Supports autostash and other features reliably

#![deny(unsafe_code)]

use std::io;
use std::path::Path;

/// Convert git2 error to `io::Error`.
fn git2_to_io_error(err: &git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}

/// Detailed classification of rebase failure modes.
///
/// This enum categorizes all known Git rebase failure modes as documented
/// in the requirements. Each variant represents a specific category of
/// failure that may occur during a rebase operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseErrorKind {
    // Category 1: Rebase Cannot Start
    /// Invalid or unresolvable revisions (branch doesn't exist, invalid ref, etc.)
    InvalidRevision { revision: String },

    /// Dirty working tree or index (unstaged or staged changes present)
    DirtyWorkingTree,

    /// Concurrent or in-progress Git operations (rebase, merge, cherry-pick, etc.)
    ConcurrentOperation { operation: String },

    /// Repository integrity or storage failures (missing/corrupt objects, disk full, etc.)
    RepositoryCorrupt { details: String },

    /// Environment or configuration failures (missing user.name, editor unavailable, etc.)
    EnvironmentFailure { reason: String },

    /// Hook-triggered aborts (pre-rebase hook rejected the operation)
    HookRejection { hook_name: String },

    // Category 2: Rebase Stops (Interrupted)
    /// Content conflicts (textual merge conflicts, add/add, modify/delete, etc.)
    ContentConflict { files: Vec<String> },

    /// Patch application failures (patch does not apply, context mismatch, etc.)
    PatchApplicationFailed { reason: String },

    /// Interactive todo-driven stops (edit, reword, break, exec commands)
    InteractiveStop { command: String },

    /// Empty or redundant commits (patch results in no changes)
    EmptyCommit,

    /// Autostash and stash reapplication failures
    AutostashFailed { reason: String },

    /// Commit creation failures mid-rebase (hook failures, signing failures, etc.)
    CommitCreationFailed { reason: String },

    /// Reference update failures (cannot lock branch ref, concurrent ref update, etc.)
    ReferenceUpdateFailed { reason: String },

    // Category 3: Post-Rebase Failures
    /// Post-rebase validation failures (tests failing, build failures, etc.)
    #[cfg(any(test, feature = "test-utils"))]
    ValidationFailed { reason: String },

    // Category 4: Interrupted/Corrupted State
    /// Process termination (agent crash, OS kill signal, CI timeout, etc.)
    #[cfg(any(test, feature = "test-utils"))]
    ProcessTerminated { reason: String },

    /// Incomplete or inconsistent rebase metadata
    #[cfg(any(test, feature = "test-utils"))]
    InconsistentState { details: String },

    // Category 5: Unknown
    /// Undefined or unknown failure modes
    Unknown { details: String },
}

impl RebaseErrorKind {
    /// Returns a human-readable description of the error.
    pub fn description(&self) -> String {
        match self {
            RebaseErrorKind::InvalidRevision { revision } => {
                format!("Invalid or unresolvable revision: '{revision}'")
            }
            RebaseErrorKind::DirtyWorkingTree => "Working tree has uncommitted changes".to_string(),
            RebaseErrorKind::ConcurrentOperation { operation } => {
                format!("Concurrent Git operation in progress: {operation}")
            }
            RebaseErrorKind::RepositoryCorrupt { details } => {
                format!("Repository integrity issue: {details}")
            }
            RebaseErrorKind::EnvironmentFailure { reason } => {
                format!("Environment or configuration failure: {reason}")
            }
            RebaseErrorKind::HookRejection { hook_name } => {
                format!("Hook '{hook_name}' rejected the operation")
            }
            RebaseErrorKind::ContentConflict { files } => {
                format!("Merge conflicts in {} file(s)", files.len())
            }
            RebaseErrorKind::PatchApplicationFailed { reason } => {
                format!("Patch application failed: {reason}")
            }
            RebaseErrorKind::InteractiveStop { command } => {
                format!("Interactive rebase stopped at command: {command}")
            }
            RebaseErrorKind::EmptyCommit => "Empty or redundant commit".to_string(),
            RebaseErrorKind::AutostashFailed { reason } => {
                format!("Autostash failed: {reason}")
            }
            RebaseErrorKind::CommitCreationFailed { reason } => {
                format!("Commit creation failed: {reason}")
            }
            RebaseErrorKind::ReferenceUpdateFailed { reason } => {
                format!("Reference update failed: {reason}")
            }
            #[cfg(any(test, feature = "test-utils"))]
            RebaseErrorKind::ValidationFailed { reason } => {
                format!("Post-rebase validation failed: {reason}")
            }
            #[cfg(any(test, feature = "test-utils"))]
            RebaseErrorKind::ProcessTerminated { reason } => {
                format!("Rebase process terminated: {reason}")
            }
            #[cfg(any(test, feature = "test-utils"))]
            RebaseErrorKind::InconsistentState { details } => {
                format!("Inconsistent rebase state: {details}")
            }
            RebaseErrorKind::Unknown { details } => {
                format!("Unknown rebase error: {details}")
            }
        }
    }

    /// Returns whether this error can potentially be recovered automatically.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_recoverable(&self) -> bool {
        match self {
            // These are generally recoverable with automatic retry or cleanup
            RebaseErrorKind::ConcurrentOperation { .. } => true,
            #[cfg(any(test, feature = "test-utils"))]
            RebaseErrorKind::ProcessTerminated { .. }
            | RebaseErrorKind::InconsistentState { .. } => true,

            // These require manual conflict resolution
            RebaseErrorKind::ContentConflict { .. } => true,

            // These are generally not recoverable without manual intervention
            RebaseErrorKind::InvalidRevision { .. }
            | RebaseErrorKind::DirtyWorkingTree
            | RebaseErrorKind::RepositoryCorrupt { .. }
            | RebaseErrorKind::EnvironmentFailure { .. }
            | RebaseErrorKind::HookRejection { .. }
            | RebaseErrorKind::PatchApplicationFailed { .. }
            | RebaseErrorKind::InteractiveStop { .. }
            | RebaseErrorKind::EmptyCommit
            | RebaseErrorKind::AutostashFailed { .. }
            | RebaseErrorKind::CommitCreationFailed { .. }
            | RebaseErrorKind::ReferenceUpdateFailed { .. } => false,
            #[cfg(any(test, feature = "test-utils"))]
            RebaseErrorKind::ValidationFailed { .. } => false,
            RebaseErrorKind::Unknown { .. } => false,
        }
    }

    /// Returns the category number (1-5) for this error.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn category(&self) -> u8 {
        match self {
            RebaseErrorKind::InvalidRevision { .. }
            | RebaseErrorKind::DirtyWorkingTree
            | RebaseErrorKind::ConcurrentOperation { .. }
            | RebaseErrorKind::RepositoryCorrupt { .. }
            | RebaseErrorKind::EnvironmentFailure { .. }
            | RebaseErrorKind::HookRejection { .. } => 1,

            RebaseErrorKind::ContentConflict { .. }
            | RebaseErrorKind::PatchApplicationFailed { .. }
            | RebaseErrorKind::InteractiveStop { .. }
            | RebaseErrorKind::EmptyCommit
            | RebaseErrorKind::AutostashFailed { .. }
            | RebaseErrorKind::CommitCreationFailed { .. }
            | RebaseErrorKind::ReferenceUpdateFailed { .. } => 2,

            #[cfg(any(test, feature = "test-utils"))]
            RebaseErrorKind::ValidationFailed { .. } => 3,

            #[cfg(any(test, feature = "test-utils"))]
            RebaseErrorKind::ProcessTerminated { .. }
            | RebaseErrorKind::InconsistentState { .. } => 4,

            RebaseErrorKind::Unknown { .. } => 5,
        }
    }
}

impl std::fmt::Display for RebaseErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl std::error::Error for RebaseErrorKind {}

/// Result of a rebase operation.
///
/// This enum represents the possible outcomes of a rebase operation,
/// including success, conflicts (recoverable), no-op (not applicable),
/// and specific failure modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseResult {
    /// Rebase completed successfully.
    Success,

    /// Rebase had conflicts that need resolution.
    Conflicts(Vec<String>),

    /// No rebase was needed (already up-to-date, not applicable, etc.).
    NoOp { reason: String },

    /// Rebase failed with a specific error kind.
    Failed(RebaseErrorKind),
}

impl RebaseResult {
    /// Returns whether the rebase was successful.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_success(&self) -> bool {
        matches!(self, RebaseResult::Success)
    }

    /// Returns whether the rebase had conflicts (needs resolution).
    #[cfg(any(test, feature = "test-utils"))]
    pub fn has_conflicts(&self) -> bool {
        matches!(self, RebaseResult::Conflicts(_))
    }

    /// Returns whether the rebase was a no-op (not applicable).
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_noop(&self) -> bool {
        matches!(self, RebaseResult::NoOp { .. })
    }

    /// Returns whether the rebase failed.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_failed(&self) -> bool {
        matches!(self, RebaseResult::Failed(_))
    }

    /// Returns the conflict files if this result contains conflicts.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn conflict_files(&self) -> Option<&[String]> {
        match self {
            RebaseResult::Conflicts(files) => Some(files),
            RebaseResult::Failed(RebaseErrorKind::ContentConflict { files }) => Some(files),
            _ => None,
        }
    }

    /// Returns the error kind if this result is a failure.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn error_kind(&self) -> Option<&RebaseErrorKind> {
        match self {
            RebaseResult::Failed(kind) => Some(kind),
            _ => None,
        }
    }

    /// Returns the no-op reason if this result is a no-op.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn noop_reason(&self) -> Option<&str> {
        match self {
            RebaseResult::NoOp { reason } => Some(reason),
            _ => None,
        }
    }
}

/// Parse Git CLI output to classify rebase errors.
///
/// This function analyzes stderr/stdout from git rebase commands
/// to determine the specific failure mode.
pub fn classify_rebase_error(stderr: &str, stdout: &str) -> RebaseErrorKind {
    let combined = format!("{stderr}\n{stdout}");

    // Category 1: Rebase Cannot Start

    // Invalid revision
    if combined.contains("invalid revision")
        || combined.contains("unknown revision")
        || combined.contains("bad revision")
        || combined.contains("ambiguous revision")
        || combined.contains("not found")
        || combined.contains("does not exist")
    {
        // Try to extract the revision name
        let revision = extract_revision(&combined);
        return RebaseErrorKind::InvalidRevision {
            revision: revision.unwrap_or_else(|| "unknown".to_string()),
        };
    }

    // Dirty working tree
    if combined.contains("dirty")
        || combined.contains("uncommitted changes")
        || combined.contains("local changes")
        || combined.contains("cannot rebase")
    {
        return RebaseErrorKind::DirtyWorkingTree;
    }

    // Concurrent operation
    if combined.contains("rebase in progress")
        || combined.contains("merge in progress")
        || combined.contains("cherry-pick in progress")
        || combined.contains("revert in progress")
        || combined.contains("bisect in progress")
        || combined.contains("Another git process")
        || combined.contains("Locked")
    {
        let operation = extract_operation(&combined);
        return RebaseErrorKind::ConcurrentOperation {
            operation: operation.unwrap_or_else(|| "unknown".to_string()),
        };
    }

    // Repository corruption
    if combined.contains("corrupt")
        || combined.contains("object not found")
        || combined.contains("missing object")
        || combined.contains("invalid object")
        || combined.contains("bad object")
        || combined.contains("disk full")
        || combined.contains("filesystem")
    {
        return RebaseErrorKind::RepositoryCorrupt {
            details: extract_error_line(&combined),
        };
    }

    // Environment failure
    if combined.contains("user.name")
        || combined.contains("user.email")
        || combined.contains("author")
        || combined.contains("committer")
        || combined.contains("terminal")
        || combined.contains("editor")
    {
        return RebaseErrorKind::EnvironmentFailure {
            reason: extract_error_line(&combined),
        };
    }

    // Hook rejection
    if combined.contains("pre-rebase")
        || combined.contains("hook")
        || combined.contains("rejected by")
    {
        return RebaseErrorKind::HookRejection {
            hook_name: extract_hook_name(&combined),
        };
    }

    // Category 2: Rebase Stops (Interrupted)

    // Content conflicts
    if combined.contains("Conflict")
        || combined.contains("conflict")
        || combined.contains("Resolve")
        || combined.contains("Merge conflict")
    {
        return RebaseErrorKind::ContentConflict {
            files: extract_conflict_files(&combined),
        };
    }

    // Patch application failure
    if combined.contains("patch does not apply")
        || combined.contains("patch failed")
        || combined.contains("hunk failed")
        || combined.contains("context mismatch")
        || combined.contains("fuzz")
    {
        return RebaseErrorKind::PatchApplicationFailed {
            reason: extract_error_line(&combined),
        };
    }

    // Interactive stop
    if combined.contains("Stopped at")
        || combined.contains("paused")
        || combined.contains("edit command")
    {
        return RebaseErrorKind::InteractiveStop {
            command: extract_command(&combined),
        };
    }

    // Empty commit
    if combined.contains("empty")
        || combined.contains("no changes")
        || combined.contains("already applied")
    {
        return RebaseErrorKind::EmptyCommit;
    }

    // Autostash failure
    if combined.contains("autostash") || combined.contains("stash") {
        return RebaseErrorKind::AutostashFailed {
            reason: extract_error_line(&combined),
        };
    }

    // Commit creation failure
    if combined.contains("pre-commit")
        || combined.contains("commit-msg")
        || combined.contains("prepare-commit-msg")
        || combined.contains("post-commit")
        || combined.contains("signing")
        || combined.contains("GPG")
    {
        return RebaseErrorKind::CommitCreationFailed {
            reason: extract_error_line(&combined),
        };
    }

    // Reference update failure
    if combined.contains("cannot lock")
        || combined.contains("ref update")
        || combined.contains("packed-refs")
        || combined.contains("reflog")
    {
        return RebaseErrorKind::ReferenceUpdateFailed {
            reason: extract_error_line(&combined),
        };
    }

    // Category 5: Unknown
    RebaseErrorKind::Unknown {
        details: extract_error_line(&combined),
    }
}

/// Extract revision name from error output.
fn extract_revision(output: &str) -> Option<String> {
    // Look for patterns like "invalid revision 'foo'" or "unknown revision 'bar'"
    // Using simple string parsing instead of regex for reliability
    let patterns = [
        ("invalid revision '", "'"),
        ("unknown revision '", "'"),
        ("bad revision '", "'"),
        ("branch '", "' not found"),
        ("upstream branch '", "' not found"),
        ("revision ", " not found"),
        ("'", "'"),
    ];

    for (start, end) in patterns {
        if let Some(start_idx) = output.find(start) {
            let after_start = &output[start_idx + start.len()..];
            if let Some(end_idx) = after_start.find(end) {
                let revision = &after_start[..end_idx];
                if !revision.is_empty() {
                    return Some(revision.to_string());
                }
            }
        }
    }

    // Also try to extract branch names from error messages
    for line in output.lines() {
        if line.contains("not found") || line.contains("does not exist") {
            // Extract potential branch/revision name
            let words: Vec<&str> = line.split_whitespace().collect();
            for (i, word) in words.iter().enumerate() {
                if *word == "'"
                    || *word == "\""
                        && i + 2 < words.len()
                        && (words[i + 2] == "'" || words[i + 2] == "\"")
                {
                    return Some(words[i + 1].to_string());
                }
            }
        }
    }

    None
}

/// Extract operation name from error output.
fn extract_operation(output: &str) -> Option<String> {
    if output.contains("rebase in progress") {
        Some("rebase".to_string())
    } else if output.contains("merge in progress") {
        Some("merge".to_string())
    } else if output.contains("cherry-pick in progress") {
        Some("cherry-pick".to_string())
    } else if output.contains("revert in progress") {
        Some("revert".to_string())
    } else if output.contains("bisect in progress") {
        Some("bisect".to_string())
    } else {
        None
    }
}

/// Extract hook name from error output.
fn extract_hook_name(output: &str) -> String {
    if output.contains("pre-rebase") {
        "pre-rebase".to_string()
    } else if output.contains("pre-commit") {
        "pre-commit".to_string()
    } else if output.contains("commit-msg") {
        "commit-msg".to_string()
    } else if output.contains("post-commit") {
        "post-commit".to_string()
    } else {
        "hook".to_string()
    }
}

/// Extract command name from error output.
fn extract_command(output: &str) -> String {
    if output.contains("edit") {
        "edit".to_string()
    } else if output.contains("reword") {
        "reword".to_string()
    } else if output.contains("break") {
        "break".to_string()
    } else if output.contains("exec") {
        "exec".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Extract the first meaningful error line from output.
fn extract_error_line(output: &str) -> String {
    output
        .lines()
        .find(|line| {
            !line.is_empty()
                && !line.starts_with("hint:")
                && !line.starts_with("Hint:")
                && !line.starts_with("note:")
                && !line.starts_with("Note:")
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| output.trim().to_string())
}

/// Extract conflict file paths from error output.
fn extract_conflict_files(output: &str) -> Vec<String> {
    let mut files = Vec::new();

    for line in output.lines() {
        if line.contains("CONFLICT") || line.contains("Conflict") || line.contains("Merge conflict")
        {
            // Extract file path from patterns like:
            // "CONFLICT (content): Merge conflict in src/file.rs"
            // "Merge conflict in src/file.rs"
            if let Some(start) = line.find("in ") {
                let path = line[start + 3..].trim();
                if !path.is_empty() {
                    files.push(path.to_string());
                }
            }
        }
    }

    files
}

/// Check if a rebase is currently in progress using Git CLI.
///
/// This is a fallback function that uses Git CLI to detect rebase state
/// when libgit2 may not accurately report it.
///
/// # Returns
///
/// Returns `Ok(true)` if a rebase is in progress, `Ok(false)` otherwise.
#[cfg(any(test, feature = "test-utils"))]
pub fn rebase_in_progress_cli() -> io::Result<bool> {
    use std::process::Command;

    let output = Command::new("git").args(["status", "--porcelain"]).output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            // Check for rebase state indicators
            Ok(stdout.contains("rebasing"))
        }
        Err(e) => Err(io::Error::other(format!(
            "Failed to check rebase status: {e}"
        ))),
    }
}

/// Clean up stale rebase state files.
///
/// This function attempts to clean up stale rebase state that may be
/// left over from interrupted operations. It's used as a recovery
/// mechanism for concurrent operation detection.
///
/// # Returns
///
/// Returns `Ok(())` if cleanup succeeded or no cleanup was needed,
/// or an error if cleanup failed.
#[cfg(any(test, feature = "test-utils"))]
pub fn cleanup_stale_rebase_state() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // List of possible stale rebase state files/directories
    let stale_paths = [
        "rebase-apply",
        "rebase-merge",
        "MERGE_HEAD",
        "MERGE_MSG",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "COMMIT_EDITMSG",
    ];

    let mut cleaned = false;

    for path in &stale_paths {
        let full_path = git_dir.join(path);
        if full_path.exists() {
            // Try to remove the stale state
            if full_path.is_dir() {
                if fs::remove_dir_all(&full_path).is_ok() {
                    cleaned = true;
                }
            } else if fs::remove_file(&full_path).is_ok() {
                cleaned = true;
            }
        }
    }

    // Also clean up index.lock if it exists and is stale
    let index_lock = git_dir.join("index.lock");
    if index_lock.exists() {
        // Try to remove stale lock
        let _ = fs::remove_file(&index_lock);
        cleaned = true;
    }

    if cleaned {
        // Log the cleanup
        eprintln!("Cleaned up stale rebase state files");
    }

    Ok(())
}

/// Detect dirty working tree using Git CLI.
///
/// This is a fallback function that uses Git CLI to detect dirty state
/// when libgit2 detection may not be sufficient.
///
/// # Returns
///
/// Returns `Ok(true)` if the working tree is dirty, `Ok(false)` otherwise.
#[cfg(any(test, feature = "test-utils"))]
pub fn is_dirty_tree_cli() -> io::Result<bool> {
    use std::process::Command;

    let output = Command::new("git").args(["status", "--porcelain"]).output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            Ok(!stdout.trim().is_empty())
        }
        Err(e) => Err(io::Error::other(format!(
            "Failed to check working tree status: {e}"
        ))),
    }
}

/// Perform a rebase onto the specified upstream branch.
///
/// This function rebases the current branch onto the specified upstream branch.
/// It handles the full rebase process including conflict detection and
/// classifies all known failure modes.
///
/// # Arguments
///
/// * `upstream_branch` - The branch to rebase onto (e.g., "main", "origin/main")
///
/// # Returns
///
/// Returns `Ok(RebaseResult)` indicating the outcome, or an error if:
/// - The repository cannot be opened
/// - The rebase operation fails in an unexpected way
///
/// # Edge Cases Handled
///
/// - Empty repository (no commits) - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Unborn branch - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Already up-to-date - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Unrelated branches (no shared ancestor) - Returns `Ok(RebaseResult::NoOp)` with reason
/// - On main/master branch - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Conflicts during rebase - Returns `Ok(RebaseResult::Conflicts)` or `Failed` with error kind
/// - Other failures - Returns `Ok(RebaseResult::Failed)` with appropriate error kind
///
/// # Note
///
/// This function uses git CLI for rebase operations as libgit2's rebase API
/// has limitations and complexity that make it unreliable for production use.
/// The git CLI is more robust and better tested for rebase operations.
pub fn rebase_onto(upstream_branch: &str) -> io::Result<RebaseResult> {
    use std::process::Command;

    // Check if we have any commits
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    match repo.head() {
        Ok(_) => {}
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // No commits yet - nothing to rebase
            return Ok(RebaseResult::NoOp {
                reason: "Repository has no commits yet (unborn branch)".to_string(),
            });
        }
        Err(e) => return Err(git2_to_io_error(&e)),
    }

    // Get the upstream branch to ensure it exists
    let upstream_object = match repo.revparse_single(upstream_branch) {
        Ok(obj) => obj,
        Err(_) => {
            return Ok(RebaseResult::Failed(RebaseErrorKind::InvalidRevision {
                revision: upstream_branch.to_string(),
            }))
        }
    };

    let upstream_commit = upstream_object
        .peel_to_commit()
        .map_err(|e| git2_to_io_error(&e))?;

    // Get our branch commit
    let head = repo.head().map_err(|e| git2_to_io_error(&e))?;
    let head_commit = head.peel_to_commit().map_err(|e| git2_to_io_error(&e))?;

    // Check if we're already up-to-date
    if repo
        .graph_descendant_of(head_commit.id(), upstream_commit.id())
        .map_err(|e| git2_to_io_error(&e))?
    {
        // Already up-to-date
        return Ok(RebaseResult::NoOp {
            reason: "Branch is already up-to-date with upstream".to_string(),
        });
    }

    // Check if branches share a common ancestor
    // If merge_base fails with NotFound, the branches are unrelated
    match repo.merge_base(head_commit.id(), upstream_commit.id()) {
        Err(e)
            if e.class() == git2::ErrorClass::Reference
                && e.code() == git2::ErrorCode::NotFound =>
        {
            // Branches are unrelated - no shared history
            return Ok(RebaseResult::NoOp {
                reason: format!(
                    "No common ancestor between current branch and '{upstream_branch}' (unrelated branches)"
                ),
            });
        }
        Err(e) => return Err(git2_to_io_error(&e)),
        Ok(_) => {}
    }

    // Check if we're on main/master
    let branch_name = head.shorthand().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine branch name from HEAD",
        )
    })?;

    if branch_name == "main" || branch_name == "master" {
        return Ok(RebaseResult::NoOp {
            reason: format!("Already on '{branch_name}' branch, rebase not applicable"),
        });
    }

    // Use git CLI for rebase - more reliable than libgit2
    let output = Command::new("git")
        .args(["rebase", upstream_branch])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                Ok(RebaseResult::Success)
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stdout = String::from_utf8_lossy(&result.stdout);

                // Use classify_rebase_error to determine the specific failure mode
                let error_kind = classify_rebase_error(&stderr, &stdout);

                match error_kind {
                    RebaseErrorKind::ContentConflict { .. } => {
                        // For conflicts, get the actual conflicted files
                        match get_conflicted_files() {
                            Ok(files) if files.is_empty() => {
                                // If we detected a conflict but can't get the files,
                                // return the error kind with the files from the error
                                if let RebaseErrorKind::ContentConflict { files } = error_kind {
                                    Ok(RebaseResult::Conflicts(files))
                                } else {
                                    Ok(RebaseResult::Conflicts(vec![]))
                                }
                            }
                            Ok(files) => Ok(RebaseResult::Conflicts(files)),
                            Err(_) => Ok(RebaseResult::Conflicts(vec![])),
                        }
                    }
                    RebaseErrorKind::Unknown { .. } => {
                        // Check for "up to date" message which is actually a no-op
                        if stderr.contains("up to date") {
                            Ok(RebaseResult::NoOp {
                                reason: "Branch is already up-to-date with upstream".to_string(),
                            })
                        } else {
                            Ok(RebaseResult::Failed(error_kind))
                        }
                    }
                    _ => Ok(RebaseResult::Failed(error_kind)),
                }
            }
        }
        Err(e) => Err(io::Error::other(format!(
            "Failed to execute git rebase: {e}"
        ))),
    }
}

/// Abort the current rebase operation.
///
/// This cleans up the rebase state and returns the repository to its
/// pre-rebase condition.
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if:
/// - No rebase is in progress
/// - The abort operation fails
pub fn abort_rebase() -> io::Result<()> {
    use std::process::Command;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Check if a rebase is in progress
    let state = repo.state();
    if state != git2::RepositoryState::Rebase
        && state != git2::RepositoryState::RebaseMerge
        && state != git2::RepositoryState::RebaseInteractive
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No rebase in progress",
        ));
    }

    // Use git CLI for abort
    let output = Command::new("git").args(["rebase", "--abort"]).output();

    match output {
        Ok(result) => {
            if result.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                Err(io::Error::other(format!(
                    "Failed to abort rebase: {stderr}"
                )))
            }
        }
        Err(e) => Err(io::Error::other(format!(
            "Failed to execute git rebase --abort: {e}"
        ))),
    }
}

/// Get a list of files that have merge conflicts.
///
/// This function queries libgit2's index to find all files that are
/// currently in a conflicted state.
///
/// # Returns
///
/// Returns `Ok(Vec<String>)` containing the paths of conflicted files,
/// or an error if the repository cannot be accessed.
pub fn get_conflicted_files() -> io::Result<Vec<String>> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let index = repo.index().map_err(|e| git2_to_io_error(&e))?;

    let mut conflicted_files = Vec::new();

    // Check if there are any conflicts
    if !index.has_conflicts() {
        return Ok(conflicted_files);
    }

    // Get the list of conflicted files
    let conflicts = index.conflicts().map_err(|e| git2_to_io_error(&e))?;

    for conflict in conflicts {
        let conflict = conflict.map_err(|e| git2_to_io_error(&e))?;
        // The conflict's `our` entry (stage 2) will have the path
        if let Some(our_entry) = conflict.our {
            if let Ok(path) = std::str::from_utf8(&our_entry.path) {
                let path_str = path.to_string();
                if !conflicted_files.contains(&path_str) {
                    conflicted_files.push(path_str);
                }
            }
        }
    }

    Ok(conflicted_files)
}

/// Extract conflict markers from a file.
///
/// This function reads a file and returns the conflict sections,
/// including both versions of the conflicted content.
///
/// # Arguments
///
/// * `path` - Path to the conflicted file (relative to repo root)
///
/// # Returns
///
/// Returns `Ok(String)` containing the conflict sections, or an error
/// if the file cannot be read.
pub fn get_conflict_markers_for_file(path: &Path) -> io::Result<String> {
    use std::fs;
    use std::io::Read;

    let mut file = fs::File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    // Extract conflict markers and their content
    let mut conflict_sections = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim_start().starts_with("<<<<<<<") {
            // Found conflict start
            let mut section = Vec::new();
            section.push(lines[i]);

            i += 1;
            // Collect "ours" version
            while i < lines.len() && !lines[i].trim_start().starts_with("=======") {
                section.push(lines[i]);
                i += 1;
            }

            if i < lines.len() {
                section.push(lines[i]); // Add the ======= line
                i += 1;
            }

            // Collect "theirs" version
            while i < lines.len() && !lines[i].trim_start().starts_with(">>>>>>>") {
                section.push(lines[i]);
                i += 1;
            }

            if i < lines.len() {
                section.push(lines[i]); // Add the >>>>>>> line
                i += 1;
            }

            conflict_sections.push(section.join("\n"));
        } else {
            i += 1;
        }
    }

    if conflict_sections.is_empty() {
        // No conflict markers found, return empty string
        Ok(String::new())
    } else {
        Ok(conflict_sections.join("\n\n"))
    }
}

/// Continue a rebase after conflict resolution.
///
/// This function continues a rebase that was paused due to conflicts.
/// It should be called after all conflicts have been resolved and
/// the resolved files have been staged with `git add`.
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if:
/// - No rebase is in progress
/// - Conflicts remain unresolved
/// - The continue operation fails
pub fn continue_rebase() -> io::Result<()> {
    use std::process::Command;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Check if a rebase is in progress
    let state = repo.state();
    if state != git2::RepositoryState::Rebase
        && state != git2::RepositoryState::RebaseMerge
        && state != git2::RepositoryState::RebaseInteractive
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No rebase in progress",
        ));
    }

    // Check if there are still conflicts
    let conflicted = get_conflicted_files()?;
    if !conflicted.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Cannot continue rebase: {} file(s) still have conflicts",
                conflicted.len()
            ),
        ));
    }

    // Use git CLI for continue
    let output = Command::new("git").args(["rebase", "--continue"]).output();

    match output {
        Ok(result) => {
            if result.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                Err(io::Error::other(format!(
                    "Failed to continue rebase: {stderr}"
                )))
            }
        }
        Err(e) => Err(io::Error::other(format!(
            "Failed to execute git rebase --continue: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebase_result_variants_exist() {
        // Test that RebaseResult has the expected variants
        let _ = RebaseResult::Success;
        let _ = RebaseResult::NoOp {
            reason: "test".to_string(),
        };
        let _ = RebaseResult::Conflicts(vec![]);
        let _ = RebaseResult::Failed(RebaseErrorKind::Unknown {
            details: "test".to_string(),
        });
    }

    #[test]
    fn test_rebase_result_is_noop() {
        // Test the is_noop method
        assert!(RebaseResult::NoOp {
            reason: "test".to_string()
        }
        .is_noop());
        assert!(!RebaseResult::Success.is_noop());
        assert!(!RebaseResult::Conflicts(vec![]).is_noop());
        assert!(!RebaseResult::Failed(RebaseErrorKind::Unknown {
            details: "test".to_string(),
        })
        .is_noop());
    }

    #[test]
    fn test_rebase_result_is_success() {
        // Test the is_success method
        assert!(RebaseResult::Success.is_success());
        assert!(!RebaseResult::NoOp {
            reason: "test".to_string()
        }
        .is_success());
        assert!(!RebaseResult::Conflicts(vec![]).is_success());
        assert!(!RebaseResult::Failed(RebaseErrorKind::Unknown {
            details: "test".to_string(),
        })
        .is_success());
    }

    #[test]
    fn test_rebase_result_has_conflicts() {
        // Test the has_conflicts method
        assert!(RebaseResult::Conflicts(vec!["file.txt".to_string()]).has_conflicts());
        assert!(!RebaseResult::Success.has_conflicts());
        assert!(!RebaseResult::NoOp {
            reason: "test".to_string()
        }
        .has_conflicts());
    }

    #[test]
    fn test_rebase_result_is_failed() {
        // Test the is_failed method
        assert!(RebaseResult::Failed(RebaseErrorKind::Unknown {
            details: "test".to_string(),
        })
        .is_failed());
        assert!(!RebaseResult::Success.is_failed());
        assert!(!RebaseResult::NoOp {
            reason: "test".to_string()
        }
        .is_failed());
        assert!(!RebaseResult::Conflicts(vec![]).is_failed());
    }

    #[test]
    fn test_rebase_error_kind_description() {
        // Test that error kinds produce descriptions
        let err = RebaseErrorKind::InvalidRevision {
            revision: "main".to_string(),
        };
        assert!(err.description().contains("main"));

        let err = RebaseErrorKind::DirtyWorkingTree;
        assert!(err.description().contains("Working tree"));
    }

    #[test]
    fn test_rebase_error_kind_category() {
        // Test that error kinds return correct categories
        assert_eq!(
            RebaseErrorKind::InvalidRevision {
                revision: "test".to_string()
            }
            .category(),
            1
        );
        assert_eq!(
            RebaseErrorKind::ContentConflict { files: vec![] }.category(),
            2
        );
        assert_eq!(
            RebaseErrorKind::ValidationFailed {
                reason: "test".to_string()
            }
            .category(),
            3
        );
        assert_eq!(
            RebaseErrorKind::ProcessTerminated {
                reason: "test".to_string()
            }
            .category(),
            4
        );
        assert_eq!(
            RebaseErrorKind::Unknown {
                details: "test".to_string()
            }
            .category(),
            5
        );
    }

    #[test]
    fn test_rebase_error_kind_is_recoverable() {
        // Test that error kinds correctly identify recoverable errors
        assert!(RebaseErrorKind::ConcurrentOperation {
            operation: "rebase".to_string()
        }
        .is_recoverable());
        assert!(RebaseErrorKind::ContentConflict { files: vec![] }.is_recoverable());
        assert!(!RebaseErrorKind::InvalidRevision {
            revision: "test".to_string()
        }
        .is_recoverable());
        assert!(!RebaseErrorKind::DirtyWorkingTree.is_recoverable());
    }

    #[test]
    fn test_classify_rebase_error_invalid_revision() {
        // Test classification of invalid revision errors
        let stderr = "error: invalid revision 'nonexistent'";
        let error = classify_rebase_error(stderr, "");
        assert!(matches!(error, RebaseErrorKind::InvalidRevision { .. }));
    }

    #[test]
    fn test_classify_rebase_error_conflict() {
        // Test classification of conflict errors
        let stderr = "CONFLICT (content): Merge conflict in src/main.rs";
        let error = classify_rebase_error(stderr, "");
        assert!(matches!(error, RebaseErrorKind::ContentConflict { .. }));
    }

    #[test]
    fn test_classify_rebase_error_dirty_tree() {
        // Test classification of dirty working tree errors
        let stderr = "Cannot rebase: Your index contains uncommitted changes";
        let error = classify_rebase_error(stderr, "");
        assert!(matches!(error, RebaseErrorKind::DirtyWorkingTree));
    }

    #[test]
    fn test_classify_rebase_error_concurrent_operation() {
        // Test classification of concurrent operation errors
        let stderr = "Cannot rebase: There is a rebase in progress already";
        let error = classify_rebase_error(stderr, "");
        assert!(matches!(error, RebaseErrorKind::ConcurrentOperation { .. }));
    }

    #[test]
    fn test_classify_rebase_error_unknown() {
        // Test classification of unknown errors
        let stderr = "Some completely unexpected error message";
        let error = classify_rebase_error(stderr, "");
        assert!(matches!(error, RebaseErrorKind::Unknown { .. }));
    }

    #[test]
    fn test_rebase_onto_returns_result() {
        use std::path::Path;
        use test_helpers::{commit_all, init_git_repo, with_temp_cwd, write_file};

        // Test that rebase_onto returns a Result
        with_temp_cwd(|dir| {
            // Initialize a git repo with an initial commit
            let repo = init_git_repo(dir);
            write_file(dir.path().join("initial.txt"), "initial content");
            let _ = commit_all(&repo, "initial commit");

            // We use a non-existent branch to test error handling
            let result = rebase_onto("nonexistent_branch_that_does_not_exist");
            // Should return Ok (either with Failed result or other outcome)
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_get_conflicted_files_returns_result() {
        use test_helpers::{init_git_repo, with_temp_cwd};

        // Test that get_conflicted_files returns a Result
        with_temp_cwd(|dir| {
            // Initialize a git repo first
            let _repo = init_git_repo(dir);

            let result = get_conflicted_files();
            // Should succeed (returns Vec, not error)
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_rebase_in_progress_cli_returns_result() {
        use test_helpers::{init_git_repo, with_temp_cwd};

        // Test that rebase_in_progress_cli returns a Result
        with_temp_cwd(|dir| {
            // Initialize a git repo first
            let _repo = init_git_repo(dir);

            let result = rebase_in_progress_cli();
            // Should succeed (returns bool)
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_is_dirty_tree_cli_returns_result() {
        use test_helpers::{init_git_repo, with_temp_cwd};

        // Test that is_dirty_tree_cli returns a Result
        with_temp_cwd(|dir| {
            // Initialize a git repo first
            let _repo = init_git_repo(dir);

            let result = is_dirty_tree_cli();
            // Should succeed (returns bool)
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_cleanup_stale_rebase_state_returns_result() {
        use test_helpers::{init_git_repo, with_temp_cwd};

        with_temp_cwd(|dir| {
            // Initialize a git repo first
            let _repo = init_git_repo(dir);

            // Test that cleanup_stale_rebase_state returns a Result
            let result = cleanup_stale_rebase_state();
            // Should succeed even if there's nothing to clean
            assert!(result.is_ok());
        });
    }
}
