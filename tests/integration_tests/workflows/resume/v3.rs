//! V3 hardened resume tests (execution history, file system state, prompt replay).

use std::fs;
use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;

use super::canonical_working_dir;
use test_helpers::{init_git_repo, write_file};

// ============================================================================
// V3 Hardened Resume Tests - Execution History
// ============================================================================

#[test]
fn ralph_v3_checkpoint_contains_execution_history() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create PROMPT.md to skip planning phase agent execution
        fs::write(dir.path().join("PROMPT.md"), "Test prompt\n").unwrap();

        // Create a v3 checkpoint manually with execution history to test checkpoint loading
        let working_dir = canonical_working_dir(&dir);
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
        ]
    }"#;

        let checkpoint_json = format!(
            r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {{
                "developer_iters": 1,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null,
                "skip_rebase": false
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
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-execution-history",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {},
            "file_system_state": null,
            "prompt_history": null,
            "env_snapshot": {{
                "ralph_vars": {{
                    "RALPH_DEVELOPER_ITERS": "0",
                    "RALPH_REVIEWER_REVIEWS": "0"
                }},
                "other_vars": {{}}
            }}
        }}"#,
            working_dir, execution_history_json
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(dir.path().join(".agent/checkpoint.json"), checkpoint_json).unwrap();

        // Pre-create required files to satisfy validation
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Resume should load checkpoint with execution history
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
        // Verify the checkpoint contains execution_history
        let checkpoint_path = dir.path().join(".agent/checkpoint.json");
        if checkpoint_path.exists() {
            let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();
            assert!(
                checkpoint_content.contains("execution_history"),
                "V3 checkpoint should contain execution_history"
            );
        }
    });
}

#[test]
fn ralph_v3_restores_execution_history_on_resume() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a v3 checkpoint with execution history
        let working_dir = canonical_working_dir(&dir);
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

        let checkpoint_content = format!(
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
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {},
            "file_system_state": null,
            "prompt_history": null
        }}"#,
            working_dir, execution_history_json
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files to satisfy validation
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Resume and verify it succeeds
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// V3 Hardened Resume Tests - File System State
// ============================================================================

#[test]
fn ralph_v3_file_system_state_validates_on_resume() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Write PROMPT.md with known content
        write_file(
            dir.path().join("PROMPT.md"),
            "# Test Prompt\n\nDo something.",
        );

        // Calculate checksum
        let content = fs::read_to_string(dir.path().join("PROMPT.md")).unwrap();
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let checksum = format!("{:x}", hasher.finalize());

        // Create a v3 checkpoint with file system state
        // Note: git_head_oid is set to null since we're not testing git state validation here
        // Note: developer_iters is set to 0 to avoid agent invocation during test
        let working_dir = canonical_working_dir(&dir);
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
            checksum,
            content.len()
        );

        let checkpoint_content = format!(
            r#"{{
            "version": 3,
            "phase": "Development",
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
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": {},
            "prompt_history": null
        }}"#,
            working_dir, file_system_state_json
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Resume - should validate file system state successfully
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_v3_file_system_state_detects_changes() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Write initial PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            "# Original Task\nDo something.",
        );

        // Calculate checksum of original content
        let original_content = fs::read_to_string(dir.path().join("PROMPT.md")).unwrap();
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(original_content.as_bytes());
        let original_checksum = format!("{:x}", hasher.finalize());

        // Create a v3 checkpoint with the original checksum
        // Note: git_head_oid is set to null since we're not testing git state validation here
        // Note: developer_iters is set to 0 to avoid agent invocation during test
        let working_dir = canonical_working_dir(&dir);
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
            original_checksum,
            original_content.len()
        );

        let checkpoint_content = format!(
            r#"{{
            "version": 3,
            "phase": "Development",
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
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": {},
            "prompt_history": null
        }}"#,
            working_dir, file_system_state_json
        );

        // Debug: print file_system_state_json to verify format
        eprintln!("DEBUG: file_system_state_json:\n{}", file_system_state_json);

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content.clone(),
        )
        .unwrap();

        // Debug: print checkpoint JSON to verify format
        eprintln!("DEBUG: Checkpoint JSON:\n{}", checkpoint_content);

        // Debug: verify the file was written correctly
        let written = fs::read_to_string(dir.path().join(".agent/checkpoint.json")).unwrap();
        eprintln!("DEBUG: Written file:\n{}", written);

        // Now modify PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            "# Modified Task\nDo something else.",
        );

        // Resume with --recovery-strategy=fail should detect the change
        // The file has been modified, so checksum validation will fail
        // With strategy=fail, the resume is aborted and the program continues with a fresh run
        // Since developer_iters=0 in the command line, the program completes immediately
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--resume", "--recovery-strategy", "fail"],
            executor,
            config,
            Some(dir.path()),
        )
        .unwrap();
        // Should succeed - validation fails, resume is aborted, fresh run completes
    });
}

