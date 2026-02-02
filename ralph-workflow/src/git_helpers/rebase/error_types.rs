// Error type definitions, classification, and parsing for rebase operations.
//
// This file contains:
// - RebaseErrorKind enum with all failure mode categories
// - RebaseResult enum for operation outcomes
// - Error classification functions for Git CLI output parsing
// - Helper functions for extracting information from error messages

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
