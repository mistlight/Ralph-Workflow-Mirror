//! Bounded memory growth tests
//!
//! These tests verify that execution_history does not grow unbounded during
//! long-running pipelines. Following TDD: these tests are written FIRST and
//! should FAIL until the bounding mechanism is implemented in Step 11.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! This module tests observable behavior:
//! - execution_history length remains bounded
//! - Oldest entries are dropped when limit is reached
//! - Checkpoint size remains reasonable with bounded history
//! - Resume from checkpoint preserves bounded history correctly

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
fn test_execution_history_does_not_exceed_default_limit() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(2000, 5);
        let limit = 1000; // default limit

        // Simulate 2000 iterations - more than default limit of 1000
        for i in 0..2000 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // After bounding implementation, history should be capped at 1000 (default limit)
        assert!(
            state.execution_history.len() <= 1000,
            "Execution history should not exceed default limit of 1000, but got {}",
            state.execution_history.len()
        );
    });
}

#[test]
fn test_execution_history_drops_oldest_entries_when_limit_reached() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1500, 5);
        let limit = 1000;

        // Add 1500 entries using bounded method
        for i in 0..1500 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // After bounding, history should be at limit (1000)
        assert_eq!(
            state.execution_history.len(),
            1000,
            "History length should equal the limit"
        );

        // Oldest entries (iterations 0-499) should be gone
        // Most recent entries (iterations 500-1499) should remain
        let first_iteration = state
            .execution_history
            .first()
            .map(|step| step.iteration)
            .unwrap_or(0);

        let last_iteration = state
            .execution_history
            .last()
            .map(|step| step.iteration)
            .unwrap_or(0);

        // First entry should be from iteration 500 or later (oldest 500 dropped)
        assert!(
            first_iteration >= 500,
            "First entry should be from iteration 500+, got {}",
            first_iteration
        );

        // Last entry should be from iteration 1499
        assert_eq!(
            last_iteration, 1499,
            "Last entry should be from most recent iteration"
        );
    });
}

#[test]
fn test_execution_history_ring_buffer_behavior() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1200, 5);
        let limit = 1000;

        // Fill to limit (1000 entries)
        for i in 0..1000 {
            state.add_execution_step(create_test_step(i), limit);
        }

        assert_eq!(state.execution_history.len(), 1000);

        // Add 200 more entries - should maintain limit by dropping oldest
        for i in 1000..1200 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // Should still be at limit
        assert_eq!(
            state.execution_history.len(),
            1000,
            "History should remain at limit after additional entries"
        );

        // First entry should now be from iteration 200 (oldest 200 dropped)
        let first_iteration = state
            .execution_history
            .first()
            .map(|step| step.iteration)
            .unwrap();

        assert_eq!(
            first_iteration, 200,
            "First entry should be from iteration 200 after ring buffer wrap"
        );

        // Last entry should be from iteration 1199
        let last_iteration = state
            .execution_history
            .last()
            .map(|step| step.iteration)
            .unwrap();

        assert_eq!(
            last_iteration, 1199,
            "Last entry should be from most recent iteration"
        );
    });
}

#[test]
fn test_execution_history_bounded_growth_prevents_oom() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(10000, 5);

        // Simulate 10,000 iterations (stress test)
        // Without bounding, this would grow to 10,000 entries
        // With bounding, should stay at limit (1000)
        let limit = 1000;
        for i in 0..10000 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // This test verifies the upper bound behavior
        // Before Step 11: This will fail with 10,000 entries (unbounded growth)
        // After Step 11: This will pass with 1,000 entries (bounded)
        let max_allowed = 1000;

        if state.execution_history.len() > max_allowed {
            panic!(
                "Execution history grew unbounded! Expected <= {}, got {}. \
                 This test will pass after Step 11 implements bounding.",
                max_allowed,
                state.execution_history.len()
            );
        }
    });
}

#[test]
fn test_checkpoint_size_remains_reasonable_with_bounded_history() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(2000, 5);
        let limit = 1000;

        // Add 2000 entries using bounded method
        for i in 0..2000 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // Serialize to JSON (checkpoint format)
        let json = serde_json::to_string(&state).expect("Serialization should succeed");
        let size_kb = json.len() / 1024;
        let size_mb = size_kb / 1024;

        println!(
            "Checkpoint size with {} entries: {} KB ({} MB)",
            state.execution_history.len(),
            size_kb,
            size_mb
        );

        // With bounded history (1000 entries), checkpoint should be < 1 MB
        // Before bounding: 2000 entries → ~750 KB
        // After bounding: 1000 entries → ~375 KB
        assert!(
            size_mb < 1,
            "Checkpoint size should be < 1 MB with bounded history, got {} MB",
            size_mb
        );

        // More specific: with 1000 entries, should be around 375-400 KB
        assert!(
            size_kb < 500,
            "Checkpoint size should be < 500 KB with 1000 entries, got {} KB",
            size_kb
        );
    });
}

#[test]
fn test_resume_from_checkpoint_preserves_bounded_history() {
    with_default_timeout(|| {
        let mut original_state = PipelineState::initial(1500, 5);
        let limit = 1000;

        // Add 1500 entries (will be bounded to 1000)
        for i in 0..1500 {
            original_state.add_execution_step(create_test_step(i), limit);
        }

        // Should be bounded
        assert_eq!(original_state.execution_history.len(), 1000);

        // Serialize and deserialize (checkpoint round-trip)
        let json = serde_json::to_string(&original_state).expect("Serialization should succeed");
        let restored_state: PipelineState =
            serde_json::from_str(&json).expect("Deserialization should succeed");

        // Restored state should have same bounded history
        assert_eq!(
            restored_state.execution_history.len(),
            1000,
            "Restored state should preserve bounded history length"
        );

        // First and last entries should match
        let original_first = original_state.execution_history.first().unwrap();
        let restored_first = restored_state.execution_history.first().unwrap();
        assert_eq!(
            original_first.iteration, restored_first.iteration,
            "First entry should be preserved after resume"
        );

        let original_last = original_state.execution_history.last().unwrap();
        let restored_last = restored_state.execution_history.last().unwrap();
        assert_eq!(
            original_last.iteration, restored_last.iteration,
            "Last entry should be preserved after resume"
        );
    });
}

#[test]
fn test_bounded_history_maintains_recent_context() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1500, 5);
        let limit = 1000;

        // Add 1500 entries using bounded method
        for i in 0..1500 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // Should be bounded to 1000
        assert_eq!(state.execution_history.len(), 1000);

        // All entries should be from recent iterations (500-1499)
        for step in &state.execution_history {
            assert!(
                step.iteration >= 500,
                "All entries should be from recent iterations, got {}",
                step.iteration
            );
        }

        // Entries should be contiguous (no gaps)
        for i in 1..state.execution_history.len() {
            let prev_iteration = state.execution_history[i - 1].iteration;
            let curr_iteration = state.execution_history[i].iteration;

            assert_eq!(
                curr_iteration,
                prev_iteration + 1,
                "Entries should be contiguous, found gap between {} and {}",
                prev_iteration,
                curr_iteration
            );
        }
    });
}
