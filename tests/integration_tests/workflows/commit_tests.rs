//! Commit behavior integration tests.
//!
//! These tests verify that commit operations work correctly across
//! different scenarios and configurations.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (git state, commit messages)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Pipeline tests use **dependency injection** via `create_test_config_struct()`
//! - Plumbing command tests use `run_ralph_cli` (they bypass config loading anyway)
//! - Tests are deterministic and isolated

use std::fs;
use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, init_git_repo, write_file};

// ============================================================================
// Commit Behavior Tests
// ============================================================================

/// Test that the pipeline succeeds without a pre-existing commit message file.
///
/// This verifies that when a user runs ralph without a commit-message.txt file,
/// the pipeline still succeeds using auto-commit behavior which generates
/// a commit message automatically.
#[test]
fn ralph_succeeds_without_commit_message_file() {
    with_default_timeout(|| {
        // With auto-commit behavior, the pipeline should succeed even without
        // a pre-existing commit-message.txt file since commits are created
        // automatically by the orchestrator using the commit message generation.
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Create a file to have something to commit
        fs::write(dir.path().join("test.txt"), "test content").unwrap();

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should succeed - auto-commit will generate a message
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Plumbing Command Tests
// ============================================================================

/// Test that the `--show-commit-msg` flag displays the commit message.
///
/// This verifies that when a user invokes ralph with the `--show-commit-msg` flag
/// and a commit-message.txt file exists, the command succeeds.
///
/// Note: Plumbing commands bypass config loading entirely, so we use `run_ralph_cli`
/// which calls the main `app::run` function that handles plumbing commands.
#[test]
fn ralph_show_commit_msg_displays_message() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Create a commit message file
        fs::write(
            dir.path().join(".agent/commit-message.txt"),
            "feat: test commit message\n",
        )
        .unwrap();

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--show-commit-msg"], executor, config, Some(dir.path())).unwrap();
    });
}

/// Test that the `--show-commit-msg` flag reads from the specified repo root.
///
/// This verifies that when a user invokes ralph with the `--show-commit-msg` flag
/// and specifies a working directory, the command reads the commit-message.txt
/// from that directory regardless of where subdirectories might have their own files.
///
/// Note: Plumbing commands bypass config loading entirely, so we use `run_ralph_cli`
/// which calls the main `app::run` function that handles plumbing commands.
#[test]
fn ralph_show_commit_msg_reads_from_working_dir() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Root commit message (the one we expect to read)
        fs::write(
            dir.path().join(".agent/commit-message.txt"),
            "feat: root commit message\n",
        )
        .unwrap();

        // Subdir has a different file that should NOT be read (we pass the repo root explicitly)
        let subdir = dir.path().join("nested/dir");
        fs::create_dir_all(subdir.join(".agent")).unwrap();
        fs::write(
            subdir.join(".agent/commit-message.txt"),
            "feat: WRONG commit message\n",
        )
        .unwrap();

        // Explicitly specify repo root as working directory
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--show-commit-msg"], executor, config, Some(dir.path())).unwrap();
    });
}

/// Test that the `--show-commit-msg` flag fails when the commit message file is missing.
///
/// This verifies that when a user invokes ralph with the `--show-commit-msg` flag
/// without a commit-message.txt file, the command fails.
///
/// Note: Plumbing commands bypass config loading entirely, so we use `run_ralph_cli`
/// which calls the main `app::run` function that handles plumbing commands.
#[test]
fn ralph_show_commit_msg_fails_if_missing() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Don't create commit-message.txt

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        let result =
            run_ralph_cli_injected(&["--show-commit-msg"], executor, config, Some(dir.path()));

        // Should fail
        assert!(result.is_err());
    });
}

/// Test that the `--apply-commit` flag creates a commit with the specified message.
///
/// This verifies that when a user invokes ralph with the `--apply-commit` flag
/// and a commit-message.txt file exists, a commit is created with that message
/// and the commit-message.txt file is cleaned up afterward.
///
/// Note: Plumbing commands bypass config loading entirely, so we use `run_ralph_cli`
/// which calls the main `app::run` function that handles plumbing commands.
#[test]
fn ralph_apply_commit_creates_commit() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let repo = init_git_repo(&dir);

        // Create an initial commit so the repo has a HEAD.
        write_file(dir.path().join("initial.txt"), "initial");
        let _ = commit_all(&repo, "initial");

        // Create a new file to commit
        fs::write(dir.path().join("new_file.txt"), "content").unwrap();

        // Create commit message file
        fs::write(
            dir.path().join(".agent/commit-message.txt"),
            "feat: add new file",
        )
        .unwrap();

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--apply-commit"], executor, config, Some(dir.path())).unwrap();

        // Verify the commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let msg = head_commit.message().unwrap_or_default();
        assert!(msg.contains("feat: add new file"));

        // Verify commit-message.txt was cleaned up
        assert!(!dir.path().join(".agent/commit-message.txt").exists());
    });
}

/// Test that the `--apply-commit` flag fails when the commit message file is missing.
///
/// This verifies that when a user invokes ralph with the `--apply-commit` flag
/// without a commit-message.txt file, the command fails.
///
/// Note: Plumbing commands bypass config loading entirely, so we use `run_ralph_cli`
/// which calls the main `app::run` function that handles plumbing commands.
#[test]
fn ralph_apply_commit_fails_without_message_file() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Don't create commit-message.txt

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        let result =
            run_ralph_cli_injected(&["--apply-commit"], executor, config, Some(dir.path()));

        // Should fail
        assert!(result.is_err());
    });
}
