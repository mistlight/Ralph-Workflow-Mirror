//! Agent execution integration tests.
//!
//! These tests verify agent command execution behavior, including:
//! - Agent command success/failure handling
//! - Multiple agent invocation across workflow phases
//! - Phase skipping with zero iterations

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use test_helpers::{commit_all, init_git_repo, write_file};

fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Use generic agents to avoid picking up user's local config
        .env("RALPH_DEVELOPER_AGENT", "codex")
        .env("RALPH_REVIEWER_AGENT", "codex")
        // Ensure git identity isn't a factor if a commit happens in the test.
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

// ============================================================================
// Agent Command Execution Tests
// ============================================================================

#[test]
fn ralph_agent_command_override_is_used() {
    // Test that RALPH_DEVELOPER_CMD is used when set
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    // Create a script that creates a marker file
    let dev_marker = dir.path().join(".agent/dev_called.txt");
    let dev_script = dir.path().join("dev.sh");
    fs::write(
        &dev_script,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
echo "custom command was called" > "{}"
echo "Plan: test" > .agent/PLAN.md
exit 0
"##,
            dev_marker.display()
        ),
    )
    .unwrap();

    let rev_script = dir.path().join("rev.sh");
    fs::write(
        &rev_script,
        r##"#!/bin/sh
mkdir -p .agent
echo "feat: test" > .agent/commit-message.txt
exit 0
"##,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script.display()),
        )
        .env("RALPH_REVIEWER_CMD", format!("sh {}", rev_script.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify our custom command was actually called
    assert!(
        dev_marker.exists(),
        "Custom developer command should have been called"
    );
    let content = fs::read_to_string(&dev_marker).unwrap();
    assert!(content.contains("custom command was called"));
}

#[test]
fn ralph_developer_success_proceeds_to_commit() {
    // Test successful developer phase leads to commit message generation
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let dev_call_log = dir.path().join(".agent/dev_calls.txt");
    let dev_script = dir.path().join("dev.sh");
    fs::write(
        &dev_script,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
echo "called" >> "{}"
echo "Plan: do the thing" > .agent/PLAN.md
exit 0
"##,
            dev_call_log.display()
        ),
    )
    .unwrap();

    let rev_script = dir.path().join("rev.sh");
    fs::write(
        &rev_script,
        r##"#!/bin/sh
mkdir -p .agent
echo "feat: test commit" > .agent/commit-message.txt
exit 0
"##,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script.display()),
        )
        .env("RALPH_REVIEWER_CMD", format!("sh {}", rev_script.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Developer should have been called for planning + execution
    assert!(
        dev_call_log.exists(),
        "Developer script should have been called"
    );
    let dev_calls: usize = fs::read_to_string(&dev_call_log).unwrap().lines().count();
    assert!(
        dev_calls >= 2,
        "Developer should be called at least twice (planning + execution)"
    );
}

#[test]
fn ralph_skips_phases_with_zero_iterations() {
    // Test that setting iterations to 0 skips the respective phase
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let dev_call_log = dir.path().join(".agent/dev_calls.txt");
    let dev_script = dir.path().join("dev.sh");
    fs::write(
        &dev_script,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
echo "called" >> "{}"
exit 0
"##,
            dev_call_log.display()
        ),
    )
    .unwrap();

    let rev_call_log = dir.path().join(".agent/rev_calls.txt");
    let rev_script = dir.path().join("rev.sh");
    fs::write(
        &rev_script,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
echo "called" >> "{}"
echo "feat: test" > .agent/commit-message.txt
exit 0
"##,
            rev_call_log.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0") // Skip developer
        .env("RALPH_REVIEWER_REVIEWS", "0") // Skip review
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script.display()),
        )
        .env("RALPH_REVIEWER_CMD", format!("sh {}", rev_script.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Developer should NOT have been called when iterations = 0
    // (but reviewer may be called for commit message)
    if dev_call_log.exists() {
        let dev_calls: usize = fs::read_to_string(&dev_call_log).unwrap().lines().count();
        // With RALPH_DEVELOPER_ITERS=0, dev should not be called
        assert_eq!(
            dev_calls, 0,
            "Developer should not be called when iterations=0"
        );
    }
}

// ============================================================================
// Multi-Phase Agent Execution Tests
// ============================================================================

#[test]
fn ralph_uses_developer_command_override() {
    // Test that developer command is used when RALPH_DEVELOPER_CMD is set
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let dev_marker = dir.path().join(".agent/dev_marker.txt");

    let dev_script = dir.path().join("dev.sh");
    fs::write(
        &dev_script,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
echo "custom developer command" > "{}"
echo "Plan: test" > .agent/PLAN.md
exit 0
"##,
            dev_marker.display()
        ),
    )
    .unwrap();

    let rev_script = dir.path().join("rev.sh");
    fs::write(
        &rev_script,
        r##"#!/bin/sh
mkdir -p .agent
echo "feat: test" > .agent/commit-message.txt
exit 0
"##,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script.display()),
        )
        .env("RALPH_REVIEWER_CMD", format!("sh {}", rev_script.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Developer marker should exist proving our custom command was used
    assert!(dev_marker.exists(), "Developer command should have run");

    // Verify our custom command was called (not the default agent)
    let dev_content = fs::read_to_string(&dev_marker).unwrap();
    assert!(dev_content.contains("custom developer command"));
}

#[test]
fn ralph_developer_failure_aborts_pipeline() {
    // Test that developer failure prevents pipeline success
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let dev_script = dir.path().join("dev.sh");
    fs::write(
        &dev_script,
        r##"#!/bin/sh
exit 1
"##,
    )
    .unwrap();

    let rev_script = dir.path().join("rev.sh");
    fs::write(
        &rev_script,
        r##"#!/bin/sh
mkdir -p .agent
echo "feat: test" > .agent/commit-message.txt
exit 0
"##,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script.display()),
        )
        .env("RALPH_REVIEWER_CMD", format!("sh {}", rev_script.display()));

    // Pipeline should fail when developer command fails
    cmd.assert().failure();
}

// ============================================================================
// Agent Output Handling Tests
// ============================================================================

#[test]
fn ralph_captures_agent_stderr() {
    // Test that agent stderr doesn't cause pipeline failure when exit is 0
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let dev_script = dir.path().join("dev.sh");
    fs::write(
        &dev_script,
        r##"#!/bin/sh
mkdir -p .agent
echo "Error: something went wrong" >&2
echo "Plan: test" > .agent/PLAN.md
exit 0
"##,
    )
    .unwrap();

    let rev_script = dir.path().join("rev.sh");
    fs::write(
        &rev_script,
        r##"#!/bin/sh
mkdir -p .agent
echo "feat: test" > .agent/commit-message.txt
exit 0
"##,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script.display()),
        )
        .env("RALPH_REVIEWER_CMD", format!("sh {}", rev_script.display()));

    // Pipeline should succeed despite stderr output
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));
}

#[test]
fn ralph_handles_agent_with_large_output() {
    // Test that ralph handles agents that produce large output
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    // Script that produces a lot of output
    let dev_script = dir.path().join("dev.sh");
    fs::write(
        &dev_script,
        r##"#!/bin/sh
mkdir -p .agent
# Generate some output
for i in $(seq 1 100); do
    echo "Line $i of output"
done
echo "Plan: test" > .agent/PLAN.md
exit 0
"##,
    )
    .unwrap();

    let rev_script = dir.path().join("rev.sh");
    fs::write(
        &rev_script,
        r##"#!/bin/sh
mkdir -p .agent
echo "feat: test" > .agent/commit-message.txt
exit 0
"##,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script.display()),
        )
        .env("RALPH_REVIEWER_CMD", format!("sh {}", rev_script.display()));

    // Pipeline should succeed
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));
}
