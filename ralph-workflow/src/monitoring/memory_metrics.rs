//! Production memory profiling and metrics.
//!
//! This module provides lightweight memory usage tracking for production
//! deployments. It enables detection of memory issues without requiring
//! external profiling tools.
//!
//! # Feature Flag
//!
//! This module is only available when the `monitoring` feature is enabled.

use serde::{Deserialize, Serialize};

/// Memory usage snapshot at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Pipeline iteration when snapshot was taken
    pub iteration: u32,
    /// Execution history length
    pub execution_history_len: usize,
    /// Estimated execution history heap size (bytes)
    pub execution_history_heap_bytes: usize,
    /// Checkpoint saved count
    pub checkpoint_count: u32,
    /// Timestamp when snapshot was taken (ISO 8601)
    pub timestamp: String,
}

impl MemorySnapshot {
    /// Create a snapshot from current pipeline state.
    pub fn from_pipeline_state(state: &crate::reducer::PipelineState) -> Self {
        let execution_history_heap_bytes = estimate_execution_history_heap_size(state);

        Self {
            iteration: state.iteration,
            execution_history_len: state.execution_history.len(),
            execution_history_heap_bytes,
            checkpoint_count: state.checkpoint_saved_count,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Estimate heap size of execution history in bytes.
fn estimate_execution_history_heap_size(state: &crate::reducer::PipelineState) -> usize {
    use crate::checkpoint::execution_history::StepOutcome;

    state
        .execution_history
        .iter()
        .map(|step| {
            // Approximate heap allocations: string fields + vec allocations
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
        .sum()
}

/// Memory metrics collector for pipeline execution.
#[derive(Debug)]
pub struct MemoryMetricsCollector {
    snapshots: Vec<MemorySnapshot>,
    snapshot_interval: u32,
}

impl MemoryMetricsCollector {
    /// Create a new metrics collector.
    ///
    /// # Arguments
    ///
    /// * `snapshot_interval` - Take snapshot every N iterations (0 = disabled)
    pub fn new(snapshot_interval: u32) -> Self {
        Self {
            snapshots: Vec::new(),
            snapshot_interval,
        }
    }

    /// Record a snapshot if at snapshot interval.
    pub fn maybe_record(&mut self, state: &crate::reducer::PipelineState) {
        if self.snapshot_interval == 0 {
            return;
        }

        if state.iteration.is_multiple_of(self.snapshot_interval) || state.iteration == 1 {
            self.snapshots
                .push(MemorySnapshot::from_pipeline_state(state));
        }
    }

    /// Get all recorded snapshots.
    pub fn snapshots(&self) -> &[MemorySnapshot] {
        &self.snapshots
    }

    /// Export snapshots as JSON for external analysis.
    pub fn export_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.snapshots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
    use crate::reducer::PipelineState;

    #[test]
    fn test_memory_snapshot_captures_state() {
        let mut state = PipelineState::initial(100, 5);
        state.execution_history.push(ExecutionStep::new(
            "Development",
            0,
            "agent_invoked",
            StepOutcome::success(Some("output".to_string()), vec!["file.rs".to_string()]),
        ));

        let snapshot = MemorySnapshot::from_pipeline_state(&state);

        assert_eq!(snapshot.iteration, 0);
        assert_eq!(snapshot.execution_history_len, 1);
        assert!(snapshot.execution_history_heap_bytes > 0);
    }

    #[test]
    fn test_metrics_collector_respects_interval() {
        let mut collector = MemoryMetricsCollector::new(10);
        let mut state = PipelineState::initial(100, 5);

        // Should record at iteration 1
        state.iteration = 1;
        collector.maybe_record(&state);
        assert_eq!(collector.snapshots().len(), 1);

        // Should not record at iteration 5
        state.iteration = 5;
        collector.maybe_record(&state);
        assert_eq!(collector.snapshots().len(), 1);

        // Should record at iteration 10
        state.iteration = 10;
        collector.maybe_record(&state);
        assert_eq!(collector.snapshots().len(), 2);
    }
}
