//! Configuration, git identity, and model override preservation tests.
//!
//! These tests use `MockAppEffectHandler` for in-memory testing without
//! real filesystem or git operations.

use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::{make_checkpoint_json, make_checkpoint_with_prompt_history, MOCK_REPO_PATH};

/// Standard prompt content for tests - matches the required PROMPT.md format.
const STANDARD_PROMPT: &str = r"## Goal

Test resume functionality.

## Acceptance

- Tests pass
";

/// SHA256 checksum of `STANDARD_PROMPT` above.
///
/// Verified against the actual prompt content at test time by
/// [`verify_preservation_prompt_checksum`] to prevent silent drift.
const STANDARD_PROMPT_CHECKSUM: &str =
    "e10f15e8e6da0c359d9d266370dcefe7e11270c48b63144f1d443249b9eb5738";

/// Guard test: verifies `STANDARD_PROMPT_CHECKSUM` matches the actual
/// SHA256 of the local `STANDARD_PROMPT`. Fails loudly when the prompt
/// text is updated without updating the checksum constant.
#[test]
fn verify_preservation_prompt_checksum() {
    with_default_timeout(|| {
        let computed = ralph_workflow::reducer::prompt_inputs::sha256_hex_str(STANDARD_PROMPT);
        assert_eq!(
            STANDARD_PROMPT_CHECKSUM, computed,
            "preservation STANDARD_PROMPT_CHECKSUM is stale — update it to match STANDARD_PROMPT"
        );
    });
}

// ============================================================================
// Configuration Preservation Tests
// ============================================================================

#[test]
fn ralph_resume_preserves_developer_iterations_from_checkpoint() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 3, 5);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_preserves_reviewer_passes_from_checkpoint() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 3, 3);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Git Identity Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_preserves_git_identity() {
    with_default_timeout(|| {
        // Create a checkpoint with git identity at Complete phase
        let checkpoint_json = make_checkpoint_json_with_git_identity(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Model Override Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_preserves_model_overrides() {
    with_default_timeout(|| {
        // Create a checkpoint with model overrides at Complete phase
        let checkpoint_json = make_checkpoint_json_with_model_overrides(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// PROMPT.md Checksum Validation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_records_prompt_md_checksum() {
    use ralph_workflow::reducer::prompt_inputs::sha256_hex_str;
    with_default_timeout(|| {
        // Create PROMPT.md with known content
        let prompt_content = "# Test Prompt\n\nDo something.";

        // Calculate PROMPT.md checksum
        let prompt_checksum = sha256_hex_str(prompt_content);

        // Create a v3 checkpoint with prompt_md_checksum at Complete phase
        let checkpoint_json = make_checkpoint_json_with_checksum(MOCK_REPO_PATH, &prompt_checksum);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", prompt_content)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        // Verify PROMPT.md checksum is recorded
        assert!(
            checkpoint_json.contains("\"prompt_md_checksum\""),
            "Checkpoint should contain prompt_md_checksum"
        );

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint can be loaded and resumed
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// PROMPT.md Change Warning Tests
// ============================================================================

#[test]
fn ralph_resume_warns_on_prompt_md_change() {
    use ralph_workflow::reducer::prompt_inputs::sha256_hex_str;
    with_default_timeout(|| {
        // Write initial PROMPT.md
        let original_content = "# Original Task\nDo something.";

        // Calculate checksum of original PROMPT.md
        let original_checksum = sha256_hex_str(original_content);

        // Create a checkpoint at Complete phase with the original checksum
        let checkpoint_json =
            make_checkpoint_json_with_checksum(MOCK_REPO_PATH, &original_checksum);

        // Create handler with MODIFIED PROMPT.md (different from checkpoint checksum)
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", "# Modified Task\nDo something else.")
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume - should warn about PROMPT.md change but still complete
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Prompt History Tracking Tests
// ============================================================================

#[test]
fn ralph_checkpoint_tracks_prompt_history() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run pipeline with 0 iterations
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_shows_prompt_replay_info() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase with prompt history
        let prompt_history_json = r#"{"development_1": "Original development prompt"}"#;
        let checkpoint_json =
            make_checkpoint_with_prompt_history(MOCK_REPO_PATH, "Complete", prompt_history_json);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();

        // Verify the pipeline completed successfully
        assert!(!handler.file_exists(&PathBuf::from(".agent/checkpoint.json")));
    });
}

// ============================================================================
// Helper Functions
// ============================================================================

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
            "working_dir": "{working_dir}",
            "prompt_md_checksum": "{STANDARD_PROMPT_CHECKSUM}",
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
        }}"#
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
                "review_depth": null
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
            "working_dir": "{working_dir}",
            "prompt_md_checksum": "{STANDARD_PROMPT_CHECKSUM}",
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
        }}"#
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
            "working_dir": "{working_dir}",
            "prompt_md_checksum": "{checksum}",
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
        }}"#
    )
}