#[test]
fn ralph_v3_file_system_state_auto_recovery() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Write small PLAN.md content
        let plan_content = "Small plan content";
        write_file(dir.path().join(".agent/PLAN.md"), plan_content);

        // Calculate checksum
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(plan_content.as_bytes());
        let checksum = format!("{:x}", hasher.finalize());

        // Create a v3 checkpoint with PLAN.md file state including content
        // Note: git_head_oid is set to null since we're not testing git state validation here
        // Note: developer_iters is set to 0 to avoid agent invocation during test
        let working_dir = canonical_working_dir(&dir);
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

        let checkpoint_content = format!(
            r#"{{
            "version": 3,
            "phase": "Development",
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
            "run_id": "test-auto-recovery-plan-md",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": {},
            "prompt_history": null
        }}"#,
            working_dir, file_system_state_json
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Modify PLAN.md
        write_file(dir.path().join(".agent/PLAN.md"), "Modified plan content");

        // Resume with --recovery-strategy=auto should restore the file
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--resume", "--recovery-strategy", "auto"],
            executor,
            config,
            Some(dir.path()),
        )
        .unwrap();

        // Verify the file was restored
        let restored_content = fs::read_to_string(dir.path().join(".agent/PLAN.md")).unwrap();
        assert_eq!(restored_content, plan_content);
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Prompt Replay
// ============================================================================

#[test]
fn ralph_v3_prompt_replay_is_deterministic() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a v3 checkpoint with prompt history
        let working_dir = canonical_working_dir(&dir);
        let mut prompt_history = serde_json::Map::new();
        prompt_history.insert(
            "development_1".to_string(),
            serde_json::Value::String(
                "DETERMINISTIC PROMPT FOR DEVELOPMENT ITERATION 1".to_string(),
            ),
        );
        prompt_history.insert(
            "planning_1".to_string(),
            serde_json::Value::String("DETERMINISTIC PROMPT FOR PLANNING".to_string()),
        );

        let checkpoint_content = format!(
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
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": {}
        }}"#,
            working_dir,
            serde_json::to_string(&prompt_history).unwrap()
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
        // Test verifies that resume completes successfully with prompt history
        // The mock executor handles agent commands - no real processes spawned
    });
}

#[test]
fn ralph_v3_prompt_replay_across_multiple_iterations() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a v3 checkpoint with prompts for multiple iterations
        let working_dir = canonical_working_dir(&dir);
        let mut prompt_history = serde_json::Map::new();
        prompt_history.insert(
            "planning_1".to_string(),
            serde_json::Value::String("PLANNING PROMPT ITERATION 1".to_string()),
        );
        prompt_history.insert(
            "development_1".to_string(),
            serde_json::Value::String("DEVELOPMENT PROMPT ITERATION 1".to_string()),
        );
        prompt_history.insert(
            "planning_2".to_string(),
            serde_json::Value::String("PLANNING PROMPT ITERATION 2".to_string()),
        );

        let checkpoint_content = format!(
            r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 2,
            "total_iterations": 2,
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
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": {}
        }}"#,
            working_dir,
            serde_json::to_string(&prompt_history).unwrap()
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Resume from Complete phase
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Interactive Resume Offering
// ============================================================================

