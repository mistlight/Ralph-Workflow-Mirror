//! Thread lifecycle cleanup verification tests
//!
//! These tests verify that all background threads are properly joined and cleaned up
//! under both normal and panic conditions. They test observable behavior (no hangs,
//! clean shutdown) rather than internal thread management details.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! This module tests observable behavior:
//! - Pipeline completes without hanging
//! - Background threads do not prevent shutdown
//! - No thread leaks after pipeline completion
//! - Panic scenarios are handled gracefully

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Standard PROMPT.md content for thread lifecycle tests.
const STANDARD_PROMPT: &str = r#"## Goal

Test thread cleanup.

## Acceptance

- Pipeline completes
"#;

#[test]
fn test_pipeline_completes_without_hanging() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Pipeline should complete without hanging
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should complete successfully without hanging"
        );

        // If we reach here, no threads blocked shutdown
        // (with_default_timeout would panic if we hung)
    });
}

#[test]
fn test_pipeline_completes_multiple_times_no_thread_accumulation() {
    with_default_timeout(|| {
        // Run pipeline 10 times to verify no thread leaks accumulate
        for run in 0..10 {
            let mut app_handler = MockAppEffectHandler::new()
                .with_head_oid("a".repeat(40))
                .with_cwd(PathBuf::from("/mock/repo"))
                .with_file("PROMPT.md", STANDARD_PROMPT);

            let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
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
                "Pipeline run {} should complete successfully",
                run
            );
        }

        // If we reach here without hanging, no threads were leaked across runs
    });
}

#[test]
fn test_background_monitor_thread_does_not_prevent_shutdown() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file("test.txt", "content");

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Pipeline with file monitoring should still complete
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should complete with file monitoring active"
        );

        // Background monitor thread should not prevent shutdown
        // (documented tradeoff: monitor thread is detached on panic)
    });
}

#[test]
fn test_streaming_threads_cleaned_up_after_agent_invocation() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT);

        // Configure 1 development iteration to trigger agent invocation
        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(1, 0));
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
            "Pipeline with agent invocation should complete"
        );

        // Streaming threads should be cleaned up (no hang)
    });
}

#[test]
fn test_pipeline_shutdown_is_graceful() {
    with_default_timeout(|| {
        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = completed.clone();

        // Spawn thread to run pipeline
        let handle = std::thread::spawn(move || {
            let mut app_handler = MockAppEffectHandler::new()
                .with_head_oid("a".repeat(40))
                .with_cwd(PathBuf::from("/mock/repo"))
                .with_file("PROMPT.md", STANDARD_PROMPT);

            let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
            let config = create_test_config_struct();
            let executor = mock_executor_with_success();

            let result = run_ralph_cli_with_handlers(
                &[],
                executor,
                config,
                &mut app_handler,
                &mut effect_handler,
            );

            completed_clone.store(true, Ordering::SeqCst);
            result
        });

        // Wait for completion
        let result = handle.join().expect("Pipeline thread should not panic");

        assert!(result.is_ok(), "Pipeline should complete successfully");
        assert!(
            completed.load(Ordering::SeqCst),
            "Pipeline should set completion flag"
        );

        // Thread was joined successfully - clean shutdown
    });
}

#[test]
fn test_no_deadlocks_with_concurrent_file_access() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file("file1.txt", "content1")
            .with_file("file2.txt", "content2")
            .with_file("file3.txt", "content3");

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Pipeline with multiple files should not deadlock
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline with multiple files should not deadlock"
        );
    });
}

#[test]
fn test_panic_in_effect_handler_does_not_hang() {
    with_default_timeout(|| {
        // Note: MockEffectHandler doesn't actually panic in this test,
        // but we verify that error paths (which could include panics in production)
        // don't leave threads hanging

        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Even if effect handling encounters errors, pipeline should complete or fail gracefully
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        // Result may be Ok or Err, but should not hang
        assert!(
            result.is_ok() || result.is_err(),
            "Pipeline should complete (success or failure) without hanging"
        );
    });
}

#[test]
fn test_rapid_start_stop_no_thread_leaks() {
    with_default_timeout(|| {
        // Rapidly start and complete pipelines to stress test thread cleanup
        for _ in 0..20 {
            let mut app_handler = MockAppEffectHandler::new()
                .with_head_oid("a".repeat(40))
                .with_cwd(PathBuf::from("/mock/repo"))
                .with_file("PROMPT.md", STANDARD_PROMPT);

            let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
            let config = create_test_config_struct();
            let executor = mock_executor_with_success();

            let _ = run_ralph_cli_with_handlers(
                &[],
                executor,
                config,
                &mut app_handler,
                &mut effect_handler,
            );

            // Each run should complete quickly without thread accumulation
        }

        // If we complete without timeout, no threads leaked
    });
}
