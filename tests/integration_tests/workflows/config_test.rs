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

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;

/// Test that `ralph --init` exits cleanly without running the pipeline.
///
/// This verifies that when --init flag is used, the system exits
/// successfully after initialization without running the AI pipeline.
///
/// NOTE: This test is disabled because --init is handled in the
/// config initialization path which reads env vars.
#[test]
#[ignore = "requires config init path injection"]
fn test_ralph_init_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Set up config dir
        let config_home = dir_path.join(".config");
        fs::create_dir_all(&config_home).unwrap();

        // Run ralph --init with injected config
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--init"], executor, config, Some(dir_path)).unwrap();

        // --init in non-interactive mode without template just shows help
        // It does NOT create config - that requires --init-global

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
///
/// NOTE: This test is disabled because --init is handled in the
/// config initialization path which reads env vars.
#[test]
#[ignore = "requires config init path injection"]
fn test_ralph_init_with_template_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Remove the PROMPT.md that init_git_repo creates, so we can test --init creating it
        fs::remove_file(dir_path.join("PROMPT.md")).unwrap();

        // Run ralph --init bug-fix with injected config
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--init=bug-fix"], executor, config, Some(dir_path)).unwrap();

        // Should have created PROMPT.md from bug-fix template
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
///
/// NOTE: This test is disabled because --init is handled in the
/// config initialization path which reads env vars.
#[test]
#[ignore = "requires config init path injection"]
fn test_ralph_init_when_setup_complete_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Create existing PROMPT.md
        fs::write(
            dir_path.join("PROMPT.md"),
            r#"## Goal

Test configuration functionality.

## Acceptance

- Tests pass
"#,
        )
        .unwrap();

        // Run ralph --init with injected config
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--init"], executor, config, Some(dir_path)).unwrap();

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
///
/// NOTE: This test is disabled because --init is handled in the
/// config initialization path which reads env vars.
#[test]
#[ignore = "requires config init path injection"]
fn test_ralph_init_with_invalid_template_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Run ralph --init with an invalid template name
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should exit successfully (even though template is invalid)
        let result = run_ralph_cli_injected(
            &["--init=not-a-real-template"],
            executor,
            config,
            Some(dir_path),
        );
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
///
/// NOTE: This test is disabled because --init is handled in the
/// config initialization path which reads env vars.
#[test]
#[ignore = "requires config init path injection"]
fn test_ralph_init_with_commit_message_exits_cleanly() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        // Initialize git repo
        let _ = init_git_repo(&dir);

        // Run ralph --init "my commit message"
        // clap will interpret "my commit message" as the value for --init
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should exit successfully
        let result = run_ralph_cli_injected(
            &["--init", "my commit message"],
            executor,
            config,
            Some(dir_path),
        );
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
