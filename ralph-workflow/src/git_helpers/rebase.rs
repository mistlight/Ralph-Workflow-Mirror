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
        || combined.contains("bad revision")
        || combined.contains("no such ref")
    {
        // Try to extract the revision name
        let revision = extract_revision(&combined);
        return RebaseErrorKind::InvalidRevision {
            revision: revision.unwrap_or_else(|| "unknown".to_string()),
        };
    }

    // Shallow clone (missing history)
    if combined.contains("shallow")
        || combined.contains("depth")
        || combined.contains("unreachable")
        || combined.contains("needed single revision")
        || combined.contains("does not have")
    {
        return RebaseErrorKind::RepositoryCorrupt {
            details: format!(
                "Shallow clone or missing history: {}",
                extract_error_line(&combined)
            ),
        };
    }

    // Worktree conflict
    if combined.contains("worktree")
        || combined.contains("checked out")
        || combined.contains("another branch")
        || combined.contains("already checked out")
    {
        return RebaseErrorKind::ConcurrentOperation {
            operation: "branch checked out in another worktree".to_string(),
        };
    }

    // Submodule conflict
    if combined.contains("submodule") || combined.contains(".gitmodules") {
        return RebaseErrorKind::ContentConflict {
            files: extract_conflict_files(&combined),
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

/// Type of concurrent Git operation detected.
///
/// Represents the various Git operations that may be in progress
/// and would block a rebase from starting.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(any(test, feature = "test-utils"))]
pub enum ConcurrentOperation {
    /// A rebase is already in progress.
    Rebase,
    /// A merge is in progress.
    Merge,
    /// A cherry-pick is in progress.
    CherryPick,
    /// A revert is in progress.
    Revert,
    /// A bisect is in progress.
    Bisect,
    /// Another Git process is holding locks.
    OtherGitProcess,
    /// Unknown concurrent operation.
    Unknown(String),
}

#[cfg(any(test, feature = "test-utils"))]
impl ConcurrentOperation {
    /// Returns a human-readable description of the operation.
    pub fn description(&self) -> String {
        match self {
            ConcurrentOperation::Rebase => "rebase".to_string(),
            ConcurrentOperation::Merge => "merge".to_string(),
            ConcurrentOperation::CherryPick => "cherry-pick".to_string(),
            ConcurrentOperation::Revert => "revert".to_string(),
            ConcurrentOperation::Bisect => "bisect".to_string(),
            ConcurrentOperation::OtherGitProcess => "another Git process".to_string(),
            ConcurrentOperation::Unknown(s) => format!("unknown operation: {s}"),
        }
    }
}

/// Detect concurrent Git operations that would block a rebase.
///
/// This function performs a comprehensive check for any in-progress Git
/// operations that would prevent a rebase from starting. It checks for:
/// - Rebase in progress (`.git/rebase-apply` or `.git/rebase-merge`)
/// - Merge in progress (`.git/MERGE_HEAD`)
/// - Cherry-pick in progress (`.git/CHERRY_PICK_HEAD`)
/// - Revert in progress (`.git/REVERT_HEAD`)
/// - Bisect in progress (`.git/BISECT_*`)
/// - Lock files held by other processes
///
/// # Returns
///
/// Returns `Ok(None)` if no concurrent operations are detected,
/// or `Ok(Some(operation))` with the type of operation detected.
/// Returns an error if unable to check the repository state.
///
/// # Example
///
/// ```no_run
/// use ralph_workflow::git_helpers::rebase::detect_concurrent_git_operations;
///
/// match detect_concurrent_git_operations() {
///     Ok(None) => println!("No concurrent operations detected"),
///     Ok(Some(op)) => println!("Concurrent operation detected: {}", op.description()),
///     Err(e) => eprintln!("Error checking: {e}"),
/// }
/// ```
#[cfg(any(test, feature = "test-utils"))]
pub fn detect_concurrent_git_operations() -> io::Result<Option<ConcurrentOperation>> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // Check for rebase in progress (multiple possible state directories)
    let rebase_merge = git_dir.join("rebase-merge");
    let rebase_apply = git_dir.join("rebase-apply");
    if rebase_merge.exists() || rebase_apply.exists() {
        return Ok(Some(ConcurrentOperation::Rebase));
    }

    // Check for merge in progress
    let merge_head = git_dir.join("MERGE_HEAD");
    if merge_head.exists() {
        return Ok(Some(ConcurrentOperation::Merge));
    }

    // Check for cherry-pick in progress
    let cherry_pick_head = git_dir.join("CHERRY_PICK_HEAD");
    if cherry_pick_head.exists() {
        return Ok(Some(ConcurrentOperation::CherryPick));
    }

    // Check for revert in progress
    let revert_head = git_dir.join("REVERT_HEAD");
    if revert_head.exists() {
        return Ok(Some(ConcurrentOperation::Revert));
    }

    // Check for bisect in progress (multiple possible state files)
    let bisect_log = git_dir.join("BISECT_LOG");
    let bisect_start = git_dir.join("BISECT_START");
    let bisect_names = git_dir.join("BISECT_NAMES");
    if bisect_log.exists() || bisect_start.exists() || bisect_names.exists() {
        return Ok(Some(ConcurrentOperation::Bisect));
    }

    // Check for lock files that might indicate concurrent operations
    let index_lock = git_dir.join("index.lock");
    let packed_refs_lock = git_dir.join("packed-refs.lock");
    let head_lock = git_dir.join("HEAD.lock");
    if index_lock.exists() || packed_refs_lock.exists() || head_lock.exists() {
        // Lock files might be stale, so we'll report as "other Git process"
        // The caller can decide whether to wait or clean up
        return Ok(Some(ConcurrentOperation::OtherGitProcess));
    }

    // Check for any other state files we might have missed
    if let Ok(entries) = fs::read_dir(git_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Look for other state patterns
            if name_str.contains("REBASE")
                || name_str.contains("MERGE")
                || name_str.contains("CHERRY")
            {
                return Ok(Some(ConcurrentOperation::Unknown(name_str.to_string())));
            }
        }
    }

    Ok(None)
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
pub fn rebase_in_progress_cli(executor: &dyn crate::executor::ProcessExecutor) -> io::Result<bool> {
    let output = executor.execute("git", &["status", "--porcelain"], &[], None)?;
    Ok(output.stdout.contains("rebasing"))
}

/// Result of cleaning up stale rebase state.
///
/// Provides information about what was cleaned up during the operation.
#[derive(Debug, Clone, Default)]
#[cfg(any(test, feature = "test-utils"))]
pub struct CleanupResult {
    /// List of state files that were cleaned up
    pub cleaned_paths: Vec<String>,
    /// Whether any lock files were removed
    pub locks_removed: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl CleanupResult {
    /// Returns true if any cleanup was performed.
    pub fn has_cleanup(&self) -> bool {
        !self.cleaned_paths.is_empty() || self.locks_removed
    }

    /// Returns the number of items cleaned up.
    pub fn count(&self) -> usize {
        self.cleaned_paths.len() + if self.locks_removed { 1 } else { 0 }
    }
}

/// Clean up stale rebase state files.
///
/// This function attempts to clean up stale rebase state that may be
/// left over from interrupted operations. It validates state before
/// removal and reports what was cleaned up.
///
/// This is a recovery mechanism for concurrent operation detection and
/// for cleaning up after interrupted rebase operations.
///
/// # Returns
///
/// Returns `Ok(CleanupResult)` with details of what was cleaned up,
/// or an error if cleanup failed catastrophically.
#[cfg(any(test, feature = "test-utils"))]
pub fn cleanup_stale_rebase_state() -> io::Result<CleanupResult> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    let mut result = CleanupResult::default();

    // List of possible stale rebase state files/directories
    let stale_paths = [
        ("rebase-apply", "rebase-apply directory"),
        ("rebase-merge", "rebase-merge directory"),
        ("MERGE_HEAD", "merge state"),
        ("MERGE_MSG", "merge message"),
        ("CHERRY_PICK_HEAD", "cherry-pick state"),
        ("REVERT_HEAD", "revert state"),
        ("COMMIT_EDITMSG", "commit message"),
    ];

    for (path_name, description) in &stale_paths {
        let full_path = git_dir.join(path_name);
        if full_path.exists() {
            // Try to validate the state before removing
            let is_valid = validate_state_file(&full_path);
            if !is_valid.unwrap_or(true) {
                // State is invalid or corrupted, safe to remove
                let removed = if full_path.is_dir() {
                    fs::remove_dir_all(&full_path)
                        .map(|_| true)
                        .unwrap_or(false)
                } else {
                    fs::remove_file(&full_path).map(|_| true).unwrap_or(false)
                };

                if removed {
                    result
                        .cleaned_paths
                        .push(format!("{path_name} ({description})"));
                }
            }
        }
    }

    // Clean up lock files if they exist
    let lock_files = ["index.lock", "packed-refs.lock", "HEAD.lock"];
    for lock_file in &lock_files {
        let lock_path = git_dir.join(lock_file);
        if lock_path.exists() {
            // Lock files are generally safe to remove if stale
            if fs::remove_file(&lock_path).is_ok() {
                result.locks_removed = true;
                result
                    .cleaned_paths
                    .push(format!("{lock_file} (lock file)"));
            }
        }
    }

    Ok(result)
}

// Validate a Git state file for corruption.
///
/// Checks if a state file is valid before attempting to remove it.
/// This prevents accidental removal of valid in-progress operations.
///
/// # Arguments
///
/// * `path` - Path to the state file to validate
///
/// # Returns
///
/// Returns `Ok(true)` if the state appears valid, `Ok(false)` if invalid,
/// or an error if validation failed.
#[cfg(any(test, feature = "test-utils"))]
fn validate_state_file(path: &Path) -> io::Result<bool> {
    use std::fs;

    if !path.exists() {
        return Ok(false);
    }

    // For directories, check if they contain required files
    if path.is_dir() {
        // A valid rebase directory should have at least some files
        let entries = fs::read_dir(path)?;
        let has_content = entries.count() > 0;
        return Ok(has_content);
    }

    // For files, check if they're readable and non-empty
    if path.is_file() {
        let metadata = fs::metadata(path)?;
        if metadata.len() == 0 {
            // Empty state file is invalid
            return Ok(false);
        }
        // Try to read a small amount to verify file integrity
        let _ = fs::read(path)?;
        return Ok(true);
    }

    Ok(false)
}

/// Attempt automatic recovery from a rebase failure.
///
/// This function implements an escalation strategy for recovering from
/// rebase failures, trying multiple approaches before giving up:
///
/// **Level 1 - Clean state retry**: Reset to clean state and retry
/// **Level 2 - Lock file removal**: Remove stale lock files
/// **Level 3 - Abort and restart**: Abort current rebase and restart from checkpoint
///
/// # Arguments
///
/// * `error_kind` - The error that occurred
/// * `phase` - The current rebase phase
/// * `phase_error_count` - Number of errors in the current phase
///
/// # Returns
///
/// Returns `Ok(true)` if automatic recovery succeeded and operation can continue,
/// `Ok(false)` if recovery was attempted but operation should still abort,
/// or an error if recovery itself failed.
///
/// # Example
///
/// ```no_run
/// use ralph_workflow::git_helpers::rebase::{attempt_automatic_recovery, RebaseErrorKind};
/// use ralph_workflow::git_helpers::rebase_checkpoint::RebasePhase;
///
/// match attempt_automatic_recovery(&executor, &RebaseErrorKind::Unknown { details: "test".to_string() }, &RebasePhase::ConflictDetected, 2) {
///     Ok(true) => println!("Recovery succeeded, can continue"),
///     Ok(false) => println!("Recovery attempted, should abort"),
///     Err(e) => println!("Recovery failed: {e}"),
/// }
/// ```
#[cfg(any(test, feature = "test-utils"))]
pub fn attempt_automatic_recovery(
    executor: &dyn crate::executor::ProcessExecutor,
    error_kind: &RebaseErrorKind,
    phase: &crate::git_helpers::rebase_checkpoint::RebasePhase,
    phase_error_count: u32,
) -> io::Result<bool> {
    // Don't attempt recovery for fatal errors
    match error_kind {
        RebaseErrorKind::InvalidRevision { .. }
        | RebaseErrorKind::DirtyWorkingTree
        | RebaseErrorKind::RepositoryCorrupt { .. }
        | RebaseErrorKind::EnvironmentFailure { .. }
        | RebaseErrorKind::HookRejection { .. }
        | RebaseErrorKind::InteractiveStop { .. }
        | RebaseErrorKind::Unknown { .. } => {
            return Ok(false);
        }
        _ => {}
    }

    let max_attempts = phase.max_recovery_attempts();
    if phase_error_count >= max_attempts {
        return Ok(false);
    }

    // Level 1: Try cleaning stale state
    if cleanup_stale_rebase_state().is_ok() {
        // If we cleaned something, try to validate the repo is in a good state
        if validate_git_state().is_ok() {
            return Ok(true);
        }
    }

    // Level 2: Try removing lock files
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();
    let lock_files = ["index.lock", "packed-refs.lock", "HEAD.lock"];
    let mut removed_any = false;

    for lock_file in &lock_files {
        let lock_path = git_dir.join(lock_file);
        // Note: std::fs is acceptable here - operating on .git/ internals, not workspace files
        if lock_path.exists() && std::fs::remove_file(&lock_path).is_ok() {
            removed_any = true;
        }
    }

    if removed_any && validate_git_state().is_ok() {
        return Ok(true);
    }

    // Level 3: For concurrent operations, try to abort the in-progress operation
    if let RebaseErrorKind::ConcurrentOperation { .. } = error_kind {
        // Try git rebase --abort via executor
        let abort_result = executor.execute("git", &["rebase", "--abort"], &[], None);

        if abort_result.is_ok() {
            // Check if state is now clean
            if validate_git_state().is_ok() {
                return Ok(true);
            }
        }
    }

    // Recovery attempts exhausted or failed
    Ok(false)
}

/// Validate the overall Git repository state for corruption.
///
/// Performs comprehensive checks on the repository to detect
/// corrupted state files, missing objects, or other integrity issues.
///
/// # Returns
///
/// Returns `Ok(())` if the repository state appears valid,
/// or an error describing the validation failure.
#[cfg(any(test, feature = "test-utils"))]
pub fn validate_git_state() -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Check if the repository head is valid
    let _ = repo.head().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Repository HEAD is invalid: {e}"),
        )
    })?;

    // Try to access the index to verify it's not corrupted
    let _ = repo.index().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Repository index is corrupted: {e}"),
        )
    })?;

    // Check for object database integrity by trying to access HEAD
    if let Ok(head) = repo.head() {
        if let Ok(commit) = head.peel_to_commit() {
            // Verify the commit tree is accessible
            let _ = commit.tree().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Object database corruption: {e}"),
                )
            })?;
        }
    }

    Ok(())
}

