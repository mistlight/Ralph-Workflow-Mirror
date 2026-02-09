//! Integration tests for legacy phase rejection.
//!
//! Verifies that checkpoints containing legacy phase names (e.g., "Fix", "Analyze")
//! are rejected and require explicit user migration rather than silent auto-migration.

use ralph_workflow::checkpoint::load_checkpoint_with_workspace;
use ralph_workflow::workspace::MemoryWorkspace;

use crate::test_timeout::with_default_timeout;

// ============================================================================
// LEGACY PHASE REJECTION TESTS
// ============================================================================

/// Test that checkpoint with legacy "Fix" phase is rejected outright.
///
/// The reducer-only architecture requires that legacy phases are rejected,
/// not silently migrated. Users must delete old checkpoints and start fresh.
#[test]
fn test_checkpoint_rejects_legacy_fix_phase() {
    with_default_timeout(|| {
        let v3_with_fix = r#"{
            "version": 3,
            "phase": "Fix",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 5,
                "reviewer_reviews": 2
            },
            "developer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "reviewer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null,
            "env_snapshot": null
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v3_with_fix);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject checkpoint with legacy Fix phase (not silently migrate)"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Fix") || err.contains("legacy") || err.contains("no longer supported"),
            "Error should mention Fix or legacy: {err}"
        );
    });
}

/// Test that checkpoint with legacy "ReviewAgain" phase is rejected outright.
///
/// The reducer-only architecture requires that legacy phases are rejected,
/// not silently migrated. Users must delete old checkpoints and start fresh.
#[test]
fn test_checkpoint_rejects_legacy_review_again_phase() {
    with_default_timeout(|| {
        let v3_with_review_again = r#"{
            "version": 3,
            "phase": "ReviewAgain",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 5,
                "reviewer_reviews": 2
            },
            "developer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "reviewer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null,
            "env_snapshot": null
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v3_with_review_again);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject checkpoint with legacy ReviewAgain phase (not silently migrate)"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("ReviewAgain")
                || err.contains("legacy")
                || err.contains("no longer supported"),
            "Error should mention ReviewAgain or legacy: {err}"
        );
    });
}