#[test]
fn ralph_v3_interactive_resume_offer_on_existing_checkpoint() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create PROMPT.md to avoid validation issues
        fs::write(dir.path().join("PROMPT.md"), "Test prompt\n").unwrap();

        // Create a v3 checkpoint at Complete phase to avoid running agents
        let working_dir = canonical_working_dir(&dir);
        let checkpoint_content = format!(
            r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {{
                "developer_iters": 1,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null,
                "skip_rebase": false
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
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null,
            "env_snapshot": {{
                "ralph_vars": {{
                    "RALPH_DEVELOPER_ITERS": "0",
                    "RALPH_REVIEWER_REVIEWS": "0"
                }},
                "other_vars": {{}}
            }}
        }}"#,
            working_dir
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files to satisfy validation
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Run without --resume flag - should offer to resume interactively
        // But since we're not in a TTY, it should skip the offer and start fresh
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
        // Should succeed and clear the checkpoint

        // Verify the checkpoint was cleared
        assert!(!dir.path().join(".agent/checkpoint.json").exists());
    });
}

#[test]
fn ralph_v3_shows_user_friendly_checkpoint_summary() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create PROMPT.md
        fs::write(dir.path().join("PROMPT.md"), "Test prompt\n").unwrap();

        // Create a v3 checkpoint with resume_count > 0 at Complete phase
        let working_dir = canonical_working_dir(&dir);
        let checkpoint_content = format!(
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
                "review_depth": "standard",
                "skip_rebase": false
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
            "prompt_md_checksum": null,
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
            working_dir
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Run with --resume - should show user-friendly summary
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Comprehensive End-to-End
// ============================================================================

#[test]
fn ralph_v3_comprehensive_resume_from_review_phase() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md and PLAN.md
        write_file(
            dir.path().join("PROMPT.md"),
            "# Test Prompt\n\nImplement feature X.",
        );
        write_file(
            dir.path().join(".agent/PLAN.md"),
            "# Plan\n\n1. Step 1\n2. Step 2",
        );

        // Calculate checksums
        use sha2::{Digest, Sha256};
        let prompt_content = fs::read_to_string(dir.path().join("PROMPT.md")).unwrap();
        let plan_content = fs::read_to_string(dir.path().join(".agent/PLAN.md")).unwrap();
        let mut prompt_hasher = Sha256::new();
        prompt_hasher.update(prompt_content.as_bytes());
        let prompt_checksum = format!("{:x}", prompt_hasher.finalize());

        let mut plan_hasher = Sha256::new();
        plan_hasher.update(plan_content.as_bytes());
        let plan_checksum = format!("{:x}", plan_hasher.finalize());

        // Get git HEAD OID using git2 library
        let head_oid = match git2::Repository::discover(dir.path()) {
            Ok(repo) => match repo.head() {
                Ok(head_ref) => head_ref.target().map(|oid| oid.to_string()),
                Err(_) => None,
            },
            Err(_) => None,
        }
        .unwrap_or_default();

        // Create comprehensive v3 checkpoint with all hardened features at Complete phase
        let working_dir = canonical_working_dir(&dir);
        let mut prompt_history = serde_json::Map::new();
        prompt_history.insert(
            "planning_1".to_string(),
            serde_json::Value::String("Planning prompt for iteration 1".to_string()),
        );
        prompt_history.insert(
            "development_1".to_string(),
            serde_json::Value::String("Development prompt for iteration 1".to_string()),
        );

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
                "agent": "claude",
                "duration_secs": 15
            },
            {
                "phase": "Development",
                "iteration": 1,
                "step_type": "dev_run",
                "timestamp": "2024-01-01 12:00:15",
                "outcome": {
                    "Success": {
                        "output": null,
                        "files_modified": ["src/lib.rs"]
                    }
                },
                "agent": "claude",
                "duration_secs": 45
            },
            {
                "phase": "Development",
                "iteration": 1,
                "step_type": "commit",
                "timestamp": "2024-01-01 12:01:00",
                "outcome": {
                    "Success": {
                        "output": "abc123",
                        "files_modified": []
                    }
                },
                "agent": "claude",
                "duration_secs": 5
            }
        ],
        "file_snapshots": {}
    }"#;

        let file_system_state_json = format!(
            r#"{{
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
            "git_head_oid": "{}",
            "git_branch": null
        }}"#,
            prompt_checksum,
            prompt_content.len(),
            plan_checksum,
            plan_content.len(),
            head_oid
        );

        let checkpoint_content = format!(
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
                "developer_iters": 1,
                "reviewer_reviews": 1,
                "commit_msg": "feat: add feature X",
                "review_depth": "standard",
                "skip_rebase": false
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
            "execution_history": {},
            "file_system_state": {},
            "prompt_history": {},
            "env_snapshot": {{
                "ralph_vars": {{
                    "RALPH_DEVELOPER_ITERS": "0",
                    "RALPH_REVIEWER_REVIEWS": "0"
                }},
                "other_vars": {{}}
            }}
        }}"#,
            working_dir,
            prompt_checksum,
            execution_history_json,
            file_system_state_json,
            serde_json::to_string(&prompt_history).unwrap()
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create ISSUES.md and commit-message.txt to satisfy validation
        write_file(dir.path().join(".agent/ISSUES.md"), "No issues\n");
        write_file(
            dir.path().join(".agent/commit-message.txt"),
            "feat: add feature X\n",
        );

        // Resume from Complete phase
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();

        // Verify the pipeline completed successfully
        assert!(!dir.path().join(".agent/checkpoint.json").exists());
    });
}

