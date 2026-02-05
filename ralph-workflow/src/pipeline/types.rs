//! Core pipeline types (cleanup guards and command results).

use crate::files::cleanup_generated_files_with_workspace;
use crate::git_helpers::{disable_git_wrapper, end_agent_phase, uninstall_hooks, GitHelpers};
use crate::logger::Logger;
use crate::workspace::Workspace;

/// Legacy statistics tracking (DEPRECATED - use PipelineState.metrics instead).
///
/// This struct is deprecated and kept only for backward compatibility with legacy code.
/// New code should use `PipelineState.metrics` (RunMetrics) instead, which is the
/// authoritative source for all execution statistics.
///
/// # Migration Path
///
/// - DO NOT add new fields to this struct
/// - DO NOT increment these counters in new code
/// - Use `PipelineState.metrics` for all new metric tracking
/// - This struct will be removed once all legacy code is migrated
pub struct Stats {
    /// Number of times repository changes were detected (unused - see metrics.commits_created_total)
    pub changes_detected: u32,
    /// Number of developer agent runs completed (unused - see metrics.dev_iterations_completed)
    pub developer_runs_completed: u32,
    /// Number of reviewer agent runs completed (unused - see metrics.review_runs_total)
    pub reviewer_runs_completed: u32,
    /// Number of commits created by the orchestrator (unused - see metrics.commits_created_total)
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
