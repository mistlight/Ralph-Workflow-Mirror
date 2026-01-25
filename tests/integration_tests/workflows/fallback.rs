//! Agent execution integration tests.
//!
//! These tests verify agent command execution behavior, including:
//! - Phase skipping with zero iterations
//! - Pipeline behavior without agent execution
//!
//! Note: Tests that require agent execution (developer_iters > 0 or reviewer_reviews > 0)
//! cannot be properly tested without the AgentExecutor trait infrastructure. Those tests
//! should be unit tests with mocked executors at the code level.
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

use tempfile::TempDir;

use crate::common::{mock_executor_with_success, run_ralph_cli, with_cwd_guard, EnvGuard};
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

// ============================================================================
// Agent Command Execution Tests
// ============================================================================

/// Test that setting iterations to zero skips the respective phase.
///
/// This verifies that when a user runs ralph with both developer_iters=0
/// and reviewer_reviews=0, the agent phases are skipped and the pipeline
/// completes successfully without creating agent-related files.
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

        with_cwd_guard(dir.path(), || {
            let _env_guard = base_env();
            std::env::set_var("RALPH_DEVELOPER_ITERS", "0"); // Skip developer
            std::env::set_var("RALPH_REVIEWER_REVIEWS", "0"); // Skip review

            let executor = mock_executor_with_success();
            run_ralph_cli(&[], executor).unwrap();

            // Verify no agent-related files were created (agents weren't called)
            assert!(!dir.path().join(".agent/PLAN.md").exists());
            assert!(!dir.path().join(".agent/ISSUES.md").exists());
        });
    });
}

/// Test that the pipeline succeeds with both developer and review phases skipped.
///
/// This verifies that when a user runs ralph with both developer_iters=0
/// and reviewer_reviews=0, the pipeline completes successfully and a commit
/// is created with a non-empty commit message.
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

        with_cwd_guard(dir.path(), || {
            let _env_guard = base_env();
            std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
            std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");

            let executor = mock_executor_with_success();
            run_ralph_cli(&[], executor).unwrap();

            // Verify a commit was created
            let repo = git2::Repository::open(dir.path()).unwrap();
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            assert!(!commit.message().unwrap().is_empty());
        });
    });
}
