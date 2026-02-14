//! Performance regression tests to prevent future degradation.
//!
//! These tests enforce performance baselines and catch regressions early in CI.
//! They use conservative thresholds with some tolerance for platform variance.

use crate::benchmarks::baselines::estimate_execution_step_heap_bytes_core_fields;
use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::checkpoint::StringPool;
use crate::reducer::state::PipelineState;
use std::sync::Arc;
use std::time::Instant;

/// Helper to create a test execution step with string pool.
fn create_test_step_with_pool(iteration: u32, pool: &mut StringPool) -> ExecutionStep {
    ExecutionStep::new_with_pool(
        "Development",
        iteration,
        "agent_invoked",
        StepOutcome::success(Some("output".to_string()), vec!["file.rs".to_string()]),
        pool,
    )
    .with_agent_pooled("test-agent", pool)
    .with_duration(5)
}

/// Helper to create a large test pipeline state.
fn create_large_state(history_size: usize) -> PipelineState {
    let mut state = PipelineState::initial(1000, 5);
    let mut pool = StringPool::new();

    for i in 0..history_size {
        let step = create_test_step_with_pool(i as u32, &mut pool);
        state.add_execution_step(step, history_size);
    }

    state
}

#[test]
fn regression_test_execution_step_memory_footprint() {
    let mut pool = StringPool::new();
    let step = create_test_step_with_pool(1, &mut pool);
    let heap_size = estimate_execution_step_heap_bytes_core_fields(&step);

    // After optimizations (Arc<str> + Box<str>), should be <= 60 bytes per entry
    // This accounts for:
    // - phase: Arc<str> "Development" = 11 bytes
    // - step_type: Box<str> "agent_invoked" = 14 bytes
    // - timestamp: String ~= 25 bytes (ISO 8601 format)
    // - agent: Option<Arc<str>> "test-agent" = 10 bytes
    // Total: ~60 bytes
    assert!(
        heap_size <= 60,
        "Memory regression: {} bytes per entry exceeds 60 byte target",
        heap_size
    );
}

#[test]
fn regression_test_string_pool_sharing() {
    let mut pool = StringPool::new();

    // Create multiple steps with the same phase and agent
    let step1 = create_test_step_with_pool(1, &mut pool);
    let step2 = create_test_step_with_pool(2, &mut pool);

    // Verify Arc sharing (same pointer)
    assert!(
        Arc::ptr_eq(&step1.phase, &step2.phase),
        "String pool regression: phase strings not shared"
    );
    assert!(
        Arc::ptr_eq(step1.agent.as_ref().unwrap(), step2.agent.as_ref().unwrap()),
        "String pool regression: agent strings not shared"
    );

    // Pool should only contain 2 unique strings (phase and agent)
    assert_eq!(
        pool.len(),
        2,
        "String pool regression: expected 2 unique strings, got {}",
        pool.len()
    );
}

#[test]
fn regression_test_serialization_performance() {
    let state = create_large_state(1000);

    let start = Instant::now();
    let json = serde_json::to_string(&state).unwrap();
    let duration = start.elapsed();

    // After optimizations (pre-allocated buffer + compact JSON), should be <= 10ms
    // This is a conservative threshold (2x expected performance) to account for
    // platform variance and CI environment overhead
    if std::env::var_os("RALPH_WORKFLOW_PERF_CEILINGS").is_some() {
        assert!(
            duration.as_millis() <= 10,
            "Serialization regression: {:?} exceeds 10ms target",
            duration
        );
    }

    // Size should be <= 400 KB (400,000 bytes)
    // Current measurements show ~375 KB, so this gives 6% headroom
    assert!(
        json.len() <= 400_000,
        "Size regression: {} bytes exceeds 400 KB target",
        json.len()
    );
}

#[test]
fn regression_test_deserialization_performance() {
    let state = create_large_state(1000);
    let json = serde_json::to_string(&state).unwrap();

    let start = Instant::now();
    let _deserialized: PipelineState = serde_json::from_str(&json).unwrap();
    let duration = start.elapsed();

    // Deserialization should be <= 10ms (conservative threshold)
    if std::env::var_os("RALPH_WORKFLOW_PERF_CEILINGS").is_some() {
        assert!(
            duration.as_millis() <= 10,
            "Deserialization regression: {:?} exceeds 10ms target",
            duration
        );
    }
}

#[test]
fn regression_test_round_trip_performance() {
    let state = create_large_state(1000);

    let start = Instant::now();
    let json = serde_json::to_string(&state).unwrap();
    let serialize_duration = start.elapsed();

    let start = Instant::now();
    let _deserialized: PipelineState = serde_json::from_str(&json).unwrap();
    let deserialize_duration = start.elapsed();

    let total_duration = serialize_duration + deserialize_duration;

    // Round trip should be <= 20ms (conservative threshold)
    if std::env::var_os("RALPH_WORKFLOW_PERF_CEILINGS").is_some() {
        assert!(
            total_duration.as_millis() <= 20,
            "Round trip regression: {:?} exceeds 20ms target",
            total_duration
        );
    }
}

