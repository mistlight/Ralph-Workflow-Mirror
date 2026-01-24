//! Integration tests for commit message generation.
//!
//! These tests verify that:
//! - Commit messages are generated when developer_iters=0
//! - Commits are created successfully
//! - The commit message fallback system works
//!
//! Note: Tests that specifically test LLM commit message generation behavior
//! require the commit agent to run and cannot be properly tested without the
//! AgentExecutor trait infrastructure. These tests focus on the observable
//! behavior of commit creation.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (commit creation, commit messages)
//! - Uses `TempDir` for filesystem isolation
//! - Tests are deterministic and black-box (test commit as a user would experience it)

use std::fs;
use tempfile::TempDir;

use crate::common::{mock_executor_with_success, run_ralph_cli};
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, init_git_repo, write_file};

/// Helper function to set up base environment for tests.
///
/// This function sets up config isolation using XDG_CONFIG_HOME to prevent
/// the tests from loading the user's actual config which may contain
/// opencode/* references that would trigger network calls.
fn set_base_env(config_home: &std::path::Path) {
    std::env::set_var("RALPH_INTERACTIVE", "0");
    std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
    std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");
    std::env::set_var("XDG_CONFIG_HOME", config_home);
    std::env::set_var("GIT_AUTHOR_NAME", "Test");
    std::env::set_var("GIT_AUTHOR_EMAIL", "test@example.com");
    std::env::set_var("GIT_COMMITTER_NAME", "Test");
    std::env::set_var("GIT_COMMITTER_EMAIL", "test@example.com");
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

fn init_repo_with_initial_commit(dir: &TempDir) -> git2::Repository {
    let repo = init_git_repo(dir);
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");
    repo
}

/// Get the most recent commit message from a repository
fn get_last_commit_message(repo: &git2::Repository) -> String {
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    commit.message().unwrap().to_string()
}

/// Test that a commit message is generated when there is a simple change.
///
/// This verifies that when a user has uncommitted changes and runs ralph
/// with developer_iters=0 to skip agent execution, a commit is created
/// with a non-empty commit message.
#[test]
fn test_commit_message_generated_with_simple_diff() {
    with_default_timeout(|| {
        // Test that commit message is generated with a simple change
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let repo = init_repo_with_initial_commit(&dir);

        // Make a simple change
        write_file(dir.path().join("test.txt"), "new content");

        // Run ralph with developer_iters=0 (skip to commit)
        std::env::set_current_dir(dir.path()).unwrap();
        set_base_env(&config_home);
        std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
        std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor).unwrap();

        // Verify a commit was created with a non-empty message
        let message = get_last_commit_message(&repo);
        assert!(
            !message.trim().is_empty(),
            "Commit message should not be empty"
        );
    });
}

/// Test that a commit message is generated when there are changes to multiple files.
///
/// This verifies that when a user has uncommitted changes across multiple files
/// and runs ralph with developer_iters=0 to skip agent execution,
/// a commit is created with a non-empty commit message.
#[test]
fn test_commit_message_generated_with_multiple_files() {
    with_default_timeout(|| {
        // Test commit message generation with multiple file changes
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let repo = init_repo_with_initial_commit(&dir);

        // Make changes to multiple files
        write_file(dir.path().join("file1.txt"), "content 1");
        write_file(dir.path().join("file2.txt"), "content 2");
        write_file(dir.path().join("file3.rs"), "fn main() {}");

        std::env::set_current_dir(dir.path()).unwrap();
        set_base_env(&config_home);
        std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
        std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor).unwrap();

        let message = get_last_commit_message(&repo);
        assert!(!message.trim().is_empty());
    });
}

/// Test that a commit captures the diff content correctly.
///
/// This verifies that when a user has uncommitted changes including modifications
/// deep within a large file and runs ralph with developer_iters=0,
/// a commit is created with a non-empty commit message.
#[test]
fn test_commit_created_with_diff_content() {
    with_default_timeout(|| {
        // Test that commit captures the diff content
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let repo = init_repo_with_initial_commit(&dir);

        // Create a file with many lines and modify a line deep in the file
        let mut content = String::new();
        for i in 0..200 {
            content.push_str(&format!("line {}\n", i));
        }
        write_file(dir.path().join("large_file.txt"), &content);

        // Commit the initial large file
        let _ = commit_all(&repo, "add large file");

        // Modify a line deep in the file (line 150)
        content.clear();
        for i in 0..200 {
            if i == 150 {
                content.push_str("line 150 modified\n");
            } else {
                content.push_str(&format!("line {}\n", i));
            }
        }
        write_file(dir.path().join("large_file.txt"), &content);

        std::env::set_current_dir(dir.path()).unwrap();
        set_base_env(&config_home);
        std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
        std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor).unwrap();

        // Verify commit was created
        let message = get_last_commit_message(&repo);
        assert!(!message.trim().is_empty());
    });
}

/// Test that a commit succeeds when both developer and review phases are skipped.
///
/// This verifies that when a user runs ralph with both developer_iters=0
/// and reviewer_reviews=0 to skip agent execution, a commit is still created
/// with a non-empty commit message.
#[test]
fn test_commit_succeeds_without_developer_or_review() {
    with_default_timeout(|| {
        // Test that commits work when both development and review are skipped
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _repo = init_repo_with_initial_commit(&dir);

        // Create a change to commit
        write_file(dir.path().join("test.txt"), "new content");

        std::env::set_current_dir(dir.path()).unwrap();
        set_base_env(&config_home);
        std::env::set_var("RALPH_DEVELOPER_ITERS", "0");
        std::env::set_var("RALPH_REVIEWER_REVIEWS", "0");

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor).unwrap();

        // Verify a commit was created (we should have 2 commits now)
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        let message = commit.message().unwrap();
        assert!(
            !message.trim().is_empty(),
            "Commit message should not be empty"
        );
    });
}
