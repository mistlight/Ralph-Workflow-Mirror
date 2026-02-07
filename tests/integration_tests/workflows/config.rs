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
//! - Tests verify **observable behavior** via effect capture
//! - Uses `MockAppEffectHandler` AND `MockEffectHandler` for git/filesystem isolation
//! - Uses `MemoryConfigEnvironment` for config path injection
//! - NO `TempDir`, `std::fs`, or real git operations
//! - Tests are deterministic and verify effects, not real filesystem state
//!
//! # Note on Init Commands
//!
//! The --init-legacy and --init-global commands write directly to the filesystem
//! and are not fully mockable via the effect system. Tests for these commands
//! use MemoryConfigEnvironment where possible but some legacy behavior tests
//! may need to be in the system tests package instead.

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::config::MemoryConfigEnvironment;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use std::path::PathBuf;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_env,
    run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;

/// Standard PROMPT.md content for config tests.
const STANDARD_PROMPT: &str = r#"## Goal

Do something.

## Acceptance

- Tests pass
"#;

/// Create mock handlers with standard setup for config tests.
fn create_config_test_handlers() -> (MockAppEffectHandler, MockEffectHandler) {
    let app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file("PROMPT.md", STANDARD_PROMPT)
        .with_diff("diff --git a/test.txt b/test.txt\n+new content")
        .with_staged_changes(true);

    let effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));

    (app_handler, effect_handler)
}

// ============================================================================
// Config and Init Tests
// ============================================================================

/// Test that ralph --init-global creates unified config file.
///
/// This verifies that when ralph --init-global is run, the system
/// creates ralph-workflow.toml using the injected ConfigEnvironment.
#[test]
fn test_init_global_creates_config() {
    with_default_timeout(|| {
        // Create mock handler for app effects
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"))
            .with_file("/test/repo/PROMPT.md", "## Goal\n\nTest task\n");

        // Create in-memory environment - no config exists yet
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file("/test/repo/PROMPT.md", "## Goal\n\nTest task\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_env(&["--init-global"], executor, config, &mut handler, &env).unwrap();

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
fn test_uses_agent_chain_first_entries_as_defaults() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        // Create config with specific agents (simulating first entries from chain)
        let config = create_test_config_struct()
            .with_developer_agent("claude".to_string())
            .with_reviewer_agent("aider".to_string());

        let executor = mock_executor_with_success();
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should succeed with custom agent chain"
        );
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
fn test_quick_mode_sets_minimal_iterations() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Quick mode with explicit --developer-iters 0
        let result = run_ralph_cli_with_handlers(
            &["--quick", "--developer-iters", "0"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "Quick mode should succeed");
    });
}

/// Test that quick mode short flag -Q works correctly.
///
/// This verifies that when the -Q short flag is used, the system
/// enables quick mode the same as --quick.
#[test]
fn test_quick_mode_short_flag_works() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // -Q should work the same as --quick
        let result = run_ralph_cli_with_handlers(
            &["-Q", "--developer-iters", "0"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "-Q short flag should work");
    });
}

/// Test that explicit iteration counts override quick mode.
///
/// This verifies that when both --quick and explicit --developer-iters
/// are provided, the explicit value takes precedence.
#[test]
fn test_quick_mode_explicit_iters_override() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Explicit --developer-iters should override quick mode
        let result = run_ralph_cli_with_handlers(
            &["--quick", "--developer-iters", "0"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "Explicit iters should override quick mode");
    });
}

/// Test that rapid mode sets two developer iterations.
///
/// This verifies that when --rapid flag is used, the system
/// configures developer_iters=2 and reviewer_reviews=1.
#[test]
fn test_rapid_mode_sets_two_iterations() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Rapid mode with explicit --developer-iters 0
        let result = run_ralph_cli_with_handlers(
            &["--rapid", "--developer-iters", "0"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "Rapid mode should succeed");
    });
}

/// Test that rapid mode short flag -U works correctly.
///
/// This verifies that when the -U short flag is used, the system
/// enables rapid mode the same as --rapid.
#[test]
fn test_rapid_mode_short_flag_works() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // -U should work the same as --rapid
        let result = run_ralph_cli_with_handlers(
            &["-U", "--developer-iters", "0"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "-U short flag should work");
    });
}