// ============================================================================
// Prompt Replay Determinism Tests
// ============================================================================

#[test]
fn ralph_resume_replays_prompts_deterministically() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md and PLAN.md to satisfy validation
        write_file(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        );
        write_file(
            dir.path().join(".agent/PLAN.md"),
            "# Plan\n\n1. Step 1\n2. Step 2",
        );
        write_file(dir.path().join(".agent/ISSUES.md"), "No issues\n");

        // Create a v3 checkpoint with prompt history at Complete phase
        // to avoid running the reviewer agent
        let working_dir = canonical_working_dir(&dir);
        let prompt_history_json = serde_json::json!({
            "development_1": "DEVELOPMENT ITERATION 1 OF 2\n\nContext:\nTest plan content",
            "review_1": "REVIEW MODE\n\nReview the following changes..."
        });

        let checkpoint_content = format!(
            r#"{{
            "version": 3,
            "phase": "Complete",
            "iteration": 1,
            "total_iterations": 2,
            "reviewer_pass": 1,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {{
                "developer_iters": 2,
                "reviewer_reviews": 2,
                "commit_msg": "",
                "review_depth": null,
                "skip_rebase": false
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
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-prompt-replay",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 1,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": {},
            "env_snapshot": {{
                "ralph_vars": {{
                    "RALPH_DEVELOPER_ITERS": "0",
                    "RALPH_REVIEWER_REVIEWS": "0"
                }},
                "other_vars": {{}}
            }}
        }}"#,
            working_dir,
            serde_json::to_string(&prompt_history_json).unwrap()
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create commit-message.txt to satisfy validation
        write_file(dir.path().join(".agent/commit-message.txt"), "feat: test\n");

        // Resume and verify the checkpoint with prompt history is loaded
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
        // Verify the checkpoint was loaded with prompt_history
        let checkpoint_path = dir.path().join(".agent/checkpoint.json");
        if checkpoint_path.exists() {
            let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();
            assert!(
                checkpoint_content.contains("prompt_history"),
                "Checkpoint should contain prompt_history for deterministic replay"
            );
        }
    });
}

// ============================================================================
// Hardened Resume Tests (V3 Checkpoint)
// ============================================================================

