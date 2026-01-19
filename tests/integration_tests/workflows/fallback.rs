//! Agent execution integration tests.
//!
//! These tests verify agent command execution behavior, including:
//! - Phase skipping with zero iterations
//! - Pipeline behavior without agent execution
//!
//! Note: Tests that require agent execution (developer_iters > 0 or reviewer_reviews > 0)
//! cannot be properly tested without the AgentExecutor trait infrastructure. Those tests
//! should be unit tests with mocked executors at the code level.

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
// Agent Command Execution Tests
// ============================================================================

#[test]
fn ralph_skips_phases_with_zero_iterations() {
    with_default_timeout(|| {
        // Test that setting iterations to 0 skips the respective phase
        let dir = TempDir::new().unwrap();
        let repo = init_git_repo(&dir);

        // Create initial commit to establish HEAD
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&repo, "initial commit");

        // Create a change to commit
        write_file(dir.path().join("test.txt"), "new content");

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0") // Skip developer
            .env("RALPH_REVIEWER_REVIEWS", "0"); // Skip review

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

        // Verify no agent-related files were created (agents weren't called)
        assert!(!dir.path().join(".agent/PLAN.md").exists());
        assert!(!dir.path().join(".agent/ISSUES.md").exists());
    });
}

#[test]
fn ralph_succeeds_with_zero_iterations() {
    with_default_timeout(|| {
        // Test that the pipeline can succeed with both developer and review skipped
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

        // Verify a commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert!(!commit.message().unwrap().is_empty());
    });
}
