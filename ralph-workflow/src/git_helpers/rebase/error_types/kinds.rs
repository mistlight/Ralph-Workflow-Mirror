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
    #[must_use] 
    pub fn description(&self) -> String {
        match self {
            Self::InvalidRevision { revision } => {
                format!("Invalid or unresolvable revision: '{revision}'")
            }
            Self::DirtyWorkingTree => "Working tree has uncommitted changes".to_string(),
            Self::ConcurrentOperation { operation } => {
                format!("Concurrent Git operation in progress: {operation}")
            }
            Self::RepositoryCorrupt { details } => {
                format!("Repository integrity issue: {details}")
            }
            Self::EnvironmentFailure { reason } => {
                format!("Environment or configuration failure: {reason}")
            }
            Self::HookRejection { hook_name } => {
                format!("Hook '{hook_name}' rejected the operation")
            }
            Self::ContentConflict { files } => {
                format!("Merge conflicts in {} file(s)", files.len())
            }
            Self::PatchApplicationFailed { reason } => {
                format!("Patch application failed: {reason}")
            }
            Self::InteractiveStop { command } => {
                format!("Interactive rebase stopped at command: {command}")
            }
            Self::EmptyCommit => "Empty or redundant commit".to_string(),
            Self::AutostashFailed { reason } => {
                format!("Autostash failed: {reason}")
            }
            Self::CommitCreationFailed { reason } => {
                format!("Commit creation failed: {reason}")
            }
            Self::ReferenceUpdateFailed { reason } => {
                format!("Reference update failed: {reason}")
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::ValidationFailed { reason } => {
                format!("Post-rebase validation failed: {reason}")
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::ProcessTerminated { reason } => {
                format!("Rebase process terminated: {reason}")
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::InconsistentState { details } => {
                format!("Inconsistent rebase state: {details}")
            }
            Self::Unknown { details } => {
                format!("Unknown rebase error: {details}")
            }
        }
    }

    /// Returns whether this error can potentially be recovered automatically.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        match self {
            // These are generally recoverable with automatic retry or cleanup
            Self::ConcurrentOperation { .. } => true,
            #[cfg(any(test, feature = "test-utils"))]
            Self::ProcessTerminated { .. }
            | Self::InconsistentState { .. } => true,

            // These require manual conflict resolution
            Self::ContentConflict { .. } => true,

            // These are generally not recoverable without manual intervention
            Self::InvalidRevision { .. }
            | Self::DirtyWorkingTree
            | Self::RepositoryCorrupt { .. }
            | Self::EnvironmentFailure { .. }
            | Self::HookRejection { .. }
            | Self::PatchApplicationFailed { .. }
            | Self::InteractiveStop { .. }
            | Self::EmptyCommit
            | Self::AutostashFailed { .. }
            | Self::CommitCreationFailed { .. }
            | Self::ReferenceUpdateFailed { .. } => false,
            #[cfg(any(test, feature = "test-utils"))]
            Self::ValidationFailed { .. } => false,
            Self::Unknown { .. } => false,
        }
    }

    /// Returns the category number (1-5) for this error.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub const fn category(&self) -> u8 {
        match self {
            Self::InvalidRevision { .. }
            | Self::DirtyWorkingTree
            | Self::ConcurrentOperation { .. }
            | Self::RepositoryCorrupt { .. }
            | Self::EnvironmentFailure { .. }
            | Self::HookRejection { .. } => 1,

            Self::ContentConflict { .. }
            | Self::PatchApplicationFailed { .. }
            | Self::InteractiveStop { .. }
            | Self::EmptyCommit
            | Self::AutostashFailed { .. }
            | Self::CommitCreationFailed { .. }
            | Self::ReferenceUpdateFailed { .. } => 2,

            #[cfg(any(test, feature = "test-utils"))]
            Self::ValidationFailed { .. } => 3,

            #[cfg(any(test, feature = "test-utils"))]
            Self::ProcessTerminated { .. }
            | Self::InconsistentState { .. } => 4,

            Self::Unknown { .. } => 5,
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
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// Returns whether the rebase had conflicts (needs resolution).
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub const fn has_conflicts(&self) -> bool {
        matches!(self, Self::Conflicts(_))
    }

    /// Returns whether the rebase was a no-op (not applicable).
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        matches!(self, Self::NoOp { .. })
    }

    /// Returns whether the rebase failed.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Returns the conflict files if this result contains conflicts.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn conflict_files(&self) -> Option<&[String]> {
        match self {
            Self::Conflicts(files) | Self::Failed(RebaseErrorKind::ContentConflict { files }) => {
                Some(files)
            }
            _ => None,
        }
    }

    /// Returns the error kind if this result is a failure.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub const fn error_kind(&self) -> Option<&RebaseErrorKind> {
        match self {
            Self::Failed(kind) => Some(kind),
            _ => None,
        }
    }

    /// Returns the no-op reason if this result is a no-op.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn noop_reason(&self) -> Option<&str> {
        match self {
            Self::NoOp { reason } => Some(reason),
            _ => None,
        }
    }
}