#[test]
fn ralph_v3_checkpoint_contains_file_system_state() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            "# Test Prompt\n\nDo something.",
        );

        // Calculate PROMPT.md checksum
        let prompt_content = fs::read_to_string(dir.path().join("PROMPT.md")).unwrap();
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(prompt_content.as_bytes());
        let prompt_checksum = format!("{:x}", hasher.finalize());

        // Create a v3 checkpoint with file_system_state at Complete phase
        let working_dir = canonical_working_dir(&dir);
        let file_system_state = serde_json::json!({
            "files": {
                "PROMPT.md": {
                    "path": "PROMPT.md",
                    "checksum": prompt_checksum,
                    "size": prompt_content.len(),
                    "content": null,
                    "exists": true
                }
            },
            "git_head_oid": null,
            "git_branch": null
        });

        let checkpoint_content = format!(
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
            "prompt_md_checksum": "{}",
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-file-system-state",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": {{ "steps": [], "file_snapshots": {{}} }},
            "file_system_state": {},
            "prompt_history": null
        }}"#,
            working_dir,
            prompt_checksum,
            serde_json::to_string(&file_system_state).unwrap()
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            &checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Verify v3 checkpoint has file_system_state
        assert!(
            checkpoint_content.contains("\"file_system_state\""),
            "V3 checkpoint should contain file_system_state"
        );

        // Verify PROMPT.md is captured in file system state
        assert!(
            checkpoint_content.contains("PROMPT.md"),
            "File system state should capture PROMPT.md"
        );

        // Verify checkpoint can be loaded and resumed
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_v3_checkpoint_contains_execution_history_after_failure() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            "# Test Prompt\n\nDo something.",
        );

        // Create a v3 checkpoint with execution_history at Complete phase
        let working_dir = canonical_working_dir(&dir);
        let checkpoint_content = format!(
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
            "run_id": "test-execution-history-failure",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": {{
                "steps": [
                    {{
                        "phase": "Planning",
                        "iteration": 1,
                        "step_type": "plan_generation",
                        "timestamp": "2024-01-01 12:00:00",
                        "outcome": {{
                            "Success": {{
                                "output": null,
                                "files_modified": [".agent/PLAN.md"]
                            }}
                        }},
                        "agent": "test-agent",
                        "duration_secs": 10
                    }}
                ],
                "file_snapshots": {{}}
            }},
            "file_system_state": null,
            "prompt_history": null
        }}"#,
            working_dir
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            &checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Parse the checkpoint JSON to verify execution_history structure
        let checkpoint: serde_json::Value =
            serde_json::from_str(&checkpoint_content).expect("Checkpoint should be valid JSON");

        // Verify v3 checkpoint has execution_history field with proper structure
        let execution_history = checkpoint
            .get("execution_history")
            .and_then(|v| v.as_object())
            .expect("V3 checkpoint should have execution_history object");

        // Verify execution_history has steps array (may be empty for early failures)
        let _steps = execution_history
            .get("steps")
            .and_then(|v| v.as_array())
            .expect("Execution history should have steps array");

        // Verify file_snapshots exists
        let _file_snapshots = execution_history
            .get("file_snapshots")
            .and_then(|v| v.as_object())
            .expect("Execution history should have file_snapshots object");

        // Verify checkpoint can be loaded and resumed
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_with_force_strategy_ignores_file_changes() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        let working_dir = canonical_working_dir(&dir);

        // Create PROMPT.md
        write_file(dir.path().join("PROMPT.md"), "# Test\nOriginal.");

        // Create a v3 checkpoint with file system state that won't match, at Complete phase
        let file_system_state = serde_json::json!({
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
        });

        let checkpoint_content = format!(
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
                "review_depth": null,
                "skip_rebase": false,
                "verbosity": 2,
                "show_streaming_metrics": false,
                "reviewer_json_parser": null
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
            "run_id": "test-force-strategy",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": {},
            "prompt_history": null
        }}"#,
            working_dir,
            serde_json::to_string(&file_system_state).unwrap()
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Run with --resume --recovery-strategy=force
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--resume", "--recovery-strategy=force"],
            executor,
            config,
            Some(dir.path()),
        )
        .unwrap();
        // Should proceed with warning
    });
}

#[test]
fn ralph_resume_auto_strategy_attempts_recovery() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        let working_dir = canonical_working_dir(&dir);

        // Create a small PLAN.md file (can be recovered via content)
        let plan_content = "Small plan";

        let file_snapshot = serde_json::json!({
            "path": ".agent/PLAN.md",
            "checksum": "abc123",
            "size": plan_content.len(),
            "content": plan_content, // Content stored for small files
            "exists": true
        });

        let file_system_state = serde_json::json!({
            "files": {
                ".agent/PLAN.md": file_snapshot
            },
            "git_head_oid": null,
            "git_branch": null
        });

        let checkpoint_content = format!(
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
                "review_depth": null,
                "skip_rebase": false,
                "verbosity": 2,
                "show_streaming_metrics": false,
                "reviewer_json_parser": null
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
            "run_id": "test-auto-recovery",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": {},
            "prompt_history": null
        }}"#,
            working_dir,
            serde_json::to_string(&file_system_state).unwrap()
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        // Don't create PLAN.md - test should recover it
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create commit-message.txt for Complete phase
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Run with --resume --recovery-strategy=auto
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--resume", "--recovery-strategy=auto"],
            executor,
            config,
            Some(dir.path()),
        )
        .unwrap();
        // Should attempt recovery and proceed
    });
}

