//! CLI integration tests.
//!
//! # IMPORTANT: Timeout Enforcement
//!
//! **ALL tests in this module MUST use `with_default_timeout()` to wrap test code.**
//! This ensures tests complete within 10 seconds and don't hang due to external I/O.
//!
//! See `test_timeout.rs` for details on the timeout enforcement mechanism.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (exit codes, stdout/stderr, file changes)
//! - Uses `assert_cmd::Command` for black-box CLI testing
//! - Uses `TempDir` for filesystem isolation
//! - Tests are deterministic and black-box (test CLI as a user would invoke it)

use crate::common::ralph_cmd;
use crate::test_timeout::with_default_timeout;
use predicates::prelude::*;
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
        ralph_cmd().arg("--version").assert().success();
    });
}

/// Test that the `--version` flag outputs a version number.
///
/// This verifies that when a user invokes ralph with the `--version` flag,
/// the output contains a semantic version number in the format MAJOR.MINOR.PATCH.
#[test]
fn ralph_version_contains_version_number() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap());
    });
}

/// Test that the `--help` flag displays usage information.
///
/// This verifies that when a user invokes ralph with the `--help` flag,
/// the output contains the program name and references to PROMPT.md.
#[test]
fn ralph_help_shows_usage() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("ralph"))
            .stdout(predicate::str::contains("PROMPT.md"));
    });
}

/// Test that the `--help` flag displays all available preset modes.
///
/// This verifies that when a user invokes ralph with the `--help` flag,
/// the output contains all preset mode names (Quick, Rapid, Standard, Thorough, Long).
#[test]
fn ralph_help_shows_preset_modes() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("Quick"))
            .stdout(predicate::str::contains("Rapid"))
            .stdout(predicate::str::contains("Standard"))
            .stdout(predicate::str::contains("Thorough"))
            .stdout(predicate::str::contains("Long"));
    });
}

// ============================================================================
// Template Listing Commands
// ============================================================================

/// Test that the `--list-templates` flag shows available templates.
///
/// This verifies that when a user invokes ralph with the `--list-templates` flag,
/// the output contains template names like "bug-fix" and "feature-spec".
#[test]
fn ralph_list_templates_shows_available() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--list-templates")
            .assert()
            .success()
            .stdout(predicate::str::contains("bug-fix"))
            .stdout(predicate::str::contains("feature-spec"));
    });
}

/// Test that the `--list-templates` flag shows template descriptions.
///
/// This verifies that when a user invokes ralph with the `--list-templates` flag,
/// the output contains descriptive keywords like "quick" and "refactor".
#[test]
fn ralph_list_templates_shows_descriptions() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--list-templates")
            .assert()
            .success()
            // Template names should appear with some description
            .stdout(predicate::str::contains("quick"))
            .stdout(predicate::str::contains("refactor"));
    });
}

// ============================================================================
// Diagnose Command
// ============================================================================

/// Test that the `--diagnose` flag displays system diagnostic information.
///
/// This verifies that when a user invokes ralph with the `--diagnose` flag
/// in a git repository, the output contains diagnostic information about ralph or the system.
#[test]
fn ralph_diagnose_shows_system_info() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        ralph_cmd()
            .current_dir(dir.path())
            .arg("--diagnose")
            .assert()
            .success()
            // Should contain system diagnostics
            .stdout(predicate::str::contains("ralph").or(predicate::str::contains("System")));
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

        // -d should work the same as --diagnose
        ralph_cmd()
            .current_dir(dir.path())
            .arg("-d")
            .assert()
            .success();
    });
}

// ============================================================================
// Dry Run Command
// ============================================================================

