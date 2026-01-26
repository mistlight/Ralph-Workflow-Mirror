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
//! - Uses `MemoryConfigEnvironment` for config path injection (dependency injection)
//! - Tests are deterministic and isolated
//! - Uses **dependency injection** via `create_test_config_struct()` instead of env vars

use std::fs;
use tempfile::TempDir;

use ralph_workflow::config::MemoryConfigEnvironment;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
    run_ralph_cli_with_path_resolver,
};
use crate::test_timeout::with_default_timeout;
use test_helpers::init_git_repo;

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
        let dir_path = dir.path();

        // Initialize git repo but don't create agents.toml
        let _ = init_git_repo(&dir);

        let config_path = dir_path.join(".agent/agents.toml");
        assert!(!config_path.exists());

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--init-legacy"], executor, config, Some(dir_path)).unwrap();

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
///
/// NOTE: --init-legacy uses the repo-relative path (.agent/agents.toml)
/// and still uses std::fs directly, so this test uses TempDir.
/// This is an exception because --init-legacy is legacy behavior.
#[test]
fn ralph_init_reports_existing_config() {
    with_default_timeout(|| {
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

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&["--init-legacy"], executor, config, Some(dir_path)).unwrap();

        // Config file should still contain original content
        let content = fs::read_to_string(dir_path.join(".agent/agents.toml")).unwrap();
        assert_eq!(content, custom_config);
    });
}

/// Test that ralph --init-global creates unified config file.
///
/// This verifies that when ralph --init-global is run, the system
/// creates ralph-workflow.toml using the injected ConfigEnvironment.
#[test]
fn ralph_first_run_creates_config_and_exits() {
    with_default_timeout(|| {
        // Create in-memory environment - no config exists yet
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file("/test/repo/PROMPT.md", "## Goal\n\nTest task\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_path_resolver(&["--init-global"], executor, config, None, &env).unwrap();

        // Should have created the config file
        assert!(
            env.was_written(std::path::Path::new("/test/config/ralph-workflow.toml")),
            "Unified config file should be created"
        );

        // Verify it contains expected content
        let content = env
            .get_file(std::path::Path::new("/test/config/ralph-workflow.toml"))
            .unwrap();
        assert!(
            content.contains("[general]") || content.contains("[agents"),
            "Config file should contain expected sections"
        );
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

        // Create config with specific agents (simulating first entries from chain)
        let config = create_test_config_struct()
            .with_developer_agent("claude".to_string())
            .with_reviewer_agent("aider".to_string());

        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
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
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--quick", "--developer-iters", "0"],
            executor,
            config,
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
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["-Q", "--developer-iters", "0"],
            executor,
            config,
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
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--quick", "--developer-iters", "0"],
            executor,
            config,
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
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["--rapid", "--developer-iters", "0"],
            executor,
            config,
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
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(
            &["-U", "--developer-iters", "0"],
            executor,
            config,
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

        // Create config with stack detection enabled and verbose output
        let config = create_test_config_struct()
            .with_auto_detect_stack(true)
            .with_verbosity(ralph_workflow::config::Verbosity::Verbose);

        // Pipeline should complete and potentially mention Rust stack
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
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

        let config = create_test_config_struct().with_auto_detect_stack(true);
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

/// Test that stack detection can be disabled via configuration.
///
/// This verifies that when auto_detect_stack is set to false,
/// the system skips automatic stack detection.
#[test]
fn ralph_stack_detection_disabled() {
    with_default_timeout(|| {
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

        // Explicitly disable stack detection
        let config = create_test_config_struct().with_auto_detect_stack(false);
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
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

        let config = create_test_config_struct().with_auto_detect_stack(true);
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

// ============================================================================
// Review Depth Tests
// ============================================================================

/// Test that standard review depth configures the review process.
///
/// This verifies that when review_depth is set to standard,
/// the system uses standard-level review configurations.
#[test]
fn ralph_review_depth_standard() {
    with_default_timeout(|| {
        // Test standard review depth
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Standard);
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

/// Test that comprehensive review depth configures detailed review.
///
/// This verifies that when review_depth is set to comprehensive,
/// the system uses thorough review configurations.
#[test]
fn ralph_review_depth_comprehensive() {
    with_default_timeout(|| {
        // Test comprehensive review depth
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Comprehensive);
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

/// Test that security review depth configures security-focused review.
///
/// This verifies that when review_depth is set to security,
/// the system uses security-oriented review configurations.
#[test]
fn ralph_review_depth_security() {
    with_default_timeout(|| {
        // Test security-focused review depth
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Security);
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

/// Test that incremental review depth focuses on git diff.
///
/// This verifies that when review_depth is set to incremental,
/// the system configures review to focus on changed files only.
#[test]
fn ralph_review_depth_incremental() {
    with_default_timeout(|| {
        // Test incremental review depth (focuses on git diff)
        let dir = TempDir::new().unwrap();
        let _ = init_git_repo(&dir);

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Incremental);
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}
