use ralph_workflow::checkpoint::load_checkpoint_with_workspace;
use ralph_workflow::workspace::MemoryWorkspace;

use crate::test_timeout::with_default_timeout;

// ============================================================================
// CHECKPOINT FORMAT TESTS
// ============================================================================

/// Test that legacy (pre-v1) checkpoint format is rejected.
///
/// Legacy checkpoints have a minimal structure without version number.
/// These should no longer be auto-migrated.
#[test]
fn test_checkpoint_rejects_legacy_format() {
    with_default_timeout(|| {
        let legacy_checkpoint = r#"{
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 00:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude"
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", legacy_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject legacy checkpoint format without version field"
        );
    });
}

/// Test that V1 checkpoint format is rejected.
///
/// V1 checkpoints have version field but lack run_id and other v2 fields.
#[test]
fn test_checkpoint_rejects_v1_format() {
    with_default_timeout(|| {
        let v1_checkpoint = r#"{
            "version": 1,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 00:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {
                "iterations": 5,
                "reviewer_passes": 2,
                "agent": null,
                "verbose": false,
                "auto_commit": true
            },
            "developer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "reviewer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "rebase_state": {
                "rebase_enabled": false,
                "current_main_commit": null,
                "original_branch": null
            },
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v1_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject V1 checkpoint format (missing run_id)"
        );
    });
}

/// Test that V2 checkpoint format is rejected.
///
/// V2 checkpoints have run_id but lack v3 fields (execution_history, etc.).
#[test]
fn test_checkpoint_rejects_v2_format() {
    with_default_timeout(|| {
        let v2_checkpoint = r#"{
            "version": 2,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 00:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {
                "iterations": 5,
                "reviewer_passes": 2,
                "agent": null,
                "verbose": false,
                "auto_commit": true
            },
            "developer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "reviewer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "rebase_state": {
                "rebase_enabled": false,
                "current_main_commit": null,
                "original_branch": null
            },
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
            "actual_reviewer_runs": 0
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v2_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject V2 checkpoint format (missing v3 fields)"
        );
    });
}

/// Test that current V3 checkpoint format is accepted.
///
/// V3 is the only supported format. This test uses the correct checkpoint structure
/// which matches the internal `make_test_checkpoint_for_workspace` helper.
#[test]
fn test_checkpoint_accepts_v3_format() {
    with_default_timeout(|| {
        // Use proper V3 format with all required fields matching AgentConfigSnapshot
        let v3_checkpoint = r#"{
            "version": 3,
            "phase": "Development",
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
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v3_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_ok(),
            "Should accept V3 checkpoint format: {:?}",
            result
        );
        assert!(result.unwrap().is_some(), "Checkpoint should be present");
    });
}

/// Guard: a V3 checkpoint missing required fields must be rejected (no implicit migration).
#[test]
fn test_checkpoint_rejects_v3_missing_required_field() {
    with_default_timeout(|| {
        // Missing `run_id` (required in V3).
        let v3_missing_run_id = r#"{
            "version": 3,
            "phase": "Development",
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
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v3_missing_run_id);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject V3 checkpoint missing required field"
        );
    });
}
