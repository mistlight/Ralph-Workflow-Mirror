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

    // Create .agent directory for workflow artifacts
    fs::create_dir_all(dir_path.join(".agent")).unwrap();
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

    // Run ralph --init-legacy
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path).arg("--init-legacy");

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
    let config_home = dir.path().join(".config");
    fs::create_dir_all(&config_home).unwrap();
    fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["opencode", "claude"]
reviewer = ["aider", "codex"]
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("OpenCode"))
        .stdout(predicate::str::contains("Aider"));
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

    // Run ralph --init-legacy
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path).arg("--init-legacy");

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

    // Use a temp config dir so the test doesn't touch the real home directory.
    let config_home = dir_path.join(".config");
    fs::create_dir_all(&config_home).unwrap();

    let unified_config_path = config_home.join("ralph-workflow.toml");
    assert!(!unified_config_path.exists());

    // Run ralph --init-global (unified config)
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path)
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--init-global");

    let output = cmd.output().unwrap();

    // Should exit successfully after creating the config
    assert!(output.status.success());

    // Unified config file should now exist
    assert!(unified_config_path.exists());

    // Output should indicate file was created or already exists
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("unified config"));
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

#[test]
fn ralph_resume_continues_from_checkpoint_phase() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let dev_script_path = dir.path().join("dev_script.sh");
    fs::write(
        &dev_script_path,
        r#"#!/bin/sh
mkdir -p .agent
case "$1" in
  *"PLANNING MODE"*)
    echo "Plan" > .agent/PLAN.md
    ;;
  *)
    echo "ran" > ran.txt
    ;;
esac
exit 0
"#,
    )
    .unwrap();

    // First run: do not create commit-message.txt so the pipeline errors late,
    // leaving a checkpoint that we can resume from.
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_DEVELOPER_CMD",
            format!("sh {}", dev_script_path.display()),
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(".agent/commit-message.txt"));

    let checkpoint_path = dir.path().join(".agent/checkpoint.json");
    assert!(
        checkpoint_path.exists(),
        "checkpoint should exist after failure"
    );
    let checkpoint = fs::read_to_string(&checkpoint_path).unwrap();
    assert!(
        checkpoint.contains("\"phase\": \"CommitMessage\""),
        "checkpoint should indicate CommitMessage phase"
    );

    let reviewer_script_path = dir.path().join("reviewer_script.sh");
    fs::write(
        &reviewer_script_path,
        r#"#!/bin/sh
mkdir -p .agent
case "$1" in
  *"Generate a commit message for all changes made."*)
    echo "chore: resume checkpoint" > .agent/commit-message.txt
    ;;
esac
exit 0
"#,
    )
    .unwrap();

    // Second run: resume and ensure we don't re-run the developer phase.
    let mut resume_cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut resume_cmd)
        .current_dir(dir.path())
        .arg("--resume")
        .env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 42'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", reviewer_script_path.display()),
        );

    resume_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("RESUME: Loading Checkpoint"));

    assert!(
        dir.path().join("ran.txt").exists(),
        "work output should exist"
    );
    assert!(
        !checkpoint_path.exists(),
        "checkpoint should be cleared after successful completion"
    );
}

// ============================================================================
// Review Workflow Integration Tests
// ============================================================================

#[test]
fn ralph_creates_issues_md_during_review() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create review script
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
cat > .agent/ISSUES.md << 'ISSUES_EOF'
# Review Issues

- [ ] High: [src/main.rs:42] Memory leak detected
- [x] Low: Code style suggestion

ISSUES_EOF
echo "feat: reviewed" > .agent/commit-message.txt
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // ISSUES.md should exist after review
    assert!(dir.path().join(".agent/ISSUES.md").exists());
}

#[test]
fn ralph_review_workflow_with_no_issues() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create review script
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
cat > .agent/ISSUES.md << 'ISSUES_EOF'
# Review Complete

No issues found. Code looks good!