/// Detect dirty working tree using Git CLI.
///
/// This is a fallback function that uses Git CLI to detect dirty state
/// when libgit2 detection may not be sufficient.
///
/// # Arguments
///
/// * `executor` - Process executor for dependency injection
///
/// # Returns
///
/// Returns `Ok(true)` if the working tree is dirty, `Ok(false)` otherwise.
#[cfg(any(test, feature = "test-utils"))]
pub fn is_dirty_tree_cli(executor: &dyn crate::executor::ProcessExecutor) -> io::Result<bool> {
    let output = executor.execute("git", &["status", "--porcelain"], &[], None)?;

    if output.status.success() {
        let stdout = output.stdout.trim();
        Ok(!stdout.is_empty())
    } else {
        Err(io::Error::other(format!(
            "Failed to check working tree status: {}",
            output.stderr
        )))
    }
}

/// Validate preconditions before starting a rebase operation.
///
/// This function performs Category 1 (pre-start) validation checks to ensure
/// the repository is in a valid state for rebasing. It checks for common
/// issues that would cause a rebase to fail immediately.
///
/// # Arguments
///
/// * `executor` - Process executor for dependency injection
///
/// # Returns
///
/// Returns `Ok(())` if all preconditions are met, or an error with a
/// descriptive message if validation fails.
///
/// # Validation Checks
///
/// - Repository integrity (valid HEAD, index, object database)
/// - No concurrent Git operations (merge, rebase, cherry-pick, etc.)
/// - Git identity is configured (user.name and user.email)
/// - Working tree is not dirty (no unstaged or staged changes)
/// - Not a shallow clone (shallow clones have limited history)
/// - No worktree conflicts (branch not checked out elsewhere)
/// - Submodules are initialized and in a valid state
/// - Sparse checkout is properly configured (if enabled)
///
/// # Example
///
/// ```no_run
/// use ralph_workflow::git_helpers::rebase::validate_rebase_preconditions;
///
/// match validate_rebase_preconditions(&executor) {
///     Ok(()) => println!("All preconditions met, safe to rebase"),
///     Err(e) => eprintln!("Cannot rebase: {e}"),
/// }
/// ```
#[cfg(any(test, feature = "test-utils"))]
pub fn validate_rebase_preconditions(
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // 1. Check repository integrity
    validate_git_state()?;

    // 2. Check for concurrent Git operations
    if let Some(concurrent_op) = detect_concurrent_git_operations()? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Cannot start rebase: {} already in progress. \
                 Please complete or abort the current operation first.",
                concurrent_op.description()
            ),
        ));
    }

    // 3. Check Git identity configuration
    let config = repo.config().map_err(|e| git2_to_io_error(&e))?;

    let user_name = config.get_string("user.name");
    let user_email = config.get_string("user.email");

    if user_name.is_err() && user_email.is_err() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Git identity is not configured. Please set user.name and user.email:\n  \
             git config --global user.name \"Your Name\"\n  \
             git config --global user.email \"you@example.com\"",
        ));
    }

    // 4. Check for dirty working tree using Git CLI via executor
    let status_output = executor.execute("git", &["status", "--porcelain"], &[], None)?;

    if status_output.status.success() {
        let stdout = status_output.stdout.trim();
        if !stdout.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Working tree is not clean. Please commit or stash changes before rebasing.",
            ));
        }
    } else {
        // If git status fails, try with libgit2 as fallback
        let statuses = repo.statuses(None).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to check working tree status: {e}"),
            )
        })?;

        if !statuses.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Working tree is not clean. Please commit or stash changes before rebasing.",
            ));
        }
    }

    // 5. Check for shallow clone (limited history)
    check_shallow_clone()?;

    // 6. Check for worktree conflicts (branch checked out in another worktree)
    check_worktree_conflicts()?;

    // 7. Check submodule state (if submodules exist)
    check_submodule_state()?;

    // 8. Check sparse checkout configuration (if enabled)
    check_sparse_checkout_state()?;

    Ok(())
}

