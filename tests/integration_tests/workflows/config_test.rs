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
use std::sync::Arc;
use tempfile::TempDir;

use test_helpers::init_git_repo;

use crate::common::run_ralph_cli;
use crate::test_timeout::with_default_timeout;
use ralph_workflow::executor::RealProcessExecutor;

/// Helper function to set up base environment for tests.
fn set_base_env(config_home: &std::path::Path) {
    std::env::set_var("RALPH_INTERACTIVE", "0");
    std::env::set_var("XDG_CONFIG_HOME", config_home);
}

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

        // Set up environment and run ralph --init
        std::env::set_current_dir(dir_path).unwrap();
        set_base_env(&config_home);
        let executor = Arc::new(RealProcessExecutor::new());
        run_ralph_cli(&["--init"], executor).unwrap();

        // Should have created config
        let unified_config_path = config_home.join("ralph-workflow.toml");
        assert!(
            unified_config_path.exists(),
            "Config should be created at {}",
            unified_config_path.display()
        );

        // Should NOT run the pipeline - verify no agent files were created
        assert!(
            !dir_path.join(".agent/PLAN.md").exists(),
            "PLAN.md should not be created when --init is used"
        );
        assert!(
            !dir_path.join(".agent/ISSUES.md").exists(),
            "ISSUES.md should not be created when --init is used"
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

        // Set up environment and run ralph --init bug-fix
        std::env::set_current_dir(dir_path).unwrap();
        set_base_env(&config_home);
        let executor = Arc::new(RealProcessExecutor::new());
        run_ralph_cli(&["--init=bug-fix"], executor).unwrap();

        // Should have created PROMPT.md
        assert!(
            dir_path.join("PROMPT.md").exists(),
            "PROMPT.md should be created"
        );

        // Should NOT run the pipeline - verify no agent files were created
        assert!(
            !dir_path.join(".agent/PLAN.md").exists(),
            "PLAN.md should not be created when --init=bug-fix is used"
        );
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

        // Set up environment and run ralph --init
        std::env::set_current_dir(dir_path).unwrap();
        set_base_env(&config_home);
        let executor = Arc::new(RealProcessExecutor::new());
        run_ralph_cli(&["--init"], executor).unwrap();

        // Should NOT run the pipeline - verify no agent files were created
        assert!(
            !dir_path.join(".agent/PLAN.md").exists(),
            "PLAN.md should not be created when --init is used with existing setup"
        );
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

        // Set up environment and run ralph --init with an invalid template name
        std::env::set_current_dir(dir_path).unwrap();
        set_base_env(&config_home);
        let executor = Arc::new(RealProcessExecutor::new());

        // Should exit successfully (even though template is invalid)
        let result = run_ralph_cli(&["--init=not-a-real-template"], executor);
        assert!(
            result.is_ok(),
            "ralph --init=not-a-real-template should exit successfully"
        );

        // Should NOT run the pipeline - verify no agent files were created
        assert!(
            !dir_path.join(".agent/PLAN.md").exists(),
            "PLAN.md should not be created when --init is used with invalid template"
        );
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

        // Set up environment and run ralph --init "my commit message"
        // clap will interpret "my commit message" as the value for --init
        std::env::set_current_dir(dir_path).unwrap();
        set_base_env(&config_home);
        let executor = Arc::new(RealProcessExecutor::new());

        // Should exit successfully
        let result = run_ralph_cli(&["--init", "my commit message"], executor);
        assert!(
            result.is_ok(),
            "ralph --init with commit message should exit successfully"
        );

        // Should NOT run the pipeline - verify no agent files were created
        assert!(
            !dir_path.join(".agent/PLAN.md").exists(),
            "PLAN.md should not be created when --init is used with commit message"
        );
        assert!(
            !dir_path.join(".agent/ISSUES.md").exists(),
            "ISSUES.md should not be created when --init is used with commit message"
        );
    });
}
