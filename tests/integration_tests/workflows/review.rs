//! Review workflow integration tests.
//!
//! These tests verify the review workflow functionality.
//!
//! Note: Tests that require agent execution (reviewer_reviews > 0) cannot be
//! properly tested without the AgentExecutor trait infrastructure. Those tests
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

/// Standard PROMPT.md content for review tests.
const STANDARD_PROMPT: &str = r#"## Goal

Do something.

## Acceptance

- Tests pass
"#;

/// Create mock handlers with standard setup for review tests.
fn create_review_test_handlers() -> (MockAppEffectHandler, MockEffectHandler) {
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
// Review Workflow Tests
//
// Note: Tests that require agent execution (reviewer_reviews > 0) cannot be
// properly tested without the AgentExecutor trait infrastructure. Those tests
// should be unit tests with mocked executors at the code level.
//
// These integration tests focus on behavior that doesn't require agent execution.
// ============================================================================

/// Test that setting reviewer_reviews to zero skips the review phase.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// the review phase is skipped entirely and no ISSUES.md file is created.
#[test]
fn test_zero_reviewer_reviews_skips_review() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_review_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // ISSUES.md should NOT be created when review is skipped
        assert!(
            app_handler
                .get_file(&PathBuf::from(".agent/ISSUES.md"))
                .is_none(),
            "ISSUES.md should not be created when review phase is skipped"
        );
    });
}

/// Test that the pipeline succeeds without a review phase.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// the pipeline completes successfully.
#[test]
fn test_pipeline_succeeds_without_review_phase() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_review_test_handlers();
        let config = create_test_config_struct();
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
            "Pipeline should succeed without review phase"
        );
    });
}

/// Test that a commit is created when the review phase is skipped.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// a commit effect is still triggered.
#[test]
fn test_commit_created_when_review_skipped() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_review_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify CreateCommit effect was called at the reducer layer
        let was_commit_created =
            effect_handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. }));

        assert!(
            was_commit_created,
            "CreateCommit effect should be called when review phase is skipped"
        );
    });
}