/// Check if the repository is a shallow clone.
///
/// Shallow clones have limited history and may not have all the commits
/// needed for a successful rebase.
///
/// # Returns
///
/// Returns `Ok(())` if the repository is a full clone, or an error if
/// it's a shallow clone.
#[cfg(any(test, feature = "test-utils"))]
fn check_shallow_clone() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // Check for shallow marker file
    let shallow_file = git_dir.join("shallow");
    if shallow_file.exists() {
        // This is a shallow clone - read the file to see how many commits we have
        let content = fs::read_to_string(&shallow_file).unwrap_or_default();
        let line_count = content.lines().count();

        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Repository is a shallow clone with {} commits. \
                 Rebasing may fail due to missing history. \
                 Consider running: git fetch --unshallow",
                line_count
            ),
        ));
    }

    Ok(())
}

/// Check if the current branch is checked out in another worktree.
///
/// Git does not allow a branch to be checked out in multiple worktrees
/// simultaneously.
///
/// # Returns
///
/// Returns `Ok(())` if the branch is not checked out elsewhere, or an
/// error if there's a worktree conflict.
#[cfg(any(test, feature = "test-utils"))]
fn check_worktree_conflicts() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Get current branch name
    let head = repo.head().map_err(|e| git2_to_io_error(&e))?;
    let branch_name = match head.shorthand() {
        Some(name) if head.is_branch() => name,
        _ => return Ok(()), // Detached HEAD or unborn branch - skip check
    };

    let git_dir = repo.path();
    let worktrees_dir = git_dir.join("worktrees");

    if !worktrees_dir.exists() {
        return Ok(());
    }

    // Check each worktree to see if our branch is checked out there
    let entries = fs::read_dir(&worktrees_dir).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to read worktrees directory: {e}"),
        )
    })?;

    for entry in entries.flatten() {
        let worktree_path = entry.path();
        let worktree_head = worktree_path.join("HEAD");

        if worktree_head.exists() {
            if let Ok(content) = fs::read_to_string(&worktree_head) {
                // Check if this worktree has our branch checked out
                if content.contains(&format!("refs/heads/{branch_name}")) {
                    // Extract worktree name from path
                    let worktree_name = worktree_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "Branch '{branch_name}' is already checked out in worktree '{worktree_name}'. \
                             Use 'git worktree add' to create a new worktree for this branch."
                        ),
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Check if submodules are in a valid state.
///
/// Submodules should be initialized and updated before rebasing to avoid
/// conflicts and errors.
///
/// # Returns
///
/// Returns `Ok(())` if submodules are in a valid state or no submodules
/// exist, or an error if there are submodule issues.
#[cfg(any(test, feature = "test-utils"))]
fn check_submodule_state() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // Check if .gitmodules exists
    let workdir = repo.workdir().unwrap_or(git_dir);
    let gitmodules_path = workdir.join(".gitmodules");

    if !gitmodules_path.exists() {
        return Ok(()); // No submodules
    }

    // We have submodules - check for common issues
    let modules_dir = git_dir.join("modules");
    if !modules_dir.exists() {
        // .gitmodules exists but .git/modules doesn't - submodules not initialized
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Submodules are not initialized. Run: git submodule update --init --recursive",
        ));
    }

    // Check for orphaned submodule references (common issue after rebasing)
    let gitmodules_content = fs::read_to_string(&gitmodules_path).unwrap_or_default();
    let submodule_count = gitmodules_content.matches("path = ").count();

    if submodule_count > 0 {
        // Verify each submodule directory exists
        for line in gitmodules_content.lines() {
            if line.contains("path = ") {
                if let Some(path) = line.split("path = ").nth(1) {
                    let submodule_path = workdir.join(path.trim());
                    if !submodule_path.exists() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!(
                                "Submodule '{}' is not initialized. Run: git submodule update --init --recursive",
                                path.trim()
                            ),
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if sparse checkout is properly configured.
///
/// Sparse checkout can cause issues during rebase if files outside the
/// sparse checkout cone are modified.
///
/// # Returns
///
/// Returns `Ok(())` if sparse checkout is not enabled or is properly
/// configured, or an error if there are issues.
#[cfg(any(test, feature = "test-utils"))]
fn check_sparse_checkout_state() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // Check if sparse checkout is enabled
    let config = repo.config().map_err(|e| git2_to_io_error(&e))?;

    let sparse_checkout = config.get_bool("core.sparseCheckout");
    let sparse_checkout_cone = config.get_bool("extensions.sparseCheckoutCone");

    match (sparse_checkout, sparse_checkout_cone) {
        (Ok(true), _) | (_, Ok(true)) => {
            // Sparse checkout is enabled - check if it's properly configured
            let info_sparse_dir = git_dir.join("info").join("sparse-checkout");

            if !info_sparse_dir.exists() {
                // Sparse checkout enabled but no config file - this is a problem
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Sparse checkout is enabled but not configured. \
                     Run: git sparse-checkout init",
                ));
            }

            // Verify the sparse-checkout file has content
            if let Ok(content) = fs::read_to_string(&info_sparse_dir) {
                if content.trim().is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Sparse checkout configuration is empty. \
                         Run: git sparse-checkout set <patterns>",
                    ));
                }
            }

            // Sparse checkout is enabled - warn but don't fail
            // Rebase should work with sparse checkout, but conflicts may occur
            // for files outside the sparse checkout cone
            // We return Ok to allow the operation, but the caller should be aware
        }
        (Err(_), _) | (_, Err(_)) => {
            // Config not set - sparse checkout not enabled
        }
        _ => {}
    }

    Ok(())
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
/// * `executor` - Process executor for dependency injection
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
///
pub fn rebase_onto(
    upstream_branch: &str,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<RebaseResult> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    rebase_onto_impl(&repo, upstream_branch, executor)
}