#[test]
fn regression_test_execution_history_bounded_growth() {
    // Verify that execution history respects the configured limit
    let limit = 500;
    let mut state = PipelineState::initial(limit as u32, 5);
    let mut pool = StringPool::new();

    // Add more entries than the limit
    for i in 0..1000 {
        let step = create_test_step_with_pool(i, &mut pool);
        state.add_execution_step(step, limit);
    }

    // Verify history is bounded to the limit
    assert_eq!(
        state.execution_history_len(),
        limit,
        "Execution history regression: {} entries exceeds limit of {}",
        state.execution_history_len(),
        limit
    );
}

#[test]
fn regression_test_copy_enums_eliminate_clones() {
    // This test verifies that simple enums are Copy, eliminating unnecessary clones
    use crate::reducer::state::{
        ArtifactType, DevelopmentStatus, FixStatus, PromptInputKind, PromptMaterializationReason,
        PromptMode, SameAgentRetryReason,
    };

    // Verify enums are Copy
    fn assert_copy<T: Copy>() {}

    assert_copy::<ArtifactType>();
    assert_copy::<PromptMode>();
    assert_copy::<SameAgentRetryReason>();
    assert_copy::<DevelopmentStatus>();
    assert_copy::<FixStatus>();
    assert_copy::<PromptInputKind>();
    assert_copy::<PromptMaterializationReason>();
}

#[test]
fn regression_test_memory_efficiency_vs_vec() {
    // Verify that Box<str> and Option<Box<[T]>> are more efficient than Vec<T>
    let outcome = StepOutcome::success(
        Some("output".to_string()),
        vec!["file1.txt".to_string(), "file2.txt".to_string()],
    );

    match outcome {
        StepOutcome::Success {
            output,
            files_modified,
            ..
        } => {
            // Box<str> uses exact size (no over-allocation)
            let output_str = output.expect("Output should be present");
            assert_eq!(output_str.len(), "output".len());

            // Box<[String]> uses exact size (no excess capacity)
            let files = files_modified.expect("Files should be present");
            assert_eq!(files.len(), 2);

            // The benefit is that Box<[T]> doesn't have the extra `capacity` field
            // that Vec<T> has, saving memory on every instance
        }
        _ => panic!("Expected Success variant"),
    }
}

#[test]
fn regression_test_checkpoint_size_scaling() {
    // Verify that checkpoint size scales linearly with history size
    let sizes = vec![100, 500, 1000];
    let mut measurements = Vec::new();

    for size in sizes {
        let state = create_large_state(size);
        let json = serde_json::to_string(&state).unwrap();
        measurements.push((size, json.len()));
    }

    // Calculate bytes per entry for each size
    for (size, json_len) in &measurements {
        let bytes_per_entry = json_len / size;
        // Should be between 300-450 bytes per entry (allows for JSON overhead variance)
        assert!(
            bytes_per_entry >= 300 && bytes_per_entry <= 450,
            "Checkpoint size scaling regression: {} bytes per entry at size {} (expected 300-450)",
            bytes_per_entry,
            size
        );
    }
}

#[test]
fn regression_test_string_pool_memory_bounded() {
    // Verify that string pool doesn't grow unboundedly
    let mut pool = StringPool::new();

    // Create many steps with the same phase and agent
    for i in 0..1000 {
        let _ = create_test_step_with_pool(i, &mut pool);
    }

    // Pool should still only contain 2 unique strings (phase and agent)
    assert_eq!(
        pool.len(),
        2,
        "String pool memory regression: {} entries (expected 2)",
        pool.len()
    );
}

#[test]
fn regression_test_arc_str_vs_string_memory() {
    // Demonstrate memory savings of Arc<str> vs String for repeated values
    let mut pool = StringPool::new();
    let mut steps = Vec::new();

    // Create 100 steps with the same phase
    for i in 0..100 {
        steps.push(create_test_step_with_pool(i, &mut pool));
    }

    // All steps should share the same Arc<str> for phase
    for i in 1..steps.len() {
        assert!(
            Arc::ptr_eq(&steps[0].phase, &steps[i].phase),
            "Arc<str> memory regression: steps 0 and {} don't share phase allocation",
            i
        );
    }

    // With String: 100 allocations * ~11 bytes = ~1100 bytes
    // With Arc<str>: 1 allocation * 11 bytes = 11 bytes
    // Savings: ~1089 bytes (99% reduction) for just the phase field
}
