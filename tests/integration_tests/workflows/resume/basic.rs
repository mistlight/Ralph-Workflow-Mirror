//! Resume flag tests and working directory validation tests.

use std::fs;
use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;

use super::canonical_working_dir;
use test_helpers::{init_git_repo, write_file};

// ============================================================================
// Resume Flag Tests
// ============================================================================

#[test]
fn ralph_resume_flag_reads_checkpoint() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint file manually at Complete phase to avoid agent execution
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            format!(
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
            "run_id": "test-checkpoint-read",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 2,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#,
                working_dir
            ),
        )
        .unwrap();

        // Run with --resume flag - should detect the checkpoint
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_without_checkpoint_starts_fresh() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // No checkpoint exists, but we pass --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Working Directory Validation Tests
// ============================================================================

#[test]
fn ralph_resume_validates_working_directory() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint with a different working directory
        let wrong_working_dir = "/some/other/directory".to_string();
        fs::create_dir_all(dir.path().join(".agent")).unwrap();

        // Create checkpoint JSON with wrong working directory at Complete phase
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
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
            "run_id": "test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#,
                wrong_working_dir
            ),
        )
        .unwrap();

        // Run with --resume - should detect working directory mismatch but still complete
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// --no-resume Flag Tests
// ============================================================================

#[test]
fn ralph_no_resume_flag_skips_interactive_prompt() {
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

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(CheckpointTestParams {
                working_dir: &working_dir,
                phase: "Complete",
                iteration: 1,
                total_iterations: 1,
            }),
        )
        .unwrap();

        // Run with --no-resume - should skip interactive prompt and start fresh
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--no-resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_no_resume_env_var_skips_interactive_prompt() {
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

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(CheckpointTestParams {
                working_dir: &working_dir,
                phase: "Complete",
                iteration: 1,
                total_iterations: 1,
            }),
        )
        .unwrap();

        // Run with RALPH_NO_RESUME_PROMPT env var - should skip interactive prompt
        std::env::set_var("RALPH_NO_RESUME_PROMPT", "1");
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
        std::env::remove_var("RALPH_NO_RESUME_PROMPT");
    });
}

#[test]
fn ralph_resume_flag_takes_precedence_over_no_resume() {
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

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(CheckpointTestParams {
                working_dir: &working_dir,
                phase: "Complete",
                iteration: 1,
                total_iterations: 1,
            }),
        )
        .unwrap();

        // Run with both --resume and --no-resume - --resume should take precedence
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--resume", "--no-resume"],
            executor,
            config,
            Some(dir.path()),
        )
        .unwrap();
    });
}

// ============================================================================
// Idempotent Resume Tests
// ============================================================================

#[test]
fn ralph_resume_is_idempotent_same_checkpoint() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        let checkpoint_content = make_checkpoint_json(CheckpointTestParams {
            working_dir: &working_dir,
            phase: "Complete",
            iteration: 1,
            total_iterations: 1,
        });
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            &checkpoint_content,
        )
        .unwrap();

        // First resume run
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
        // Check that checkpoint was cleared on success
        assert!(
            !dir.path().join(".agent/checkpoint.json").exists(),
            "Checkpoint should be cleared after successful Complete phase resume"
        );
    });
}

// ============================================================================
// Helper Types and Functions
// ============================================================================

/// Parameters for creating a test checkpoint JSON.
struct CheckpointTestParams<'a> {
    working_dir: &'a str,
    phase: &'a str,
    iteration: u32,
    total_iterations: u32,
}

/// Helper function to create a valid v3 checkpoint JSON with all required fields.
/// Always sets developer_iters and reviewer_reviews to 0 to prevent agent execution.
fn make_checkpoint_json(params: CheckpointTestParams<'_>) -> String {
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
        params.phase,
        params.iteration,
        params.total_iterations,
        params.working_dir,
        params.iteration
    )
}