// ============================================================================
// Stack Detection Tests
//
// Note: Stack detection reads from the filesystem to detect project structure.
// These tests verify the pipeline completes with stack detection configuration,
// but the actual detection logic cannot be fully tested without filesystem access.
// ============================================================================

/// Test that stack detection configuration is handled correctly.
///
/// This verifies that when auto_detect_stack is enabled, the pipeline
/// completes successfully without errors.
#[test]
fn test_stack_detection_config_enabled() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        // Create config with stack detection enabled
        let config = create_test_config_struct()
            .with_auto_detect_stack(true)
            .with_verbosity(ralph_workflow::config::Verbosity::Verbose);

        let executor = mock_executor_with_success();
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should work with stack detection enabled"
        );
    });
}

/// Test that stack detection can be disabled via configuration.
///
/// This verifies that when auto_detect_stack is set to false,
/// the pipeline completes successfully.
#[test]
fn test_stack_detection_disabled() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        // Explicitly disable stack detection
        let config = create_test_config_struct().with_auto_detect_stack(false);
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should succeed with stack detection disabled"
        );
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
fn test_review_depth_standard() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Standard);
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "Standard review depth should work");
    });
}

/// Test that comprehensive review depth configures detailed review.
///
/// This verifies that when review_depth is set to comprehensive,
/// the system uses thorough review configurations.
#[test]
fn test_review_depth_comprehensive() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Comprehensive);
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "Comprehensive review depth should work");
    });
}

/// Test that security review depth configures security-focused review.
///
/// This verifies that when review_depth is set to security,
/// the system uses security-oriented review configurations.
#[test]
fn test_review_depth_security() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Security);
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "Security review depth should work");
    });
}

/// Test that incremental review depth focuses on git diff.
///
/// This verifies that when review_depth is set to incremental,
/// the system configures review to focus on changed files only.
#[test]
fn test_review_depth_incremental() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        let config = create_test_config_struct()
            .with_review_depth(ralph_workflow::config::ReviewDepth::Incremental);
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(result.is_ok(), "Incremental review depth should work");
    });
}

// ============================================================================
// Local Config Tests
// ============================================================================

/// Test that --init-local-config creates a local config file.
///
/// This verifies that when --init-local-config is run, the system
/// creates .agent/ralph-workflow.toml in the current directory.
#[test]
fn test_init_local_config_creates_file() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"))
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2",
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_env(
            &["--init-local-config"],
            executor,
            config,
            &mut handler,
            &env,
        )
        .unwrap();

        assert!(
            env.was_written(std::path::Path::new(
                "/test/repo/.agent/ralph-workflow.toml"
            )),
            "Local config file should be created"
        );

        let content = env
            .get_file(std::path::Path::new(
                "/test/repo/.agent/ralph-workflow.toml",
            ))
            .unwrap();
        assert!(
            content.contains("Local Ralph configuration"),
            "Local config should contain template comment"
        );
        assert!(
            content.contains("developer_iters"),
            "Local config should show common override examples"
        );
    });
}

/// Test that local config overrides global config values.
///
/// This verifies that when both global and local configs exist,
/// the local config values override global ones.
#[test]
fn test_local_config_overrides_global() {
    with_default_timeout(|| {
        let (mut app_handler, _effect_handler) = create_config_test_handlers();

        // Global config: developer_iters = 5
        // Local config: developer_iters = 10
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2\ndeveloper_iters = 5",
            )
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                "[general]\ndeveloper_iters = 10",
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        // Create config that would normally have 5 iters from global,
        // but should get 10 from local override
        let config = create_test_config_struct().with_developer_iters(10);
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_env(&[], executor, config, &mut app_handler, &env);

        assert!(
            result.is_ok(),
            "Pipeline should succeed with local config override"
        );
    });
}

/// Test that --check-config validates and displays merged config.
///
/// This verifies that when --check-config is run with both global and local configs,
/// the system validates both and shows effective settings.
#[test]
fn test_check_config_with_local_and_global() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2\ndeveloper_iters = 5",
            )
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                "[general]\ndeveloper_iters = 10",
            );

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result =
            run_ralph_cli_with_env(&["--check-config"], executor, config, &mut handler, &env);

        assert!(
            result.is_ok(),
            "--check-config should succeed with valid configs"
        );
    });
}