ISSUES_EOF
echo "feat: clean code" > .agent/commit-message.txt
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));
}

#[test]
fn ralph_review_multiple_passes() {
    // Test that RALPH_REVIEWER_REVIEWS=N runs exactly N review-fix cycles
    // N=0 means no review, N=1 means 1 review+fix, N=2 means 2 cycles, etc.
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let counter_path = dir.path().join(".agent/review_counter");
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
# Increment review counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Create commit message
echo "feat: review pass $count" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "3") // 3 review-fix cycles
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // With RALPH_REVIEWER_REVIEWS=3, the reviewer is called:
    // - Cycle 1: review + fix = 2 calls
    // - Cycle 2: review + fix = 2 calls
    // - Cycle 3: review + fix = 2 calls
    // - Commit message generation = 1 call
    // = 7 total calls (3 × 2 + 1)
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 7,
        "Expected 7 reviewer calls (3 × (review + fix) + 1 commit msg)"
    );
}

#[test]
fn ralph_stack_detection_rust_project() {
    // Test that stack detection works in an integration context
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a Rust project structure
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
tokio = "1.0"
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::create_dir_all(dir.path().join("tests")).unwrap();
    fs::write(dir.path().join("tests/test.rs"), "#[test] fn it_works() {}").unwrap();

    // Run ralph with verbose output to see stack detection
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "true")
        .env("RALPH_VERBOSITY", "2") // Verbose mode
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: rust\" > .agent/commit-message.txt'",
        );

    // Pipeline should complete and potentially mention Rust stack
    cmd.assert().success();
}

#[test]
fn ralph_stack_detection_javascript_project() {
    // Test stack detection for a JavaScript/React project
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a JavaScript/React project structure
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "name": "test",
  "dependencies": {
    "react": "^18.0.0"
  },
  "devDependencies": {
    "jest": "^29.0.0"
  }
}"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/App.jsx"),
        "export default () => <div />",
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "true")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: react\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();
}

#[test]
fn ralph_stack_detection_disabled() {
    // Test that stack detection can be disabled
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a project structure
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "test"
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "false") // Explicitly disable
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: no stack\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();
}

#[test]
fn ralph_review_depth_standard() {
    // Test standard review depth
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "standard")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: standard\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();
}

#[test]
fn ralph_review_depth_comprehensive() {
    // Test comprehensive review depth
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "comprehensive")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: thorough\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();
}

#[test]
fn ralph_review_depth_security() {
    // Test security-focused review depth
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "security")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: secure\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();
}

#[test]
fn ralph_review_depth_incremental() {
    // Test incremental review depth (focuses on git diff)
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "incremental")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: incremental\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();
}

#[test]
fn ralph_mixed_language_project() {
    // Test stack detection with multiple languages
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a mixed-language project (Rust backend + Python scripts)
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "backend"
version = "0.1.0"
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    fs::create_dir_all(dir.path().join("scripts")).unwrap();
    fs::write(dir.path().join("scripts/deploy.py"), "print('deploy')").unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "true")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: mixed\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();
}

// ============================================================================
// Error Handling and Recovery Tests
// ============================================================================

#[test]
fn ralph_handles_agent_timeout_gracefully() {
    // Test that ralph handles slow/hanging agents with timeout
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Use a short timeout for testing
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // This should complete quickly (no actual sleep in testing)
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'",
        )
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: timeout test\" > .agent/commit-message.txt'",
        );

    // Should complete successfully
    cmd.assert().success();
}

