//! Edge case tests for memory safety
//!
//! These tests verify corner cases and extreme scenarios for memory management:
//! - Extreme execution history limits (0, 1, very large)
//! - Checkpoint serialization edge cases
//! - Recovery from failures
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use ralph_workflow::reducer::state::PipelineState;

/// Helper function to create a test execution step.
fn create_test_step(iteration: u32) -> ExecutionStep {
    ExecutionStep::new(
        "Development",
        iteration,
        "agent_invoked",
        StepOutcome::success(Some("output".to_string()), vec!["file.rs".to_string()]),
    )
    .with_agent("test-agent")
    .with_duration(5)
}

#[test]
fn test_execution_history_limit_zero_prevents_all_growth() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(100, 5);
        let limit = 0; // extreme case: no history allowed

        // Add 100 entries with limit=0
        for i in 0..100 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // With limit=0, no history should be retained
        assert_eq!(
            state.execution_history.len(),
            0,
            "Execution history with limit=0 should retain no entries"
        );
    });
}

#[test]
fn test_execution_history_limit_one_retains_only_latest() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(100, 5);
        let limit = 1; // extreme case: only keep latest entry

        // Add 100 entries with limit=1
        for i in 0..100 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // With limit=1, only the last entry should be retained
        assert_eq!(
            state.execution_history.len(),
            1,
            "Execution history with limit=1 should retain only 1 entry"
        );

        // Verify it's the most recent entry (iteration 99)
        assert_eq!(
            state.execution_history[0].iteration, 99,
            "Should retain the most recent entry"
        );
    });
}

#[test]
fn test_execution_history_very_large_limit_works() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(100, 5);
        let limit = 1_000_000; // very large limit (unlikely in practice)

        // Add 100 entries with very large limit
        for i in 0..100 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // All entries should be retained since we're under the limit
        assert_eq!(
            state.execution_history.len(),
            100,
            "Execution history should retain all entries when under large limit"
        );
    });
}

#[test]
fn test_execution_history_bounding_at_exact_limit() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(100, 5);
        let limit = 50;

        // Add exactly 50 entries (at the limit)
        for i in 0..50 {
            state.add_execution_step(create_test_step(i), limit);
        }

        assert_eq!(
            state.execution_history.len(),
            50,
            "Should retain all entries when exactly at limit"
        );

        // Add one more entry (should trigger bounding)
        state.add_execution_step(create_test_step(50), limit);

        assert_eq!(
            state.execution_history.len(),
            50,
            "Should maintain limit after adding one more entry"
        );

        // First entry should now be iteration 1 (iteration 0 dropped)
        assert_eq!(
            state.execution_history[0].iteration, 1,
            "Oldest entry should have been dropped"
        );
    });
}

#[test]
fn test_execution_history_large_single_step() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(10, 5);
        let limit = 1000;

        // Create a step with very large output (10 MB of text)
        let large_output = "x".repeat(10 * 1024 * 1024); // 10 MB
        let large_step = ExecutionStep::new(
            "Development",
            0,
            "agent_invoked",
            StepOutcome::success(Some(large_output), vec!["file.rs".to_string()]),
        );

        state.add_execution_step(large_step, limit);

        // Should handle large individual steps without panic
        assert_eq!(
            state.execution_history.len(),
            1,
            "Should successfully add very large execution step"
        );
    });
}

#[test]
fn test_execution_history_many_files_modified() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(10, 5);
        let limit = 1000;

        // Create a step that modified many files
        let many_files: Vec<String> = (0..1000).map(|i| format!("file_{i}.rs")).collect();
        let step_with_many_files = ExecutionStep::new(
            "Development",
            0,
            "agent_invoked",
            StepOutcome::success(Some("output".to_string()), many_files),
        );

        state.add_execution_step(step_with_many_files, limit);

        // Should handle steps with many files modified
        assert_eq!(
            state.execution_history.len(),
            1,
            "Should successfully add step with many files modified"
        );

        // Verify files_modified is preserved
        if let StepOutcome::Success { files_modified, .. } = &state.execution_history[0].outcome {
            assert_eq!(
                files_modified.len(),
                1000,
                "Should preserve all files_modified entries"
            );
        } else {
            panic!("Expected Success outcome");
        }
    });
}

