//! Rebase operation events.
//!
//! Events related to git rebase operations including conflict detection
//! and resolution.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Rebase phase (initial or post-review).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebasePhase {
    /// Initial rebase before development starts.
    Initial,
    /// Post-review rebase after review fixes.
    PostReview,
}

/// Conflict resolution strategy.
///
/// Determines how the pipeline should handle merge conflicts during rebase operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Abort the rebase and restore original state.
    Abort,
    /// Continue rebase after conflict resolution.
    Continue,
    /// Skip the conflicting commit.
    Skip,
}

/// Rebase operation events.
///
/// Events related to git rebase operations including conflict detection
/// and resolution. Rebase operations can occur at multiple points in the
/// pipeline (initial and post-review).
///
/// # State Machine
///
/// ```text
/// NotStarted -> InProgress -> Conflicted -> InProgress -> Completed
///                    |                           |
///                    +---------> Skipped <-------+
///                    |
///                    +---------> Failed (resets to NotStarted)
/// ```
///
/// # Emitted By
///
/// - Rebase handlers in `handler/rebase.rs`
/// - Git integration layer
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RebaseEvent {
    /// Rebase operation started.
    ///
    /// Emitted when a rebase begins. The reducer uses this to:
    /// - Track which rebase phase is active (initial or post-review)
    /// - Record the target branch for observability
    Started {
        /// The rebase phase (initial or post-review).
        phase: RebasePhase,
        /// The target branch to rebase onto.
        target_branch: String,
    },
    /// Merge conflict detected during rebase.
    ///
    /// Emitted when git detects merge conflicts. The handler will attempt
    /// automated resolution; the reducer tracks which files are conflicted.
    ConflictDetected {
        /// The files with conflicts.
        files: Vec<PathBuf>,
    },
    /// Merge conflicts were resolved.
    ///
    /// Emitted after successful conflict resolution. The reducer uses this
    /// to clear the conflict state and allow rebase to continue.
    ConflictResolved {
        /// The files that were resolved.
        files: Vec<PathBuf>,
    },
    /// Rebase completed successfully.
    ///
    /// Emitted when rebase finishes without errors. The reducer uses this to:
    /// - Mark rebase as complete
    /// - Record the new HEAD commit
    /// - Transition to the next pipeline phase
    Succeeded {
        /// The rebase phase that completed.
        phase: RebasePhase,
        /// The new HEAD after rebase.
        new_head: String,
    },
    /// Rebase failed and was reset.
    ///
    /// Emitted when rebase encounters an unrecoverable error. The reducer
    /// uses this to decide whether to retry or abort the pipeline.
    Failed {
        /// The rebase phase that failed.
        phase: RebasePhase,
        /// The reason for failure.
        reason: String,
    },
    /// Rebase was aborted and state restored.
    ///
    /// Emitted when rebase is explicitly aborted (e.g., user interrupt).
    /// The reducer marks rebase as not attempted.
    Aborted {
        /// The rebase phase that was aborted.
        phase: RebasePhase,
        /// The commit that was restored.
        restored_to: String,
    },
    /// Rebase was skipped (e.g., already up to date).
    ///
    /// Emitted when rebase is unnecessary. The reducer marks rebase as
    /// complete without actually performing the operation.
    Skipped {
        /// The rebase phase that was skipped.
        phase: RebasePhase,
        /// The reason for skipping.
        reason: String,
    },
}
