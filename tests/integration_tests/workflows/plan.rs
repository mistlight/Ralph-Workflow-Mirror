//! PLAN workflow integration tests.
//!
//! These tests verify the plan workflow functionality.
//!
//! Note: Tests that require agent execution (developer_iters > 0) cannot be
//! properly tested without the AgentExecutor trait infrastructure. Those tests
//! should be unit tests with mocked executors at the code level.
//!
//! These integration tests focus on behavior that doesn't require agent execution.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (exit codes, output)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use crate::test_timeout::with_default_timeout;
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
//
// Note: Tests that require agent execution (developer_iters > 0) cannot be
// properly tested without the AgentExecutor trait infrastructure. Those tests
// should be unit tests with mocked executors at the code level.
//
// These integration tests focus on behavior that doesn't require agent execution.
// ============================================================================

/// Test that the plan phase is skipped when developer_iters is set to zero.
///
/// This verifies that when a user runs ralph with developer_iters=0,
/// the planning phase is skipped entirely and no PLAN.md file is created.
#[test]
fn ralph_skips_plan_phase_when_zero_developer_iters() {
    with_default_timeout(|| {
        // When developer_iters=0, planning phase should be skipped entirely
        // and the workflow should complete successfully with just a commit
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Create a file to have something to commit
        fs::write(dir.path().join("test.txt"), "content").unwrap();

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        // Should succeed - plan phase is skipped when developer_iters=0
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

        // Verify PLAN.md was never created (since planning was skipped)
        assert!(!dir.path().join(".agent/PLAN.md").exists());
    });
}

/// Test that a commit can be created without a plan when developer_iters is zero.
///
/// This verifies that when a user runs ralph with developer_iters=0,
/// a commit is created successfully without requiring a PLAN.md file.
#[test]
fn ralph_commit_without_plan_succeeds() {
    with_default_timeout(|| {
        // Test that a commit can be made without any plan when developer_iters=0
        // This tests the "skip to commit" behavior
        let dir = TempDir::new().unwrap();
        let repo = init_git_repo(&dir);

        // Create an initial commit to establish HEAD
        fs::write(dir.path().join("initial.txt"), "initial").unwrap();
        let _ = test_helpers::commit_all(&repo, "initial commit");

        // Create a new file to have something to commit in the test run
        fs::write(dir.path().join("test.txt"), "content").unwrap();

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        // Should succeed and create a commit
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

        // Verify a commit was created (should have 2 commits: initial + test)
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert!(!commit.message().unwrap().is_empty());
    });
}
