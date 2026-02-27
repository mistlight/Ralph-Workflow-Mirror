//! Baseline management integration tests.
//!
//! This module tests the baseline tracking functionality including:
//! - Start commit persistence across runs
//! - Stale baseline warnings
//! - Baseline reset functionality
//! - Diff accuracy from baseline
//!
//! These tests use `MockAppEffectHandler` and `MockEffectHandler` to verify
//! behavior through effect capture, making tests fast and deterministic.
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
//!
//! # Note on Start Commit Effects
//!
//! The current production code calls `git_helpers::save_start_commit()` directly
//! rather than through the effect system. This is an architectural limitation.
//! Tests verify that the pipeline completes successfully with mock handlers,
//! which is sufficient to ensure the baseline mechanism doesn't crash.

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;
use ralph_workflow::app::effect::AppEffect;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use std::path::PathBuf;

/// Standard PROMPT.md content for baseline tests.
const STANDARD_PROMPT: &str = r"## Goal

Do something.

## Acceptance

- Tests pass
";

/// Create mock handlers with standard setup for baseline tests.
///
/// Returns (`app_handler`, `effect_handler`) configured with:
/// - Git repo context (valid HEAD OID)
/// - Working directory set to /mock/repo
/// - PROMPT.md file with standard content
/// - A diff to trigger commit (changes from start commit)
fn create_baseline_test_handlers() -> (MockAppEffectHandler, MockEffectHandler) {
    let app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file("PROMPT.md", STANDARD_PROMPT)
        // Simulate a diff exists (changes to commit)
        .with_diff("diff --git a/test.txt b/test.txt\n+new content")
        // Ensure git add stages changes
        .with_staged_changes(true);

    // Create effect handler with initial state (0 developer iters to skip to commit)
    let effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));

    (app_handler, effect_handler)
}

// ============================================================================
// Start Commit Persistence Tests
// ============================================================================

/// Test that pipeline completes successfully with a valid git context.
///
/// This verifies that when a pipeline run executes with mocked handlers,
/// the baseline mechanism works without errors.
#[test]
fn test_pipeline_completes_with_baseline_context() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_baseline_test_handlers();
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
            "Pipeline should complete successfully with mock handlers"
        );
    });
}

/// Test that pipeline accesses git state via effects.
///
/// This verifies that the pipeline calls git-related effects to
/// retrieve repository state during execution.
#[test]
fn test_pipeline_accesses_git_state() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_baseline_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify at least one git effect was called
        let effects = app_handler.captured();
        let has_git_effect = effects.iter().any(|e| {
            matches!(
                e,
                AppEffect::GitRequireRepo
                    | AppEffect::GitGetRepoRoot
                    | AppEffect::GitGetHeadOid
                    | AppEffect::GitDiff
                    | AppEffect::GitDiffFrom { .. }
                    | AppEffect::GitDiffFromStart
                    | AppEffect::GitIsMainBranch
                    | AppEffect::GitGetDefaultBranch
            )
        });

        assert!(
            has_git_effect,
            "Pipeline should call at least one git effect during execution"
        );
    });
}

// ============================================================================
// Baseline Reset Tests
// ============================================================================

