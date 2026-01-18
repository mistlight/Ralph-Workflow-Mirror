//! Test to reproduce the --init bug where ralph continues to run the pipeline
//! after --init should have exited.
//!
//! The bug: When --init is used, the system should exit cleanly after
//! initialization. It should NEVER continue to run the AI pipeline.

use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

use test_helpers::init_git_repo;

use crate::common::ralph_bin_path;

/// Test that `ralph --init` exits cleanly without running the pipeline.
#[test]
fn test_ralph_init_exits_cleanly() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    let _ = init_git_repo(&dir);

    // Set up config dir
    let config_home = dir_path.join(".config");
    fs::create_dir_all(&config_home).unwrap();

    // Run ralph --init with no config or prompt
    let mut cmd = StdCommand::new(ralph_bin_path());
    cmd.current_dir(dir_path)
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--init")
        .env("RALPH_INTERACTIVE", "0");

    let output = cmd.output().unwrap();

    // Should exit successfully
    assert!(
        output.status.success(),
        "ralph --init should exit successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should have created config
    let unified_config_path = config_home.join("ralph-workflow.toml");
    assert!(
        unified_config_path.exists(),
        "Config should be created at {}",
        unified_config_path.display()
    );

    // Should NOT run the pipeline - check that there's no pipeline output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // These strings indicate the pipeline ran
    assert!(
        !stdout.contains("PHASE"),
        "Should not show PHASE output. Found: {}",
        stdout
    );
    assert!(
        !stdout.contains("Development"),
        "Should not show Development phase. Found: {}",
        stdout
    );
    assert!(
        !stdout.contains("Review"),
        "Should not show Review phase. Found: {}",
        stdout
    );
    assert!(
        !stderr.contains("PHASE"),
        "Should not show PHASE in stderr. Found: {}",
        stderr
    );
}

/// Test that `ralph --init bug-fix` creates PROMPT.md and exits.
#[test]
fn test_ralph_init_with_template_exits_cleanly() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    let _ = init_git_repo(&dir);

    // Set up config dir with existing config
    let config_home = dir_path.join(".config");
    fs::create_dir_all(&config_home).unwrap();
    fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["opencode"]
reviewer = ["codex"]
"#,
    )
    .unwrap();

    // Run ralph --init bug-fix
    let mut cmd = StdCommand::new(ralph_bin_path());
    cmd.current_dir(dir_path)
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--init=bug-fix")
        .env("RALPH_INTERACTIVE", "0");

    let output = cmd.output().unwrap();

    // Should exit successfully
    assert!(
        output.status.success(),
        "ralph --init=bug-fix should exit successfully"
    );

    // Should have created PROMPT.md
    assert!(
        dir_path.join("PROMPT.md").exists(),
        "PROMPT.md should be created"
    );

    // Should NOT run the pipeline
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("PHASE"), "Should not show PHASE output");
}

/// Test that `ralph --init` when both config and PROMPT.md exist exits cleanly.
#[test]
fn test_ralph_init_when_setup_complete_exits_cleanly() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    let _ = init_git_repo(&dir);

    // Set up config dir with existing config
    let config_home = dir_path.join(".config");
    fs::create_dir_all(&config_home).unwrap();
    fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["opencode"]
reviewer = ["codex"]
"#,
    )
    .unwrap();

    // Create existing PROMPT.md
    fs::write(dir_path.join("PROMPT.md"), "# Test\n").unwrap();

    // Run ralph --init
    let mut cmd = StdCommand::new(ralph_bin_path());
    cmd.current_dir(dir_path)
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--init")
        .env("RALPH_INTERACTIVE", "0");

    let output = cmd.output().unwrap();

    // Should exit successfully
    assert!(
        output.status.success(),
        "ralph --init should exit successfully"
    );

    // Should show "Setup complete" message
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Setup complete"),
        "Should show setup complete message"
    );

    // Should NOT run the pipeline
    assert!(!stdout.contains("PHASE"), "Should not show PHASE output");
}

/// Test that `ralph --init` with an invalid template name exits cleanly.
#[test]
fn test_ralph_init_with_invalid_template_exits_cleanly() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    let _ = init_git_repo(&dir);

    // Set up config dir with existing config
    let config_home = dir_path.join(".config");
    fs::create_dir_all(&config_home).unwrap();
    fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["opencode"]
reviewer = ["codex"]
"#,
    )
    .unwrap();

    // Run ralph --init with an invalid template name
    let mut cmd = StdCommand::new(ralph_bin_path());
    cmd.current_dir(dir_path)
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--init=not-a-real-template")
        .env("RALPH_INTERACTIVE", "0");

    let output = cmd.output().unwrap();

    // Should exit successfully (even though template is invalid)
    assert!(
        output.status.success(),
        "ralph --init=not-a-real-template should exit successfully"
    );

    // Should show error about unknown template/work guide
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unknown Work Guide"),
        "Should show unknown work guide error. Got: {}",
        stdout
    );

    // Should NOT run the pipeline
    assert!(!stdout.contains("PHASE"), "Should not show PHASE output");
}

/// Test that `ralph --init` when passed with a commit message positionally
/// interprets the commit message as the template value and exits cleanly.
#[test]
fn test_ralph_init_with_commit_message_exits_cleanly() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    // Initialize git repo
    let _ = init_git_repo(&dir);

    // Set up config dir with existing config
    let config_home = dir_path.join(".config");
    fs::create_dir_all(&config_home).unwrap();
    fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["opencode"]
reviewer = ["codex"]
"#,
    )
    .unwrap();

    // Run ralph --init "my commit message"
    // clap will interpret "my commit message" as the value for --init
    let mut cmd = StdCommand::new(ralph_bin_path());
    cmd.current_dir(dir_path)
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("--init")
        .arg("my commit message")
        .env("RALPH_INTERACTIVE", "0");

    let output = cmd.output().unwrap();

    // Should exit successfully
    assert!(
        output.status.success(),
        "ralph --init with commit message should exit successfully"
    );

    // Should show error about unknown work guide (since "my commit message" is not a template)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unknown Work Guide"),
        "Should show unknown work guide error for 'my commit message'. Got: {}",
        stdout
    );

    // Should NOT run the pipeline
    assert!(!stdout.contains("PHASE"), "Should not show PHASE output");
    assert!(
        !stdout.contains("Development"),
        "Should not show Development phase"
    );
    assert!(!stdout.contains("Review"), "Should not show Review phase");
}
