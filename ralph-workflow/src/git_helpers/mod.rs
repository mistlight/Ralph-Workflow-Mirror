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
//! - Rebase operations are provided via the `rebase` module functions

#![deny(unsafe_code)]

pub mod branch;
mod hooks;
pub mod identity;
mod rebase;
mod repo;
mod review_baseline;
mod start_commit;
mod wrapper;

#[cfg(any(test, feature = "test-utils"))]
pub mod ops;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_trait;

pub use branch::{get_default_branch, is_main_or_master_branch};
pub use hooks::uninstall_hooks;
pub use rebase::{
    abort_rebase, continue_rebase, get_conflict_markers_for_file, get_conflicted_files,
    rebase_onto, RebaseResult,
};
pub use repo::{
    get_repo_root, git_add_all, git_commit, git_diff, git_snapshot, require_git_repo,
    validate_and_truncate_diff, CommitResultFallback,
};
pub use review_baseline::{
    get_git_diff_from_review_baseline, get_review_baseline_info, load_review_baseline,
    update_review_baseline, ReviewBaseline,
};
pub use start_commit::{
    get_current_head_oid, load_start_point, reset_start_commit, save_start_commit, StartPoint,
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

#[cfg(test)]
mod tests;
