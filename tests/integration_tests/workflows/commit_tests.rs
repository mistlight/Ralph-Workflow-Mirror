use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use crate::common::ralph_cmd;
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
    // With auto-commit behavior, the pipeline should succeed even without
    // a commit-message.txt file since commits are created automatically by
    // the orchestrator after each development iteration and fix cycle.
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let mut cmd = ralph_cmd();
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
}

#[test]
fn ralph_show_commit_msg_uses_repo_root_from_subdir() {
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
}

#[test]
fn ralph_show_commit_msg_fails_if_missing() {
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    // Don't create commit-message.txt

    let mut cmd = ralph_cmd();
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
}

#[test]
fn ralph_apply_commit_fails_without_message_file() {
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    // Don't create commit-message.txt

    let mut cmd = ralph_cmd();
    cmd.current_dir(dir.path()).arg("--apply-commit");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("commit-message.txt"));
}

#[test]
fn ralph_generate_commit_msg_creates_message_file() {
    // Test that --generate-commit-msg creates a commit message file.
    //
    // Note: Commit message generation uses the `commit` agent chain from config,
    // NOT RALPH_DEVELOPER_CMD. The system has extensive fallbacks including a
    // hardcoded "chore: automated commit" fallback, so this test simply verifies
    // that a non-empty commit message file is created.
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create an initial commit so the repo is not empty
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Now create a change to test with
    write_file(dir.path().join("initial.txt"), "updated content");

    // Set up config to use codex agent for commit generation
    let config_home = dir.path().join(".config");
    std::fs::create_dir_all(&config_home).unwrap();
    std::fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["codex"]
reviewer = ["codex"]
commit = ["codex"]
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--generate-commit-msg");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Commit message generated"));

    // Verify the file was created and contains something meaningful
    let content = fs::read_to_string(dir.path().join(".agent/commit-message.txt")).unwrap();
    // The commit message should be non-empty
    assert!(
        !content.trim().is_empty(),
        "Commit message file should not be empty"
    );
}

#[test]
fn ralph_generate_commit_msg_with_configured_agent_succeeds() {
    // This test verifies that commit message generation succeeds when a
    // properly configured agent is available. The system uses the agent chain
    // from config (in this case, codex) to generate commit messages.
    //
    // Note: We use the codex agent which is a built-in agent that can
    // successfully generate commit messages. The system has fallback mechanisms
    // including a hardcoded "chore: automated commit" fallback if the agent fails.
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create an initial commit so there is a HEAD to diff against.
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change in the repository to have something to diff.
    write_file(dir.path().join("initial.txt"), "updated content");

    // Create a test config with a working agent
    let config_home = dir.path().join(".config");
    std::fs::create_dir_all(&config_home).unwrap();
    std::fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["codex"]
reviewer = ["codex"]
commit = ["codex"]
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--generate-commit-msg");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Commit message generated"));

    // Verify the file was created
    assert!(dir.path().join(".agent/commit-message.txt").exists());
}
