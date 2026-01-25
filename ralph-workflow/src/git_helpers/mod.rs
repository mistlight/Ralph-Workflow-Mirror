//! Git Helper Functions
//!
//! Provides git hooks management, a git wrapper for blocking commits during the
//! agent phase, and basic repository utilities.
//!
//! # Module Structure
//!
//! - [`hooks`] - Git hooks installation and removal
//! - [`identity`] - Git identity resolution with comprehensive fallback chain
//! - [`repo`] - Basic git repository operations (add, commit, snapshot)
//! - [`start_commit`] - Starting commit tracking for incremental diffs
//! - [`review_baseline`] - Per-review-cycle baseline tracking
//! - [`wrapper`] - Agent phase git wrapper for safe concurrent execution
//! - [`branch`] - Branch detection and default branch resolution
//! - [`rebase`] - Rebase operations with fault tolerance

#![deny(unsafe_code)]

pub mod branch;
mod hooks;
pub mod identity;
mod rebase;

#[cfg(any(test, feature = "test-utils"))]
pub mod rebase_checkpoint;

#[cfg(any(test, feature = "test-utils"))]
pub mod rebase_state_machine;

mod repo;
mod review_baseline;
mod start_commit;
mod wrapper;

#[cfg(any(test, feature = "test-utils"))]
pub mod ops;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_trait;

pub use branch::{get_default_branch, is_main_or_master_branch};
#[cfg(any(test, feature = "test-utils"))]
pub use branch::{get_default_branch_at, is_main_or_master_branch_at};
pub use hooks::uninstall_hooks;
pub use rebase::{
    abort_rebase, continue_rebase, get_conflict_markers_for_file, get_conflicted_files,
    rebase_in_progress, rebase_onto, RebaseResult,
};
#[cfg(any(test, feature = "test-utils"))]
pub use rebase::{
    abort_rebase_at, continue_rebase_at, get_conflicted_files_at, rebase_in_progress_at,
    rebase_onto_at,
};

// Types that are part of the public API but not used in binary
#[cfg(any(test, feature = "test-utils"))]
pub use rebase::{CleanupResult, ConcurrentOperation};

#[cfg(any(test, feature = "test-utils"))]
pub use rebase::{
    attempt_automatic_recovery, cleanup_stale_rebase_state, detect_concurrent_git_operations,
    is_dirty_tree_cli, rebase_in_progress_cli, restore_from_reflog,
    validate_post_rebase_with_checks, validate_rebase_preconditions, verify_rebase_completed,
    PostRebaseValidationResult,
};

#[cfg(any(test, feature = "test-utils"))]
pub use rebase::RebaseErrorKind;

#[cfg(any(test, feature = "test-utils"))]
pub use rebase_checkpoint::RebasePhase;

#[cfg(any(test, feature = "test-utils"))]
pub use rebase_state_machine::{RebaseLock, RebaseStateMachine};
#[cfg(any(test, feature = "test-utils"))]
pub use repo::{get_hooks_dir_at, git_add_all_at, git_commit_at, git_diff_at, git_snapshot_at};
pub use repo::{
    get_repo_root, git_add_all, git_commit, git_diff, git_snapshot, require_git_repo,
    CommitResultFallback, DiffReviewContent, DiffTruncationLevel,
};
pub use review_baseline::{
    get_baseline_summary, get_review_baseline_info, load_review_baseline, update_review_baseline,
    ReviewBaseline,
};
#[cfg(any(test, feature = "test-utils"))]
pub use review_baseline::{
    get_baseline_summary_at, get_review_baseline_info_at, load_review_baseline_at,
    update_review_baseline_at,
};
pub use start_commit::{
    get_current_head_oid, get_start_commit_summary, load_start_point, reset_start_commit,
    save_start_commit, StartPoint,
};
#[cfg(any(test, feature = "test-utils"))]
pub use start_commit::{
    get_current_head_oid_at, get_start_commit_summary_at, load_start_point_at,
    reset_start_commit_at, save_start_commit_at,
};
pub use wrapper::{
    cleanup_agent_phase_silent, cleanup_orphaned_marker, disable_git_wrapper, end_agent_phase,
    start_agent_phase, GitHelpers,
};

#[cfg(any(test, feature = "test-utils"))]
pub use ops::{CommitResult, GitOps, RealGit};

#[cfg(any(test, feature = "test-utils"))]
pub use ops::RebaseResult as OpsRebaseResult;

#[cfg(any(test, feature = "test-utils"))]
pub use test_trait::MockGit;

// Re-export checkpoint and recovery action for tests only
#[cfg(any(test, feature = "test-utils"))]
pub use rebase_checkpoint::RebaseCheckpoint;

#[cfg(any(test, feature = "test-utils"))]
pub use rebase_state_machine::RecoveryAction;

#[cfg(test)]
mod tests;
