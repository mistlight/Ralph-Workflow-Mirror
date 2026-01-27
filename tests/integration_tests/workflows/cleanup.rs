//! Cleanup and error recovery integration tests.
//!
//! These tests verify that the pipeline properly cleans up resources
//! and handles errors gracefully.
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
    create_test_config_struct, create_test_config_struct_with_isolation,
    mock_executor_with_success, run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;
use ralph_workflow::app::effect::AppEffect;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use std::path::PathBuf;

/// Standard PROMPT.md content for cleanup tests.
const STANDARD_PROMPT: &str = r#"## Goal

Test cleanup functionality.

## Acceptance

- Tests pass
"#;

/// Create mock handlers with standard setup for cleanup tests.
fn create_cleanup_test_handlers() -> (MockAppEffectHandler, MockEffectHandler) {
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
// Cleanup and Error Recovery Tests
// ============================================================================

/// Test that the pipeline completes cleanly with 0 iterations.
///
/// This verifies that when pipeline runs with developer_iters=0 and reviewer_reviews=0,
/// system completes successfully via effect capture.
#[test]
fn test_pipeline_completes_cleanly_with_zero_iterations() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_cleanup_test_handlers();
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
            "Pipeline should complete successfully with zero iterations"
        );
    });
}

/// Test that cleanup happens and leaves no uncommitted changes.
///
/// This verifies that when pipeline completes, it leaves the repository
/// in a clean state (via effect capture showing commit was created).
#[test]
fn test_pipeline_creates_commit_on_completion() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_cleanup_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify CreateCommit effect was called at the reducer layer
        let was_commit_created =
            effect_handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. }));

        assert!(
            was_commit_created,
            "CreateCommit effect should be called when there are changes"
        );
    });
}

/// Test that agent phase skipping is handled gracefully.
///
/// This verifies that when agent phases are skipped due to zero iterations,
/// the pipeline completes successfully without agent execution.
#[test]
fn test_agent_phase_skipping_handled_gracefully() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_cleanup_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should complete successfully without agent execution
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should complete successfully when agent phases are skipped"
        );
    });
}

/// Test that malformed config in mock filesystem is handled gracefully.
///
/// This verifies that when the config file is malformed, the system
/// uses default configuration and continues successfully.
#[test]
fn test_malformed_config_handled_gracefully() {
    with_default_timeout(|| {
        // Add malformed config file to mock filesystem
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/agents.toml", "this is not valid { toml ] syntax")
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Pipeline should succeed using defaults (config loader is lenient)
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should handle malformed config gracefully"
        );
    });
}

// ============================================================================
// Isolation Mode Tests
// ============================================================================

/// Test that isolation mode does not create STATUS.md, NOTES.md, or ISSUES.md.
///
/// This verifies that when isolation mode is enabled (default), the system
/// does not write STATUS.md, NOTES.md, or ISSUES.md files via effects.
#[test]
fn test_isolation_mode_does_not_create_context_files() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_cleanup_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify STATUS.md, NOTES.md and ISSUES.md are NOT in mock filesystem
        assert!(
            app_handler
                .get_file(&PathBuf::from(".agent/STATUS.md"))
                .is_none(),
            "STATUS.md should not be created in isolation mode"
        );
        assert!(
            app_handler
                .get_file(&PathBuf::from(".agent/NOTES.md"))
                .is_none(),
            "NOTES.md should not be created in isolation mode"
        );
        assert!(
            app_handler
                .get_file(&PathBuf::from(".agent/ISSUES.md"))
                .is_none(),
            "ISSUES.md should not be created in isolation mode"
        );
    });
}

/// Test that isolation mode deletes existing STATUS.md, NOTES.md, and ISSUES.md.
///
/// This verifies that when isolation mode is enabled and these files exist,
/// the system deletes them via DeleteFile effects.
#[test]
fn test_isolation_mode_deletes_existing_context_files() {
    with_default_timeout(|| {
        // Pre-create STATUS.md, NOTES.md and ISSUES.md
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/STATUS.md", "old status")
            .with_file(".agent/NOTES.md", "old notes")
            .with_file(".agent/ISSUES.md", "old issues")
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Check that DeleteFile effects were called for these files
        let effects = app_handler.captured();
        let status_deleted = effects
            .iter()
            .any(|e| matches!(e, AppEffect::DeleteFile { path } if path.ends_with("STATUS.md")));
        let notes_deleted = effects
            .iter()
            .any(|e| matches!(e, AppEffect::DeleteFile { path } if path.ends_with("NOTES.md")));
        let issues_deleted = effects
            .iter()
            .any(|e| matches!(e, AppEffect::DeleteFile { path } if path.ends_with("ISSUES.md")));

        assert!(
            status_deleted,
            "DeleteFile effect should be called for STATUS.md in isolation mode"
        );
        assert!(
            notes_deleted,
            "DeleteFile effect should be called for NOTES.md in isolation mode"
        );
        assert!(
            issues_deleted,
            "DeleteFile effect should be called for ISSUES.md in isolation mode"
        );
    });
}

