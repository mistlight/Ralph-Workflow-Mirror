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

    // Create .agent directory and minimal agents.toml with required agent_chain
    fs::create_dir_all(dir_path.join(".agent")).unwrap();
    fs::write(
        dir_path.join(".agent/agents.toml"),
        r#"# Minimal test config
[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#,
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
        // Need at least 1 developer iteration to trigger planning phase
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Developer command that doesn't create PLAN.md
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
    assert!(content.contains("[agent_chain]"));

    // Output should indicate file was created
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Created"));
}

#[test]
fn ralph_uses_agent_chain_first_entries_as_defaults() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Ensure no explicit agent selection via env is in play.
    // base_env doesn't set RALPH_DEVELOPER_AGENT / RALPH_REVIEWER_AGENT.
    fs::write(
        dir.path().join(".agent/agents.toml"),
        r#"[agent_chain]
developer = ["opencode", "claude"]
reviewer = ["aider", "codex"]
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("opencode"))
        .stdout(predicate::str::contains("aider"));
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

    // Create existing config with valid agent_chain
    fs::create_dir_all(dir_path.join(".agent")).unwrap();
    let custom_config = r#"# Custom config
[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#;
    fs::write(dir_path.join(".agent/agents.toml"), custom_config).unwrap();

    // Run ralph --init
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path).arg("--init");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    // Config file should still contain original content
    let content = fs::read_to_string(dir_path.join(".agent/agents.toml")).unwrap();
    assert_eq!(content, custom_config);

    // Output should indicate file already exists
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("already exists"));
}

// ============================================================================
// PLAN Workflow Tests
// ============================================================================

#[test]
fn ralph_skips_plan_when_zero_developer_iters() {
    // When developer_iters=0, planning phase should be skipped entirely
    // and the workflow should complete successfully if commit message is provided
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Developer command doesn't create PLAN.md - should still work since plan is skipped
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    // Should succeed - plan phase is skipped when developer_iters=0
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify PLAN.md was never created (since planning was skipped)
    assert!(!dir.path().join(".agent/PLAN.md").exists());
}

#[test]
fn ralph_fails_on_empty_plan() {
    // Empty PLAN.md should be rejected
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Create an empty PLAN.md (whitespace only)
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo \"   \" > .agent/PLAN.md'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("PLAN.md"));
}

#[test]
fn ralph_plan_deleted_after_iteration() {
    // PLAN.md should be deleted after each iteration completes
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a script that creates PLAN.md on first call
    let script_path = dir.path().join("dev_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
# Check if we're in planning (PLAN.md doesn't exist) or executing (it does)
if [ ! -f .agent/PLAN.md ]; then
    echo "Step 1: Do the thing" > .agent/PLAN.md
fi
exit 0
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", script_path.display()),
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // PLAN.md should be deleted after iteration
    assert!(!dir.path().join(".agent/PLAN.md").exists());
}

#[test]
fn ralph_runs_planning_for_each_iteration() {
    // Each developer iteration should run planning -> execution -> cleanup
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a script that tracks how many times it's called
    let counter_path = dir.path().join(".agent/call_counter");
    let script_path = dir.path().join("dev_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
# Increment counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Create PLAN.md if it doesn't exist (planning phase)
if [ ! -f .agent/PLAN.md ]; then
    echo "Plan for iteration" > .agent/PLAN.md
fi
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "2") // 2 iterations
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", script_path.display()),
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // Developer command should be called 4 times:
    // - Iteration 1: plan + execute = 2 calls
    // - Iteration 2: plan + execute = 2 calls
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 4,
        "Expected 4 developer calls (2 iterations × 2 phases)"
    );
}

// ============================================================================
// Config and Init Tests
// ============================================================================

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
    init_git_repo(&dir);

    // Create initial commit so we have a branch
    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();

    // Create a new file to commit
    fs::write(dir.path().join("new_file.txt"), "content").unwrap();

    // Create commit message file
    fs::write(
        dir.path().join(".agent/commit-message.txt"),
        "feat: add new file",
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(dir.path())
        .arg("--apply-commit")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Commit created successfully"));

    // Verify the commit was created
    let log_output = StdCommand::new("git")
        .args(["log", "--oneline", "-1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let log_str = String::from_utf8_lossy(&log_output.stdout);
    assert!(log_str.contains("feat: add new file"));

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
    init_git_repo(&dir);

    // Create a script that generates a commit message
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--generate-commit-msg")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: auto-generated message\" > .agent/commit-message.txt'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Commit message generated"))
        .stdout(predicate::str::contains("feat: auto-generated message"));

    // Verify the file was created
    let content = fs::read_to_string(dir.path().join(".agent/commit-message.txt")).unwrap();
    assert!(content.contains("feat: auto-generated message"));
}

#[test]
fn ralph_generate_commit_msg_fails_if_agent_doesnt_create_file() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--generate-commit-msg")
        // Agent that doesn't create the file
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Commit message generation failed"));
}

// ============================================================================
// Quick Mode Tests
// ============================================================================

#[test]
fn ralph_quick_mode_sets_minimal_iterations() {
    // Quick mode should set developer_iters=1 and reviewer_reviews=1
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a script that tracks how many times planning is called
    let counter_path = dir.path().join(".agent/plan_counter");
    let script_path = dir.path().join("dev_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
# Only count planning phase calls (when PLAN.md doesn't exist)
if [ ! -f .agent/PLAN.md ]; then
    if [ -f "{counter}" ]; then
        count=$(cat "{counter}")
        count=$((count + 1))
    else
        count=1
    fi
    echo $count > "{counter}"
    echo "Plan for iteration" > .agent/PLAN.md
fi
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(dir.path())
        .arg("--quick") // Use quick mode
        .env("RALPH_INTERACTIVE", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", script_path.display()),
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: quick test\" > .agent/commit-message.txt'",
        )
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();

    // Should only have 1 planning call (quick mode = 1 iteration)
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 1,
        "Quick mode should result in exactly 1 developer iteration"
    );
}

#[test]
fn ralph_quick_mode_short_flag_works() {
    // -Q should work the same as --quick
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let counter_path = dir.path().join(".agent/plan_counter");
    let script_path = dir.path().join("dev_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
if [ ! -f .agent/PLAN.md ]; then
    if [ -f "{counter}" ]; then
        count=$(cat "{counter}")
        count=$((count + 1))
    else
        count=1
    fi
    echo $count > "{counter}"
    echo "Plan" > .agent/PLAN.md
fi
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(dir.path())
        .arg("-Q") // Short flag
        .env("RALPH_INTERACTIVE", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", script_path.display()),
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: short flag\" > .agent/commit-message.txt'",
        )
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();

    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 1,
        "-Q should result in exactly 1 developer iteration"
    );
}

#[test]
fn ralph_quick_mode_explicit_iters_override() {
    // Explicit --developer-iters should override quick mode
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let counter_path = dir.path().join(".agent/plan_counter");
    let script_path = dir.path().join("dev_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
if [ ! -f .agent/PLAN.md ]; then
    if [ -f "{counter}" ]; then
        count=$(cat "{counter}")
        count=$((count + 1))
    else
        count=1
    fi
    echo $count > "{counter}"
    echo "Plan" > .agent/PLAN.md
fi
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    cmd.current_dir(dir.path())
        .arg("--quick")
        .arg("--developer-iters")
        .arg("2") // Explicit override
        .env("RALPH_INTERACTIVE", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", script_path.display()),
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: override\" > .agent/commit-message.txt'",
        )
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();

    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 2,
        "Explicit --developer-iters should override quick mode"
    );
}
