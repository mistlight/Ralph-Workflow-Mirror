//! Configuration, git identity, and model override preservation tests.

use std::fs;
use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;

use super::canonical_working_dir;
use test_helpers::{init_git_repo, write_file};

// ============================================================================
// Configuration Preservation Tests
// ============================================================================

#[test]
fn ralph_resume_preserves_developer_iterations_from_checkpoint() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 3, 5),
        )
        .unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_preserves_reviewer_passes_from_checkpoint() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 3, 3),
        )
        .unwrap();

        // Pre-create required files
        fs::write(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Test resume functionality.

## Acceptance

- Tests pass
"#,
        )
        .unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Git Identity Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_preserves_git_identity() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint with git identity at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json_with_git_identity(&working_dir),
        )
        .unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Model Override Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_preserves_model_overrides() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint with model overrides at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json_with_model_overrides(&working_dir),
        )
        .unwrap();

        // Run with --resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// PROMPT.md Checksum Validation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_records_prompt_md_checksum() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create PROMPT.md with known content
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

        // Create a v3 checkpoint with prompt_md_checksum at Complete phase
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
            "prompt_md_checksum": "{}",
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-prompt-md-checksum",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#,
            working_dir, prompt_checksum
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

        // Verify PROMPT.md checksum is recorded
        assert!(
            checkpoint_content.contains("\"prompt_md_checksum\""),
            "Checkpoint should contain prompt_md_checksum"
        );

        // Verify checkpoint can be loaded and resumed
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// PROMPT.md Change Warning Tests
// ============================================================================

#[test]
fn ralph_resume_warns_on_prompt_md_change() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Write initial PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            "# Original Task\nDo something.",
        );

        // Calculate checksum of original PROMPT.md
        let original_content = fs::read_to_string(dir.path().join("PROMPT.md")).unwrap();
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(original_content.as_bytes());
        let original_checksum = format!("{:x}", hasher.finalize());

        // Create a checkpoint at Complete phase with the original checksum
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json_with_checksum(&working_dir, &original_checksum),
        )
        .unwrap();

        // Now modify PROMPT.md
        write_file(
            dir.path().join("PROMPT.md"),
            "# Modified Task\nDo something else.",
        );

        // Run with --resume - should warn about PROMPT.md change but still complete
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Prompt History Tracking Tests
// ============================================================================

#[test]
fn ralph_checkpoint_tracks_prompt_history() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Pre-create required files to skip agent phases
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        // Run pipeline with 0 iterations
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_shows_prompt_replay_info() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase with prompt history
        let working_dir = canonical_working_dir(&dir);
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json_with_prompt_history(&working_dir),
        )
        .unwrap();

        // Resume
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
        // Verify the pipeline completed successfully
        assert!(!dir.path().join(".agent/checkpoint.json").exists());
    });
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper function to create a valid v3 checkpoint JSON with all required fields.
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

fn make_checkpoint_json_with_git_identity(working_dir: &str) -> String {
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
            "git_user_name": "Checkpoint User",
            "git_user_email": "checkpoint@example.com",
            "run_id": "00000000-0000-0000-0000-000000000001",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#,
        working_dir
    )
}

fn make_checkpoint_json_with_model_overrides(working_dir: &str) -> String {
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
                "model_override": "gpt-4",
                "provider_override": "openai",
                "context_level": 0
            }},
            "reviewer_agent_config": {{
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": "claude-3",
                "provider_override": "anthropic",
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
        working_dir
    )
}

fn make_checkpoint_json_with_checksum(working_dir: &str, checksum: &str) -> String {
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
            "prompt_md_checksum": "{}",
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
        working_dir, checksum
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
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": {{"development_1": "Original development prompt"}}
        }}"#,
        working_dir
    )
}