#[test]
fn test_execution_history_rapid_limit_changes() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(100, 5);

        // Add entries with varying limits
        for i in 0..20 {
            state.add_execution_step(create_test_step(i), 100);
        }
        assert_eq!(state.execution_history.len(), 20);

        // Switch to smaller limit mid-execution
        for i in 20..40 {
            state.add_execution_step(create_test_step(i), 10);
        }

        // History should be bounded to 10 (the new limit)
        assert_eq!(
            state.execution_history.len(),
            10,
            "Should enforce new smaller limit"
        );

        // Verify we have the most recent entries (30-39)
        assert_eq!(
            state.execution_history[0].iteration, 30,
            "Should have oldest entry from recent window"
        );
        assert_eq!(
            state.execution_history[9].iteration, 39,
            "Should have newest entry from recent window"
        );
    });
}

#[test]
fn test_checkpoint_serialization_with_empty_history() {
    with_default_timeout(|| {
        let state = PipelineState::initial(100, 5);

        // Serialize state with empty execution history
        let json = serde_json::to_string(&state).expect("Should serialize empty state");

        // Deserialize back
        let _deserialized: PipelineState =
            serde_json::from_str(&json).expect("Should deserialize empty state");

        // Empty history should serialize/deserialize correctly
        assert!(json.contains("execution_history"));
    });
}

#[test]
fn test_checkpoint_serialization_roundtrip_preserves_bounded_history() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(100, 5);
        let limit = 50;

        // Add 100 entries with limit=50
        for i in 0..100 {
            state.add_execution_step(create_test_step(i), limit);
        }

        assert_eq!(state.execution_history.len(), 50);

        // Serialize
        let json = serde_json::to_string(&state).expect("Should serialize");

        // Deserialize
        let deserialized: PipelineState = serde_json::from_str(&json).expect("Should deserialize");

        // Verify bounded history is preserved
        assert_eq!(
            deserialized.execution_history.len(),
            50,
            "Deserialized state should preserve bounded history length"
        );

        // Verify entries are the most recent ones (50-99)
        assert_eq!(
            deserialized.execution_history[0].iteration, 50,
            "Deserialized state should have oldest entry from bounded window"
        );
        assert_eq!(
            deserialized.execution_history[49].iteration, 99,
            "Deserialized state should have newest entry from bounded window"
        );
    });
}

#[test]
fn test_execution_history_with_all_outcome_types() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(100, 5);
        let limit = 1000;

        // Add steps with different outcome types
        state.add_execution_step(
            ExecutionStep::new(
                "Development",
                0,
                "agent_invoked",
                StepOutcome::success(Some("success output".to_string()), vec![]),
            ),
            limit,
        );

        state.add_execution_step(
            ExecutionStep::new(
                "Review",
                1,
                "review_completed",
                StepOutcome::failure("error message".to_string(), true),
            ),
            limit,
        );

        state.add_execution_step(
            ExecutionStep::new(
                "Development",
                2,
                "continuation",
                StepOutcome::partial("completed part".to_string(), "remaining work".to_string()),
            ),
            limit,
        );

        state.add_execution_step(
            ExecutionStep::new(
                "Review",
                3,
                "skipped",
                StepOutcome::skipped("no review needed".to_string()),
            ),
            limit,
        );

        // All outcome types should be handled correctly
        assert_eq!(
            state.execution_history.len(),
            4,
            "Should handle all outcome types"
        );

        // Verify we can serialize/deserialize with all outcome types
        let json = serde_json::to_string(&state).expect("Should serialize all outcome types");
        let _deserialized: PipelineState =
            serde_json::from_str(&json).expect("Should deserialize all outcome types");
    });
}
