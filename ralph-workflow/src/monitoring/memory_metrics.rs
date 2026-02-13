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
use std::rc::Rc;

const DEFAULT_MAX_SNAPSHOTS: usize = 1024;

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

    fn enforce_snapshot_limit(&mut self) {
        if self.snapshots.len() > DEFAULT_MAX_SNAPSHOTS {
            let excess = self.snapshots.len() - DEFAULT_MAX_SNAPSHOTS;
            self.snapshots.drain(0..excess);
        }
    }

    /// Record a snapshot if at snapshot interval.
    pub fn maybe_record(&mut self, state: &crate::reducer::PipelineState) {
        if self.snapshot_interval == 0 {
            return;
        }

        // Treat iteration 0 as "pre-run" (initial state). Recording here is surprising
        // and skews exported metrics since 0 is a multiple of any non-zero interval.
        if state.iteration == 0 {
            return;
        }

        if state.iteration == 1 || state.iteration.is_multiple_of(self.snapshot_interval) {
            self.snapshots
                .push(MemorySnapshot::from_pipeline_state(state));
            self.enforce_snapshot_limit();
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

    /// Record a snapshot and send to telemetry backend.
    pub fn record_and_emit(
        &mut self,
        state: &crate::reducer::PipelineState,
        backend: &mut dyn TelemetryBackend,
    ) {
        if self.snapshot_interval == 0 {
            return;
        }

        if state.iteration == 0 {
            return;
        }

        if state.iteration == 1 || state.iteration.is_multiple_of(self.snapshot_interval) {
            let snapshot = MemorySnapshot::from_pipeline_state(state);
            backend.emit_snapshot(&snapshot);
            self.snapshots.push(snapshot);
            self.enforce_snapshot_limit();
        }
    }
}

/// Pluggable backend for telemetry integration.
///
/// Implement this trait to integrate with external monitoring systems
/// (Prometheus, DataDog, CloudWatch, etc.)
pub trait TelemetryBackend {
    /// Emit a memory snapshot to the telemetry system.
    fn emit_snapshot(&mut self, snapshot: &MemorySnapshot);

    /// Emit a warning when memory usage approaches threshold.
    fn emit_warning(&mut self, message: &str);

    /// Flush any buffered metrics.
    fn flush(&mut self);
}

/// No-op telemetry backend for testing.
#[derive(Debug, Default)]
pub struct NoOpBackend;

impl TelemetryBackend for NoOpBackend {
    fn emit_snapshot(&mut self, _snapshot: &MemorySnapshot) {}
    fn emit_warning(&mut self, _message: &str) {}
    fn flush(&mut self) {}
}

/// Logging-based telemetry backend.
///
/// Routes metrics through the project's logger implementation.
pub struct LoggingBackend {
    warn_threshold_bytes: usize,
    logger: Rc<dyn crate::logger::Loggable>,
}

impl LoggingBackend {
    /// Create a new logging backend with warning threshold.
    pub fn new(warn_threshold_bytes: usize) -> Self {
        Self {
            warn_threshold_bytes,
            logger: Rc::new(crate::logger::Logger::default()),
        }
    }

    /// Create a logging backend that writes via the provided logger.
    pub fn with_logger(
        warn_threshold_bytes: usize,
        logger: Rc<dyn crate::logger::Loggable>,
    ) -> Self {
        Self {
            warn_threshold_bytes,
            logger,
        }
    }
}

impl TelemetryBackend for LoggingBackend {
    fn emit_snapshot(&mut self, snapshot: &MemorySnapshot) {
        self.logger.info(&format!(
            "[METRICS] iteration={} history_len={} heap_bytes={} checkpoint_count={}",
            snapshot.iteration,
            snapshot.execution_history_len,
            snapshot.execution_history_heap_bytes,
            snapshot.checkpoint_count
        ));

        if snapshot.execution_history_heap_bytes > self.warn_threshold_bytes {
            self.emit_warning(&format!(
                "Execution history heap size {} bytes exceeds warning threshold {} bytes",
                snapshot.execution_history_heap_bytes, self.warn_threshold_bytes
            ));
        }
    }

    fn emit_warning(&mut self, message: &str) {
        self.logger.warn(&format!("[METRICS WARNING] {message}"));
    }

    fn flush(&mut self) {
        // Logging backend doesn't buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
    use crate::logger::output::TestLogger;
    use crate::reducer::PipelineState;

    #[test]
    fn test_memory_snapshot_captures_state() {
        let mut state = PipelineState::initial(100, 5);
        state.execution_history.push_back(ExecutionStep::new(
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

        // Should not record at iteration 0 (initial state)
        state.iteration = 0;
        collector.maybe_record(&state);
        assert_eq!(collector.snapshots().len(), 0);

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

    #[test]
    fn test_metrics_collector_retains_bounded_snapshots_by_default() {
        let mut collector = MemoryMetricsCollector::new(1);
        let mut state = PipelineState::initial(100, 5);

        for i in 1..=2000 {
            state.iteration = i;
            collector.maybe_record(&state);
        }

        let snapshots = collector.snapshots();
        assert!(
            snapshots.len() <= 1024,
            "expected default snapshot retention to be bounded"
        );
        assert_eq!(
            snapshots
                .last()
                .expect("should record at least one snapshot")
                .iteration,
            2000
        );
        assert_eq!(
            snapshots
                .first()
                .expect("should record at least one snapshot")
                .iteration,
            2000 - snapshots.len() as u32 + 1
        );
    }

    #[test]
    fn test_telemetry_backend_noop() {
        let mut backend = NoOpBackend;
        let state = PipelineState::initial(100, 5);
        let snapshot = MemorySnapshot::from_pipeline_state(&state);

        // Should not panic
        backend.emit_snapshot(&snapshot);
        backend.emit_warning("test warning");
        backend.flush();
    }

    #[test]
    fn test_record_and_emit_integrates_with_backend() {
        struct CountingBackend {
            snapshot_count: usize,
        }

        impl TelemetryBackend for CountingBackend {
            fn emit_snapshot(&mut self, _snapshot: &MemorySnapshot) {
                self.snapshot_count += 1;
            }
            fn emit_warning(&mut self, _message: &str) {}
            fn flush(&mut self) {}
        }

        let mut collector = MemoryMetricsCollector::new(10);
        let mut backend = CountingBackend { snapshot_count: 0 };
        let mut state = PipelineState::initial(100, 5);

        // Should not emit at iteration 0 (initial state)
        state.iteration = 0;
        collector.record_and_emit(&state, &mut backend);
        assert_eq!(backend.snapshot_count, 0);
        assert_eq!(collector.snapshots().len(), 0);

        // Should emit at iteration 1
        state.iteration = 1;
        collector.record_and_emit(&state, &mut backend);
        assert_eq!(backend.snapshot_count, 1);
        assert_eq!(collector.snapshots().len(), 1);

        // Should not emit at iteration 5
        state.iteration = 5;
        collector.record_and_emit(&state, &mut backend);
        assert_eq!(backend.snapshot_count, 1);

        // Should emit at iteration 10
        state.iteration = 10;
        collector.record_and_emit(&state, &mut backend);
        assert_eq!(backend.snapshot_count, 2);
        assert_eq!(collector.snapshots().len(), 2);
    }

    #[test]
    fn test_logging_backend_emits_warnings_above_threshold() {
        let logger = Rc::new(TestLogger::new());
        let mut backend = LoggingBackend::with_logger(100, logger.clone()); // 100 byte threshold
        let mut state = PipelineState::initial(100, 5);

        // Add enough history to exceed threshold
        for i in 0..50 {
            state.execution_history.push_back(ExecutionStep::new(
                "Development",
                i,
                "agent_invoked",
                StepOutcome::success(
                    Some("output with sufficient content".to_string()),
                    vec!["file.rs".to_string()],
                ),
            ));
        }

        let snapshot = MemorySnapshot::from_pipeline_state(&state);
        assert!(
            snapshot.execution_history_heap_bytes > 100,
            "Test setup should create heap usage > 100 bytes"
        );

        // This should emit both snapshot and warning
        backend.emit_snapshot(&snapshot);
        let logs = logger.get_logs();
        assert!(logs.iter().any(|l| l.contains("[METRICS]")));
        assert!(logs.iter().any(|l| l.contains("[METRICS WARNING]")));
    }

    #[test]
    fn test_memory_metrics_library_code_does_not_write_directly_to_stderr() {
        // Writing to stderr from library code bypasses the project's logger and
        // can spam output in production. Logging should route through Loggable.
        let src = include_str!("memory_metrics.rs");
        assert!(
            !src.contains("eprintln!(\"[METRICS]")
                && !src.contains("eprintln!(\"[METRICS WARNING]"),
            "memory_metrics.rs should not use eprintln! in library code"
        );
    }
}
