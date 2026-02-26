//! PLAN workflow integration tests.
//!
//! These tests verify the plan workflow functionality.
//!
//! Note: Tests that require agent execution (`developer_iters` > 0) cannot be
//! properly tested without the `AgentExecutor` trait infrastructure. Those tests
//! should be unit tests with mocked executors at the code level.
//!
//! These integration tests focus on behavior that doesn't require agent execution.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** via effect capture
//! - Uses `MockAppEffectHandler` AND `MockEffectHandler` for git/filesystem isolation
//! - NO `TempDir`, `std::fs`, or real git operations
//! - Tests are deterministic and verify effects, not real filesystem state

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use std::path::PathBuf;

/// Standard PROMPT.md content for plan tests.
const STANDARD_PROMPT: &str = r"## Goal

Do something.

## Acceptance

- Tests pass
";

/// Create mock handlers with standard setup for plan tests.
fn create_plan_test_handlers() -> (MockAppEffectHandler, MockEffectHandler) {
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
// PLAN Workflow Tests
//
// Note: Tests that require agent execution (developer_iters > 0) cannot be
// properly tested without the AgentExecutor trait infrastructure. Those tests
// should be unit tests with mocked executors at the code level.
//
// These integration tests focus on behavior that doesn't require agent execution.
// ============================================================================

/// Test that the plan phase is skipped when `developer_iters` is set to zero.
///
/// This verifies that when a user runs ralph with `developer_iters=0`,
/// the planning phase is skipped entirely and no PLAN.md file is created.
#[test]
fn test_skips_plan_phase_when_zero_developer_iters() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_plan_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify PLAN.md was never created (since planning was skipped)
        assert!(
            app_handler
                .get_file(&PathBuf::from(".agent/PLAN.md"))
                .is_none(),
            "PLAN.md should not be created when developer_iters=0"
        );
    });
}

/// Test that a commit can be created without a plan when `developer_iters` is zero.
///
/// This verifies that when a user runs ralph with `developer_iters=0`,
/// a commit effect is triggered successfully without requiring a PLAN.md file.
#[test]
fn test_commit_without_plan_succeeds() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_plan_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify CreateCommit effect was called at the reducer layer
        let was_commit_created =
            effect_handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. }));

        assert!(
            was_commit_created,
            "CreateCommit effect should be called without requiring PLAN.md"
        );
    });
}
