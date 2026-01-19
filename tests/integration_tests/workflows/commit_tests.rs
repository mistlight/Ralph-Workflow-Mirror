use predicates::prelude::*;
use std::fs;
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
// Commit Behavior Tests
// ============================================================================

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

        let mut cmd = ralph_cmd();
        base_env(&mut cmd).current_dir(dir.path());

        // Should succeed - auto-commit will generate a message
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Pipeline Complete"));
    });
}

// ============================================================================
// Plumbing Command Tests
// ============================================================================

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

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path()).arg("--show-commit-msg");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("feat: test commit message"));
    });
}

#[test]
fn ralph_show_commit_msg_uses_repo_root_from_subdir() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Root commit message (the one we expect to read)
        fs::write(
            dir.path().join(".agent/commit-message.txt"),
            "feat: root commit message\n",
        )
        .unwrap();

        // Subdir has a different file that should NOT be read (we always chdir to repo root)
        let subdir = dir.path().join("nested/dir");
        fs::create_dir_all(subdir.join(".agent")).unwrap();
        fs::write(
            subdir.join(".agent/commit-message.txt"),
            "feat: WRONG commit message\n",
        )
        .unwrap();

        let mut cmd = ralph_cmd();
        cmd.current_dir(&subdir).arg("--show-commit-msg");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("feat: root commit message"))
            .stdout(predicate::str::contains("WRONG").not());
    });
}

#[test]
fn ralph_show_commit_msg_fails_if_missing() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Don't create commit-message.txt

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path()).arg("--show-commit-msg");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("Failed to read commit message"));
    });
}

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

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path()).arg("--apply-commit");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Commit created successfully"));

        // Verify the commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let msg = head_commit.message().unwrap_or_default();
        assert!(msg.contains("feat: add new file"));

        // Verify commit-message.txt was cleaned up
        assert!(!dir.path().join(".agent/commit-message.txt").exists());
    });
}

#[test]
fn ralph_apply_commit_fails_without_message_file() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Don't create commit-message.txt

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path()).arg("--apply-commit");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("commit-message.txt"));
    });
}
