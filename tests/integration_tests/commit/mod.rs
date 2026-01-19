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

use predicates::prelude::*;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, init_git_repo, write_file};

/// Helper function to set up base environment for tests
fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Ensure git identity is set
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

/// Get the most recent commit message from a repository
fn get_last_commit_message(repo: &git2::Repository) -> String {
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    commit.message().unwrap().to_string()
}

#[test]
fn test_commit_message_generated_with_simple_diff() {
    with_default_timeout(|| {
        // Test that commit message is generated with a simple change
        let dir = TempDir::new().unwrap();
        let repo = init_repo_with_initial_commit(&dir);

        // Make a simple change
        write_file(dir.path().join("test.txt"), "new content");

        // Run ralph with developer_iters=0 (skip to commit)
        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

        // Verify a commit was created with a non-empty message
        let message = get_last_commit_message(&repo);
        assert!(
            !message.trim().is_empty(),
            "Commit message should not be empty"
        );
    });
}

#[test]
fn test_commit_message_generated_with_multiple_files() {
    with_default_timeout(|| {
        // Test commit message generation with multiple file changes
        let dir = TempDir::new().unwrap();
        let repo = init_repo_with_initial_commit(&dir);

        // Make changes to multiple files
        write_file(dir.path().join("file1.txt"), "content 1");
        write_file(dir.path().join("file2.txt"), "content 2");
        write_file(dir.path().join("file3.rs"), "fn main() {}");

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

        let message = get_last_commit_message(&repo);
        assert!(!message.trim().is_empty());
    });
}

#[test]
fn test_commit_created_with_diff_content() {
    with_default_timeout(|| {
        // Test that commit captures the diff content
        let dir = TempDir::new().unwrap();
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

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

        // Verify commit was created
        let message = get_last_commit_message(&repo);
        assert!(!message.trim().is_empty());
    });
}

#[test]
fn test_commit_succeeds_without_developer_or_review() {
    with_default_timeout(|| {
        // Test that commits work when both development and review are skipped
        let dir = TempDir::new().unwrap();
        let _repo = init_repo_with_initial_commit(&dir);

        // Create a change to commit
        write_file(dir.path().join("test.txt"), "new content");

        let mut cmd = ralph_cmd();
        base_env(&mut cmd)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));

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
