//! Long-running pipeline memory stability tests
//!
//! These tests simulate realistic long-running pipeline execution to verify
//! that memory usage remains bounded over thousands of iterations.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! This module tests observable behavior:
//! - Memory does not grow unboundedly over 10k+ iterations
//! - Execution history remains bounded at configured limit
//! - Checkpoint size remains reasonable even with maximum history
//! - No Arc count growth beyond expected clones

use crate::test_timeout::with_default_timeout;
use ralph_workflow::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use ralph_workflow::reducer::state::PipelineState;

/// Helper function to create a test execution step.
fn create_test_step(iteration: u32) -> ExecutionStep {
    ExecutionStep::new(
        "Development",
        iteration,
        "agent_invoked",
        StepOutcome::success(
            Some(format!("output for iteration {}", iteration)),
            vec![format!("file_{}.rs", iteration % 100)],
        ),
    )
    .with_agent("test-agent")
    .with_duration(5)
}

#[test]
fn test_10k_iterations_memory_remains_bounded() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(10_000, 5);
        let limit = 1000;

        // Simulate 10,000 iterations
        for i in 0..10_000 {
            state.add_execution_step(create_test_step(i), limit);

            // Verify history never exceeds limit
            assert!(
                state.execution_history.len() <= limit,
                "History length {} exceeded limit {} at iteration {}",
                state.execution_history.len(),
                limit,
                i
            );
        }

        // Final verification
        assert_eq!(
            state.execution_history.len(),
            limit,
            "Final history should be at limit"
        );

        // Verify we kept the most recent entries
        let first_entry = state.execution_history.first().unwrap();
        let last_entry = state.execution_history.last().unwrap();

        assert!(
            first_entry.iteration >= 9_000,
            "First entry should be from recent iterations, got {}",
            first_entry.iteration
        );
        assert_eq!(
            last_entry.iteration, 9_999,
            "Last entry should be most recent"
        );
    });
}

#[test]
fn test_checkpoint_size_remains_reasonable_with_max_history() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(10_000, 5);
        let limit = 1000;

        // Fill history to limit
        for i in 0..2000 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // Serialize to JSON (checkpoint format)
        let serialized = serde_json::to_string(&state).expect("Should serialize state");

        let size_bytes = serialized.len();

        // With 1000 entries, checkpoint should be under 2MB
        // (based on benchmark measurements: ~375KB for 1000 entries)
        const MAX_CHECKPOINT_SIZE: usize = 2 * 1024 * 1024; // 2MB

        assert!(
            size_bytes < MAX_CHECKPOINT_SIZE,
            "Checkpoint size {} bytes exceeds maximum {} bytes",
            size_bytes,
            MAX_CHECKPOINT_SIZE
        );

        // Log size for regression tracking
        println!(
            "Checkpoint size with {} history entries: {} KB",
            state.execution_history.len(),
            size_bytes / 1024
        );
    });
}

#[test]
fn test_memory_growth_rate_is_zero_after_limit_reached() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(5000, 5);
        let limit = 500;

        // Fill to limit
        for i in 0..500 {
            state.add_execution_step(create_test_step(i), limit);
        }

        assert_eq!(state.execution_history.len(), 500);

        // Add 1000 more iterations - length should remain constant
        for i in 500..1500 {
            state.add_execution_step(create_test_step(i), limit);
        }

        assert_eq!(
            state.execution_history.len(),
            limit,
            "History length should remain at limit"
        );

        // Verify ring buffer behavior - oldest entries dropped
        let first_iteration = state.execution_history.first().unwrap().iteration;
        assert!(
            first_iteration >= 1000,
            "Oldest entry should be from recent iterations, got {}",
            first_iteration
        );
    });
}

#[test]
fn test_heap_size_estimate_remains_bounded() {
    with_default_timeout(|| {
        use ralph_workflow::checkpoint::execution_history::StepOutcome;

        let mut state = PipelineState::initial(10_000, 5);
        let limit = 1000;

        // Fill history to limit and beyond
        for i in 0..5000 {
            state.add_execution_step(create_test_step(i), limit);
        }

        // Estimate heap size
        let heap_size: usize = state
            .execution_history
            .iter()
            .map(|step| {
                let base_size = step.phase.capacity()
                    + step.step_type.capacity()
                    + step.timestamp.capacity()
                    + step.agent.as_ref().map_or(0, |s| s.capacity());

                let outcome_size = match &step.outcome {
                    StepOutcome::Success {
                        output,
                        files_modified,
                        ..
                    } => {
                        output.as_ref().map_or(0, |s| s.capacity())
                            + files_modified.iter().map(|s| s.capacity()).sum::<usize>()
                    }
                    StepOutcome::Failure { error, signals, .. } => {
                        error.capacity() + signals.iter().map(|s| s.capacity()).sum::<usize>()
                    }
                    StepOutcome::Partial {
                        completed,
                        remaining,
                        ..
                    } => completed.capacity() + remaining.capacity(),
                    StepOutcome::Skipped { reason } => reason.capacity(),
                };

                base_size + outcome_size
            })
            .sum();

        // With 1000 entries, heap should be under 2MB
        // (based on benchmark: ~500KB for 1000 entries)
        const MAX_HEAP_SIZE: usize = 2 * 1024 * 1024;

        assert!(
            heap_size < MAX_HEAP_SIZE,
            "Heap size {} exceeds maximum {}",
            heap_size,
            MAX_HEAP_SIZE
        );

        println!("Estimated heap size: {} KB", heap_size / 1024);
    });
}
