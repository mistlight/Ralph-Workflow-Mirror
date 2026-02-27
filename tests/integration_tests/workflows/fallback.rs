//! Agent execution integration tests.
//!
//! These tests verify agent command execution behavior, including:
//! - Phase skipping with zero iterations
//! - Pipeline behavior without agent execution
//!
//! Note: Tests that require agent execution (`developer_iters` > 0 or `reviewer_reviews` > 0)
//! cannot be properly tested without the `AgentExecutor` trait infrastructure. Those tests
//! should be unit tests with mocked executors at the code level.
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

/// Standard PROMPT.md content for fallback tests.
const STANDARD_PROMPT: &str = r"## Goal

Do something.

## Acceptance

- Tests pass
";

/// Create mock handlers with standard setup for fallback tests.
fn create_fallback_test_handlers() -> (MockAppEffectHandler, MockEffectHandler) {
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
// Agent Command Execution Tests
// ============================================================================

/// Test that setting iterations to zero skips the respective phase.
///
/// This verifies that when a user runs ralph with both `developer_iters=0`
/// and `reviewer_reviews=0`, the pipeline completes successfully.
/// The pipeline may still create some tracking files (STATUS.md, etc.) for
/// pipeline state management, but agent execution is skipped.
#[test]
fn test_skips_phases_with_zero_iterations() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_fallback_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify ISSUES.md is not created in isolation mode (default)
        assert!(
            app_handler
                .get_file(&PathBuf::from(".agent/ISSUES.md"))
                .is_none(),
            "ISSUES.md should not exist in isolation mode (default)"
        );
    });
}

/// Test that the pipeline succeeds with both developer and review phases skipped.
///
/// This verifies that when a user runs ralph with both `developer_iters=0`
/// and `reviewer_reviews=0`, the pipeline completes successfully and a commit
/// effect is triggered.
#[test]
fn test_succeeds_with_zero_iterations() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_fallback_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify CreateCommit effect was called at the reducer layer
        let was_commit_created =
            effect_handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. }));

        assert!(
            was_commit_created,
            "CreateCommit effect should be called when phases are skipped"
        );
    });
}
