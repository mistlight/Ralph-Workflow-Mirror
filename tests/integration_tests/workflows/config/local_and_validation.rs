//! Local config and fail-fast validation tests.
//!
//! Tests for `--init-local-config`, local/global config override behavior,
//! `--check-config`, and fail-fast validation of invalid TOML and unknown keys.
//!
//! **CRITICAL:** Follow the integration test style guide in
//! **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::config::MemoryConfigEnvironment;
use std::path::PathBuf;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_env,
};
use crate::test_timeout::with_default_timeout;

use super::{create_config_test_handlers, STANDARD_PROMPT};

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

        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("config should load");
        let unified = merged.expect("expected merged unified config");

        assert_eq!(unified.general.verbosity, 2);
        assert_eq!(unified.general.developer_iters, 10);
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
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                "[general]\nverbosity = 4\ndeveloper_iters = 8",
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        // Validate config loading/merge directly (behavior under test).
        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("local-only config should load");

        let unified = merged.expect("expected merged unified config from local file");
        assert_eq!(unified.general.developer_iters, 8);
        assert_eq!(unified.general.verbosity, 4);
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
            "Error should mention config validation or TOML: {err_msg}"
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
            "Error should mention config validation or TOML: {err_msg}"
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
            "Error should mention validation error or TOML: {err_msg}"
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
/// This verifies that validation catches typos and suggests corrections.
#[test]
fn test_unknown_key_detection_with_suggestions() {
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

        // Validation should now catch unknown keys and fail
        assert!(
            result.is_err(),
            "Unknown keys should be caught by validation"
        );

        // The detailed error message is printed to stderr during validation,
        // but the Error returned just says "Configuration validation failed"
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Configuration validation failed") || err_msg.contains("validation"),
            "Error should mention configuration validation: {err_msg}"
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
            "Error should mention validation or type error: {err_msg}"
        );
    });
}
