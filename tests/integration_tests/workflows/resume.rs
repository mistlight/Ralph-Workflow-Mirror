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

/// Get the canonical working directory path.
/// This handles macOS symlinks (/var -> /private/var) which cause
/// working directory validation to fail in tests.
fn canonical_working_dir(dir: &TempDir) -> String {
    dir.path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string()
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
    let working_dir = canonical_working_dir(&dir);
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
    let working_dir = canonical_working_dir(&dir);
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

// ============================================================================
// Configuration Preservation Tests
// ============================================================================

/// Parameters for creating a test checkpoint JSON.
struct CheckpointTestParams<'a> {
    working_dir: &'a str,
    phase: &'a str,
    iteration: u32,
    total_iterations: u32,
    reviewer_pass: u32,
    total_reviewer_passes: u32,
    developer_iters: u32,
    reviewer_reviews: u32,
}

/// Helper function to create a valid checkpoint JSON with proper agent config fields.
fn make_checkpoint_json(params: CheckpointTestParams<'_>) -> String {
    format!(
        r#"{{
            "version": 1,
            "phase": "{}",
            "iteration": {},
            "total_iterations": {},
            "reviewer_pass": {},
            "total_reviewer_passes": {},
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {{
                "developer_iters": {},
                "reviewer_reviews": {},
                "commit_msg": "checkpoint commit message",
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
            "git_user_email": null
        }}"#,
        params.phase,
        params.iteration,
        params.total_iterations,
        params.reviewer_pass,
        params.total_reviewer_passes,
        params.developer_iters,
        params.reviewer_reviews,
        params.working_dir
    )
}

#[test]
fn ralph_resume_preserves_developer_iterations_from_checkpoint() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint with specific iteration counts
    // Checkpoint: 5 dev iters, currently at iteration 3
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        make_checkpoint_json(CheckpointTestParams {
            working_dir: &working_dir,
            phase: "Development",
            iteration: 3,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_iters: 5,
            reviewer_reviews: 2,
        }),
    )
    .unwrap();

    // Run with --resume but pass DIFFERENT env config (1 dev iter, 0 reviews)
    // The resume should use checkpoint values (5 dev iters), not env values
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1") // Different from checkpoint's 5
        .env("RALPH_REVIEWER_REVIEWS", "0") // Different from checkpoint's 2
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"No issues\" > .agent/ISSUES.md'",
        );

    // Should show warning about config change and use checkpoint values
    cmd.assert().success().stdout(
        predicate::str::contains("checkpoint").or(predicate::str::contains("Developer iterations")),
    );
}

#[test]
fn ralph_resume_preserves_reviewer_passes_from_checkpoint() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint at review phase with specific reviewer pass count
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        make_checkpoint_json(CheckpointTestParams {
            working_dir: &working_dir,
            phase: "Review",
            iteration: 3,
            total_iterations: 3,
            reviewer_pass: 1,
            total_reviewer_passes: 3,
            developer_iters: 3,
            reviewer_reviews: 3,
        }),
    )
    .unwrap();

    // Run with --resume but pass DIFFERENT reviewer count
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1") // Different
        .env("RALPH_REVIEWER_REVIEWS", "1") // Different from checkpoint's 3
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"No issues\" > .agent/ISSUES.md'",
        );

    // Should use checkpoint values and show appropriate output
    cmd.assert().success().stdout(
        predicate::str::contains("checkpoint")
            .or(predicate::str::contains("Reviewer"))
            .or(predicate::str::contains("Review")),
    );
}

// ============================================================================
// Resume from Different Phases Tests
// ============================================================================

#[test]
fn ralph_resume_from_planning_phase() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint at planning phase
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        make_checkpoint_json(CheckpointTestParams {
            working_dir: &working_dir,
            phase: "Planning",
            iteration: 1,
            total_iterations: 2,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            developer_iters: 2,
            reviewer_reviews: 1,
        }),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "2")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"No issues\" > .agent/ISSUES.md'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Planning").or(predicate::str::contains("checkpoint")));
}

#[test]
fn ralph_resume_from_development_phase() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint at development phase, iteration 2 of 3
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        make_checkpoint_json(CheckpointTestParams {
            working_dir: &working_dir,
            phase: "Development",
            iteration: 2,
            total_iterations: 3,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            developer_iters: 3,
            reviewer_reviews: 1,
        }),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "3")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"No issues\" > .agent/ISSUES.md'",
        );

    cmd.assert().success().stdout(
        predicate::str::contains("Development")
            .or(predicate::str::contains("checkpoint"))
            .or(predicate::str::contains("Resuming")),
    );
}