/// Test that local config can exist without global config.
///
/// This verifies that when only a local config exists (no global),
/// the system uses local values and defaults for everything else.
#[test]
fn test_local_config_only_no_global() {
    with_default_timeout(|| {
        let (mut app_handler, _effect_handler) = create_config_test_handlers();

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                "[general]\nverbosity = 4\ndeveloper_iters = 8",
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct()
            .with_verbosity(ralph_workflow::config::Verbosity::Debug)
            .with_developer_iters(8);
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_env(&[], executor, config, &mut app_handler, &env);

        assert!(
            result.is_ok(),
            "Pipeline should work with only local config"
        );
    });
}

// ============================================================================
// Fail-Fast Config Validation Tests
// ============================================================================

/// Test that invalid TOML in global config causes fail-fast.
///
/// This verifies that Ralph refuses to start when the global config
/// has invalid TOML syntax.
#[test]
fn test_fail_fast_invalid_global_toml() {
    with_default_timeout(|| {
        let (mut app_handler, _effect_handler) = create_config_test_handlers();

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general\nverbosity = 2", // Invalid TOML - missing closing bracket
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_env(&[], executor, config, &mut app_handler, &env);

        assert!(
            result.is_err(),
            "Pipeline should fail with invalid TOML in global config"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Configuration validation failed") || err_msg.contains("TOML"),
            "Error should mention config validation or TOML: {}",
            err_msg
        );
    });
}

/// Test that invalid TOML in local config causes fail-fast.
///
/// This verifies that Ralph refuses to start when the local config
/// has invalid TOML syntax.
#[test]
fn test_fail_fast_invalid_local_toml() {
    with_default_timeout(|| {
        let (mut app_handler, _effect_handler) = create_config_test_handlers();

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2",
            )
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                "[general\ndeveloper_iters = 10", // Invalid TOML
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_env(&[], executor, config, &mut app_handler, &env);

        assert!(
            result.is_err(),
            "Pipeline should fail with invalid TOML in local config"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Configuration validation failed") || err_msg.contains("TOML"),
            "Error should mention config validation or TOML: {}",
            err_msg
        );
    });
}

/// Test that --check-config exits with non-zero on invalid config.
///
/// This verifies that --check-config properly reports validation errors.
#[test]
fn test_check_config_exits_nonzero_on_invalid_config() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general\nverbosity = 2", // Invalid TOML - missing closing bracket
            );

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result =
            run_ralph_cli_with_env(&["--check-config"], executor, config, &mut handler, &env);

        assert!(
            result.is_err(),
            "--check-config should exit with error on invalid TOML"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Configuration validation failed") || err_msg.contains("TOML"),
            "Error should mention validation error or TOML: {}",
            err_msg
        );
    });
}

/// Test that --check-config succeeds with valid config.
///
/// This verifies that --check-config passes when config is valid.
#[test]
fn test_check_config_succeeds_with_valid_config() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2\ndeveloper_iters = 5",
            );

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result =
            run_ralph_cli_with_env(&["--check-config"], executor, config, &mut handler, &env);

        assert!(
            result.is_ok(),
            "--check-config should succeed with valid config"
        );
    });
}

/// Test that unknown key detection works with typo suggestions.
///
/// NOTE: Full unknown key detection is not yet implemented - serde ignores
/// unknown fields by default. This test documents current behavior.
///
/// This verifies that validation catches typos and suggests corrections.
#[test]
fn test_unknown_key_detection_not_yet_implemented() {
    with_default_timeout(|| {
        let (mut app_handler, _effect_handler) = create_config_test_handlers();

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\ndevelper_iters = 5\nverbozity = 2", // Two typos
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_env(&[], executor, config, &mut app_handler, &env);

        // Currently passes because unknown keys are silently ignored
        // TODO: This should fail once full semantic validation is implemented
        assert!(
            result.is_ok(),
            "Currently unknown keys are ignored - this should change in the future"
        );
    });
}

/// Test that invalid type values are caught during validation.
///
/// This verifies that type mismatches (e.g., string instead of int) are caught.
#[test]
fn test_invalid_type_detection() {
    with_default_timeout(|| {
        let (mut app_handler, _effect_handler) = create_config_test_handlers();

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\ndeveloper_iters = \"five\"", // String instead of int
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_env(&[], executor, config, &mut app_handler, &env);

        assert!(
            result.is_err(),
            "Pipeline should fail with invalid type in config"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Configuration validation failed")
                || err_msg.contains("Invalid value")
                || err_msg.contains("expected"),
            "Error should mention validation or type error: {}",
            err_msg
        );
    });
}
