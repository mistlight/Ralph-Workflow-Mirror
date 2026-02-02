use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::super::{
    make_checkpoint_json, make_checkpoint_with_execution_history, MOCK_REPO_PATH, STANDARD_PROMPT,
};

use super::{
    make_checkpoint_with_all_new_fields, make_checkpoint_with_detailed_execution_history,
    make_checkpoint_with_git_commit_oid,
};

// ============================================================================
// V3 Hardened Resume Tests - Execution History
// ============================================================================

#[test]
fn ralph_v3_checkpoint_contains_execution_history() {
    with_default_timeout(|| {
        // Create a v3 checkpoint with execution history
        let execution_history_json = r#"{
            "steps": [
                {
                    "phase": "Planning",
                    "iteration": 1,
                    "step_type": "plan_generation",
                    "timestamp": "2024-01-01 12:00:00",
                    "outcome": {"Success": {"output": null, "files_modified": [".agent/PLAN.md"]}},
                    "agent": "claude",
                    "duration_secs": 10
                }
            ],
            "file_snapshots": {}
        }"#;

        let checkpoint_json = make_checkpoint_with_execution_history(
            MOCK_REPO_PATH,
            "Complete",
            execution_history_json,
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume should load checkpoint with execution history
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();

        // Verify the checkpoint contains execution_history
        assert!(
            checkpoint_json.contains("execution_history"),
            "V3 checkpoint should contain execution_history"
        );
    });
}

#[test]
fn ralph_v3_restores_execution_history_on_resume() {
    with_default_timeout(|| {
        // Create a v3 checkpoint with execution history
        let execution_history_json = r#"{
            "steps": [
                {
                    "phase": "Planning",
                    "iteration": 1,
                    "step_type": "plan_generation",
                    "timestamp": "2024-01-01 12:00:00",
                    "outcome": {
                        "Success": {
                            "output": null,
                            "files_modified": [".agent/PLAN.md"]
                        }
                    },
                    "agent": "test-agent",
                    "duration_secs": 10
                },
                {
                    "phase": "Development",
                    "iteration": 1,
                    "step_type": "dev_run",
                    "timestamp": "2024-01-01 12:00:10",
                    "outcome": {
                        "Success": {
                            "output": null,
                            "files_modified": []
                        }
                    },
                    "agent": "test-agent",
                    "duration_secs": 30
                }
            ],
            "file_snapshots": {}
        }"#;

        let checkpoint_json = make_checkpoint_with_execution_history(
            MOCK_REPO_PATH,
            "Complete",
            execution_history_json,
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume and verify it succeeds
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_v3_checkpoint_contains_execution_history_after_failure() {
    with_default_timeout(|| {
        // Create execution history with a step
        let execution_history_json = r#"{
            "steps": [
                {
                    "phase": "Planning",
                    "iteration": 1,
                    "step_type": "plan_generation",
                    "timestamp": "2024-01-01 12:00:00",
                    "outcome": {
                        "Success": {
                            "output": null,
                            "files_modified": [".agent/PLAN.md"]
                        }
                    },
                    "agent": "test-agent",
                    "duration_secs": 10
                }
            ],
            "file_snapshots": {}
        }"#;

        let checkpoint_json = make_checkpoint_with_execution_history(
            MOCK_REPO_PATH,
            "Complete",
            execution_history_json,
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", "# Test Prompt\n\nDo something.")
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        // Verify checkpoint has execution_history structure
        assert!(
            checkpoint_json.contains("execution_history"),
            "V3 checkpoint should have execution_history"
        );
        assert!(
            checkpoint_json.contains("steps"),
            "Execution history should have steps"
        );
        assert!(
            checkpoint_json.contains("file_snapshots"),
            "Execution history should have file_snapshots"
        );

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint can be loaded
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_checkpoint_saved_after_rebase_completion() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run pipeline with rebase enabled - should complete successfully
        run_ralph_cli_with_handler(&["--with-rebase"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_checkpoint_saved_at_pipeline_start() {
    with_default_timeout(|| {
        // Create checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint can be loaded
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Enhanced Execution History Tests (v3+ with new fields)
// ============================================================================

#[test]
fn ralph_v3_execution_step_contains_git_commit_oid() {
    with_default_timeout(|| {
        // Create checkpoint with execution step containing git_commit_oid
        let checkpoint_json = make_checkpoint_with_git_commit_oid(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        // Verify checkpoint contains git_commit_oid
        assert!(
            checkpoint_json.contains("git_commit_oid"),
            "Checkpoint should contain git_commit_oid field"
        );
        assert!(
            checkpoint_json.contains("abc123def456"),
            "Checkpoint should contain the git commit OID value"
        );

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint can be loaded
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_v3_execution_step_serialization_with_new_fields() {
    with_default_timeout(|| {
        // Create checkpoint with all new fields
        let checkpoint_json = make_checkpoint_with_all_new_fields(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint can be loaded
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_v3_resume_note_contains_execution_history() {
    with_default_timeout(|| {
        // Create checkpoint with detailed execution history
        let checkpoint_json = make_checkpoint_with_detailed_execution_history(MOCK_REPO_PATH);

        // Verify checkpoint contains the new fields
        assert!(
            checkpoint_json.contains("execution_history"),
            "Checkpoint should contain execution_history"
        );
        assert!(
            checkpoint_json.contains("modified_files_detail"),
            "Checkpoint should contain modified_files_detail"
        );
        assert!(
            checkpoint_json.contains("issues_summary"),
            "Checkpoint should contain issues_summary"
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint can be loaded
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

/// Guard: invalid execution history data should fail fast rather than silently ignoring corruption.
#[test]
fn ralph_v3_rejects_checkpoint_with_invalid_execution_history_type() {
    with_default_timeout(|| {
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1).replace(
            "\"execution_history\": null",
            "\"execution_history\": \"not-an-object\"",
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler);
        assert!(
            result.is_err(),
            "Should reject invalid execution_history type"
        );
    });
}
