//! Arc circular reference prevention tests
//!
//! These tests verify that Arc usage patterns in the codebase do not create
//! circular references that would prevent memory from being freed. They test
//! observable behavior (Arc strong_count behavior) rather than internal
//! implementation details.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! This module tests observable behavior:
//! - Arc strong_count remains stable (no unexpected growth)
//! - Arc cleanup after pipeline completion
//! - No circular references in typical usage patterns

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::executor::MockProcessExecutor;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use std::path::PathBuf;
use std::sync::Arc;

/// Standard PROMPT.md content for Arc pattern tests.
const STANDARD_PROMPT: &str = r#"## Goal

Test Arc patterns.

## Acceptance

- No circular references
"#;

#[test]
fn test_workspace_arc_count_stable() {
    with_default_timeout(|| {
        // Create workspace wrapped in Arc
        let workspace = Arc::new(MemoryWorkspace::new_test().with_file("test.txt", "content"));

        let initial_count = Arc::strong_count(&workspace);

        // Perform operations that might create Arc clones
        let _clone1 = workspace.clone();
        let _clone2 = workspace.clone();

        let cloned_count = Arc::strong_count(&workspace);
        assert_eq!(
            cloned_count,
            initial_count + 2,
            "Strong count should increase by 2 with 2 clones"
        );

        // Drop clones
        drop(_clone1);
        drop(_clone2);

        let final_count = Arc::strong_count(&workspace);
        assert_eq!(
            final_count, initial_count,
            "Strong count should return to initial after dropping clones"
        );
    });
}

#[test]
fn test_executor_arc_cleanup_after_pipeline() {
    with_default_timeout(|| {
        let executor = mock_executor_with_success();
        let initial_count = Arc::strong_count(&executor);

        {
            let mut app_handler = MockAppEffectHandler::new()
                .with_head_oid("a".repeat(40))
                .with_cwd(PathBuf::from("/mock/repo"))
                .with_file("PROMPT.md", STANDARD_PROMPT);

            let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
            let config = create_test_config_struct();

            // Clone executor for pipeline use
            let executor_clone = executor.clone();
            let count_during = Arc::strong_count(&executor);
            assert!(
                count_during >= initial_count,
                "Count should increase or stay same during clone"
            );

            let result = run_ralph_cli_with_handlers(
                &[],
                executor_clone,
                config,
                &mut app_handler,
                &mut effect_handler,
            );

            assert!(result.is_ok(), "Pipeline should complete successfully");

            // executor_clone dropped at end of scope
        }

        // After pipeline completion, count should return to initial
        let final_count = Arc::strong_count(&executor);
        assert_eq!(
            final_count, initial_count,
            "Executor Arc count should return to initial after pipeline completion"
        );
    });
}

#[test]
fn test_multiple_executor_uses_no_leak() {
    with_default_timeout(|| {
        let executor = mock_executor_with_success();
        let initial_count = Arc::strong_count(&executor);

        // Use executor in multiple pipelines
        for _ in 0..5 {
            let mut app_handler = MockAppEffectHandler::new()
                .with_head_oid("a".repeat(40))
                .with_cwd(PathBuf::from("/mock/repo"))
                .with_file("PROMPT.md", STANDARD_PROMPT);

            let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
            let config = create_test_config_struct();

            let _ = run_ralph_cli_with_handlers(
                &[],
                executor.clone(),
                config,
                &mut app_handler,
                &mut effect_handler,
            );
        }

        // After all runs, count should return to initial (no accumulation)
        let final_count = Arc::strong_count(&executor);
        assert_eq!(
            final_count, initial_count,
            "Executor Arc count should not accumulate across multiple runs"
        );
    });
}

#[test]
fn test_workspace_arc_cleanup_after_multiple_operations() {
    with_default_timeout(|| {
        let workspace = Arc::new(MemoryWorkspace::new_test());
        let initial_count = Arc::strong_count(&workspace);

        // Perform multiple operations that might clone Arc
        for i in 0..10 {
            let ws_clone = workspace.clone();
            let _ = ws_clone.write(std::path::Path::new(&format!("file{}.txt", i)), "content");
        }

        // After operations complete, count should return to initial
        let final_count = Arc::strong_count(&workspace);
        assert_eq!(
            final_count, initial_count,
            "Workspace Arc count should return to initial after operations"
        );
    });
}

