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
//! - [`wrapper`] - Agent phase git wrapper for safe concurrent execution

#![deny(unsafe_code)]

mod hooks;
pub mod identity;
mod repo;
mod start_commit;
pub mod wrapper;

pub use hooks::uninstall_hooks;
pub use repo::{
    get_git_diff_from_start, get_repo_root, git_add_all, git_commit, git_diff, git_snapshot,
    require_git_repo, validate_and_truncate_diff, CommitResultFallback,
};
pub use start_commit::{reset_start_commit, save_start_commit};
pub use wrapper::{
    cleanup_agent_phase_silent, cleanup_orphaned_marker, disable_git_wrapper, end_agent_phase,
    start_agent_phase, GitHelpers,
};

#[cfg(test)]
mod tests;
