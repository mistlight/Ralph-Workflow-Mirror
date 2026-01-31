//! Core pipeline types (stats and cleanup guards).

use crate::files::cleanup_generated_files_with_workspace;
use crate::git_helpers::{disable_git_wrapper, end_agent_phase, uninstall_hooks, GitHelpers};
use crate::logger::Logger;
use crate::workspace::Workspace;

/// Statistics tracking for pipeline execution.
pub struct Stats {
    /// Number of times repository changes were detected
    pub changes_detected: u32,
    /// Number of developer agent runs completed
    pub developer_runs_completed: u32,
    /// Number of reviewer agent runs completed
    pub reviewer_runs_completed: u32,
    /// Number of commits created by the orchestrator
    pub commits_created: u32,
}

impl Stats {
    /// Create a new Stats instance with all counters at zero.
    pub const fn new() -> Self {
        Self {
            changes_detected: 0,
            developer_runs_completed: 0,
            reviewer_runs_completed: 0,
            commits_created: 0,
        }
    }
}

impl Default for Stats {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of running a command, including stderr for error classification.
pub struct CommandResult {
    /// Exit code from the command (0 = success)
    pub(crate) exit_code: i32,
    /// Standard error output captured from the command
    pub(crate) stderr: String,
    /// Session ID from the agent's init event (if available).
    ///
    /// This is extracted from the agent's JSON output and can be used for
    /// session continuation (XSD retry). Not all agents provide session IDs.
    pub session_id: Option<String>,
}

/// RAII guard for agent phase cleanup.
///
/// Ensures that agent phase cleanup happens even if the pipeline is interrupted
/// by panics or early returns. Call `disarm()` on successful completion to
/// prevent cleanup.
pub struct AgentPhaseGuard<'a> {
    /// Mutable reference to git helpers for cleanup operations
    pub git_helpers: &'a mut GitHelpers,
    logger: &'a Logger,
    workspace: &'a dyn Workspace,
    active: bool,
}

impl<'a> AgentPhaseGuard<'a> {
    /// Create a new guard that will clean up on drop unless disarmed.
    pub fn new(
        git_helpers: &'a mut GitHelpers,
        logger: &'a Logger,
        workspace: &'a dyn Workspace,
    ) -> Self {
        Self {
            git_helpers,
            logger,
            workspace,
            active: true,
        }
    }

    /// Disarm the guard, preventing cleanup on drop.
    ///
    /// Call this when the pipeline completes successfully.
    pub const fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for AgentPhaseGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        end_agent_phase();
        disable_git_wrapper(self.git_helpers);
        let _ = uninstall_hooks(self.logger);
        cleanup_generated_files_with_workspace(self.workspace);
    }
}
