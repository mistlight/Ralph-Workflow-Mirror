use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn init_git_repo(dir: &TempDir) {
    let dir_path = dir.path();
    StdCommand::new("git")
        .args(["init"])
        .current_dir(dir_path)
        .output()
        .unwrap();

    fs::write(
        dir_path.join(".gitignore"),
        ".agent/\n.no_agent_commit\nPROMPT.md\n",
    )
    .unwrap();
}

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

#[test]
fn ralph_fails_if_plan_missing() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(".agent/PLAN.md"));
}

#[test]
fn ralph_fails_if_commit_message_missing() {
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

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(".agent/commit-message.txt"));
}

#[test]
fn ralph_cleans_up_on_early_error() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'",
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"chore: test\" > .agent/commit-message.txt'",
        )
        .env("FULL_CHECK_CMD", "false");

    cmd.assert().failure();

    assert!(!dir.path().join(".no_agent_commit").exists());
    assert!(!dir.path().join(".agent/PLAN.md").exists());
    assert!(!dir.path().join(".agent/commit-message.txt").exists());
    assert!(!dir.path().join(".agent/git-wrapper-dir.txt").exists());

    let hooks_dir = dir.path().join(".git/hooks");
    assert!(!hooks_dir.join("pre-commit").exists());
    assert!(!hooks_dir.join("pre-push").exists());
}