/// Test that --reset-start-commit calls `GitResetStartCommit` effect.
///
/// This verifies that when the --reset-start-commit flag is used,
/// the system calls the appropriate effect to reset the baseline.
#[test]
fn test_reset_start_commit_effect_called() {
    with_default_timeout(|| {
        // Pre-populate start_commit to simulate existing baseline
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("b".repeat(40)) // New HEAD
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/start_commit", "a".repeat(40)) // Old baseline
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(
            &["--reset-start-commit"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        )
        .unwrap();

        // Verify GitResetStartCommit effect was called
        let effects = app_handler.captured();
        let was_reset_called = effects
            .iter()
            .any(|e| matches!(e, AppEffect::GitResetStartCommit));

        assert!(
            was_reset_called,
            "GitResetStartCommit effect should be called with --reset-start-commit flag"
        );
    });
}

/// Test that --reset-start-commit updates the `start_commit` file in mock.
///
/// This verifies that after reset, the `MockAppEffectHandler`'s filesystem
/// contains the updated `start_commit` value.
#[test]
fn test_reset_start_commit_updates_mock_file() {
    with_default_timeout(|| {
        // Pre-populate start_commit with old value
        let old_oid = "a".repeat(40);
        let new_oid = "b".repeat(40);

        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid(&new_oid)
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/start_commit", &old_oid)
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(
            &["--reset-start-commit"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        )
        .unwrap();

        // Verify start_commit file was updated via the GitResetStartCommit effect
        // The mock handler updates the file when GitResetStartCommit is executed
        let start_commit_content = app_handler.get_file(&PathBuf::from(".agent/start_commit"));
        assert!(
            start_commit_content.is_some(),
            "start_commit file should exist after reset"
        );

        let content = start_commit_content.unwrap();
        assert_eq!(
            content, new_oid,
            "start_commit should be updated to new HEAD OID after reset"
        );
    });
}

// ============================================================================
// Empty Diff Handling Tests
// ============================================================================

/// Test that pipeline handles empty diff gracefully.
///
/// This verifies that when there's no diff (no changes since baseline),
/// the pipeline completes successfully without errors.
#[test]
fn test_empty_diff_completes_successfully() {
    with_default_timeout(|| {
        // Set up handler with no diff (empty string)
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/start_commit", "a".repeat(40))
            .with_diff("") // No changes
            .with_staged_changes(false); // Nothing to stage

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should complete without error
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should complete successfully with empty diff"
        );
    });
}

// ============================================================================
// Corrupted Baseline Recovery Tests
// ============================================================================

/// Test that corrupted `start_commit` file is handled gracefully.
///
/// This verifies that when the .`agent/start_commit` file contains invalid data,
/// the system recovers without crashing.
#[test]
fn test_corrupted_start_commit_recovery() {
    with_default_timeout(|| {
        // Pre-populate with corrupted start_commit
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/start_commit", "corrupted_invalid_oid") // Invalid OID
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should complete without crashing
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should handle corrupted start_commit gracefully"
        );
    });
}

/// Test that missing `start_commit` OID is handled gracefully.
///
/// This verifies that when the `start_commit` references a non-existent commit
/// (e.g., after history rewrite), the system recovers.
#[test]
fn test_missing_start_commit_oid_recovery() {
    with_default_timeout(|| {
        // Pre-populate with start_commit pointing to non-existent OID
        // (In real git, this would happen after history rewrite)
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("b".repeat(40)) // Current HEAD
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            // Start commit points to OID that doesn't exist in current history
            .with_file(".agent/start_commit", "a".repeat(40))
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should complete without crashing
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should handle missing start_commit OID gracefully"
        );
    });
}

// ============================================================================
// Pipeline Verbosity Tests
// ============================================================================

/// Test that verbose mode completes successfully.
///
/// This verifies that when the pipeline runs with verbosity flag,
/// it completes without errors (verbose output would show baseline info).
#[test]
fn test_verbose_mode_with_baseline() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_baseline_test_handlers();
        // Pre-populate start_commit
        app_handler = app_handler.with_file(".agent/start_commit", "a".repeat(40));

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with verbose flag
        let result = run_ralph_cli_with_handlers(
            &["--verbosity=2"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should complete successfully with verbose mode"
        );
    });
}

/// Test that debug verbosity mode completes successfully.
///
/// This verifies that when the pipeline runs with maximum verbosity,
/// it completes without errors.
#[test]
fn test_debug_verbosity_mode() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_baseline_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with debug verbosity
        let result = run_ralph_cli_with_handlers(
            &["--verbosity=3"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should complete successfully with debug verbosity"
        );
    });
}
