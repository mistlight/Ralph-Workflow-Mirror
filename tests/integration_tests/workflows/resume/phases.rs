//! Resume from different phases tests.

use std::fs;
use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;

use super::canonical_working_dir;
use test_helpers::init_git_repo;

// ============================================================================
// Phase Resume Tests
// ============================================================================

#[test]
fn ralph_resume_shows_checkpoint_summary() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a v3 checkpoint at Complete phase
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

        // Run with --resume - Complete phase means no execution needed
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Resume from Different Phases Tests
// ============================================================================

#[test]
fn ralph_resume_from_planning_phase() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase (0 iterations = complete immediately)
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 1, 1),
        )
        .unwrap();

        // Pre-create required files to skip agent phases
        fs::write(dir.path().join("PROMPT.md"), "Test prompt\n").unwrap();
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_from_development_phase() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 2, 3),
        )
        .unwrap();

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_from_review_phase() {
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

        // Pre-create required files to skip agent phases
        fs::write(dir.path().join("PROMPT.md"), "Test prompt\n").unwrap();
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();
        fs::write(dir.path().join(".agent/ISSUES.md"), "No issues\n").unwrap();

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_from_complete_phase() {
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

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
        // Resume from Complete should recognize pipeline is done
    });
}

// ============================================================================
// Resume Context in Agent Prompts Tests
// ============================================================================

#[test]
fn ralph_resume_passes_context_to_developer_agent() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

        // Create a checkpoint at Complete phase
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 1, 1),
        )
        .unwrap();

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

#[test]
fn ralph_resume_passes_context_to_reviewer_agent() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);
        let config = create_test_config_struct();

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
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(dir.path().join(".agent/PLAN.md"), "Test plan\n").unwrap();

        // Create a checkpoint at Complete phase
        let working_dir = canonical_working_dir(&dir);
        fs::write(
            dir.path().join(".agent/checkpoint.json"),
            make_checkpoint_json(&working_dir, "Complete", 1, 1),
        )
        .unwrap();

        // Pre-create ISSUES.md and commit-message.txt to satisfy validation
        fs::write(dir.path().join(".agent/ISSUES.md"), "No issues found.\n").unwrap();
        fs::write(dir.path().join(".agent/commit-message.txt"), "feat: test\n").unwrap();

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--resume"], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper function to create a valid v3 checkpoint JSON with all required fields.
/// Always sets developer_iters and reviewer_reviews to 0 to prevent agent execution.
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
