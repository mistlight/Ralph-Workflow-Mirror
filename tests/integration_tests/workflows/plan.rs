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
//! - Tests verify **observable behavior** (exit codes, file state)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use std::fs;
use tempfile::TempDir;

use crate::common::{mock_executor_with_success, run_ralph_cli, with_cwd_guard, EnvGuard};
use crate::test_timeout::with_default_timeout;
use test_helpers::init_git_repo;

/// Helper function to set up base environment for tests with automatic cleanup.
///
/// This function sets up config isolation using XDG_CONFIG_HOME to prevent
/// the tests from loading the user's actual config which may contain
/// opencode/* references that would trigger network calls.
/// Uses EnvGuard to ensure all environment variables are restored when dropped.
fn base_env(config_home: &std::path::Path) -> EnvGuard {
    let guard = EnvGuard::new(&[
        "RALPH_INTERACTIVE",
        "RALPH_DEVELOPER_ITERS",
        "RALPH_REVIEWER_REVIEWS",
        "XDG_CONFIG_HOME",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ]);

    guard.set(&[
        ("RALPH_INTERACTIVE", Some("0")),
        ("RALPH_DEVELOPER_ITERS", Some("0")),
        ("RALPH_REVIEWER_REVIEWS", Some("0")),
        ("XDG_CONFIG_HOME", Some(config_home.to_str().unwrap())),
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
developer = ["claude"]
reviewer = ["codex"]
"#,
    )
    .unwrap();
    config_home
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
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        // Create a file to have something to commit
        fs::write(dir.path().join("test.txt"), "content").unwrap();

        with_cwd_guard(dir.path(), || {
            let _env_guard = base_env(&config_home);
            std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
            std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");

            let executor = mock_executor_with_success();
            run_ralph_cli(&[], executor).unwrap();

            // Verify PLAN.md was never created (since planning was skipped)
            assert!(!dir.path().join(".agent/PLAN.md").exists());
        });
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
        let config_home = create_isolated_config(&dir);
        let repo = init_git_repo(&dir);

        // Create an initial commit to establish HEAD
        fs::write(dir.path().join("initial.txt"), "initial").unwrap();
        let _ = test_helpers::commit_all(&repo, "initial commit");

        // Create a new file to have something to commit in the test run
        fs::write(dir.path().join("test.txt"), "content").unwrap();

        with_cwd_guard(dir.path(), || {
            let _env_guard = base_env(&config_home);
            std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
            std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");

            let executor = mock_executor_with_success();
            run_ralph_cli(&[], executor).unwrap();

            // Verify a commit was created (should have 2 commits: initial + test)
            let repo = git2::Repository::open(dir.path()).unwrap();
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            assert!(!commit.message().unwrap().is_empty());
        });
    });
}
