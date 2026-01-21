//! Test to reproduce the --init bug where ralph continues to run the pipeline
//! after --init should have exited.
//!
//! The bug: When --init is used, the system should exit cleanly after
//! initialization. It should NEVER continue to run the AI pipeline.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (exit codes, file creation)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use std::fs;
use tempfile::TempDir;

use test_helpers::init_git_repo;

use crate::common::ralph_cmd;
use crate::test_timeout::with_default_timeout;

/// Test that `ralph --init` exits cleanly without running the pipeline.
///
/// This verifies that when --init flag is used, the system exits
/// successfully after initialization without running the AI pipeline.
#[test]
fn test_ralph_init_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Set up config dir
        let config_home = dir_path.join(".config");
        fs::create_dir_all(&config_home).unwrap();

        // Run ralph --init with no config or prompt
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init")
            .env("RALPH_INTERACTIVE", "0")
            .assert()
            .success()
            .get_output()
            .clone();

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
    });
}

/// Test that `ralph --init bug-fix` creates PROMPT.md and exits.
///
/// This verifies that when --init=bug-fix is used, the system creates
/// the PROMPT.md template file and exits without running the pipeline.
#[test]
fn test_ralph_init_with_template_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Remove the PROMPT.md that init_git_repo creates, so we can test --init creating it
        fs::remove_file(dir_path.join("PROMPT.md")).unwrap();

        // Set up config dir with existing config (using non-opencode agents to avoid network calls)
        let config_home = dir_path.join(".config");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("ralph-workflow.toml"),
            r#"[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#,
        )
        .unwrap();

        // Run ralph --init bug-fix
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init=bug-fix")
            .env("RALPH_INTERACTIVE", "0")
            .assert()
            .success()
            .get_output()
            .clone();

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
    });
}

/// Test that `ralph --init` when both config and PROMPT.md exist exits cleanly.
///
/// This verifies that when setup is complete and --init is run, the system
/// shows "Setup complete" message and exits without running the pipeline.
#[test]
fn test_ralph_init_when_setup_complete_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Set up config dir with existing config (using non-opencode agents to avoid network calls)
        let config_home = dir_path.join(".config");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("ralph-workflow.toml"),
            r#"[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#,
        )
        .unwrap();

        // Create existing PROMPT.md
        fs::write(dir_path.join("PROMPT.md"), "# Test\n").unwrap();

        // Run ralph --init
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init")
            .env("RALPH_INTERACTIVE", "0")
            .assert()
            .success()
            .get_output()
            .clone();

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
    });
}

/// Test that `ralph --init` with an invalid template name exits cleanly.
///
/// This verifies that when an invalid template name is provided, the system
/// shows an error message and exits without running the pipeline.
#[test]
fn test_ralph_init_with_invalid_template_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Set up config dir with existing config (using non-opencode agents to avoid network calls)
        let config_home = dir_path.join(".config");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("ralph-workflow.toml"),
            r#"[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#,
        )
        .unwrap();

        // Run ralph --init with an invalid template name
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init=not-a-real-template")
            .env("RALPH_INTERACTIVE", "0")
            .assert()
            .success()
            .get_output()
            .clone();

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
    });
}

/// Test that `ralph --init` with commit message treats it as template value.
///
/// This verifies that when --init is passed with a commit message positionally,
/// the system interprets it as the template value and exits without running pipeline.
#[test]
fn test_ralph_init_with_commit_message_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Set up config dir with existing config (using non-opencode agents to avoid network calls)
        let config_home = dir_path.join(".config");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("ralph-workflow.toml"),
            r#"[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#,
        )
        .unwrap();

        // Run ralph --init "my commit message"
        // clap will interpret "my commit message" as the value for --init
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init")
            .arg("my commit message")
            .env("RALPH_INTERACTIVE", "0")
            .assert()
            .success()
            .get_output()
            .clone();

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
    });
}
