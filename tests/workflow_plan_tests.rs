use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use test_helpers::init_git_repo;

fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Ensure git identity isn't a factor if a commit happens in the test.
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

// ============================================================================
// PLAN Workflow Tests
// ============================================================================

#[test]
fn ralph_fails_if_plan_missing() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        // Need at least 1 developer iteration to trigger planning phase
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Developer command that doesn't create PLAN.md
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("no plan was found"));
}

#[test]
fn ralph_skips_plan_when_zero_developer_iters() {
    // When developer_iters=0, planning phase should be skipped entirely
    // and the workflow should complete successfully if commit message is provided
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Developer command doesn't create PLAN.md - should still work since plan is skipped
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    // Should succeed - plan phase is skipped when developer_iters=0
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify PLAN.md was never created (since planning was skipped)
    assert!(!dir.path().join(".agent/PLAN.md").exists());
}

#[test]
fn ralph_fails_on_empty_plan() {
    // Empty PLAN.md should be rejected
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Create an empty PLAN.md (whitespace only)
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo \"   \" > .agent/PLAN.md'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("no plan was found"));
}

#[test]
fn ralph_plan_deleted_after_iteration() {
    // PLAN.md should be deleted after each iteration completes
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a script that creates PLAN.md on first call
    let script_path = dir.path().join("dev_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
# Check if we're in planning (PLAN.md doesn't exist) or executing (it does)
if [ ! -f .agent/PLAN.md ]; then
    echo "Step 1: Do the thing" > .agent/PLAN.md
fi
exit 0
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", script_path.display()),
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // PLAN.md should be deleted after iteration
    assert!(!dir.path().join(".agent/PLAN.md").exists());
}

#[test]
fn ralph_runs_planning_for_each_iteration() {
    // Each developer iteration should run planning -> execution -> cleanup
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a script that tracks how many times it's called
    let counter_path = dir.path().join(".agent/call_counter");
    let script_path = dir.path().join("dev_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
# Increment counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Create PLAN.md if it doesn't exist (planning phase)
if [ ! -f .agent/PLAN.md ]; then
    echo "Plan for iteration" > .agent/PLAN.md
fi
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "2") // 2 iterations
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", script_path.display()),
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // Developer command should be called multiple times for planning + execution phases
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // With 2 iterations, we expect multiple calls (planning + execution per iteration)
    assert!(
        count >= 4,
        "Expected at least 4 developer calls (2 iterations × 2 phases), got {}",
        count
    );
}
