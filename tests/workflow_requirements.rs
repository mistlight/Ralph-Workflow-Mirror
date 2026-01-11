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

    // Create required files for workflow tests
    fs::write(dir_path.join("PROMPT.md"), "# Test Requirements\nTest task").unwrap();

    // Create .agent directory and minimal agents.toml to skip first-run init
    fs::create_dir_all(dir_path.join(".agent")).unwrap();
    fs::write(
        dir_path.join(".agent/agents.toml"),
        "# Minimal test config\n",
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

#[test]
fn ralph_init_creates_config_file() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo but don't create agents.toml
    StdCommand::new("git")
        .args(["init"])
        .current_dir(dir_path)
        .output()
        .unwrap();

    let config_path = dir_path.join(".agent/agents.toml");
    assert!(!config_path.exists());

    // Run ralph --init
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path).arg("--init");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    // Config file should now exist
    assert!(config_path.exists());

    // Verify content contains expected sections
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("Ralph Agents Configuration File"));
    assert!(content.contains("[agents.claude]"));
    assert!(content.contains("[agents.codex]"));

    // Output should indicate file was created
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Created"));
}

#[test]
fn ralph_init_reports_existing_config() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    StdCommand::new("git")
        .args(["init"])
        .current_dir(dir_path)
        .output()
        .unwrap();

    // Create existing config
    fs::create_dir_all(dir_path.join(".agent")).unwrap();
    fs::write(dir_path.join(".agent/agents.toml"), "# Custom config\n").unwrap();

    // Run ralph --init
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path).arg("--init");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    // Config file should still contain original content
    let content = fs::read_to_string(dir_path.join(".agent/agents.toml")).unwrap();
    assert_eq!(content, "# Custom config\n");

    // Output should indicate file already exists
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("already exists"));
}

#[test]
fn ralph_first_run_creates_config_and_exits() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo but don't create agents.toml
    StdCommand::new("git")
        .args(["init"])
        .current_dir(dir_path)
        .output()
        .unwrap();

    // Create PROMPT.md (required)
    fs::write(dir_path.join("PROMPT.md"), "# Test\n").unwrap();

    let config_path = dir_path.join(".agent/agents.toml");
    assert!(!config_path.exists());

    // Run ralph without --init (first run behavior)
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path).env("RALPH_INTERACTIVE", "0");

    let output = cmd.output().unwrap();

    // Should exit successfully (not run the full pipeline)
    assert!(output.status.success());

    // Config file should now exist
    assert!(config_path.exists());

    // Output should prompt user to edit or run again
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No agents.toml found"));
    assert!(stdout.contains("Options"));
}