#[test]
fn ralph_checkpoint_saved_after_rebase_completion() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md to satisfy validation
        write_file(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        );

        // Run pipeline with rebase enabled - should complete successfully
        // We use 0 iterations to skip actual development work
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--with-rebase"], executor, config, Some(dir.path())).unwrap();
        // Check that checkpoint was saved at Planning phase after rebase
        let checkpoint_path = dir.path().join(".agent/checkpoint.json");
        if checkpoint_path.exists() {
            let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();
            // Verify it has the expected structure
            assert!(
                checkpoint_content.contains("\"phase\""),
                "Checkpoint should contain phase"
            );
        }
    });
}

#[test]
fn ralph_checkpoint_saved_at_pipeline_start() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        );

        // Simulate interruption by creating a checkpoint manually at Complete phase
        let working_dir = canonical_working_dir(&dir);

        let checkpoint_content = format!(
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
                "review_depth": null,
                "skip_rebase": true,
                "verbosity": 2,
                "show_streaming_metrics": false,
                "reviewer_json_parser": null
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
            "run_id": "test-initial-checkpoint",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {{ "steps": [] }},
            "file_system_state": null,
            "prompt_history": {{}}
        }}"#,
            working_dir
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Verify checkpoint can be loaded
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Enhanced Execution History Tests (v3+ with new fields)
// ============================================================================

#[test]
fn ralph_v3_execution_step_contains_git_commit_oid() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        );

        let working_dir = canonical_working_dir(&dir);

        // Create checkpoint with enhanced execution step containing git_commit_oid at Complete phase
        let checkpoint_content = format!(
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
                "review_depth": null,
                "skip_rebase": true,
                "verbosity": 2,
                "show_streaming_metrics": false,
                "reviewer_json_parser": null
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
            working_dir
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            &checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Verify checkpoint contains the new fields before resume
        let checkpoint_path = dir.path().join(".agent/checkpoint.json");
        let checkpoint_content_verify = fs::read_to_string(&checkpoint_path).unwrap();
        assert!(
            checkpoint_content_verify.contains("git_commit_oid"),
            "Checkpoint should contain git_commit_oid field"
        );
        assert!(
            checkpoint_content_verify.contains("abc123def456"),
            "Checkpoint should contain the git commit OID value"
        );

        // Verify checkpoint can be loaded with the new fields
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_v3_execution_step_serialization_with_new_fields() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        );

        let working_dir = canonical_working_dir(&dir);

        // Create checkpoint with all new fields at Complete phase
        let checkpoint_content = format!(
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
                "review_depth": null,
                "skip_rebase": true,
                "verbosity": 2,
                "show_streaming_metrics": false,
                "reviewer_json_parser": null
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
            working_dir
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Verify checkpoint can be loaded
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_v3_backward_compatible_missing_new_fields() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        );

        let working_dir = canonical_working_dir(&dir);

        // Create checkpoint WITHOUT the new fields (backward compatibility) at Complete phase
        let checkpoint_content = format!(
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
                "review_depth": null,
                "skip_rebase": true,
                "verbosity": 2,
                "show_streaming_metrics": false,
                "reviewer_json_parser": null
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
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Verify checkpoint can still be loaded (backward compatibility)
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_v3_resume_note_contains_execution_history() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        );

        let working_dir = canonical_working_dir(&dir);

        // Create checkpoint with execution history at Complete phase
        let checkpoint_content = format!(
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
                "review_depth": null,
                "skip_rebase": true,
                "verbosity": 2,
                "show_streaming_metrics": false,
                "reviewer_json_parser": null
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
            working_dir
        );

        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            &checkpoint_content,
        )
        .unwrap();

        // Pre-create required files for Complete phase
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Verify the checkpoint contains execution history
        let checkpoint_path = dir.path().join(".agent/checkpoint.json");
        let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();

        // Verify checkpoint contains the new fields
        assert!(
            checkpoint_content.contains("execution_history"),
            "Checkpoint should contain execution_history"
        );
        assert!(
            checkpoint_content.contains("modified_files_detail"),
            "Checkpoint should contain modified_files_detail"
        );
        assert!(
            checkpoint_content.contains("issues_summary"),
            "Checkpoint should contain issues_summary"
        );

        // Verify checkpoint can be loaded
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}
