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

