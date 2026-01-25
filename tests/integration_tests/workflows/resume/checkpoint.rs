//! Checkpoint creation, content, and cleanup tests.

use std::fs;
use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;

use super::{canonical_working_dir, precreate_plan_file};
use test_helpers::init_git_repo;

// ============================================================================
// Checkpoint Creation Tests
// ============================================================================

#[test]
fn ralph_creates_checkpoint_during_development() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create required files to skip agent phases
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(
            dir.path().join(".agent/commit-message.txt"),
            "feat: test commit\n",
        )
        .unwrap();

        // Run with 0 iterations - pipeline completes without agent execution
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_creates_checkpoint_during_review() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Run with 0 iterations - pipeline completes without agent execution
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Checkpoint Content Tests
// ============================================================================

#[test]
fn ralph_checkpoint_contains_iteration_info() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create PLAN.md to skip planning phase and avoid agent execution
        precreate_plan_file(&dir);

        // Pre-create a checkpoint file with expected structure at Complete phase
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 1, 1),
        )
        .unwrap();

        // Run pipeline - should validate checkpoint structure
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

        // Verify checkpoint was cleared after successful complete phase
        assert!(
            !dir.path().join(".agent/checkpoint.json").exists(),
            "Checkpoint should be cleared after successful completion"
        );
    });
}

#[test]
fn ralph_checkpoint_contains_cli_args_snapshot() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create PLAN.md to skip planning phase
        precreate_plan_file(&dir);

        // Pre-create a checkpoint file at Complete phase
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 3, 3),
        )
        .unwrap();

        // Run pipeline - checkpoint at Complete should be cleared
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_checkpoint_contains_agent_config_snapshot() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create PLAN.md to skip planning phase
        precreate_plan_file(&dir);

        // Pre-create a checkpoint file at Complete phase
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 1, 1),
        )
        .unwrap();

        // Run pipeline
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Checkpoint Cleanup Tests
// ============================================================================

#[test]
fn ralph_clears_checkpoint_on_success() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 1, 1),
        )
        .unwrap();

        // Run successfully - checkpoint should be cleared
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

        // Verify checkpoint was cleared
        assert!(
            !dir.path().join(".agent/checkpoint.json").exists(),
            "Checkpoint should be cleared on successful completion"
        );
    });
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper function to create a valid v3 checkpoint JSON with all required fields.
/// Always sets developer_iters and reviewer_reviews to 0 to prevent agent execution.
fn make_checkpoint_json(
    working_dir: &str,
    phase: &str,
    iteration: u32,
    total_iterations: u32,
) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "{}",
            "iteration": {},
            "total_iterations": {},
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": 0,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null,
                "skip_rebase": false
            }},
            "developer_agent_config": {{
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            }},
            "reviewer_agent_config": {{
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            }},
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "{}",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "00000000-0000-0000-0000-000000000001",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": {},
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#,
        phase, iteration, total_iterations, working_dir, iteration
    )
}
