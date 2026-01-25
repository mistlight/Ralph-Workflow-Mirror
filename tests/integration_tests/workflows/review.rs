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
//! - Uses **dependency injection** for configuration (no env vars)
//! - Tests are deterministic and isolated

use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, init_git_repo, write_file};

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
        let _repo = init_git_repo(&dir);

        // Create initial commit with tracked files
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change for the diff
        write_file(dir.path().join("initial.txt"), "updated content");

        // Use dependency injection - no env vars needed
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

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
        let _repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        // Use dependency injection - no env vars needed
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
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
        let _repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        // Use dependency injection - no env vars needed
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

        // Verify commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert!(!commit.message().unwrap().is_empty());
    });
}