#[test]
fn ralph_handles_invalid_json_in_config() {
    // Test recovery from malformed config
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    StdCommand::new("git")
        .args(["init"])
        .current_dir(dir_path)
        .output()
        .unwrap();

    // Create PROMPT.md
    fs::write(dir_path.join("PROMPT.md"), "# Test\n").unwrap();

    // Create malformed agents.toml (invalid TOML)
    fs::create_dir_all(dir_path.join(".agent")).unwrap();
    fs::write(
        dir_path.join(".agent/agents.toml"),
        "this is not valid { toml ] syntax",
    )
    .unwrap();

    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_ralph"));
    cmd.current_dir(dir_path)
        .env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    let output = cmd.output().unwrap();

    // Should fail gracefully with an error message about config
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to load config")
            || stderr.contains("parse")
            || stderr.contains("TOML")
            || stderr.contains("error")
    );
}

// ============================================================================
// Isolation Mode Tests
// ============================================================================

#[test]
fn ralph_isolation_mode_does_not_create_status_notes_issues() {
    // Isolation mode (default) should NOT create STATUS.md, NOTES.md or ISSUES.md
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // STATUS.md, NOTES.md and ISSUES.md should NOT exist in isolation mode (default)
    assert!(
        !dir.path().join(".agent/STATUS.md").exists(),
        "STATUS.md should not be created in isolation mode"
    );
    assert!(
        !dir.path().join(".agent/NOTES.md").exists(),
        "NOTES.md should not be created in isolation mode"
    );
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should not be created in isolation mode"
    );
}

#[test]
fn ralph_isolation_mode_deletes_existing_status_notes_issues() {
    // Isolation mode should DELETE existing STATUS.md, NOTES.md and ISSUES.md
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Pre-create STATUS.md, NOTES.md and ISSUES.md
    fs::write(dir.path().join(".agent/STATUS.md"), "old status").unwrap();
    fs::write(dir.path().join(".agent/NOTES.md"), "old notes").unwrap();
    fs::write(dir.path().join(".agent/ISSUES.md"), "old issues").unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // Files should be deleted
    assert!(
        !dir.path().join(".agent/STATUS.md").exists(),
        "STATUS.md should be deleted in isolation mode"
    );
    assert!(
        !dir.path().join(".agent/NOTES.md").exists(),
        "NOTES.md should be deleted in isolation mode"
    );
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted in isolation mode"
    );
}

#[test]
fn ralph_no_isolation_creates_status_notes_issues() {
    // --no-isolation flag should create STATUS.md, NOTES.md and ISSUES.md
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--no-isolation")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // STATUS.md, NOTES.md and ISSUES.md should exist when not in isolation mode
    assert!(
        dir.path().join(".agent/STATUS.md").exists(),
        "STATUS.md should be created when --no-isolation is used"
    );
    assert!(
        dir.path().join(".agent/NOTES.md").exists(),
        "NOTES.md should be created when --no-isolation is used"
    );
    assert!(
        dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be created when --no-isolation is used"
    );
}

#[test]
fn ralph_isolation_mode_env_false_creates_status_notes_issues() {
    // RALPH_ISOLATION_MODE=0 should create STATUS.md, NOTES.md and ISSUES.md
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_ISOLATION_MODE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // STATUS.md, NOTES.md and ISSUES.md should exist when isolation mode is disabled via env
    assert!(
        dir.path().join(".agent/STATUS.md").exists(),
        "STATUS.md should be created when RALPH_ISOLATION_MODE=0"
    );
    assert!(
        dir.path().join(".agent/NOTES.md").exists(),
        "NOTES.md should be created when RALPH_ISOLATION_MODE=0"
    );
    assert!(
        dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be created when RALPH_ISOLATION_MODE=0"
    );
}

