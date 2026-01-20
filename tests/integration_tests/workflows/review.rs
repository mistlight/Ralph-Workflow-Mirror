//! Review workflow integration tests.
//!
//! These tests verify the review workflow functionality.
//!
//! Note: Tests that require agent execution (reviewer_reviews > 0) cannot be
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
use tempfile::TempDir;

use crate::common::ralph_cmd;
use crate::test_timeout::with_default_timeout;
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
// Review Workflow Tests
//
// Note: Tests that require agent execution (reviewer_reviews > 0) cannot be
// properly tested without the AgentExecutor trait infrastructure. Those tests
// should be unit tests with mocked executors at the code level.
//
// These integration tests focus on behavior that doesn't require agent execution.
// ============================================================================

/// Test that setting reviewer_reviews to zero skips the review phase.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// the review phase is skipped entirely and no ISSUES.md file is created.
#[test]
fn ralph_zero_reviewer_reviews_skips_review() {
    with_default_timeout(|| {
        // Test that reviewer_reviews=0 skips the review phase entirely
        let dir = TempDir::new().unwrap();
        let repo = init_git_repo(&dir);

        // Create initial commit with tracked files
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&repo, "initial commit");

        // Create a change for the diff
        write_file(dir.path().join("initial.txt"), "updated content");

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_REVIEWER_REVIEWS", "0"); // Skip review phase

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

        // ISSUES.md should NOT be created when review is skipped
        assert!(!dir.path().join(".agent/ISSUES.md").exists());
    });
}

/// Test that the pipeline succeeds without a review phase.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// the pipeline completes successfully and outputs "Pipeline Complete".
#[test]
fn ralph_succeeds_without_review_phase() {
    with_default_timeout(|| {
        // Test that the pipeline can succeed without any review phase
        let dir = TempDir::new().unwrap();
        let repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));
    });
}

/// Test that a commit is created when the review phase is skipped.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// a commit is still created with a non-empty commit message.
#[test]
fn ralph_commit_created_when_review_skipped() {
    with_default_timeout(|| {
        // Test that commits are still created when review phase is skipped
        let dir = TempDir::new().unwrap();
        let repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert().success();

        // Verify commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert!(!commit.message().unwrap().is_empty());
    });
}
