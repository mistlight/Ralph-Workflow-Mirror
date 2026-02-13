//! Memory usage benchmarks
//!
//! These benchmarks measure memory growth patterns during pipeline execution.
//! They establish baseline metrics for:
//! - Execution history growth rate
//! - Pipeline state size over iterations
//! - Memory usage per iteration
//!
//! **These are measurement benchmarks, not pass/fail tests.**
//! Run with `--nocapture` to see output.

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::reducer::state::PipelineState;
use std::time::Instant;

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
fn benchmark_execution_history_growth_10_iterations() {
    let mut state = PipelineState::initial(100, 5);
    let start_size = std::mem::size_of_val(&state.execution_history);

    // Simulate 10 iterations
    for i in 0..10 {
        state.execution_history.push(create_test_step(i));
    }

    let end_size = std::mem::size_of_val(&state.execution_history);
    let actual_heap_size: usize = state
        .execution_history
        .iter()
        .map(|step| {
            // Approximate heap size: string fields + vec allocations
            step.phase.capacity()
                + step.step_type.capacity()
                + step.timestamp.capacity()
                + step.agent.as_ref().map_or(0, |s| s.capacity())
        })
        .sum();

    let growth_per_iteration = if !state.execution_history.is_empty() {
        actual_heap_size / state.execution_history.len()
    } else {
        0
    };

    println!("\n=== Execution History Growth (10 iterations) ===");
    println!("Stack size start: {} bytes", start_size);
    println!("Stack size end: {} bytes", end_size);
    println!("Heap size (estimated): {} bytes", actual_heap_size);
    println!("Growth per iteration: ~{} bytes", growth_per_iteration);
    println!("Total entries: {}", state.execution_history.len());

    // Document baseline - this is NOT a failure, just measurement
    assert_eq!(state.execution_history.len(), 10);
}

#[test]
fn benchmark_execution_history_growth_100_iterations() {
    let mut state = PipelineState::initial(100, 5);
    let start = Instant::now();

    // Simulate 100 iterations
    for i in 0..100 {
        state.execution_history.push(create_test_step(i));
    }

    let duration = start.elapsed();
    let actual_heap_size: usize = state
        .execution_history
        .iter()
        .map(|step| {
            step.phase.capacity()
                + step.step_type.capacity()
                + step.timestamp.capacity()
                + step.agent.as_ref().map_or(0, |s| s.capacity())
        })
        .sum();

    let growth_per_iteration = if !state.execution_history.is_empty() {
        actual_heap_size / state.execution_history.len()
    } else {
        0
    };

    println!("\n=== Execution History Growth (100 iterations) ===");
    println!("Heap size (estimated): {} bytes", actual_heap_size);
    println!("Growth per iteration: ~{} bytes", growth_per_iteration);
    println!("Total entries: {}", state.execution_history.len());
    println!("Time to populate: {:?}", duration);

    // Document baseline
    assert_eq!(state.execution_history.len(), 100);

    // This demonstrates unbounded growth - after step 11 implementation,
    // this should be bounded to configured limit (default 1000)
}

#[test]
fn benchmark_execution_history_growth_1000_iterations() {
    let mut state = PipelineState::initial(1000, 5);
    let start = Instant::now();

    // Simulate 1000 iterations (stress test)
    for i in 0..1000 {
        state.execution_history.push(create_test_step(i));
    }

    let duration = start.elapsed();
    let actual_heap_size: usize = state
        .execution_history
        .iter()
        .map(|step| {
            step.phase.capacity()
                + step.step_type.capacity()
                + step.timestamp.capacity()
                + step.agent.as_ref().map_or(0, |s| s.capacity())
        })
        .sum();

    let growth_per_iteration = if !state.execution_history.is_empty() {
        actual_heap_size / state.execution_history.len()
    } else {
        0
    };
    let size_kb = actual_heap_size / 1024;
    let size_mb = size_kb / 1024;

    println!("\n=== Execution History Growth (1000 iterations) ===");
    println!(
        "Heap size (estimated): {} bytes ({} KB, {} MB)",
        actual_heap_size, size_kb, size_mb
    );
    println!("Growth per iteration: ~{} bytes", growth_per_iteration);
    println!("Total entries: {}", state.execution_history.len());
    println!("Time to populate: {:?}", duration);

    // Document baseline
    assert_eq!(state.execution_history.len(), 1000);

    // This demonstrates the scale of unbounded growth.
    // After step 11 implementation with default limit of 1000,
    // this should show bounded behavior with oldest entries dropped.
}

