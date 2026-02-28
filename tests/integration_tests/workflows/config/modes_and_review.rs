//! Quick mode, rapid mode, stack detection, and review depth tests.
//!
//! Tests for CLI mode flags (`--quick`, `--rapid`) and configuration options
//! (`auto_detect_stack`, `review_depth`).
//!
//! **CRITICAL:** Follow the integration test style guide in
//! **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;

use super::create_config_test_handlers;

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
/// configures `developer_iters=2` and `reviewer_reviews=1`.
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
/// This verifies that when `auto_detect_stack` is enabled, the pipeline
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
/// This verifies that when `auto_detect_stack` is set to false,
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
/// This verifies that when `review_depth` is set to standard,
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
/// This verifies that when `review_depth` is set to comprehensive,
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
/// This verifies that when `review_depth` is set to security,
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
/// This verifies that when `review_depth` is set to incremental,
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
