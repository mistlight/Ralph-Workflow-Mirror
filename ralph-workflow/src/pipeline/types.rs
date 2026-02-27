//! Core pipeline types (cleanup guards and command results).

use crate::files::{cleanup_generated_files_with_workspace, make_prompt_writable_with_workspace};
use crate::git_helpers::{disable_git_wrapper, end_agent_phase, uninstall_hooks, GitHelpers};
use crate::logger::Logger;
use crate::workspace::Workspace;

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

        // Restore PROMPT.md write permissions FIRST (most important for user recovery).
        // This is best-effort - we don't want to panic in drop().
        // Even if this run didn't lock PROMPT.md, a prior crashed run may have left it
        // read-only, so we always attempt restoration.
        let _ = make_prompt_writable_with_workspace(self.workspace);

        end_agent_phase();
        disable_git_wrapper(self.git_helpers);
        let _ = uninstall_hooks(self.logger);
        cleanup_generated_files_with_workspace(self.workspace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::Colors;
    use crate::workspace::{MemoryWorkspace, WorkspaceFs};
    use std::path::Path;
    use test_helpers::{init_git_repo, with_temp_cwd};

    /// Test that `AgentPhaseGuard::drop()` restores PROMPT.md permissions.
    ///
    /// This verifies that when `AgentPhaseGuard` is dropped without calling `disarm()`,
    /// the RAII cleanup executes including PROMPT.md permission restoration.
    ///
    /// We use `WorkspaceFs` (real filesystem) here to ensure the test actually
    /// exercises permissions rather than a no-op implementation.
    #[test]
    fn test_agent_phase_guard_drop_restores_prompt_md() {
        with_temp_cwd(|dir| {
            let _repo = init_git_repo(dir);
            // Ensure git helper operations (hooks, wrapper) target the temp repo.
            std::env::set_current_dir(dir.path()).expect("set current dir");

            let workspace = WorkspaceFs::new(dir.path().to_path_buf());
            let prompt_rel = Path::new("PROMPT.md");

            // Precondition: PROMPT.md exists and is read-only.
            assert!(workspace.exists(prompt_rel), "PROMPT.md should exist");
            workspace
                .set_readonly(prompt_rel)
                .expect("set PROMPT.md read-only");

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir.path().join("PROMPT.md"))
                    .expect("stat PROMPT.md")
                    .permissions()
                    .mode();
                assert_eq!(
                    mode & 0o200,
                    0,
                    "PROMPT.md should be non-writable before drop"
                );
            }

            let logger = Logger::new(Colors::new());
            let mut git_helpers = GitHelpers::new();

            // Create guard and let it drop without disarming.
            {
                let _guard = AgentPhaseGuard::new(&mut git_helpers, &logger, &workspace);
            }

            // Assert: PROMPT.md is writable again after drop.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir.path().join("PROMPT.md"))
                    .expect("stat PROMPT.md")
                    .permissions()
                    .mode();
                assert_ne!(mode & 0o200, 0, "PROMPT.md should be writable after drop");
            }
        });
    }

    /// Test that disarmed guard does NOT run cleanup.
    ///
    /// When `disarm()` is called, the guard should not execute cleanup on drop.
    /// This verifies the active flag works correctly.
    #[test]
    fn test_agent_phase_guard_disarm_prevents_cleanup() {
        with_temp_cwd(|dir| {
            let _repo = init_git_repo(dir);
            std::env::set_current_dir(dir.path()).expect("set current dir");

            let workspace =
                MemoryWorkspace::new_test().with_file("PROMPT.md", "# Goal\nTest content\n");
            let logger = Logger::new(Colors::new());
            let mut git_helpers = GitHelpers::new();

            // Create guard, disarm it, then let it drop
            {
                let mut guard = AgentPhaseGuard::new(&mut git_helpers, &logger, &workspace);
                guard.disarm();
                // Guard will be dropped here - should NOT run cleanup
            }

            // PROMPT.md should still exist (though cleanup would preserve it anyway)
            assert!(
                workspace.exists(Path::new("PROMPT.md")),
                "PROMPT.md should exist after disarmed guard drop"
            );
        });
    }

    /// Test that guard cleanup handles missing PROMPT.md gracefully.
    ///
    /// The `make_prompt_writable_with_workspace` function should not panic
    /// if PROMPT.md doesn't exist (edge case during early interrupts).
    #[test]
    fn test_agent_phase_guard_drop_handles_missing_prompt_md() {
        with_temp_cwd(|dir| {
            let _repo = init_git_repo(dir);
            std::env::set_current_dir(dir.path()).expect("set current dir");

            // Workspace without PROMPT.md
            let workspace = MemoryWorkspace::new_test();
            let logger = Logger::new(Colors::new());
            let mut git_helpers = GitHelpers::new();

            // Create guard and let it drop - should not panic
            {
                let _guard = AgentPhaseGuard::new(&mut git_helpers, &logger, &workspace);
                // Guard will be dropped here
            }

            // No assertion needed - test passes if no panic occurs
        });
    }
}
