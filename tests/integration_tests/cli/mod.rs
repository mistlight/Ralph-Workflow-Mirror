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

#[test]
fn ralph_prints_version() {
    with_default_timeout(|| {
        ralph_cmd().arg("--version").assert().success();
    });
}

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