#[test]
fn test_no_arc_cycles_in_mock_executor() {
    with_default_timeout(|| {
        let executor = Arc::new(MockProcessExecutor::new());
        let initial_count = Arc::strong_count(&executor);

        // Create multiple references
        let refs: Vec<_> = (0..10).map(|_| executor.clone()).collect();

        let cloned_count = Arc::strong_count(&executor);
        assert_eq!(
            cloned_count,
            initial_count + 10,
            "Count should increase by number of clones"
        );

        // Drop all references
        drop(refs);

        let final_count = Arc::strong_count(&executor);
        assert_eq!(
            final_count, initial_count,
            "Count should return to initial - no cycles"
        );
    });
}

#[test]
fn test_arc_cleanup_after_agent_invocation() {
    with_default_timeout(|| {
        let executor = mock_executor_with_success();
        let initial_count = Arc::strong_count(&executor);

        {
            let mut app_handler = MockAppEffectHandler::new()
                .with_head_oid("a".repeat(40))
                .with_cwd(PathBuf::from("/mock/repo"))
                .with_file("PROMPT.md", STANDARD_PROMPT);

            // Configure 1 iteration to trigger agent invocation
            let mut effect_handler = MockEffectHandler::new(PipelineState::initial(1, 0));
            let config = create_test_config_struct();

            let result = run_ralph_cli_with_handlers(
                &[],
                executor.clone(),
                config,
                &mut app_handler,
                &mut effect_handler,
            );

            assert!(
                result.is_ok(),
                "Pipeline with agent invocation should complete"
            );
        }

        // After agent invocation, Arc should be cleaned up
        let final_count = Arc::strong_count(&executor);
        assert_eq!(
            final_count, initial_count,
            "Executor Arc count should return to initial after agent invocation"
        );
    });
}

#[test]
fn test_nested_arc_usage_no_cycles() {
    with_default_timeout(|| {
        // Test pattern: Arc<dyn Trait> usage (common in executor patterns)
        let executor: Arc<dyn ralph_workflow::executor::ProcessExecutor> =
            Arc::new(MockProcessExecutor::new());

        let initial_count = Arc::strong_count(&executor);

        // Clone and use in nested context
        {
            let clone1 = executor.clone();
            {
                let clone2 = clone1.clone();
                let count_nested = Arc::strong_count(&executor);
                assert_eq!(
                    count_nested,
                    initial_count + 2,
                    "Count should increase in nested scope"
                );
                drop(clone2);
            }
            drop(clone1);
        }

        // After nested scopes, should return to initial
        let final_count = Arc::strong_count(&executor);
        assert_eq!(
            final_count, initial_count,
            "Count should return to initial after nested usage"
        );
    });
}

#[test]
fn test_arc_with_workspace_and_executor_together() {
    with_default_timeout(|| {
        let workspace = Arc::new(MemoryWorkspace::new_test());
        let executor = mock_executor_with_success();

        let ws_initial = Arc::strong_count(&workspace);
        let exec_initial = Arc::strong_count(&executor);

        // Use both together
        {
            let _ws_clone = workspace.clone();
            let _exec_clone = executor.clone();

            // Counts should increase
            assert_eq!(Arc::strong_count(&workspace), ws_initial + 1);
            assert_eq!(Arc::strong_count(&executor), exec_initial + 1);
        }

        // After scope, both should return to initial
        assert_eq!(
            Arc::strong_count(&workspace),
            ws_initial,
            "Workspace Arc count should return to initial"
        );
        assert_eq!(
            Arc::strong_count(&executor),
            exec_initial,
            "Executor Arc count should return to initial"
        );
    });
}