/// Implementation of rebase_onto.
fn rebase_onto_impl(
    repo: &git2::Repository,
    upstream_branch: &str,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<RebaseResult> {
    // Check if we have any commits

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
    // If merge_base fails with NotFound, branches are unrelated
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

    // Check if we're on main/master or in a detached HEAD state
    let branch_name = match head.shorthand() {
        Some(name) => name,
        None => {
            // Detached HEAD state - rebase is not applicable
            return Ok(RebaseResult::NoOp {
                reason: "HEAD is detached (not on any branch), rebase not applicable".to_string(),
            });
        }
    };

    if branch_name == "main" || branch_name == "master" {
        return Ok(RebaseResult::NoOp {
            reason: format!("Already on '{branch_name}' branch, rebase not applicable"),
        });
    }

    // Use git CLI for rebase via executor - more reliable than libgit2
    let output = executor.execute("git", &["rebase", upstream_branch], &[], None)?;

    if output.status.success() {
        Ok(RebaseResult::Success)
    } else {
        let stderr = &output.stderr;
        let stdout = &output.stdout;

        // Use classify_rebase_error to determine specific failure mode
        let error_kind = classify_rebase_error(stderr, stdout);

        match error_kind {
            RebaseErrorKind::ContentConflict { .. } => {
                // For conflicts, get of actual conflicted files
                match get_conflicted_files() {
                    Ok(files) if files.is_empty() => {
                        // If we detected a conflict but can't get of files,
                        // return error kind with files from error
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

/// Abort the current rebase operation.
///
/// This cleans up the rebase state and returns the repository to its
/// pre-rebase condition.
///
/// # Arguments
///
/// * `executor` - Process executor for dependency injection
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if:
/// - No rebase is in progress
/// - The abort operation fails
///
pub fn abort_rebase(executor: &dyn crate::executor::ProcessExecutor) -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    abort_rebase_impl(&repo, executor)
}

/// Implementation of abort_rebase.
fn abort_rebase_impl(
    repo: &git2::Repository,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<()> {
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

    // Use git CLI for abort via executor
    let output = executor.execute("git", &["rebase", "--abort"], &[], None)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Failed to abort rebase: {}",
            output.stderr
        )))
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
///
pub fn get_conflicted_files() -> io::Result<Vec<String>> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    get_conflicted_files_impl(&repo)
}

/// Implementation of get_conflicted_files.
fn get_conflicted_files_impl(repo: &git2::Repository) -> io::Result<Vec<String>> {
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

/// Verify that a rebase has completed successfully using LibGit2.
///
/// This function uses LibGit2 exclusively to verify that a rebase operation
/// has completed successfully. It checks:
/// - Repository state is clean (no rebase in progress)
/// - HEAD is valid and not detached (unless expected)
/// - Index has no conflicts
/// - Current branch is descendant of upstream (rebase succeeded)
///
/// # Returns
///
/// Returns `Ok(true)` if rebase is verified as complete, `Ok(false)` if
/// rebase is still in progress (conflicts remain), or an error if the
/// repository state is invalid.
///
/// # Note
///
/// This is the authoritative source for rebase completion verification.
/// It does NOT depend on parsing agent output or any other external signals.
#[cfg(any(test, feature = "test-utils"))]
pub fn verify_rebase_completed(upstream_branch: &str) -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // 1. Check if a rebase is still in progress
    let state = repo.state();
    if state == git2::RepositoryState::Rebase
        || state == git2::RepositoryState::RebaseMerge
        || state == git2::RepositoryState::RebaseInteractive
    {
        // Rebase is still in progress
        return Ok(false);
    }

    // 2. Check if there are any remaining conflicts in the index
    let index = repo.index().map_err(|e| git2_to_io_error(&e))?;
    if index.has_conflicts() {
        // Conflicts remain in the index
        return Ok(false);
    }

    // 3. Verify HEAD is valid
    let head = repo.head().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Repository HEAD is invalid: {e}"),
        )
    })?;

    // 4. Verify the current branch is a descendant of upstream
    // This confirms the rebase actually succeeded
    if let Ok(head_commit) = head.peel_to_commit() {
        if let Ok(upstream_object) = repo.revparse_single(upstream_branch) {
            if let Ok(upstream_commit) = upstream_object.peel_to_commit() {
                // Check if HEAD is now a descendant of upstream
                // This means the rebase moved our commits on top of upstream
                match repo.graph_descendant_of(head_commit.id(), upstream_commit.id()) {
                    Ok(is_descendant) => {
                        if is_descendant {
                            // HEAD is descendant of upstream - rebase successful
                            return Ok(true);
                        } else {
                            // HEAD is not a descendant - rebase not complete or not applicable
                            // (e.g., diverged branches, feature branch ahead of upstream)
                            return Ok(false);
                        }
                    }
                    Err(e) => {
                        // Can't determine descendant relationship - fall back to conflict check
                        let _ = e; // suppress unused warning
                    }
                }
            }
        }
    }

    // If we can't verify descendant relationship, check for conflicts
    // as a fallback - if no conflicts and no rebase in progress, consider it complete
    Ok(!index.has_conflicts())
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
///
/// **Note:** This function uses the current working directory to discover the repo.
pub fn continue_rebase(executor: &dyn crate::executor::ProcessExecutor) -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    continue_rebase_impl(&repo, executor)
}

/// Implementation of continue_rebase.
fn continue_rebase_impl(
    repo: &git2::Repository,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<()> {
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

    // Use git CLI for continue via executor
    let output = executor.execute("git", &["rebase", "--continue"], &[], None)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Failed to continue rebase: {}",
            output.stderr
        )))
    }
}

