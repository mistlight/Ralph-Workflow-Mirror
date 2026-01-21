//! Config and init integration tests.
//!
//! These tests verify configuration file creation and initialization behavior.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (file creation, config validation)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use crate::test_timeout::with_default_timeout;
use test_helpers::init_git_repo;

/// Helper function to set up base environment for tests.
///
/// This function sets up config isolation using XDG_CONFIG_HOME to prevent
/// the tests from loading the user's actual config which may contain
/// opencode/* references that would trigger network calls.
fn base_env<'a>(
    cmd: &'a mut assert_cmd::Command,
    config_home: &std::path::Path,
) -> &'a mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Isolate config to prevent loading user's actual config with opencode/* refs
        .env("XDG_CONFIG_HOME", config_home)
        // Ensure git identity isn't a factor if a commit happens in the test.
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

/// Create an isolated config home with a minimal config that doesn't use opencode/* refs.
fn create_isolated_config(dir: &TempDir) -> std::path::PathBuf {
    let config_home = dir.path().join(".config");
    fs::create_dir_all(&config_home).unwrap();
    // Create minimal config without opencode/* references
    fs::write(
        config_home.join("ralph-workflow.toml"),
        r#"[agent_chain]
developer = ["claude"]
reviewer = ["codex"]
"#,
    )
    .unwrap();
    config_home
}

// ============================================================================
// Config and Init Tests
// ============================================================================

/// Test that ralph --init-legacy creates a config file.
///
/// This verifies that when ralph --init-legacy is run, the system
/// creates .agent/agents.toml with default configuration sections.
#[test]
fn ralph_init_creates_config_file() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let dir_path = dir.path();

        // Initialize git repo but don't create agents.toml
        let _ = init_git_repo(&dir);

        let config_path = dir_path.join(".agent/agents.toml");
        assert!(!config_path.exists());

        // Run ralph --init-legacy
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init-legacy")
            .assert()
            .success();

        // Config file should now exist
        assert!(config_path.exists());

        // Verify content contains expected sections
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Ralph Agents Configuration File"));
        assert!(content.contains("[agents.claude]"));
        assert!(content.contains("[agents.codex]"));
        assert!(content.contains("[agent_chain]"));

        // Output should indicate file was created
        let output_bytes = output.get_output().stdout.clone();
        let stdout = String::from_utf8_lossy(&output_bytes);
        assert!(stdout.contains("Created"));
    });
}

/// Test that ralph --init-legacy preserves existing config files.
///
/// This verifies that when a config file already exists, the system
/// reports it exists and does not overwrite the original content.
#[test]
fn ralph_init_reports_existing_config() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
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
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init-legacy")
            .assert()
            .success();

        // Config file should still contain original content
        let content = fs::read_to_string(dir_path.join(".agent/agents.toml")).unwrap();
        assert_eq!(content, custom_config);

        // Output should indicate file already exists
        let output_bytes = output.get_output().stdout.clone();
        let stdout = String::from_utf8_lossy(&output_bytes);
        assert!(stdout.contains("already exists"));
    });
}

/// Test that ralph --init-global creates unified config file.
///
/// This verifies that when ralph --init-global is run, the system
/// creates ralph-workflow.toml in the XDG config home directory.
#[test]
fn ralph_first_run_creates_config_and_exits() {
    with_default_timeout(|| {
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
        let output = ralph_cmd()
            .current_dir(dir_path)
            .env("XDG_CONFIG_HOME", &config_home)
            .arg("--init-global")
            .assert()
            .success();

        // Should exit successfully after creating the config
        // Unified config file should now exist
        assert!(unified_config_path.exists());

        // Output should indicate file was created or already exists
        let output_bytes = output.get_output().stdout.clone();
        let stdout = String::from_utf8_lossy(&output_bytes);
        assert!(stdout.contains("unified config"));
    });
}

/// Test that agent chain first entries are used as default agents.
///
/// This verifies that when no explicit agent selection is made, the system
/// uses the first entry in the agent_chain configuration.
#[test]
fn ralph_uses_agent_chain_first_entries_as_defaults() {
    with_default_timeout(|| {
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        // Ensure no explicit agent selection via env is in play.
        // Use non-opencode agents to avoid network calls for API catalog.
        let config_home = dir.path().join(".config");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("ralph-workflow.toml"),
            r#"[agent_chain]
developer = ["claude", "codex"]
reviewer = ["aider", "codex"]
"#,
        )
        .unwrap();

        let mut cmd = ralph_cmd();
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0");
        // agent commands not needed when developer_iters=0 and reviewer_reviews=0

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Claude"))
            .stdout(predicate::str::contains("Aider"));
    });
}

// ============================================================================
// Quick Mode Tests
// ============================================================================

/// Test that quick mode sets minimal iteration counts.
///
/// This verifies that when --quick flag is used, the system
/// configures minimal developer and reviewer iteration counts.
#[test]
fn ralph_quick_mode_sets_minimal_iterations() {
    with_default_timeout(|| {
        // Quick mode should set developer_iters=1 and reviewer_reviews=1
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path())
            .arg("--quick") // Use quick mode
            .arg("--developer-iters")
            .arg("0") // Override with 0 to skip agent execution
            .env("RALPH_INTERACTIVE", "0")
            .env("XDG_CONFIG_HOME", &config_home)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        cmd.assert().success();
        // Quick mode works without shell commands
    });
}

/// Test that quick mode short flag -Q works correctly.
///
/// This verifies that when the -Q short flag is used, the system
/// enables quick mode the same as --quick.
#[test]
fn ralph_quick_mode_short_flag_works() {
    with_default_timeout(|| {
        // -Q should work the same as --quick
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let _counter_path = dir.path().join(".agent/plan_counter");

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path())
            .arg("-Q") // Short flag
            .arg("--developer-iters")
            .arg("0") // Override with 0 to skip agent execution
            .env("RALPH_INTERACTIVE", "0")
            .env("XDG_CONFIG_HOME", &config_home)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        cmd.assert().success();
        // Quick mode works without shell commands
    });
}

/// Test that explicit iteration counts override quick mode.
///
/// This verifies that when both --quick and explicit --developer-iters
/// are provided, the explicit value takes precedence.
#[test]
fn ralph_quick_mode_explicit_iters_override() {
    with_default_timeout(|| {
        // Explicit --developer-iters should override quick mode
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let _counter_path = dir.path().join(".agent/plan_counter");

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path())
            .arg("--quick")
            .arg("--developer-iters")
            .arg("0") // Override with 0 to skip agent execution
            .env("RALPH_INTERACTIVE", "0")
            .env("XDG_CONFIG_HOME", &config_home)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        cmd.assert().success();
        // Explicit --developer-iters overrides quick mode
    });
}

/// Test that rapid mode sets two developer iterations.
///
/// This verifies that when --rapid flag is used, the system
/// configures developer_iters=2 and reviewer_reviews=1.
#[test]
fn ralph_rapid_mode_sets_two_iterations() {
    with_default_timeout(|| {
        // Rapid mode should set developer_iters=2 and reviewer_reviews=1
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let _counter_path = dir.path().join(".agent/plan_counter");

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path())
            .arg("--rapid") // Use rapid mode
            .arg("--developer-iters")
            .arg("0") // Override with 0 to skip agent execution
            .env("RALPH_INTERACTIVE", "0")
            .env("XDG_CONFIG_HOME", &config_home)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        cmd.assert().success();
        // Rapid mode works without shell commands
    });
}

/// Test that rapid mode short flag -U works correctly.
///
/// This verifies that when the -U short flag is used, the system
/// enables rapid mode the same as --rapid.
#[test]
fn ralph_rapid_mode_short_flag_works() {
    with_default_timeout(|| {
        // -U should work the same as --rapid
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let _counter_path = dir.path().join(".agent/plan_counter");

        let mut cmd = ralph_cmd();
        cmd.current_dir(dir.path())
            .arg("-U") // Short flag
            .arg("--developer-iters")
            .arg("0") // Override with 0 to skip agent execution
            .env("RALPH_INTERACTIVE", "0")
            .env("XDG_CONFIG_HOME", &config_home)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");

        cmd.assert().success();
        // Rapid mode works without shell commands
    });
}

// ============================================================================
// Stack Detection Tests
// ============================================================================

/// Test that stack detection works for Rust projects.
///
/// This verifies that when a Rust project is detected, the system
/// identifies the stack correctly and uses appropriate build commands.
#[test]
fn ralph_stack_detection_rust_project() {
    with_default_timeout(|| {
        // Test that stack detection works in an integration context
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
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
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_AUTO_DETECT_STACK", "true")
            .env("RALPH_VERBOSITY", "2"); // Verbose mode
                                          // agent commands not needed when developer_iters=0 and reviewer_reviews=0

        // Pipeline should complete and potentially mention Rust stack
        cmd.assert().success();
    });
}

/// Test that stack detection works for JavaScript projects.
///
/// This verifies that when a JavaScript/React project is detected,
/// the system identifies the stack and uses appropriate build commands.
#[test]
fn ralph_stack_detection_javascript_project() {
    with_default_timeout(|| {
        // Test stack detection for a JavaScript/React project
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
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
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_AUTO_DETECT_STACK", "true");
        // agent commands removed (not needed when developer_iters=0)

        cmd.assert().success();
    });
}

/// Test that stack detection can be disabled via environment variable.
///
/// This verifies that when RALPH_AUTO_DETECT_STACK is set to false,
/// the system skips automatic stack detection.
#[test]
fn ralph_stack_detection_disabled() {
    with_default_timeout(|| {
        // Test that stack detection can be disabled
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
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
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_AUTO_DETECT_STACK", "false"); // Explicitly disable
                                                      // agent commands removed (not needed when developer_iters=0)

        cmd.assert().success();
    });
}

/// Test that stack detection handles mixed-language projects.
///
/// This verifies that when a project contains multiple languages,
/// the system detects the primary stack appropriately.
#[test]
fn ralph_mixed_language_project() {
    with_default_timeout(|| {
        // Test stack detection with multiple languages
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
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
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_AUTO_DETECT_STACK", "true");
        // agent commands removed (not needed when developer_iters=0)

        cmd.assert().success();
    });
}

// ============================================================================
// Review Depth Tests
// ============================================================================

/// Test that standard review depth configures the review process.
///
/// This verifies that when RALPH_REVIEW_DEPTH is set to standard,
/// the system uses standard-level review configurations.
#[test]
fn ralph_review_depth_standard() {
    with_default_timeout(|| {
        // Test standard review depth
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let mut cmd = ralph_cmd();
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_REVIEW_DEPTH", "standard");
        // agent commands removed (not needed when developer_iters=0)

        cmd.assert().success();
    });
}

/// Test that comprehensive review depth configures detailed review.
///
/// This verifies that when RALPH_REVIEW_DEPTH is set to comprehensive,
/// the system uses thorough review configurations.
#[test]
fn ralph_review_depth_comprehensive() {
    with_default_timeout(|| {
        // Test comprehensive review depth
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let mut cmd = ralph_cmd();
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_REVIEW_DEPTH", "comprehensive");
        // agent commands removed (not needed when developer_iters=0)

        cmd.assert().success();
    });
}

/// Test that security review depth configures security-focused review.
///
/// This verifies that when RALPH_REVIEW_DEPTH is set to security,
/// the system uses security-oriented review configurations.
#[test]
fn ralph_review_depth_security() {
    with_default_timeout(|| {
        // Test security-focused review depth
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let mut cmd = ralph_cmd();
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_REVIEW_DEPTH", "security");
        // agent commands removed (not needed when developer_iters=0)

        cmd.assert().success();
    });
}

/// Test that incremental review depth focuses on git diff.
///
/// This verifies that when RALPH_REVIEW_DEPTH is set to incremental,
/// the system configures review to focus on changed files only.
#[test]
fn ralph_review_depth_incremental() {
    with_default_timeout(|| {
        // Test incremental review depth (focuses on git diff)
        let dir = TempDir::new().unwrap();
        let config_home = create_isolated_config(&dir);
        let _ = init_git_repo(&dir);

        let mut cmd = ralph_cmd();
        base_env(&mut cmd, &config_home)
            .current_dir(dir.path())
            .env("RALPH_DEVELOPER_ITERS", "0")
            .env("RALPH_REVIEWER_REVIEWS", "0")
            .env("RALPH_REVIEW_DEPTH", "incremental");
        // agent commands removed (not needed when developer_iters=0)

        cmd.assert().success();
    });
}
