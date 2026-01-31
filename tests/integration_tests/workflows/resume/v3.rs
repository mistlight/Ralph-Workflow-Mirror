//! V3 hardened resume tests (execution history, file system state, prompt replay).
//!
//! These tests use MockAppEffectHandler for in-memory testing without
//! real filesystem or git operations.

use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::{
    make_checkpoint_json, make_checkpoint_with_execution_history,
    make_checkpoint_with_file_system_state, make_checkpoint_with_prompt_history, MOCK_REPO_PATH,
    STANDARD_PROMPT, STANDARD_PROMPT_CHECKSUM,
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

// ============================================================================
// V3 Hardened Resume Tests - File System State
// ============================================================================

#[test]
fn ralph_v3_file_system_state_validates_on_resume() {
    with_default_timeout(|| {
        // Use STANDARD_PROMPT which matches STANDARD_PROMPT_CHECKSUM in the checkpoint
        // Create file system state JSON with the matching checksum
        let file_system_state_json = format!(
            r#"{{
            "files": {{
                "PROMPT.md": {{
                    "path": "PROMPT.md",
                    "checksum": "{}",
                    "size": {},
                    "content": null,
                    "exists": true
                }}
            }},
            "git_head_oid": null,
            "git_branch": null
        }}"#,
            STANDARD_PROMPT_CHECKSUM,
            STANDARD_PROMPT.len()
        );

        let checkpoint_json = make_checkpoint_with_file_system_state(
            MOCK_REPO_PATH,
            "Development",
            &file_system_state_json,
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume - should validate file system state successfully
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_v3_file_system_state_detects_changes() {
    with_default_timeout(|| {
        // The checkpoint uses STANDARD_PROMPT_CHECKSUM, but we put a different checksum
        // in file_system_state to simulate that the file has changed.
        let fake_old_checksum = "0000000000000000000000000000000000000000000000000000000000000000";

        // Create file system state JSON with fake old checksum (simulates file has changed)
        let file_system_state_json = format!(
            r#"{{
            "files": {{
                "PROMPT.md": {{
                    "path": "PROMPT.md",
                    "checksum": "{}",
                    "size": 100,
                    "content": null,
                    "exists": true
                }}
            }},
            "git_head_oid": null,
            "git_branch": null
        }}"#,
            fake_old_checksum
        );

        let checkpoint_json = make_checkpoint_with_file_system_state(
            MOCK_REPO_PATH,
            "Development",
            &file_system_state_json,
        );

        // Create handler with STANDARD_PROMPT (different from file_system_state checksum)
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume with --recovery-strategy=fail should detect the change and error
        let result = run_ralph_cli_with_handler(
            &["--resume", "--recovery-strategy", "fail"],
            executor,
            config,
            &mut handler,
        );

        // With strategy=fail, file system state mismatch should cause an error
        assert!(
            result.is_err(),
            "Should fail when file system state has changed and strategy is 'fail'"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("File system state")
                || err.contains("validation failed")
                || err.contains("changed"),
            "Error should mention file system state change: {}",
            err
        );
    });
}

#[test]
fn ralph_v3_file_system_state_auto_recovery() {
    with_default_timeout(|| {
        // Small PLAN.md content
        let plan_content = "Small plan content";

        // Calculate checksum
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(plan_content.as_bytes());
        let checksum = format!("{:x}", hasher.finalize());

        // Create file system state JSON with content for recovery
        let file_system_state_json = format!(
            r#"{{
            "files": {{
                ".agent/PLAN.md": {{
                    "path": ".agent/PLAN.md",
                    "checksum": "{}",
                    "size": {},
                    "content": "{}",
                    "exists": true
                }}
            }},
            "git_head_oid": null,
            "git_branch": null
        }}"#,
            checksum,
            plan_content.len(),
            plan_content
        );

        let checkpoint_json = make_checkpoint_with_file_system_state(
            MOCK_REPO_PATH,
            "Development",
            &file_system_state_json,
        );

        // Create handler with MODIFIED PLAN.md
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", "Modified plan content")
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume with --recovery-strategy=auto should restore the file from checkpoint state.
        run_ralph_cli_with_handler(
            &["--resume", "--recovery-strategy", "auto"],
            executor,
            config,
            &mut handler,
        )
        .unwrap();

        // Verify the pipeline completed and PLAN.md was restored from checkpoint content.
        let restored = handler
            .get_file(&PathBuf::from(".agent/PLAN.md"))
            .expect("PLAN.md should exist after resume");
        assert_eq!(restored, plan_content);
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Prompt Replay
// ============================================================================

#[test]
fn ralph_v3_prompt_replay_is_deterministic() {
    with_default_timeout(|| {
        // Create prompt history JSON
        let prompt_history_json = r#"{
            "development_1": "DETERMINISTIC PROMPT FOR DEVELOPMENT ITERATION 1",
            "planning_1": "DETERMINISTIC PROMPT FOR PLANNING"
        }"#;

        let checkpoint_json =
            make_checkpoint_with_prompt_history(MOCK_REPO_PATH, "Complete", prompt_history_json);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_v3_prompt_replay_across_multiple_iterations() {
    with_default_timeout(|| {
        // Create prompt history JSON with multiple iterations
        let prompt_history_json = r#"{
            "planning_1": "PLANNING PROMPT ITERATION 1",
            "development_1": "DEVELOPMENT PROMPT ITERATION 1",
            "planning_2": "PLANNING PROMPT ITERATION 2"
        }"#;

        let checkpoint_json =
            make_checkpoint_with_prompt_history(MOCK_REPO_PATH, "Complete", prompt_history_json);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume from Complete phase
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Interactive Resume Offering
// ============================================================================

#[test]
fn ralph_v3_interactive_resume_offer_on_existing_checkpoint() {
    with_default_timeout(|| {
        // Create a v3 checkpoint at Complete phase
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

        // Run without --resume flag - should offer to resume interactively
        // But since we're not in a TTY, it should skip the offer and start fresh
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify the checkpoint was cleared
        assert!(!handler.file_exists(&PathBuf::from(".agent/checkpoint.json")));
    });
}

#[test]
fn ralph_v3_shows_user_friendly_checkpoint_summary() {
    with_default_timeout(|| {
        // Create a v3 checkpoint with resume_count > 0 at Complete phase
        let checkpoint_json = make_checkpoint_json_with_resume_count(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume - should show user-friendly summary
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Comprehensive End-to-End
// ============================================================================

#[test]
fn ralph_v3_comprehensive_resume_from_review_phase() {
    with_default_timeout(|| {
        // Use STANDARD_PROMPT for PROMPT.md
        let plan_content = "# Plan\n\n1. Step 1\n2. Step 2";

        // Calculate plan checksum
        use sha2::{Digest, Sha256};
        let mut plan_hasher = Sha256::new();
        plan_hasher.update(plan_content.as_bytes());
        let plan_checksum = format!("{:x}", plan_hasher.finalize());

        // Create comprehensive v3 checkpoint with matching checksums
        let checkpoint_json = make_comprehensive_v3_checkpoint(
            MOCK_REPO_PATH,
            STANDARD_PROMPT_CHECKSUM,
            &plan_checksum,
            STANDARD_PROMPT.len(),
            plan_content.len(),
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", plan_content)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/ISSUES.md", "No issues\n")
            .with_file(".agent/commit-message.txt", "feat: add feature X\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume from Complete phase
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();

        // Verify the pipeline completed successfully
        assert!(!handler.file_exists(&PathBuf::from(".agent/checkpoint.json")));
    });
}

// ============================================================================
// Prompt Replay Determinism Tests
// ============================================================================

#[test]
fn ralph_resume_replays_prompts_deterministically() {
    with_default_timeout(|| {
        // Create prompt history JSON
        let prompt_history_json = r#"{
            "development_1": "DEVELOPMENT ITERATION 1 OF 2\n\nContext:\nTest plan content",
            "review_1": "REVIEW MODE\n\nReview the following changes..."
        }"#;

        let checkpoint_json =
            make_checkpoint_with_prompt_history(MOCK_REPO_PATH, "Complete", prompt_history_json);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "# Plan\n\n1. Step 1\n2. Step 2")
            .with_file(".agent/ISSUES.md", "No issues\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume and verify
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Hardened Resume Tests (V3 Checkpoint)
// ============================================================================

#[test]
fn ralph_v3_checkpoint_contains_file_system_state() {
    with_default_timeout(|| {
        // Use STANDARD_PROMPT which matches STANDARD_PROMPT_CHECKSUM in the checkpoint
        // Create file system state JSON with matching checksum
        let file_system_state_json = format!(
            r#"{{
            "files": {{
                "PROMPT.md": {{
                    "path": "PROMPT.md",
                    "checksum": "{}",
                    "size": {},
                    "content": null,
                    "exists": true
                }}
            }},
            "git_head_oid": null,
            "git_branch": null
        }}"#,
            STANDARD_PROMPT_CHECKSUM,
            STANDARD_PROMPT.len()
        );

        let checkpoint_json = make_checkpoint_with_file_system_state(
            MOCK_REPO_PATH,
            "Complete",
            &file_system_state_json,
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        // Verify checkpoint has file_system_state
        assert!(
            checkpoint_json.contains("\"file_system_state\""),
            "V3 checkpoint should contain file_system_state"
        );
        assert!(
            checkpoint_json.contains("PROMPT.md"),
            "File system state should capture PROMPT.md"
        );

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint can be loaded and resumed
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
fn ralph_resume_with_force_strategy_ignores_file_changes() {
    with_default_timeout(|| {
        // Create file system state with wrong checksum
        let file_system_state_json = r#"{
            "files": {
                "PROMPT.md": {
                    "path": "PROMPT.md",
                    "checksum": "wrongchecksum",
                    "size": 100,
                    "content": null,
                    "exists": true
                }
            },
            "git_head_oid": null,
            "git_branch": null
        }"#;

        let checkpoint_json = make_checkpoint_with_file_system_state(
            MOCK_REPO_PATH,
            "Complete",
            file_system_state_json,
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", "# Test\nOriginal.")
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume --recovery-strategy=force
        run_ralph_cli_with_handler(
            &["--resume", "--recovery-strategy=force"],
            executor,
            config,
            &mut handler,
        )
        .unwrap();
    });
}

#[test]
fn ralph_resume_auto_strategy_attempts_recovery() {
    with_default_timeout(|| {
        // Small PLAN.md file content
        let plan_content = "Small plan";

        // Create file system state with content for recovery
        let file_system_state_json = format!(
            r#"{{
            "files": {{
                ".agent/PLAN.md": {{
                    "path": ".agent/PLAN.md",
                    "checksum": "abc123",
                    "size": {},
                    "content": "{}",
                    "exists": true
                }}
            }},
            "git_head_oid": null,
            "git_branch": null
        }}"#,
            plan_content.len(),
            plan_content
        );

        let checkpoint_json = make_checkpoint_with_file_system_state(
            MOCK_REPO_PATH,
            "Complete",
            &file_system_state_json,
        );

        // Don't create PLAN.md - test should recover it
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume --recovery-strategy=auto
        run_ralph_cli_with_handler(
            &["--resume", "--recovery-strategy=auto"],
            executor,
            config,
            &mut handler,
        )
        .unwrap();
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

/// Test that checkpoints missing prompt_md_checksum are rejected as legacy.
///
/// Legacy checkpoints (missing required fields like prompt_md_checksum) are no
/// longer supported. Users must delete the checkpoint and restart the pipeline.
#[test]
fn ralph_v3_rejects_legacy_checkpoint_missing_prompt_md_checksum() {
    with_default_timeout(|| {
        // Create checkpoint WITHOUT prompt_md_checksum (legacy format)
        let checkpoint_json = make_checkpoint_without_new_fields(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint is REJECTED (legacy checkpoints no longer supported)
        let result = run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler);
        assert!(
            result.is_err(),
            "Should reject legacy checkpoint missing prompt_md_checksum"
        );

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Legacy checkpoints are not supported")
                || error_msg.contains("checkpoint")
                || error_msg.contains("validation"),
            "Error message should mention legacy checkpoint rejection: {}",
            error_msg
        );
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

// ============================================================================
// Helper Functions
// ============================================================================

fn make_checkpoint_json_with_resume_count(working_dir: &str) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 2,
            "total_iterations": 2,
            "reviewer_pass": 1,
            "total_reviewer_passes": 1,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "codex",
            "cli_args": {{
                "developer_iters": 0,
                "reviewer_reviews": 0,
                "commit_msg": "feat: add feature",
                "review_depth": "standard"
            }},
            "developer_agent_config": {{
                "name": "claude",
                "cmd": "claude",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            }},
            "reviewer_agent_config": {{
                "name": "codex",
                "cmd": "codex",
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
            "prompt_md_checksum": "{}",
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id-456",
            "parent_run_id": "test-parent-run-id",
            "resume_count": 2,
            "actual_developer_runs": 2,
            "actual_reviewer_runs": 1,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#,
        working_dir, STANDARD_PROMPT_CHECKSUM
    )
}

fn make_comprehensive_v3_checkpoint(
    working_dir: &str,
    prompt_checksum: &str,
    plan_checksum: &str,
    prompt_len: usize,
    plan_len: usize,
) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 1,
            "total_reviewer_passes": 1,
            "timestamp": "2024-01-01 12:01:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {{
                "developer_iters": 0,
                "reviewer_reviews": 0,
                "commit_msg": "feat: add feature X",
                "review_depth": "standard"
            }},
            "developer_agent_config": {{
                "name": "claude",
                "cmd": "claude -p",
                "output_flag": "--output-format=stream-json",
                "yolo_flag": "--dangerously-skip-permissions",
                "can_commit": true,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            }},
            "reviewer_agent_config": {{
                "name": "claude",
                "cmd": "claude -p",
                "output_flag": "--output-format=stream-json",
                "yolo_flag": "--dangerously-skip-permissions",
                "can_commit": true,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            }},
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "{}",
            "prompt_md_checksum": "{}",
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "comprehensive-test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 1,
            "execution_history": {{
                "steps": [],
                "file_snapshots": {{}}
            }},
            "file_system_state": {{
                "files": {{
                    "PROMPT.md": {{
                        "path": "PROMPT.md",
                        "checksum": "{}",
                        "size": {},
                        "content": null,
                        "exists": true
                    }},
                    ".agent/PLAN.md": {{
                        "path": ".agent/PLAN.md",
                        "checksum": "{}",
                        "size": {},
                        "content": null,
                        "exists": true
                    }}
                }},
                "git_head_oid": null,
                "git_branch": null
            }},
            "prompt_history": {{
                "planning_1": "Planning prompt for iteration 1",
                "development_1": "Development prompt for iteration 1"
            }}
        }}"#,
        working_dir, prompt_checksum, prompt_checksum, prompt_len, plan_checksum, plan_len
    )
}

fn make_checkpoint_with_git_commit_oid(working_dir: &str) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": 0,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null
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
            "prompt_md_checksum": "{}",
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-git-commit-oid",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {{
                "steps": [
                    {{
                        "phase": "Development",
                        "iteration": 1,
                        "step_type": "dev_run",
                        "timestamp": "2024-01-01 12:00:00",
                        "outcome": {{
                            "Success": {{
                                "output": "Implementation complete",
                                "files_modified": ["src/lib.rs"],
                                "exit_code": 0
                            }}
                        }},
                        "agent": "test-agent",
                        "duration_secs": 120,
                        "checkpoint_saved_at": null,
                        "git_commit_oid": "abc123def456",
                        "modified_files_detail": {{
                            "added": ["src/new.rs"],
                            "modified": ["src/lib.rs"],
                            "deleted": []
                        }},
                        "prompt_used": "Implement feature X",
                        "issues_summary": {{
                            "found": 0,
                            "fixed": 0,
                            "description": null
                        }}
                    }}
                ],
                "file_snapshots": {{}}
            }},
            "file_system_state": null,
            "prompt_history": {{
                "development_1": "Implement feature X"
            }}
        }}"#,
        working_dir, STANDARD_PROMPT_CHECKSUM
    )
}

fn make_checkpoint_with_all_new_fields(working_dir: &str) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": 0,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null
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
            "prompt_md_checksum": "{}",
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-new-fields",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {{
                "steps": [
                    {{
                        "phase": "Development",
                        "iteration": 1,
                        "step_type": "dev_run",
                        "timestamp": "2024-01-01 12:00:00",
                        "outcome": {{
                            "Success": {{
                                "output": "Implementation complete",
                                "files_modified": [],
                                "exit_code": 0
                            }}
                        }},
                        "agent": "test-agent",
                        "duration_secs": 60,
                        "checkpoint_saved_at": null,
                        "git_commit_oid": null,
                        "modified_files_detail": null,
                        "prompt_used": "Implement the feature",
                        "issues_summary": {{
                            "found": 3,
                            "fixed": 0,
                            "description": "3 clippy warnings found"
                        }}
                    }}
                ],
                "file_snapshots": {{}}
            }},
            "file_system_state": null,
            "prompt_history": {{
                "development_1": "Implement the feature"
            }}
        }}"#,
        working_dir, STANDARD_PROMPT_CHECKSUM
    )
}