#[test]
fn ralph_no_isolation_overwrites_existing_status_notes_issues() {
    // --no-isolation should overwrite/truncate STATUS.md, NOTES.md and ISSUES.md
    // to a single vague sentence, to prevent detailed context from persisting.
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Pre-create STATUS.md, NOTES.md and ISSUES.md with detailed multi-line content.
    fs::write(
        dir.path().join(".agent/STATUS.md"),
        "Planning.\nDid X.\nDid Y.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".agent/NOTES.md"),
        "Lots of context.\nDetails.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".agent/ISSUES.md"),
        "Issue A: details.\nIssue B: details.\n",
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--no-isolation")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent; echo \"feat: test\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // Files should exist (non-isolation mode), but should be overwritten to 1 line.
    assert_eq!(
        fs::read_to_string(dir.path().join(".agent/STATUS.md")).unwrap(),
        "In progress.\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".agent/NOTES.md")).unwrap(),
        "Notes.\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".agent/ISSUES.md")).unwrap(),
        "No issues recorded.\n"
    );

    // No archived context should be left behind.
    assert!(
        !dir.path().join(".agent/archive").exists(),
        ".agent/archive should not be created during cleanup"
    );
}

#[test]
fn ralph_cleanup_on_interrupt_simulation() {
    // Test that cleanup happens even when the pipeline fails at various stages
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "1")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Create PLAN.md but then fail the next step
        .env(
            "RALPH_DEVELOPER_CMD",
            "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md; exit 1'",
        )
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert().failure();

    // Cleanup should have removed workflow artifacts
    assert!(!dir.path().join(".no_agent_commit").exists());
}

// ============================================================================
// Review Cycle Count Tests
// ============================================================================

#[test]
fn ralph_reviewer_reviews_zero_skips_review() {
    // Test that N=0 skips review phase entirely
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let counter_path = dir.path().join(".agent/review_counter");
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
# Increment review counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Create commit message (required for pipeline to complete)
echo "feat: commit" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0") // N=0 should skip review
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // With RALPH_REVIEWER_REVIEWS=0, only commit message generation runs
    // = 1 total call (just commit message)
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 1,
        "Expected 1 reviewer call (only commit message generation, no review cycles)"
    );
}

#[test]
fn ralph_reviewer_reviews_one_runs_single_cycle() {
    // Test that N=1 runs exactly one review-fix cycle
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let counter_path = dir.path().join(".agent/review_counter");
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
# Increment review counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Create commit message
echo "feat: cycle $count" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1") // N=1 should run one cycle
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // With RALPH_REVIEWER_REVIEWS=1:
    // - Cycle 1: review + fix = 2 calls
    // - Commit message = 1 call
    // = 3 total calls
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 3,
        "Expected 3 reviewer calls (1 × (review + fix) + 1 commit msg)"
    );
}

#[test]
fn ralph_isolation_mode_deletes_issues_after_fix() {
    // Test that ISSUES.md is deleted after the final fix in isolation mode
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Script that creates ISSUES.md during review but not during commit message generation
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent

# Only create ISSUES.md if it doesn't exist (i.e., during review phase)
# The commit message generation phase should NOT recreate ISSUES.md
if [ ! -f .agent/commit-message.txt ]; then
    # This is a review or fix phase
    echo "- [ ] Critical: [src/main.rs:42] Bug found" > .agent/ISSUES.md
fi

# Create commit message (always, for all phases)
echo "feat: test" > .agent/commit-message.txt
exit 0
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_ISOLATION_MODE", "true") // Isolation mode (default)
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // In isolation mode, ISSUES.md should be deleted after the final fix
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after final fix in isolation mode"
    );
}

#[test]
fn ralph_non_isolation_mode_keeps_issues_after_fix() {
    // Test that ISSUES.md is preserved after the final fix when NOT in isolation mode
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
# Create ISSUES.md during review
echo "- [ ] Critical: [src/main.rs:42] Bug found" > .agent/ISSUES.md
# Create commit message
echo "feat: test" > .agent/commit-message.txt
exit 0
"#,
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_ISOLATION_MODE", "false") // Non-isolation mode
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // In non-isolation mode, ISSUES.md should persist
    assert!(
        dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should persist after final fix in non-isolation mode"
    );
}

// =============================================================================
// ISSUES.md Edge Case Tests
// =============================================================================

