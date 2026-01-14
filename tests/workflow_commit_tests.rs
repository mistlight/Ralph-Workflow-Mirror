use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use test_helpers::{commit_all, init_git_repo, write_file};

fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
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
    // With auto-commit behavior, the pipeline should succeed even without
    // a commit-message.txt file since commits are created automatically by
    // the orchestrator after each development iteration and fix cycle.
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    // Should succeed even without commit-message.txt (auto-commit behavior)
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));
}

// ============================================================================
// Plumbing Command Tests
// ============================================================================

#[test]
fn ralph_show_commit_msg_displays_message() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a commit message file
    fs::write(
        dir.path().join(".agent/commit-message.txt"),
        "feat: test commit message\n",
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(dir.path()).arg("--show-commit-msg");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("feat: test commit message"));
}

#[test]
fn ralph_show_commit_msg_uses_repo_root_from_subdir() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(&subdir).arg("--show-commit-msg");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("feat: root commit message"))
        .stdout(predicate::str::contains("WRONG").not());
}

#[test]
fn ralph_show_commit_msg_fails_if_missing() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Don't create commit-message.txt

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(dir.path()).arg("--show-commit-msg");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to read commit message"));
}

#[test]
fn ralph_apply_commit_creates_commit() {
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create an initial commit so the repo has a HEAD.
    write_file(dir.path().join("initial.txt"), "initial");
    commit_all(&repo, "initial");

    // Create a new file to commit
    fs::write(dir.path().join("new_file.txt"), "content").unwrap();

    // Create commit message file
    fs::write(
        dir.path().join(".agent/commit-message.txt"),
        "feat: add new file",
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
}

#[test]
fn ralph_apply_commit_fails_without_message_file() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Don't create commit-message.txt

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(dir.path()).arg("--apply-commit");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("commit-message.txt"));
}

#[test]
fn ralph_generate_commit_msg_creates_message_file() {
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create an initial commit so the repo is not empty
    write_file(dir.path().join("initial.txt"), "initial content");
    commit_all(&repo, "initial commit");

    // Now create a change to test with
    write_file(dir.path().join("initial.txt"), "updated content");

    // Create a script that generates a commit message
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--generate-commit-msg")
        .env(
            "RALPH_DEVELOPER_CMD",
            "/bin/sh -c 'cat >/dev/null; echo \"chore: test commit message\"'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Commit message generated"));

    // Verify the file was created and contains something meaningful
    let content = fs::read_to_string(dir.path().join(".agent/commit-message.txt")).unwrap();
    // The commit message should be non-empty
    assert!(!content.trim().is_empty());
    // It should contain some form of commit message (not JSON metadata)
    assert!(content.contains("chore") || content.contains("test") || content.contains("commit"));
}

#[test]
fn ralph_generate_commit_msg_fails_if_agent_doesnt_create_file() {
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create an initial commit so there is a HEAD to diff against.
    write_file(dir.path().join("initial.txt"), "initial content");
    commit_all(&repo, "initial commit");

    // Create a change in the repository to have something to diff.
    write_file(dir.path().join("initial.txt"), "updated content");

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--generate-commit-msg")
        // Agent that fails (returns non-zero exit code)
        .env("RALPH_DEVELOPER_CMD", "sh -c 'echo error >&2; exit 1'");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to generate commit message",
    ));
}
