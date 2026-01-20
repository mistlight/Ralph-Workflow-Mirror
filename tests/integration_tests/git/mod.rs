//! Integration tests for git workflow with per-iteration commits.
//!
//! These tests verify that:
//! - start_commit file tracking works
//! - The --reset-start-commit flag works
//!
//! Note: Tests that require agent execution (developer_iters > 0 or reviewer_reviews > 0)
//! cannot be properly tested without the AgentExecutor trait infrastructure. Those tests
//! should be unit tests with mocked executors at the code level.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (file creation, git state)
//! - Uses `TempDir` for filesystem isolation
//! - Tests are deterministic and black-box (test git workflow as a user would experience it)

use std::fs;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, head_oid, init_git_repo, write_file};

/// Helper function to set up base environment for tests
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

fn init_repo_with_initial_commit(dir: &TempDir) -> git2::Repository {
    let repo = init_git_repo(dir);
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");
    repo
}

/// Test that the `--reset-start-commit` flag updates the `.agent/start_commit` file.
///
/// This verifies that when a user invokes ralph with the `--reset-start-commit` flag,
/// the `.agent/start_commit` file is updated to contain the OID of the current HEAD commit.
#[test]
fn ralph_reset_start_commit_flag_works() {
    with_default_timeout(|| {
        // Test that --reset-start-commit updates .agent/start_commit
        let dir = TempDir::new().unwrap();
        let repo = init_repo_with_initial_commit(&dir);

        // First, create a new commit so we have a new HEAD
        fs::write(dir.path().join("new_file.txt"), "content").unwrap();
        let _ = commit_all(&repo, "second commit");

        // Get the current HEAD commit OID
        let head_oid_str = head_oid(&repo);

        // Run ralph with --reset-start-commit
        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path())
            .arg("--reset-start-commit")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        cmd.assert().success();

        // Verify .agent/start_commit was updated
        let start_commit_content =
            fs::read_to_string(dir.path().join(".agent/start_commit")).unwrap();
        assert_eq!(
            start_commit_content.trim(),
            head_oid_str,
            "start_commit should be updated to current HEAD"
        );
    });
}

/// Test that the `.agent/start_commit` file is created during pipeline execution.
///
/// This verifies that when a user runs ralph with a change to commit,
/// the `.agent/start_commit` file is created containing a valid OID
/// or the empty repo marker, enabling cumulative diffs for reviewers.
#[test]
fn ralph_start_commit_created_during_pipeline() {
    with_default_timeout(|| {
        // Test that .agent/start_commit is created during pipeline execution
        let dir = TempDir::new().unwrap();
        init_repo_with_initial_commit(&dir);

        // Create a change to commit
        write_file(dir.path().join("test.txt"), "new content");

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert().success();

        // Verify .agent/start_commit exists (enables cumulative diffs for reviewers)
        assert!(
            dir.path().join(".agent/start_commit").exists(),
            ".agent/start_commit should be created at pipeline start"
        );

        // Verify it contains a valid OID (40 hex characters or empty repo marker)
        let start_commit_content =
            fs::read_to_string(dir.path().join(".agent/start_commit")).unwrap();
        let is_valid_oid = start_commit_content.trim().len() == 40;
        let is_empty_repo_marker = start_commit_content.trim() == "__EMPTY_REPO__";
        assert!(
            is_valid_oid || is_empty_repo_marker,
            "start_commit should contain a valid OID or empty repo marker"
        );
    });
}

/// Test that the start_commit persists across multiple pipeline runs.
///
/// This verifies that when a user runs ralph multiple times without resetting,
/// the `.agent/start_commit` file maintains the same OID value across runs,
/// ensuring cumulative diffs work correctly for reviewers.
#[test]
fn ralph_start_commit_persists_across_pipeline_runs() {
    with_default_timeout(|| {
        // Test that start_commit persists across pipeline runs
        // This ensures cumulative diffs work correctly
        let dir = TempDir::new().unwrap();
        init_repo_with_initial_commit(&dir);

        // First run - creates start_commit
        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert().success();

        // Get the start_commit OID from first run
        let first_start_commit =
            fs::read_to_string(dir.path().join(".agent/start_commit")).unwrap();

        // Create a new commit
        write_file(dir.path().join("new_file.txt"), "content");
        let repo = git2::Repository::open(dir.path()).unwrap();
        let _ = commit_all(&repo, "new commit");

        // Second run - start_commit should still be the same (not reset)
        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert().success();

        // Verify start_commit hasn't changed
        let second_start_commit =
            fs::read_to_string(dir.path().join(".agent/start_commit")).unwrap();
        assert_eq!(
            first_start_commit, second_start_commit,
            "start_commit should persist across pipeline runs unless reset"
        );
    });
}

/// Test that the `--reset-start-commit` flag fails gracefully on an empty repository.
///
/// This verifies that when a user invokes ralph with `--reset-start-commit`
/// in a repository with no commits (unborn HEAD), the command fails
/// but succeeds after an initial commit is created.
#[test]
fn ralph_save_start_commit_handles_empty_repo() {
    with_default_timeout(|| {
        // Test that the pipeline handles an empty repository (no commits)
        // This verifies the graceful handling when HEAD is unborn
        let dir = TempDir::new().unwrap();

        // Initialize an empty git repo (no commits)
        let _ = init_git_repo(&dir);
        fs::write(dir.path().join("PROMPT.md"), "# Test\n").unwrap();

        // Try to run ralph with --reset-start-commit on empty repo
        // This should fail because there's no HEAD commit to reference
        let mut cmd = ralph_cmd();
        let result = cmd
            .current_dir(dir.path())
            .arg("--reset-start-commit")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        // Should fail because there's no HEAD commit
        let assert = result.assert();
        assert.failure();

        // Now create an initial commit and verify --reset-start-commit succeeds
        write_file(dir.path().join("initial.txt"), "initial content");
        let repo = git2::Repository::open(dir.path()).unwrap();
        let _ = commit_all(&repo, "initial commit");

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path())
            .arg("--reset-start-commit")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        cmd.assert().success();

        // Verify the start_commit file was created with a valid OID
        assert!(dir.path().join(".agent/start_commit").exists());
        let start_commit_content =
            fs::read_to_string(dir.path().join(".agent/start_commit")).unwrap();
        assert_eq!(
            start_commit_content.trim().len(),
            40,
            "start_commit should contain a 40-character OID"
        );
    });
}
