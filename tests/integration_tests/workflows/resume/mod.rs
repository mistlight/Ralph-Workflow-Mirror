//! Resume functionality integration tests.
//!
//! Tests are organized into focused modules:
//! - checkpoint: Checkpoint creation, content, and cleanup
//! - basic: Resume flags and working directory validation
//! - phases: Resume from different pipeline phases
//! - preservation: Configuration and state preservation
//! - v3: V3 hardened resume features
//! - rebase: Rebase-related resume tests
//!
//! All tests use `MockAppEffectHandler` for in-memory testing per
//! `INTEGRATION_TESTS.md` requirements.

mod basic;
mod checkpoint;
mod phases;
mod preservation;
mod rebase;
mod v3;

/// Mock repository path used consistently across resume tests.
pub const MOCK_REPO_PATH: &str = "/mock/repo";

/// Standard prompt content for tests - matches the required PROMPT.md format.
/// NOTE: If you change this, update `STANDARD_PROMPT_CHECKSUM` below!
pub const STANDARD_PROMPT: &str = r"## Goal

Do something.

## Acceptance

- Tests pass
";

/// SHA256 checksum of `STANDARD_PROMPT` above.
/// Compute with: echo -n '<content>' | sha256sum
pub const STANDARD_PROMPT_CHECKSUM: &str =
    "f3172db90fb9245992bd8ad018ed77821a8765c16d57ca889dc2aa8501953556";

/// Helper function to create a valid v3 checkpoint JSON with all required fields.
/// Always sets `developer_iters` and `reviewer_reviews` to 0 to prevent agent execution.
pub fn make_checkpoint_json(
    working_dir: &str,
    phase: &str,
    iteration: u32,
    total_iterations: u32,
) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "{phase}",
            "iteration": {iteration},
            "total_iterations": {total_iterations},
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
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "00000000-0000-0000-0000-000000000001",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": {iteration},
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null
        }}"#
    )
}

/// Helper function to create a v3 checkpoint JSON with execution history.
pub fn make_checkpoint_with_execution_history(
    working_dir: &str,
    phase: &str,
    execution_history_json: &str,
) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "{phase}",
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
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 1,
            "actual_reviewer_runs": 0,
            "execution_history": {execution_history_json},
            "file_system_state": null,
            "prompt_history": null
        }}"#
    )
}

/// Helper function to create a v3 checkpoint JSON with file system state.
pub fn make_checkpoint_with_file_system_state(
    working_dir: &str,
    phase: &str,
    file_system_state_json: &str,
) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "{phase}",
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
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": {file_system_state_json},
            "prompt_history": null
        }}"#
    )
}

/// Helper function to create a v3 checkpoint JSON with prompt history.
pub fn make_checkpoint_with_prompt_history(
    working_dir: &str,
    phase: &str,
    prompt_history_json: &str,
) -> String {
    format!(
        r#"{{
            "version": 3,
            "phase": "{phase}",
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
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id-123",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": {prompt_history_json}
        }}"#
    )
}
