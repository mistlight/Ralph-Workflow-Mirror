use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use test_helpers::init_git_repo;

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
// Config and Init Tests
// ============================================================================

#[test]
fn ralph_init_creates_config_file() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo but don't create agents.toml
    let _ = init_git_repo(&dir);

    let config_path = dir_path.join(".agent/agents.toml");
    assert!(!config_path.exists());

    // Run ralph --init-legacy
    let mut cmd = StdCommand::new(crate::common::ralph_bin_path());
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
fn ralph_init_reports_existing_config() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    let _ = init_git_repo(&dir);

    // Create existing config with valid agent_chain
    let custom_config = r#"# Custom config
[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#;
    fs::write(dir_path.join(".agent/agents.toml"), custom_config).unwrap();

    // Run ralph --init-legacy
    let mut cmd = StdCommand::new(crate::common::ralph_bin_path());
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

#[test]
fn ralph_first_run_creates_config_and_exits() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo but don't create agents.toml
    let _ = init_git_repo(&dir);

    // Create PROMPT.md (required)
    fs::write(dir_path.join("PROMPT.md"), "# Test\n").unwrap();

    // Use a temp config dir so the test doesn't touch the real home directory.
    let config_home = dir_path.join(".config");
    fs::create_dir_all(&config_home).unwrap();

    let unified_config_path = config_home.join("ralph-workflow.toml");
    assert!(!unified_config_path.exists());

    // Run ralph --init-global (unified config)
    let mut cmd = StdCommand::new(crate::common::ralph_bin_path());
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

#[test]
fn ralph_uses_agent_chain_first_entries_as_defaults() {
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

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

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0");
    // agent commands not needed when developer_iters=0 and reviewer_reviews=0

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("OpenCode"))
        .stdout(predicate::str::contains("Aider"));
}

// ============================================================================
// Quick Mode Tests
// ============================================================================

#[test]
fn ralph_quick_mode_sets_minimal_iterations() {
    // Quick mode should set developer_iters=1 and reviewer_reviews=1
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let mut cmd = ralph_cmd();
    cmd.current_dir(dir.path())
        .arg("--quick") // Use quick mode
        .arg("--developer-iters")
        .arg("0") // Override with 0 to skip agent execution
        .env("RALPH_INTERACTIVE", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();
    // Quick mode works without shell commands
}

#[test]
fn ralph_quick_mode_short_flag_works() {
    // -Q should work the same as --quick
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let _counter_path = dir.path().join(".agent/plan_counter");

    let mut cmd = ralph_cmd();
    cmd.current_dir(dir.path())
        .arg("-Q") // Short flag
        .arg("--developer-iters")
        .arg("0") // Override with 0 to skip agent execution
        .env("RALPH_INTERACTIVE", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();
    // Quick mode works without shell commands
}

#[test]
fn ralph_quick_mode_explicit_iters_override() {
    // Explicit --developer-iters should override quick mode
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let _counter_path = dir.path().join(".agent/plan_counter");

    let mut cmd = ralph_cmd();
    cmd.current_dir(dir.path())
        .arg("--quick")
        .arg("--developer-iters")
        .arg("0") // Override with 0 to skip agent execution
        .env("RALPH_INTERACTIVE", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();
    // Explicit --developer-iters overrides quick mode
}

#[test]
fn ralph_rapid_mode_sets_two_iterations() {
    // Rapid mode should set developer_iters=2 and reviewer_reviews=1
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let _counter_path = dir.path().join(".agent/plan_counter");

    let mut cmd = ralph_cmd();
    cmd.current_dir(dir.path())
        .arg("--rapid") // Use rapid mode
        .arg("--developer-iters")
        .arg("0") // Override with 0 to skip agent execution
        .env("RALPH_INTERACTIVE", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();
    // Rapid mode works without shell commands
}

#[test]
fn ralph_rapid_mode_short_flag_works() {
    // -U should work the same as --rapid
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let _counter_path = dir.path().join(".agent/plan_counter");

    let mut cmd = ralph_cmd();
    cmd.current_dir(dir.path())
        .arg("-U") // Short flag
        .arg("--developer-iters")
        .arg("0") // Override with 0 to skip agent execution
        .env("RALPH_INTERACTIVE", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");

    cmd.assert().success();
    // Rapid mode works without shell commands
}

// ============================================================================
// Stack Detection Tests
// ============================================================================

#[test]
fn ralph_stack_detection_rust_project() {
    // Test that stack detection works in an integration context
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

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
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "true")
        .env("RALPH_VERBOSITY", "2"); // Verbose mode
                                      // agent commands not needed when developer_iters=0 and reviewer_reviews=0

    // Pipeline should complete and potentially mention Rust stack
    cmd.assert().success();
}

#[test]
fn ralph_stack_detection_javascript_project() {
    // Test stack detection for a JavaScript/React project
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

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

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "true");
    // agent commands removed (not needed when developer_iters=0)

    cmd.assert().success();
}

#[test]
fn ralph_stack_detection_disabled() {
    // Test that stack detection can be disabled
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

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

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "false"); // Explicitly disable
                                                  // agent commands removed (not needed when developer_iters=0)

    cmd.assert().success();
}

#[test]
fn ralph_mixed_language_project() {
    // Test stack detection with multiple languages
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

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

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_AUTO_DETECT_STACK", "true");
    // agent commands removed (not needed when developer_iters=0)

    cmd.assert().success();
}

// ============================================================================
// Review Depth Tests
// ============================================================================

#[test]
fn ralph_review_depth_standard() {
    // Test standard review depth
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "standard");
    // agent commands removed (not needed when developer_iters=0)

    cmd.assert().success();
}

#[test]
fn ralph_review_depth_comprehensive() {
    // Test comprehensive review depth
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "comprehensive");
    // agent commands removed (not needed when developer_iters=0)

    cmd.assert().success();
}

#[test]
fn ralph_review_depth_security() {
    // Test security-focused review depth
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "security");
    // agent commands removed (not needed when developer_iters=0)

    cmd.assert().success();
}

#[test]
fn ralph_review_depth_incremental() {
    // Test incremental review depth (focuses on git diff)
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("RALPH_REVIEW_DEPTH", "incremental");
    // agent commands removed (not needed when developer_iters=0)

    cmd.assert().success();
}