/// Check if a rebase is currently in progress.
///
/// This function checks the repository state to determine if a rebase
/// operation is in progress. This is useful for detecting interrupted
/// rebases that need to be resumed or aborted.
///
/// # Returns
///
/// Returns `Ok(true)` if a rebase is in progress, `Ok(false)` otherwise,
/// or an error if the repository cannot be accessed.
///
pub fn rebase_in_progress() -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    rebase_in_progress_impl(&repo)
}

/// Implementation of rebase_in_progress.
fn rebase_in_progress_impl(repo: &git2::Repository) -> io::Result<bool> {
    let state = repo.state();
    Ok(state == git2::RepositoryState::Rebase
        || state == git2::RepositoryState::RebaseMerge
        || state == git2::RepositoryState::RebaseInteractive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::MockProcessExecutor;
    use std::sync::Arc;

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
        use test_helpers::{commit_all, init_git_repo, with_temp_cwd, write_file};

        // Test that rebase_onto returns a Result
        with_temp_cwd(|dir| {
            // Initialize a git repo with an initial commit
            let repo = init_git_repo(dir);
            write_file(dir.path().join("initial.txt"), "initial content");
            let _ = commit_all(&repo, "initial commit");

            // Use MockProcessExecutor to avoid spawning real processes
            // The mock will return failure for the nonexistent branch
            let executor =
                Arc::new(MockProcessExecutor::new()) as Arc<dyn crate::executor::ProcessExecutor>;
            let result = rebase_onto("nonexistent_branch_that_does_not_exist", executor.as_ref());
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

            // Use MockProcessExecutor to avoid spawning real processes
            let executor =
                Arc::new(MockProcessExecutor::new()) as Arc<dyn crate::executor::ProcessExecutor>;
            let result = rebase_in_progress_cli(executor.as_ref());
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

            // Use MockProcessExecutor to avoid spawning real processes
            let executor =
                Arc::new(MockProcessExecutor::new()) as Arc<dyn crate::executor::ProcessExecutor>;
            let result = is_dirty_tree_cli(executor.as_ref());
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