/// Test that the `--dry-run` flag validates configuration without executing agents.
///
/// This verifies that when a user invokes ralph with the `--dry-run` flag
/// with a valid PROMPT.md and config, the pipeline validates without running agents
/// and outputs an indication of dry-run mode or validation success.
#[test]
fn ralph_dry_run_validates_without_executing() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Create a PROMPT.md for validation
        fs::write(dir.path().join("PROMPT.md"), "# Test Task\n\nDo something.").unwrap();

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

        ralph_cmd()
            .current_dir(dir.path())
            .env("XDG_CONFIG_HOME", &config_home)
            .env("RALPH_INTERACTIVE", "0")
            .arg("--dry-run")
            .assert()
            .success()
            // Should validate without running agents
            .stdout(predicate::str::contains("dry").or(predicate::str::contains("valid")));
    });
}

/// Test that the `--dry-run` flag warns about missing PROMPT.md sections.
///
/// This verifies that when a user invokes ralph with the `--dry-run` flag
/// without a PROMPT.md or with an incomplete one, the pipeline succeeds
/// but outputs a warning about missing required sections like Goal.
#[test]
fn ralph_dry_run_warns_on_missing_prompt_sections() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Dry run succeeds but warns about missing Goal section in empty PROMPT.md
        // (Missing PROMPT.md is handled gracefully with a warning)
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

        ralph_cmd()
            .current_dir(dir.path())
            .env("XDG_CONFIG_HOME", &config_home)
            .env("RALPH_INTERACTIVE", "0")
            .arg("--dry-run")
            .assert()
            .success()
            // Should warn about missing Goal section
            .stdout(predicate::str::contains("Goal"));
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

        ralph_cmd()
            .current_dir(dir.path())
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init")
            .arg("bug-fix")
            .assert()
            .success()
            .stdout(predicate::str::contains("PROMPT.md"));

        // PROMPT.md should be created
        assert!(
            prompt_path.exists(),
            "PROMPT.md should be created by --init bug-fix"
        );

        // Should contain bug-fix template content (Goal section)
        let content = fs::read_to_string(&prompt_path).unwrap();
        assert!(
            content.contains("## Goal"),
            "Template should contain Goal section, got: {}",
            &content[..content.len().min(200)]
        );
    });
}

/// Test that the `--init-prompt` flag works as an alias for `--init`.
///
/// This verifies that when a user invokes ralph with the `--init-prompt` flag
/// and a template name, a PROMPT.md file is created successfully.
#[test]
fn ralph_init_prompt_is_alias_for_init() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Remove the PROMPT.md created by init_git_repo to test --init-prompt creating it
        let prompt_path = dir.path().join("PROMPT.md");
        fs::remove_file(&prompt_path).unwrap();

        // Create config
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

        ralph_cmd()
            .current_dir(dir.path())
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init-prompt")
            .arg("quick")
            .assert()
            .success();

        // PROMPT.md should be created
        assert!(prompt_path.exists());
    });
}

// ============================================================================
// Shell Completion Generation
// ============================================================================

/// Test that shell completion generation works for bash.
///
/// This verifies that when a user invokes ralph with `--generate-completion=bash`,
/// the output contains bash-specific completion script content.
#[test]
fn ralph_generate_completion_bash() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--generate-completion=bash")
            .assert()
            .success()
            .stdout(predicate::str::contains("_ralph"));
    });
}

/// Test that shell completion generation works for zsh.
///
/// This verifies that when a user invokes ralph with `--generate-completion=zsh`,
/// the output contains zsh-specific completion script content including the compdef directive.
#[test]
fn ralph_generate_completion_zsh() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--generate-completion=zsh")
            .assert()
            .success()
            .stdout(predicate::str::contains("#compdef"));
    });
}

/// Test that shell completion generation works for fish.
///
/// This verifies that when a user invokes ralph with `--generate-completion=fish`,
/// the output contains fish-specific completion script content including the complete directive.
#[test]
fn ralph_generate_completion_fish() {
    with_default_timeout(|| {
        ralph_cmd()
            .arg("--generate-completion=fish")
            .assert()
            .success()
            .stdout(predicate::str::contains("complete"));
    });
}
