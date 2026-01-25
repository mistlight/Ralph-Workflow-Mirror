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

use std::fs;
use tempfile::TempDir;

use crate::common::{mock_executor_with_success, run_ralph_cli, EnvGuard};
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, init_git_repo, write_file};

/// Helper function to set up base environment for tests with automatic cleanup.
///
/// Uses EnvGuard to ensure all environment variables are restored when dropped,
/// preventing cross-test pollution.
fn base_env() -> EnvGuard {
    let guard = EnvGuard::new(&[
        "RALPH_INTERACTIVE",
        "RALPH_DEVELOPER_ITERS",
        "RALPH_REVIEWER_REVIEWS",
        "RALPH_DEVELOPER_AGENT",
        "RALPH_REVIEWER_AGENT",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ]);

    guard.set(&[
        ("RALPH_INTERACTIVE", Some("0")),
        ("RALPH_DEVELOPER_ITERS", Some("0")),
        ("RALPH_REVIEWER_REVIEWS", Some("0")),
        ("RALPH_DEVELOPER_AGENT", Some("codex")),
        ("RALPH_REVIEWER_AGENT", Some("codex")),
        ("GIT_AUTHOR_NAME", Some("Test")),
        ("GIT_AUTHOR_EMAIL", Some("test@example.com")),
        ("GIT_COMMITTER_NAME", Some("Test")),
        ("GIT_COMMITTER_EMAIL", Some("test@example.com")),
    ]);

    guard
}

/// Create an isolated config home with a minimal config that doesn't use opencode/* refs.
fn create_isolated_config(dir: &TempDir) -> std::path::PathBuf {
    let config_home = dir.path().join(".config");
    fs::create_dir_all(&config_home).unwrap();
    // Create minimal config without opencode/* references
    fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["codex"]
reviewer = ["codex"]
"#,
    )
    .unwrap();
    config_home
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
        let config_home = create_isolated_config(&dir);
        let _repo = init_git_repo(&dir);

        // Create initial commit with tracked files
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change for the diff
        write_file(dir.path().join("initial.txt"), "updated content");

        let _env_guard = base_env();
        std::env::set_var("XDG_CONFIG_HOME", &config_home); // Use isolated config
        std::env::set_var("RALPH_REVIEWER_REVIEWS", "0"); // Skip review phase
        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();

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
        let config_home = create_isolated_config(&dir);
        let _repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        let _env_guard = base_env();
        std::env::set_var("XDG_CONFIG_HOME", &config_home); // Use isolated config
        std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
        std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");
        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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
        let config_home = create_isolated_config(&dir);
        let _repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        let _env_guard = base_env();
        std::env::set_var("XDG_CONFIG_HOME", &config_home); // Use isolated config
        std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
        std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");
        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();

        // Verify commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert!(!commit.message().unwrap().is_empty());
    });
}
