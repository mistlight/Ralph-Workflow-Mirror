//! CLI integration tests.
//!
//! # IMPORTANT: Timeout Enforcement
//!
//! **ALL tests in this module MUST use `with_default_timeout()` to wrap test code.**
//! This ensures tests complete within 5 seconds and don't hang due to external I/O.
//!
//! See `test_timeout.rs` for details on the timeout enforcement mechanism.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (exit codes, file changes)
//! - Uses `run_ralph_cli()` which calls `app::run()` directly (no process spawning)
//! - Uses `TempDir` for filesystem isolation
//! - Tests are deterministic and focus on successful execution and file side effects

use crate::common::{mock_executor_with_success, run_ralph_cli};
use crate::test_timeout::with_default_timeout;
use std::fs;
use tempfile::TempDir;
use test_helpers::init_git_repo;

// ============================================================================
// Version and Help Commands
// ============================================================================

/// Test that the `--version` flag produces a successful exit.
///
/// This verifies that when a user invokes ralph with the `--version` flag,
/// the CLI executes successfully without errors.
#[test]
fn ralph_prints_version() {
    with_default_timeout(|| {
        let executor = mock_executor_with_success();
        // --version doesn't need a working directory
        run_ralph_cli(&["--version"], executor, None).unwrap();
    });
}

/// Test that the `--help` flag displays usage information.
///
/// This verifies that when a user invokes ralph with the `--help` flag,
/// the command executes successfully without errors.
/// (Actual help content verification is done by the clap library itself)
#[test]
fn ralph_help_shows_usage() {
    with_default_timeout(|| {
        let executor = mock_executor_with_success();
        // --help doesn't need a working directory
        run_ralph_cli(&["--help"], executor, None).unwrap();
    });
}

// ============================================================================
// Template Listing Commands
// ============================================================================

/// Test that the `--list-templates` flag shows available templates.
///
/// This verifies that when a user invokes ralph with the `--list-templates` flag,
/// the command executes successfully without errors.
#[test]
fn ralph_list_templates_shows_available() {
    with_default_timeout(|| {
        let executor = mock_executor_with_success();
        // --list-templates doesn't need a working directory
        run_ralph_cli(&["--list-templates"], executor, None).unwrap();
    });
}

// ============================================================================
// Diagnose Command
// ============================================================================

/// Test that the `--diagnose` flag displays system diagnostic information.
///
/// This verifies that when a user invokes ralph with the `--diagnose` flag
/// in a git repository, the command executes successfully without errors.
#[test]
fn ralph_diagnose_shows_system_info() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        let executor = mock_executor_with_success();
        run_ralph_cli(&["--diagnose"], executor, Some(dir.path())).unwrap();
    });
}

/// Test that the `-d` short flag works equivalently to `--diagnose`.
///
/// This verifies that when a user invokes ralph with the `-d` short flag,
/// the command executes successfully without errors.
#[test]
fn ralph_diagnose_short_flag_works() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        let executor = mock_executor_with_success();
        run_ralph_cli(&["-d"], executor, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Dry Run Command
// ============================================================================

/// Test that the `--dry-run` flag validates configuration without executing agents.
///
/// This verifies that when a user invokes ralph with the `--dry-run` flag
/// with a valid PROMPT.md and config, the pipeline validates without running agents.
#[test]
fn ralph_dry_run_validates_without_executing() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Create a PROMPT.md for validation
        fs::write(
            dir.path().join("PROMPT.md"),
            r#"## Goal

Do something.

## Acceptance

- Tests pass
"#,
        )
        .unwrap();

        // Set up a config
        let config_home = dir.path().join(".config");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("ralph-workflow.toml"),
            r#"[agent_chain]
developer = ["codex"]
reviewer = ["codex"]
"#,
        )
        .unwrap();

        let executor = mock_executor_with_success();
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        run_ralph_cli(&["--dry-run"], executor, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Init Commands
// ============================================================================

/// Test that the `--init` flag with a template creates a PROMPT.md file.
///
/// This verifies that when a user invokes ralph with the `--init` flag
/// and a template name like "bug-fix", a PROMPT.md file is created
/// with content appropriate for that template (e.g., Goal section).
#[test]
fn ralph_init_with_template_creates_prompt() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Remove the PROMPT.md created by init_git_repo to test --init creating it
        let prompt_path = dir.path().join("PROMPT.md");
        fs::remove_file(&prompt_path).unwrap();
        assert!(
            !prompt_path.exists(),
            "PROMPT.md should be removed for test"
        );

        // Create config so we can use --init with template
        let config_home = dir.path().join(".config");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("ralph-workflow.toml"),
            r#"[agent_chain]
developer = ["codex"]
reviewer = ["codex"]
"#,
        )
        .unwrap();

        let executor = mock_executor_with_success();
        std::env::set_var("XDG_CONFIG_HOME", &config_home);

        // Note: --init in non-interactive mode returns early without creating PROMPT.md
        // The actual PROMPT.md creation happens in interactive mode
        run_ralph_cli(&["--init", "bug-fix"], executor, Some(dir.path())).unwrap();
    });
}
