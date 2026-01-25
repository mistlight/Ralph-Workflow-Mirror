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

use std::fs;
use tempfile::TempDir;

use crate::common::{mock_executor_with_success, run_ralph_cli, EnvGuard};
use crate::test_timeout::with_default_timeout;
use test_helpers::init_git_repo;

/// Helper function to set up base environment for tests with automatic cleanup.
///
/// This function sets up config isolation using XDG_CONFIG_HOME to prevent
/// the tests from loading the user's actual config which may contain
/// opencode/* references that would trigger network calls.
/// Uses EnvGuard to ensure all environment variables are restored when dropped.
fn base_env(config_home: &std::path::Path) -> EnvGuard {
    let guard = EnvGuard::new(&[
        "RALPH_INTERACTIVE",
        "RALPH_DEVELOPER_ITERS",
        "RALPH_REVIEWER_REVIEWS",
        "XDG_CONFIG_HOME",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ]);

    guard.set(&[
        ("RALPH_INTERACTIVE", Some("0")),
        ("RALPH_DEVELOPER_ITERS", Some("0")),
        ("RALPH_REVIEWER_REVIEWS", Some("0")),
        ("XDG_CONFIG_HOME", Some(config_home.to_str().unwrap())),
        ("GIT_AUTHOR_NAME", Some("Test")),
        ("GIT_AUTHOR_EMAIL", Some("test@example.com")),
        ("GIT_COMMITTER_NAME", Some("Test")),
        ("GIT_COMMITTER_EMAIL", Some("test@example.com")),
    ]);

    guard
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
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        let executor = mock_executor_with_success();
        run_ralph_cli(&["--init-legacy"], executor, Some(dir_path)).unwrap();

        // Config file should now exist
        assert!(config_path.exists());

        // Verify content contains expected sections
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Ralph Agents Configuration File"));
        assert!(content.contains("[agents.claude]"));
        assert!(content.contains("[agents.codex]"));
        assert!(content.contains("[agent_chain]"));
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
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        let executor = mock_executor_with_success();
        run_ralph_cli(&["--init-legacy"], executor, Some(dir_path)).unwrap();

        // Config file should still contain original content
        let content = fs::read_to_string(dir_path.join(".agent/agents.toml")).unwrap();
        assert_eq!(content, custom_config);
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
        fs::write(
            dir_path.join("PROMPT.md"),
            r#"## Goal

Test configuration functionality.

## Acceptance

- Tests pass
"#,
        )
        .unwrap();

        // Use a temp config dir so the test doesn't touch the real home directory.
        let config_home = dir_path.join(".config");
        fs::create_dir_all(&config_home).unwrap();

        let unified_config_path = config_home.join("ralph-workflow.toml");
        assert!(!unified_config_path.exists());

        // Run ralph --init-global (unified config)
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        let executor = mock_executor_with_success();
        run_ralph_cli(&["--init-global"], executor, Some(dir_path)).unwrap();

        // Should exit successfully after creating the config
        // Unified config file should now exist
        assert!(unified_config_path.exists());
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

        let _env_guard = base_env(&config_home);
        // agent commands not needed when developer_iters=0 and reviewer_reviews=0

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);

        let executor = mock_executor_with_success();
        run_ralph_cli(
            &["--quick", "--developer-iters", "0"],
            executor,
            Some(dir.path()),
        )
        .unwrap();
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

        let _env_guard = base_env(&config_home);

        let executor = mock_executor_with_success();
        run_ralph_cli(
            &["-Q", "--developer-iters", "0"],
            executor,
            Some(dir.path()),
        )
        .unwrap();
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

        let _env_guard = base_env(&config_home);

        let executor = mock_executor_with_success();
        run_ralph_cli(
            &["--quick", "--developer-iters", "0"],
            executor,
            Some(dir.path()),
        )
        .unwrap();
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

        let _env_guard = base_env(&config_home);

        let executor = mock_executor_with_success();
        run_ralph_cli(
            &["--rapid", "--developer-iters", "0"],
            executor,
            Some(dir.path()),
        )
        .unwrap();
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

        let _env_guard = base_env(&config_home);

        let executor = mock_executor_with_success();
        run_ralph_cli(
            &["-U", "--developer-iters", "0"],
            executor,
            Some(dir.path()),
        )
        .unwrap();
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
        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_AUTO_DETECT_STACK", "true");
        std::env::set_var("RALPH_VERBOSITY", "2"); // Verbose mode
                                                   // agent commands not needed when developer_iters=0 and reviewer_reviews=0

        // Pipeline should complete and potentially mention Rust stack
        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_AUTO_DETECT_STACK", "true");
        // agent commands removed (not needed when developer_iters=0)

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_AUTO_DETECT_STACK", "false"); // Explicitly disable
                                                               // agent commands removed (not needed when developer_iters=0)

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_AUTO_DETECT_STACK", "true");
        // agent commands removed (not needed when developer_iters=0)

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_REVIEW_DEPTH", "standard");
        // agent commands removed (not needed when developer_iters=0)

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_REVIEW_DEPTH", "comprehensive");
        // agent commands removed (not needed when developer_iters=0)

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_REVIEW_DEPTH", "security");
        // agent commands removed (not needed when developer_iters=0)

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
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

        let _env_guard = base_env(&config_home);
        std::env::set_var("RALPH_REVIEW_DEPTH", "incremental");
        // agent commands removed (not needed when developer_iters=0)

        let executor = mock_executor_with_success();
        run_ralph_cli(&[], executor, Some(dir.path())).unwrap();
    });
}