#[test]
fn ralph_issues_persists_between_review_and_fix_phases() {
    // Test that ISSUES.md created during Review is readable during Fix phase
    // within the SAME cycle. This is critical for the review-fix cycle to work.
    // Note: ISSUES.md is deleted AFTER each fix, not between review and fix.
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Create a marker file to track which phases have run
    let phase_log = dir.path().join(".agent/phase_log.txt");
    let call_counter = dir.path().join(".agent/call_counter");

    // Script that:
    // - Review phase (call 1): creates ISSUES.md
    // - Fix phase (call 2): reads ISSUES.md and logs its presence
    // - Commit msg phase (call 3): does NOT create ISSUES.md
    // We use a counter to distinguish phases since ISSUES.md gets deleted between calls
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Log state and handle each phase
case $count in
    1)
        # Review phase: create ISSUES.md
        echo "REVIEW: Creating ISSUES.md" >> "{phase_log}"
        echo "- [ ] High: [src/main.rs:10] Found bug" > .agent/ISSUES.md
        ;;
    2)
        # Fix phase: ISSUES.md should exist from review
        if [ -f .agent/ISSUES.md ]; then
            echo "FIX: ISSUES.md exists" >> "{phase_log}"
        else
            echo "FIX: ERROR - ISSUES.md missing!" >> "{phase_log}"
            exit 1
        fi
        ;;
    3)
        # Commit message phase: do NOT create ISSUES.md
        echo "COMMIT: Not creating ISSUES.md" >> "{phase_log}"
        ;;
esac

# Always create commit message
echo "feat: test" > .agent/commit-message.txt
exit 0
"#,
            counter = call_counter.display(),
            phase_log = phase_log.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1") // 1 review-fix cycle
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // Verify phase log shows both Review and Fix phases ran,
    // and Fix phase could see ISSUES.md
    let log_content = fs::read_to_string(&phase_log).unwrap();
    assert!(
        log_content.contains("REVIEW: Creating ISSUES.md"),
        "Review phase should have created ISSUES.md. Log: {}",
        log_content
    );
    assert!(
        log_content.contains("FIX: ISSUES.md exists"),
        "Fix phase should have seen ISSUES.md. Log: {}",
        log_content
    );

    // After completion in isolation mode, ISSUES.md should be cleaned up
    // (deleted after each fix cycle, including the final one)
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after fix cycle completes in isolation mode"
    );
}

#[test]
fn ralph_early_exit_no_issues_still_cleans_up() {
    // Test that ISSUES.md is cleaned up even when review exits early
    // due to finding no issues
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let call_counter = dir.path().join(".agent/call_counter");

    // Script that creates ISSUES.md with "No issues found" marker only during review
    // (call 1). The early exit means fix phase is skipped, but commit message phase
    // (call 2) should NOT recreate ISSUES.md.
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Only create ISSUES.md on first call (review phase)
# Do NOT recreate during commit message phase
if [ "$count" -eq 1 ]; then
    # Create ISSUES.md with the "no issues" marker that triggers early exit
    cat > .agent/ISSUES.md << 'ISSUES_EOF'
# Review Complete

✓ **No issues found.** The code meets all requirements.
ISSUES_EOF
fi

# Create commit message
echo "feat: no issues" > .agent/commit-message.txt
exit 0
"#,
            counter = call_counter.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "3") // Request 3 cycles, should exit early
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Pipeline should succeed and exit early
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));

    // ISSUES.md should be cleaned up even with early exit
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after early exit in isolation mode"
    );
}

