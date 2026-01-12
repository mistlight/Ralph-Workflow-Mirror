//! Git helper functions.
//!
//! Provides git hooks management, a git wrapper for blocking commits during the
//! agent phase, and basic repository utilities.

#![deny(unsafe_code)]

mod hooks;
mod repo;
mod wrapper;

pub(crate) use hooks::uninstall_hooks;
pub(crate) use repo::{get_repo_root, git_add_all, git_commit, git_snapshot, require_git_repo};
pub(crate) use wrapper::{
    cleanup_agent_phase_silent, cleanup_orphaned_marker, disable_git_wrapper, end_agent_phase,
    start_agent_phase, GitHelpers,
};

#[cfg(test)]
mod tests;