#[test]
fn benchmark_pipeline_state_size_empty() {
    let state = PipelineState::initial(100, 5);

    // Approximate size calculation
    let base_size = std::mem::size_of_val(&state);
    let execution_history_size = std::mem::size_of_val(&state.execution_history);

    println!("\n=== Pipeline State Size (Empty) ===");
    println!("Base state size: {} bytes", base_size);
    println!(
        "Execution history Vec size: {} bytes",
        execution_history_size
    );
    println!(
        "Total execution history entries: {}",
        state.execution_history.len()
    );

    // Baseline measurement
    assert!(state.execution_history.is_empty());
}

#[test]
fn benchmark_pipeline_state_size_with_100_steps() {
    let mut state = PipelineState::initial(100, 5);

    // Add 100 execution steps
    for i in 0..100 {
        state.execution_history.push(create_test_step(i));
    }

    let base_size = std::mem::size_of_val(&state);
    let execution_history_size = std::mem::size_of_val(&state.execution_history);
    let heap_size: usize = state
        .execution_history
        .iter()
        .map(|step| {
            step.phase.capacity()
                + step.step_type.capacity()
                + step.timestamp.capacity()
                + step.agent.as_ref().map_or(0, |s| s.capacity())
        })
        .sum();

    println!("\n=== Pipeline State Size (100 steps) ===");
    println!("Base state size: {} bytes", base_size);
    println!(
        "Execution history Vec size: {} bytes",
        execution_history_size
    );
    println!("Execution history heap size: ~{} bytes", heap_size);
    println!("Total size estimate: ~{} bytes", base_size + heap_size);
    println!(
        "Total execution history entries: {}",
        state.execution_history.len()
    );

    // Baseline measurement
    assert_eq!(state.execution_history.len(), 100);
}

#[test]
fn benchmark_memory_growth_rate() {
    let mut state = PipelineState::initial(1000, 5);
    let mut sizes = Vec::new();

    // Measure growth at intervals
    for i in 0..1000 {
        state.execution_history.push(create_test_step(i));

        // Sample every 100 iterations
        if (i + 1) % 100 == 0 {
            let heap_size: usize = state
                .execution_history
                .iter()
                .map(|step| {
                    step.phase.capacity()
                        + step.step_type.capacity()
                        + step.timestamp.capacity()
                        + step.agent.as_ref().map_or(0, |s| s.capacity())
                })
                .sum();
            sizes.push((i + 1, heap_size));
        }
    }

    println!("\n=== Memory Growth Rate ===");
    println!("Iterations | Heap Size (KB) | Growth from prev");
    println!("-----------|----------------|------------------");

    for (idx, (iter, size)) in sizes.iter().enumerate() {
        let size_kb = size / 1024;
        let growth = if idx > 0 {
            let prev_size = sizes[idx - 1].1;
            let growth_kb = (size - prev_size) / 1024;
            format!("+{} KB", growth_kb)
        } else {
            String::from("baseline")
        };
        println!("{:10} | {:14} | {}", iter, size_kb, growth);
    }

    // Verify we have the expected samples
    assert_eq!(sizes.len(), 10); // 1000 / 100 = 10 samples

    // This demonstrates linear growth with unbounded Vec
    // After step 11 implementation, growth should plateau at the limit
}