#[test]
fn ralph_multiple_review_cycles_final_cleanup() {
    // Test that with N=2 review cycles, ISSUES.md is cleaned up
    // after EACH fix cycle to prevent context contamination
    // Sequence: Review1 -> Fix1 -> DELETE -> Review2 -> Fix2 -> DELETE -> Commit
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Track each phase with a counter and ISSUES.md state
    let counter_path = dir.path().join(".agent/call_counter");
    let issues_state_log = dir.path().join(".agent/issues_state.txt");

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Log ISSUES.md state at start of each call
if [ -f .agent/ISSUES.md ]; then
    echo "Call $count: ISSUES.md exists" >> "{log}"
else
    echo "Call $count: ISSUES.md missing" >> "{log}"
fi

# Call sequence for N=2:
# Call 1: Review1 - ISSUES.md should be missing (fresh start)
# Call 2: Fix1 - ISSUES.md should exist (from Review1)
# Call 3: Review2 - ISSUES.md should be missing (deleted after Fix1)
# Call 4: Fix2 - ISSUES.md should exist (from Review2)
# Call 5: Commit - ISSUES.md should be missing (deleted after Fix2)
case $count in
    1) # Review1 - ISSUES.md should be missing at start
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md should not exist at start of Review1!" >> "{log}"
        fi
        echo "- [ ] Issue from Review1" > .agent/ISSUES.md
        ;;
    2) # Fix1 - ISSUES.md should exist from Review1
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix1!" >> "{log}"
            exit 1
        fi
        ;;
    3) # Review2 - ISSUES.md should be MISSING (deleted after Fix1)
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md should have been deleted after Fix1!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review2" > .agent/ISSUES.md
        ;;
    4) # Fix2 - ISSUES.md should exist from Review2
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix2!" >> "{log}"
            exit 1
        fi
        ;;
    5) # Commit - ISSUES.md should be MISSING (deleted after Fix2)
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md should have been deleted after Fix2!" >> "{log}"
            exit 1
        fi
        ;;
esac

# Always create commit message
echo "feat: cycle $count" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display(),
            log = issues_state_log.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "2") // 2 review-fix cycles
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // Verify the call count: 2 cycles × 2 calls + 1 commit = 5 calls
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 5,
        "Expected 5 reviewer calls (2 × (review + fix) + 1 commit msg)"
    );

    // Verify the state log shows correct ISSUES.md lifecycle
    let state_log = fs::read_to_string(&issues_state_log).unwrap();
    assert!(
        !state_log.contains("ERROR"),
        "ISSUES.md lifecycle was incorrect. Log:\n{}",
        state_log
    );

    // Verify expected state at each call:
    // Call 1 (Review1): missing (fresh start)
    // Call 2 (Fix1): exists (from Review1)
    // Call 3 (Review2): missing (deleted after Fix1)
    // Call 4 (Fix2): exists (from Review2)
    // Call 5 (Commit): missing (deleted after Fix2)
    assert!(
        state_log.contains("Call 1: ISSUES.md missing"),
        "Review1 should start with no ISSUES.md. Log:\n{}",
        state_log
    );
    assert!(
        state_log.contains("Call 2: ISSUES.md exists"),
        "Fix1 should see ISSUES.md from Review1. Log:\n{}",
        state_log
    );
    assert!(
        state_log.contains("Call 3: ISSUES.md missing"),
        "Review2 should start fresh (ISSUES.md deleted after Fix1). Log:\n{}",
        state_log
    );
    assert!(
        state_log.contains("Call 4: ISSUES.md exists"),
        "Fix2 should see ISSUES.md from Review2. Log:\n{}",
        state_log
    );
    assert!(
        state_log.contains("Call 5: ISSUES.md missing"),
        "Commit should start fresh (ISSUES.md deleted after Fix2). Log:\n{}",
        state_log
    );

    // After completion, ISSUES.md should still be absent
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after all review-fix cycles complete"
    );
}