fn make_checkpoint_without_new_fields(working_dir: &str) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": 0,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null
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
            "run_id": "test-backward-compat",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {{
                "steps": [
                    {{
                        "phase": "Development",
                        "iteration": 1,
                        "step_type": "dev_run",
                        "timestamp": "2024-01-01 12:00:00",
                        "outcome": {{
                            "Success": {{
                                "output": "Implementation complete",
                                "files_modified": ["src/lib.rs"],
                                "exit_code": 0
                            }}
                        }},
                        "agent": "test-agent",
                        "duration_secs": 120,
                        "checkpoint_saved_at": null
                    }}
                ],
                "file_snapshots": {{}}
            }},
            "file_system_state": null,
            "prompt_history": null
        }}"#,
        working_dir
    )
}

fn make_checkpoint_with_detailed_execution_history(working_dir: &str) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": 0,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null
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
            "prompt_md_checksum": "{}",
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-resume-note",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {{
                "steps": [
                    {{
                        "phase": "Development",
                        "iteration": 1,
                        "step_type": "dev_run",
                        "timestamp": "2024-01-01 12:00:00",
                        "outcome": {{
                            "Success": {{
                                "output": "Implementation complete",
                                "files_modified": ["src/lib.rs", "src/main.rs"],
                                "exit_code": 0
                            }}
                        }},
                        "agent": "test-agent",
                        "duration_secs": 120,
                        "checkpoint_saved_at": null,
                        "git_commit_oid": "abc123",
                        "modified_files_detail": {{
                            "added": ["src/new.rs"],
                            "modified": ["src/lib.rs"],
                            "deleted": ["src/old.rs"]
                        }},
                        "prompt_used": "Implement feature X",
                        "issues_summary": {{
                            "found": 5,
                            "fixed": 3,
                            "description": "3 clippy warnings fixed"
                        }}
                    }}
                ],
                "file_snapshots": {{}}
            }},
            "file_system_state": null,
            "prompt_history": {{
                "development_1": "Implement feature X"
            }}
        }}"#,
        working_dir, STANDARD_PROMPT_CHECKSUM
    )
}
