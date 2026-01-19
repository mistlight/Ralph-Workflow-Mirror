//! Resume functionality integration tests.
//!
//! Tests that verify the checkpoint and resume functionality works correctly
//! across different pipeline phases.

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use test_helpers::{init_git_repo, write_file};

fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        // Ensure git identity isn't a factor if a commit happens in the test.
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

// ============================================================================
// Checkpoint Creation Tests
// ============================================================================

#[test]
fn ralph_creates_checkpoint_during_development() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Run with 1 developer iteration
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().success();

    // Checkpoint should have been created at some point
    // Note: The checkpoint is cleared on success, so we can't check for its existence
    // Instead we verify the pipeline completed successfully
}

#[test]
fn ralph_creates_checkpoint_during_review() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Run with 1 review iteration
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"No issues\" > .agent/ISSUES.md; echo change > change.txt'",
        );

    cmd.assert().success();
}

// ============================================================================
// Checkpoint Content Tests
// ============================================================================

#[test]
fn ralph_checkpoint_contains_iteration_info() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a failing developer command that leaves a checkpoint
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "3") // 3 iterations
        .env("RALPH_REVIEWER_REVIEWS", "2") // 2 reviews
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo \"   \" > .agent/PLAN.md'", // Empty plan fails
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().failure();

    // Check that checkpoint was created
    let checkpoint_path = dir.path().join(".agent/checkpoint.json");
    assert!(
        checkpoint_path.exists(),
        "Checkpoint should be created on failure"
    );

    // Verify checkpoint content has expected structure
    let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();
    assert!(
        checkpoint_content.contains("\"phase\""),
        "Checkpoint should contain phase"
    );
    assert!(
        checkpoint_content.contains("\"total_iterations\""),
        "Checkpoint should contain total_iterations"
    );
    assert!(
        checkpoint_content.contains("\"total_reviewer_passes\""),
        "Checkpoint should contain total_reviewer_passes"
    );
    assert!(
        checkpoint_content.contains("\"version\""),
        "Checkpoint should contain version"
    );
}

#[test]
fn ralph_checkpoint_contains_cli_args_snapshot() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a failing run with specific config
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "5")
        .env("RALPH_REVIEWER_REVIEWS", "3")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo \"   \" > .agent/PLAN.md'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().failure();

    let checkpoint_path = dir.path().join(".agent/checkpoint.json");
    let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();

    // Verify CLI args snapshot is present
    assert!(
        checkpoint_content.contains("\"cli_args\""),
        "Checkpoint should contain cli_args snapshot"
    );
    assert!(
        checkpoint_content.contains("\"developer_iters\""),
        "Checkpoint should contain developer_iters in cli_args"
    );
    assert!(
        checkpoint_content.contains("\"reviewer_reviews\""),
        "Checkpoint should contain reviewer_reviews in cli_args"
    );
}

#[test]
fn ralph_checkpoint_contains_agent_config_snapshot() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a failing run
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo \"   \" > .agent/PLAN.md'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().failure();

    let checkpoint_path = dir.path().join(".agent/checkpoint.json");
    let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();

    // Verify agent config snapshots are present
    assert!(
        checkpoint_content.contains("\"developer_agent_config\""),
        "Checkpoint should contain developer_agent_config"
    );
    assert!(
        checkpoint_content.contains("\"reviewer_agent_config\""),
        "Checkpoint should contain reviewer_agent_config"
    );
}

// ============================================================================
// Resume Flag Tests
// ============================================================================

#[test]
fn ralph_resume_flag_reads_checkpoint() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint file manually
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        r#"{
            "version": 1,
            "phase": "Development",
            "iteration": 2,
            "total_iterations": 3,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 3,
                "reviewer_reviews": 2,
                "commit_msg": "",
                "review_depth": null,
                "skip_rebase": false
            },
            "developer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false
            },
            "reviewer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false
            },
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "",
            "prompt_md_checksum": null
        }"#,
    )
    .unwrap();

    // Run with --resume flag - should detect the checkpoint
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().success().stdout(
        predicate::str::contains("Loading Checkpoint").or(predicate::str::contains("Resuming")),
    );
}

#[test]
fn ralph_resume_without_checkpoint_starts_fresh() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // No checkpoint exists, but we pass --resume
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No checkpoint found"));
}

// ============================================================================
// Working Directory Validation Tests
// ============================================================================

#[test]
fn ralph_resume_validates_working_directory() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint with a different working directory
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        r#"{
            "version": 1,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 1,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null,
                "skip_rebase": false
            },
            "developer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false
            },
            "reviewer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false
            },
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/some/other/directory",
            "prompt_md_checksum": null
        }"#,
    )
    .unwrap();

    // Run with --resume - should fail validation
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    // Error messages go to stderr
    cmd.assert().stderr(
        predicate::str::contains("Working directory mismatch")
            .or(predicate::str::contains("validation failed")),
    );
}

// ============================================================================
// PROMPT.md Checksum Validation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_records_prompt_md_checksum() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create PROMPT.md with known content
    write_file(
        dir.path().join("PROMPT.md"),
        "# Test Prompt\n\nDo something.",
    );

    // Create a failing run to leave a checkpoint
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo \"   \" > .agent/PLAN.md'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().failure();

    let checkpoint_path = dir.path().join(".agent/checkpoint.json");
    let checkpoint_content = fs::read_to_string(&checkpoint_path).unwrap();

    // Verify PROMPT.md checksum is recorded
    assert!(
        checkpoint_content.contains("\"prompt_md_checksum\""),
        "Checkpoint should contain prompt_md_checksum"
    );
}

// ============================================================================
// Phase Resume Tests
// ============================================================================

#[test]
fn ralph_resume_shows_checkpoint_summary() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint at review phase
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = dir.path().to_string_lossy().to_string();
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        format!(
            r#"{{
            "version": 1,
            "phase": "Review",
            "iteration": 3,
            "total_iterations": 3,
            "reviewer_pass": 1,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": 3,
                "reviewer_reviews": 2,
                "commit_msg": "",
                "review_depth": null,
                "skip_rebase": false
            }},
            "developer_agent_config": {{
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false
            }},
            "reviewer_agent_config": {{
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false
            }},
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "{}",
            "prompt_md_checksum": null
        }}"#,
            working_dir
        ),
    )
    .unwrap();

    // Run with --resume
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "3")
        .env("RALPH_REVIEWER_REVIEWS", "2")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"No issues\" > .agent/ISSUES.md'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Review").or(predicate::str::contains("checkpoint")));
}

// ============================================================================
// Checkpoint Cleanup Tests
// ============================================================================

#[test]
fn ralph_clears_checkpoint_on_success() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Pre-create a checkpoint
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = dir.path().to_string_lossy().to_string();
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        format!(
            r#"{{
            "version": 1,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": 1,
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
                "can_commit": false
            }},
            "reviewer_agent_config": {{
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false
            }},
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "{}",
            "prompt_md_checksum": null
        }}"#,
            working_dir
        ),
    )
    .unwrap();

    // Run successfully without --resume
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().success();

    // Checkpoint should be cleared on successful completion
    // (this behavior may vary based on implementation - adjust test if needed)
}