#[test]
fn ralph_issues_md_deleted_after_each_fix_cycle() {
    // Comprehensive test for N=3 cycles verifying exact ISSUES.md lifecycle:
    // Review1 -> Fix1 -> DELETE -> Review2 -> Fix2 -> DELETE -> Review3 -> Fix3 -> DELETE -> Commit
    // This ensures N review-fix cycles corresponds to exactly N deletion operations
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let counter_path = dir.path().join(".agent/call_counter");
    let issues_state_log = dir.path().join(".agent/issues_state.txt");

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Log ISSUES.md state at start of each call
if [ -f .agent/ISSUES.md ]; then
    echo "Call $count: ISSUES.md exists" >> "{log}"
else
    echo "Call $count: ISSUES.md missing" >> "{log}"
fi

# Call sequence for N=3 (7 calls total):
# Call 1: Review1 - missing (fresh start)
# Call 2: Fix1 - exists (from Review1)
# Call 3: Review2 - missing (deleted after Fix1)
# Call 4: Fix2 - exists (from Review2)
# Call 5: Review3 - missing (deleted after Fix2)
# Call 6: Fix3 - exists (from Review3)
# Call 7: Commit - missing (deleted after Fix3)
case $count in
    1) # Review1
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md should not exist at Review1!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review1" > .agent/ISSUES.md
        ;;
    2) # Fix1
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix1!" >> "{log}"
            exit 1
        fi
        ;;
    3) # Review2
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md not deleted after Fix1!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review2" > .agent/ISSUES.md
        ;;
    4) # Fix2
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix2!" >> "{log}"
            exit 1
        fi
        ;;
    5) # Review3
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md not deleted after Fix2!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review3" > .agent/ISSUES.md
        ;;
    6) # Fix3
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix3!" >> "{log}"
            exit 1
        fi
        ;;
    7) # Commit
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md not deleted after Fix3!" >> "{log}"
            exit 1
        fi
        ;;
esac

# Always create commit message
echo "feat: N=3 test" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display(),
            log = issues_state_log.display()
        ),
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "3") // 3 review-fix cycles
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // Verify the call count: 3 cycles × 2 calls + 1 commit = 7 calls
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        count, 7,
        "Expected 7 reviewer calls (3 × (review + fix) + 1 commit msg)"
    );

    // Verify no errors occurred in the lifecycle
    let state_log = fs::read_to_string(&issues_state_log).unwrap();
    assert!(
        !state_log.contains("ERROR"),
        "ISSUES.md lifecycle was incorrect. Log:\n{}",
        state_log
    );

    // Verify the exact pattern: missing, exists, missing, exists, missing, exists, missing
    let expected_states = [
        ("Call 1: ISSUES.md missing", "Review1 should start fresh"),
        ("Call 2: ISSUES.md exists", "Fix1 should see ISSUES.md"),
        (
            "Call 3: ISSUES.md missing",
            "Review2 should start fresh after Fix1 cleanup",
        ),
        ("Call 4: ISSUES.md exists", "Fix2 should see ISSUES.md"),
        (
            "Call 5: ISSUES.md missing",
            "Review3 should start fresh after Fix2 cleanup",
        ),
        ("Call 6: ISSUES.md exists", "Fix3 should see ISSUES.md"),
        (
            "Call 7: ISSUES.md missing",
            "Commit should start fresh after Fix3 cleanup",
        ),
    ];

    for (expected, msg) in expected_states {
        assert!(
            state_log.contains(expected),
            "{}. Expected '{}' in log:\n{}",
            msg,
            expected,
            state_log
        );
    }

    // Final state: ISSUES.md should not exist
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after all cycles complete"
    );
}

#[test]
fn ralph_zero_reviewer_reviews_no_issues_created() {
    // Test that with N=0 reviewer reviews, pre-existing ISSUES.md gets cleaned at start
    // and commit message generation still works without review phases
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    // Pre-create an ISSUES.md to verify it gets cleaned at start of run (isolation mode)
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    fs::write(
        dir.path().join(".agent/ISSUES.md"),
        "old issues from previous run",
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0") // Skip all review phases
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        // Even with no review cycles, commit message generation runs
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent && echo \"feat: zero reviews\" > .agent/commit-message.txt'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Skipping review phase"));

    // ISSUES.md should be cleaned at the start of run (reset_context_for_isolation)
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted at start of run in isolation mode"
    );
}
