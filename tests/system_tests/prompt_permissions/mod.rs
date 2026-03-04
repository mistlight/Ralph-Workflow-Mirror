//! System tests for real filesystem PROMPT.md permission toggling and
//! `AgentPhaseGuard` RAII cleanup behaviour.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::git_helpers::GitHelpers;
use ralph_workflow::logger::{Colors, Logger};
use ralph_workflow::pipeline::AgentPhaseGuard;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace, WorkspaceFs, PROMPT_MD};
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;
use test_helpers::{init_git_repo, with_temp_cwd};

#[test]
#[serial]
fn test_prompt_md_permission_toggle() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().expect("create temp dir");
        let _repo = init_git_repo(&temp_dir);

        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());
        let prompt_path = temp_dir.path().join(PROMPT_MD);

        workspace
            .set_readonly(Path::new(PROMPT_MD))
            .expect("set PROMPT.md read-only");
        assert_prompt_readonly(&prompt_path);

        workspace
            .set_writable(Path::new(PROMPT_MD))
            .expect("set PROMPT.md writable");
        assert_prompt_writable(&prompt_path);
    });
}

fn assert_prompt_readonly(path: &Path) {
    let metadata = std::fs::metadata(path).expect("stat PROMPT.md");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o222, 0, "expected PROMPT.md to be read-only");
    }

    #[cfg(windows)]
    {
        assert!(
            metadata.permissions().readonly(),
            "expected PROMPT.md to be read-only"
        );
    }
}

fn assert_prompt_writable(path: &Path) {
    let metadata = std::fs::metadata(path).expect("stat PROMPT.md");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        assert_ne!(mode & 0o200, 0, "expected PROMPT.md to be writable");
    }

    #[cfg(windows)]
    {
        assert!(
            !metadata.permissions().readonly(),
            "expected PROMPT.md to be writable"
        );
    }
}

/// Test that `AgentPhaseGuard::drop()` restores PROMPT.md permissions.
///
/// Verifies that when `AgentPhaseGuard` is dropped without calling `disarm()`,
/// the RAII cleanup executes including PROMPT.md permission restoration.
#[test]
#[serial]
fn test_agent_phase_guard_drop_restores_prompt_md() {
    with_temp_cwd(|dir| {
        let _repo = init_git_repo(dir);

        let workspace = WorkspaceFs::new(dir.path().to_path_buf());
        let prompt_rel = Path::new("PROMPT.md");

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
        let mut git_helpers = GitHelpers::default();

        {
            let _guard = AgentPhaseGuard::new(&mut git_helpers, &logger, &workspace);
        }

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
#[test]
#[serial]
fn test_agent_phase_guard_disarm_prevents_cleanup() {
    with_temp_cwd(|dir| {
        let _repo = init_git_repo(dir);

        let workspace =
            MemoryWorkspace::new_test().with_file("PROMPT.md", "# Goal\nTest content\n");
        let logger = Logger::new(Colors::new());
        let mut git_helpers = GitHelpers::default();

        {
            let mut guard = AgentPhaseGuard::new(&mut git_helpers, &logger, &workspace);
            guard.disarm();
        }

        assert!(
            workspace.exists(Path::new("PROMPT.md")),
            "PROMPT.md should exist after disarmed guard drop"
        );
    });
}

/// Test that guard cleanup handles missing PROMPT.md gracefully.
///
/// `make_prompt_writable_with_workspace` should not panic if PROMPT.md
/// doesn't exist (edge case during early interrupts).
#[test]
#[serial]
fn test_agent_phase_guard_drop_handles_missing_prompt_md() {
    with_temp_cwd(|dir| {
        let _repo = init_git_repo(dir);

        let workspace = MemoryWorkspace::new_test();
        let logger = Logger::new(Colors::new());
        let mut git_helpers = GitHelpers::default();

        {
            let _guard = AgentPhaseGuard::new(&mut git_helpers, &logger, &workspace);
        }
        // Test passes if no panic occurs
    });
}