/// Test that --no-isolation flag creates STATUS.md, NOTES.md, and ISSUES.md.
///
/// This verifies that when the --no-isolation flag is used, the system
/// creates STATUS.md, NOTES.md, and ISSUES.md files via WriteFile effects.
#[test]
fn test_no_isolation_creates_context_files() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct_with_isolation(false);
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(
            &["--no-isolation"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        )
        .unwrap();

        // Verify STATUS.md, NOTES.md and ISSUES.md were written via WriteFile effects
        let effects = app_handler.captured();
        let status_written = effects
            .iter()
            .any(|e| matches!(e, AppEffect::WriteFile { path, .. } if path.ends_with("STATUS.md")));
        let notes_written = effects
            .iter()
            .any(|e| matches!(e, AppEffect::WriteFile { path, .. } if path.ends_with("NOTES.md")));
        let issues_written = effects
            .iter()
            .any(|e| matches!(e, AppEffect::WriteFile { path, .. } if path.ends_with("ISSUES.md")));

        assert!(
            status_written,
            "WriteFile effect should be called for STATUS.md when --no-isolation is used"
        );
        assert!(
            notes_written,
            "WriteFile effect should be called for NOTES.md when --no-isolation is used"
        );
        assert!(
            issues_written,
            "WriteFile effect should be called for ISSUES.md when --no-isolation is used"
        );
    });
}

/// Test that isolation_mode = false creates STATUS.md, NOTES.md, and ISSUES.md.
///
/// This verifies that when isolation mode is disabled via config,
/// the system creates STATUS.md, NOTES.md, and ISSUES.md files.
#[test]
fn test_isolation_mode_config_false_creates_context_files() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct_with_isolation(false);
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify context files were written
        let effects = app_handler.captured();
        let has_write_effects = effects.iter().any(|e| {
            matches!(e, AppEffect::WriteFile { path, .. }
                if path.ends_with("STATUS.md") || path.ends_with("NOTES.md") || path.ends_with("ISSUES.md"))
        });

        assert!(
            has_write_effects,
            "WriteFile effects should be called for context files when isolation_mode = false"
        );
    });
}

/// Test that --no-isolation overwrites existing STATUS.md, NOTES.md, and ISSUES.md.
///
/// This verifies that when --no-isolation is used and these files already exist,
/// the system overwrites them with new content.
#[test]
fn test_no_isolation_overwrites_existing_context_files() {
    with_default_timeout(|| {
        // Pre-create context files with detailed content
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/STATUS.md", "Planning.\nDid X.\nDid Y.\n")
            .with_file(".agent/NOTES.md", "Lots of context.\nDetails.\n")
            .with_file(".agent/ISSUES.md", "Issue A: details.\nIssue B: details.\n")
            .with_diff("diff --git a/test.txt b/test.txt\n+new content")
            .with_staged_changes(true);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct_with_isolation(false);
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(
            &["--no-isolation"],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        )
        .unwrap();

        // Verify files were overwritten with single-line content
        let status_content = app_handler.get_file(&PathBuf::from(".agent/STATUS.md"));
        let notes_content = app_handler.get_file(&PathBuf::from(".agent/NOTES.md"));
        let issues_content = app_handler.get_file(&PathBuf::from(".agent/ISSUES.md"));

        assert!(
            status_content.is_some(),
            "STATUS.md should exist after --no-isolation"
        );
        assert!(
            notes_content.is_some(),
            "NOTES.md should exist after --no-isolation"
        );
        assert!(
            issues_content.is_some(),
            "ISSUES.md should exist after --no-isolation"
        );

        // Verify content was overwritten to single line
        assert_eq!(
            status_content.unwrap(),
            "In progress.\n",
            "STATUS.md should be overwritten to single line"
        );
        assert_eq!(
            notes_content.unwrap(),
            "Notes.\n",
            "NOTES.md should be overwritten to single line"
        );
        assert_eq!(
            issues_content.unwrap(),
            "No issues recorded.\n",
            "ISSUES.md should be overwritten to single line"
        );
    });
}

// ============================================================================
// Resume/Checkpoint Tests
// ============================================================================

/// Test that phase-skipping behavior works correctly.
///
/// This verifies that when phases are skipped due to zero iterations,
/// the pipeline completes successfully.
#[test]
fn test_phase_skipping_completes_successfully() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_cleanup_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should complete successfully without agent execution
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should complete successfully when phases are skipped"
        );
    });
}

// ============================================================================
// Incremental Commit Tests
// ============================================================================

/// Test that development infrastructure is in place for creating commits.
///
/// This verifies that the commit creation effect is called when
/// there are changes to commit.
#[test]
fn test_commit_infrastructure_in_place() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_cleanup_test_handlers();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify the commit infrastructure works (CreateCommit effect is called)
        let was_commit_created =
            effect_handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. }));

        assert!(
            was_commit_created,
            "CreateCommit effect should be called when there are changes"
        );
    });
}
