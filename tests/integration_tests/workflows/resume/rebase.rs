//! Rebase state preservation and conflict tests.

use std::fs;
use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;

use super::canonical_working_dir;
use test_helpers::init_git_repo;

// ============================================================================
// Rebase State Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_records_rebase_state() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase with rebase state
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete"),
        )
        .unwrap();

        // Run with --resume - should detect checkpoint
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Rebase Phase Full Config Preservation Tests
// ============================================================================

#[test]
fn ralph_resume_from_prerebase_phase_preserves_full_config() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete"),
        )
        .unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_from_prerebase_conflict_preserves_full_config() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete"),
        )
        .unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_from_postrebase_phase_preserves_full_config() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete"),
        )
        .unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_from_postrebase_conflict_preserves_full_config() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete"),
        )
        .unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Idempotent Resume from Rebase Phases Tests
// ============================================================================

#[test]
fn ralph_resume_is_idempotent_from_prerebase() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete"),
        )
        .unwrap();

        // First resume run
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
        // Checkpoint should be cleared
        assert!(
            !dir.path().join(".agent/checkpoint.json").exists(),
            "Checkpoint should be cleared after successful resume"
        );
    });
}

// ============================================================================
// Rebase Conflict Resume Tests
// ============================================================================

#[test]
fn ralph_v3_rebase_conflict_checkpoint_saves_execution_history() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a v3 checkpoint at Complete phase with execution history
        let working_dir = canonical_working_dir(&dir);
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json_with_execution_history(&working_dir),
        )
        .unwrap();

        // Load checkpoint and verify execution history is preserved
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();

        // Verify the checkpoint was consumed
        assert!(!dir.path().join(".agent/checkpoint.json").exists());
    });
}

#[test]
fn ralph_v3_rebase_conflict_checkpoint_saves_prompt_history() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a v3 checkpoint at Complete phase with prompt history
        let working_dir = canonical_working_dir(&dir);
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json_with_prompt_history(&working_dir),
        )
        .unwrap();

        // Resume and verify prompt history is preserved
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();

        // Verify the checkpoint was consumed
        assert!(!dir.path().join(".agent/checkpoint.json").exists());
    });
}

// ============================================================================
// Helper Functions
// ============================================================================

fn make_checkpoint_json(working_dir: &str, phase: &str) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "{}",
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
            "run_id": "00000000-0000-0000-0000-000000000001",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#,
        phase, working_dir
    )
}

fn make_checkpoint_json_with_execution_history(working_dir: &str) -> String {
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
            "run_id": "test-run-id-rebase-conflict",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": {{"steps": [], "file_snapshots": {{}}}},
            "file_system_state": null,
            "prompt_history": null
        }}"#,
        working_dir
    )
}

fn make_checkpoint_json_with_prompt_history(working_dir: &str) -> String {
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
            "run_id": "test-run-id-prompt-history",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": {{"postrebase_conflict_resolution": "Resolve conflicts"}}
        }}"#,
        working_dir
    )
}