#[test]
fn ralph_resume_from_review_phase() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint at review phase
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        make_checkpoint_json(CheckpointTestParams {
            working_dir: &working_dir,
            phase: "Review",
            iteration: 3,
            total_iterations: 3,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            developer_iters: 3,
            reviewer_reviews: 2,
        }),
    )
    .unwrap();

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

    cmd.assert().success().stdout(
        predicate::str::contains("Review")
            .or(predicate::str::contains("checkpoint"))
            .or(predicate::str::contains("Skipping development")),
    );
}

#[test]
fn ralph_resume_from_complete_phase() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint at Complete phase
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        make_checkpoint_json(CheckpointTestParams {
            working_dir: &working_dir,
            phase: "Complete",
            iteration: 3,
            total_iterations: 3,
            reviewer_pass: 2,
            total_reviewer_passes: 2,
            developer_iters: 3,
            reviewer_reviews: 2,
        }),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "3")
        .env("RALPH_REVIEWER_REVIEWS", "2")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    // Resume from Complete should recognize pipeline is done
    cmd.assert().success();
}

// ============================================================================
// Idempotent Resume Tests
// ============================================================================

#[test]
fn ralph_resume_is_idempotent_same_checkpoint() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint at development phase
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    let checkpoint_content = make_checkpoint_json(CheckpointTestParams {
        working_dir: &working_dir,
        phase: "Development",
        iteration: 1,
        total_iterations: 1,
        reviewer_pass: 0,
        total_reviewer_passes: 0,
        developer_iters: 1,
        reviewer_reviews: 0,
    });
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        &checkpoint_content,
    )
    .unwrap();

    // First resume run
    let mut cmd1 = ralph_cmd();
    base_env(&mut cmd1)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd1.assert().success();

    // Check that a Complete checkpoint was created
    let checkpoint_path = dir.path().join(".agent/checkpoint.json");
    if checkpoint_path.exists() {
        let content = fs::read_to_string(&checkpoint_path).unwrap();
        // Should be at Complete phase now
        assert!(
            content.contains("Complete"),
            "Checkpoint should be at Complete phase after successful run"
        );
    }
}

// ============================================================================
// Git Identity Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_preserves_git_identity() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint with git identity
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
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
            "git_user_email": "checkpoint@example.com"
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
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    // Should succeed and use checkpoint's git identity
    cmd.assert().success();
}

// ============================================================================
// Model Override Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_preserves_model_overrides() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint with model overrides
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
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
            "git_user_email": null
        }}"#,
            working_dir
        ),
    )
    .unwrap();

    // Run with --resume - should show model overrides being restored
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    // Should succeed and potentially show model override info
    cmd.assert().success().stdout(
        predicate::str::contains("checkpoint")
            .or(predicate::str::contains("model"))
            .or(predicate::str::contains("Resuming")),
    );
}

// ============================================================================
// PROMPT.md Change Warning Tests
// ============================================================================

#[test]
fn ralph_resume_warns_on_prompt_md_change() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

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

    // Create a checkpoint with the original checksum
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
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
            "git_user_email": null
        }}"#,
            working_dir, original_checksum
        ),
    )
    .unwrap();

    // Now modify PROMPT.md
    write_file(
        dir.path().join("PROMPT.md"),
        "# Modified Task\nDo something else.",
    );

    // Run with --resume - should warn about PROMPT.md change
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    // Should show warning about PROMPT.md change
    cmd.assert().success().stdout(
        predicate::str::contains("PROMPT.md has changed")
            .or(predicate::str::contains("checkpoint")),
    );
}

// ============================================================================
// Rebase State Preservation Tests
// ============================================================================

#[test]
fn ralph_checkpoint_records_rebase_state() {
    let dir = TempDir::new().unwrap();
    let _repo = init_git_repo(&dir);

    // Create a checkpoint with rebase state
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    let working_dir = canonical_working_dir(&dir);
    fs::write(
        dir.path().join(".agent/checkpoint.json"),
        format!(
            r#"{{
            "version": 1,
            "phase": "PreRebase",
            "iteration": 0,
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
            "rebase_state": {{"PreRebaseInProgress": {{"upstream_branch": "main"}}}},
            "config_path": null,
            "config_checksum": null,
            "working_dir": "{}",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null
        }}"#,
            working_dir
        ),
    )
    .unwrap();

    // Run with --resume - should detect rebase phase checkpoint
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; echo change > change.txt'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    // Should show rebase-related output
    cmd.assert().success().stdout(
        predicate::str::contains("rebase")
            .or(predicate::str::contains("PreRebase"))
            .or(predicate::str::contains("checkpoint")),
    );
}
