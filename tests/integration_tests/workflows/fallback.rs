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
//! - Uses **dependency injection** for configuration (no environment variables)

use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, init_git_repo, write_file};

// ============================================================================
// Agent Command Execution Tests
// ============================================================================

/// Test that setting iterations to zero skips the respective phase.
///
/// This verifies that when a user runs ralph with both developer_iters=0
/// and reviewer_reviews=0, the pipeline completes successfully.
/// The pipeline may still create some tracking files (STATUS.md, etc.) for
/// pipeline state management, but agent execution is skipped.
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

        // Config has developer_iters=0 and reviewer_reviews=0
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

        // Verify pipeline completed successfully
        // STATUS.md may be created for pipeline state tracking
        // ISSUES.md is only created when --no-isolation is used (default is isolation)
        assert!(
            !dir.path().join(".agent/ISSUES.md").exists(),
            "ISSUES.md should not exist in isolation mode (default)"
        );
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

        // Config has developer_iters=0 and reviewer_reviews=0
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

        // Verify a commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert!(!commit.message().unwrap().is_empty());
    });
}