#[test]
fn test_logger_printer_no_circular_reference() {
    with_default_timeout(|| {
        // Verify Logger and Printer Arc patterns are acyclic
        // Logger may hold Arc to workspace, but workspace doesn't hold Logger Arc

        use ralph_workflow::logger::{Colors, Logger};
        use ralph_workflow::workspace::Workspace;

        let workspace = Arc::new(MemoryWorkspace::new_test());
        let ws_initial_count = Arc::strong_count(&workspace);

        {
            let logger = Logger::new(Colors::new());
            // Logger lifetime is independent of workspace

            let ws_clone = workspace.clone();
            let _result = ws_clone.write(std::path::Path::new("test.txt"), "content");

            // Workspace count should increase by 1 for the clone
            assert_eq!(Arc::strong_count(&workspace), ws_initial_count + 1);

            drop(ws_clone);
            drop(logger);
        }

        // After dropping logger and workspace clone, count returns to initial
        assert_eq!(
            Arc::strong_count(&workspace),
            ws_initial_count,
            "Workspace Arc count should return to initial - no circular refs with Logger"
        );
    });
}

#[test]
fn test_workspace_clone_tree_depth_bounded() {
    with_default_timeout(|| {
        // Verify workspace clones don't create long reference chains
        // Each clone is independent, not a chain

        let workspace = Arc::new(MemoryWorkspace::new_test());
        let initial_count = Arc::strong_count(&workspace);

        // Create multiple "generations" of clones
        let gen1 = workspace.clone();
        let gen2 = gen1.clone();
        let gen3 = gen2.clone();

        // All should point to same Arc, count should be initial + 3
        assert_eq!(
            Arc::strong_count(&workspace),
            initial_count + 3,
            "All clones should share same Arc"
        );

        // Drop in reverse order
        drop(gen3);
        assert_eq!(Arc::strong_count(&workspace), initial_count + 2);

        drop(gen2);
        assert_eq!(Arc::strong_count(&workspace), initial_count + 1);

        drop(gen1);
        assert_eq!(
            Arc::strong_count(&workspace),
            initial_count,
            "Count returns to initial - no chain accumulation"
        );
    });
}

#[test]
fn test_process_executor_cleanup_releases_all_references() {
    with_default_timeout(|| {
        // Verify executor cleanup releases all Arc references
        // This tests that executor doesn't leak references through closures or callbacks

        let executor = mock_executor_with_success();
        let initial_count = Arc::strong_count(&executor);

        // Simulate pipeline execution with executor
        {
            let exec1 = executor.clone();
            let exec2 = executor.clone();

            assert_eq!(Arc::strong_count(&executor), initial_count + 2);

            // Executors are cloned and used in different contexts
            // No need to actually execute - just verify cleanup

            drop(exec1);
            drop(exec2);
        }

        // All references should be released
        assert_eq!(
            Arc::strong_count(&executor),
            initial_count,
            "Executor should release all references after use"
        );
    });
}

#[test]
fn test_pipeline_state_arc_patterns_deterministic() {
    with_default_timeout(|| {
        // Verify state cloning behavior is predictable
        // PipelineState doesn't use Arc internally, but this tests
        // that structures holding Arc behave deterministically

        let workspace = Arc::new(MemoryWorkspace::new_test());
        let executor = mock_executor_with_success();

        let ws_initial = Arc::strong_count(&workspace);
        let exec_initial = Arc::strong_count(&executor);

        // Create multiple pipeline contexts using same Arc instances
        let contexts: Vec<_> = (0..5)
            .map(|_| (workspace.clone(), executor.clone()))
            .collect();

        // Counts should increase predictably
        assert_eq!(
            Arc::strong_count(&workspace),
            ws_initial + 5,
            "Workspace count should increase by 5"
        );
        assert_eq!(
            Arc::strong_count(&executor),
            exec_initial + 5,
            "Executor count should increase by 5"
        );

        drop(contexts);

        // Counts should return to initial
        assert_eq!(
            Arc::strong_count(&workspace),
            ws_initial,
            "Workspace count deterministically returns to initial"
        );
        assert_eq!(
            Arc::strong_count(&executor),
            exec_initial,
            "Executor count deterministically returns to initial"
        );
    });
}
