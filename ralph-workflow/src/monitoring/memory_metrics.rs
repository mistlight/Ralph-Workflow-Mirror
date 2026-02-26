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
    /// Deterministic size proxy for execution history (bytes).
    ///
    /// This is not a true allocator-backed heap measurement. It uses string lengths as
    /// a stable, platform-independent proxy suitable for regression tracking.
    pub execution_history_heap_bytes: usize,
    /// Checkpoint saved count
    pub checkpoint_count: u32,
    /// Timestamp when snapshot was taken (ISO 8601)
    pub timestamp: String,
}

impl MemorySnapshot {
    /// Create a snapshot from current pipeline state.
    #[must_use]
    pub fn from_pipeline_state(state: &crate::reducer::PipelineState) -> Self {
        let execution_history_heap_bytes = estimate_execution_history_heap_size(state);

        Self {
            iteration: state.iteration,
            execution_history_len: state.execution_history_len(),
            execution_history_heap_bytes,
            checkpoint_count: state.checkpoint_saved_count,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Estimate a deterministic "heap bytes" proxy for execution history.
///
/// Uses string lengths (and collection element lengths) to produce a stable number that
/// tracks payload growth without depending on allocator behavior.
fn estimate_execution_history_heap_size(state: &crate::reducer::PipelineState) -> usize {
    use crate::checkpoint::execution_history::StepOutcome;

    state
        .execution_history()
        .iter()
        .map(|step| {
            let modified_files_detail_size = step.modified_files_detail.as_ref().map_or(0, |d| {
                let sum_list = |xs: &Option<Box<[String]>>| {
                    xs.as_ref()
                        .map_or(0, |v| v.iter().map(std::string::String::len).sum::<usize>())
                };

                sum_list(&d.added) + sum_list(&d.modified) + sum_list(&d.deleted)
            });

            let issues_summary_size = step
                .issues_summary
                .as_ref()
                .and_then(|s| s.description.as_ref())
                .map_or(0, std::string::String::len);

            // Approximate heap allocations: string fields + vec allocations
            // Use `len()` consistently as a deterministic size proxy.
            let base_size = step.phase.len()
                + step.step_type.len()
                + step.timestamp.len()
                + step.agent.as_ref().map_or(0, |s| s.len())
                + step
                    .checkpoint_saved_at
                    .as_ref()
                    .map_or(0, std::string::String::len)
                + step
                    .git_commit_oid
                    .as_ref()
                    .map_or(0, std::string::String::len)
                + step
                    .prompt_used
                    .as_ref()
                    .map_or(0, std::string::String::len)
                + modified_files_detail_size
                + issues_summary_size;

            let outcome_size = match &step.outcome {
                StepOutcome::Success {
                    output,
                    files_modified,
                    ..
                } => {
                    output.as_ref().map_or(0, |s| s.len())
                        + files_modified.as_ref().map_or(0, |files| {
                            files.iter().map(std::string::String::len).sum::<usize>()
                        })
                }
                StepOutcome::Failure { error, signals, .. } => {
                    error.len()
                        + signals.as_ref().map_or(0, |sigs| {
                            sigs.iter().map(std::string::String::len).sum::<usize>()
                        })
                }
                StepOutcome::Partial {
                    completed,
                    remaining,
                    ..
                } => completed.len() + remaining.len(),
                StepOutcome::Skipped { reason } => reason.len(),
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
    #[must_use]
    pub const fn new(snapshot_interval: u32) -> Self {
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
    #[must_use]
    pub fn snapshots(&self) -> &[MemorySnapshot] {
        &self.snapshots
    }

    /// Export snapshots as JSON for external analysis.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
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
/// (Prometheus, `DataDog`, `CloudWatch`, etc.)
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
    use crate::checkpoint::execution_history::{
        ExecutionStep, IssuesSummary, ModifiedFilesDetail, StepOutcome,
    };
    use crate::logger::output::TestLogger;
    use crate::reducer::PipelineState;

    #[test]
    fn test_execution_history_heap_estimate_uses_len_not_capacity() {
        let mut state = PipelineState::initial(100, 5);

        let mut timestamp = String::with_capacity(2048);
        timestamp.push('t');
        let mut file = String::with_capacity(4096);
        file.push('f');

        let mut checkpoint_saved_at = String::with_capacity(2048);
        checkpoint_saved_at.push('c');
        let mut git_commit_oid = String::with_capacity(2048);
        git_commit_oid.push('g');
        let mut prompt_used = String::with_capacity(2048);
        prompt_used.push('p');
        let mut issues_desc = String::with_capacity(2048);
        issues_desc.push('i');

        let mut added = String::with_capacity(2048);
        added.push('a');
        let mut modified = String::with_capacity(2048);
        modified.push('m');
        let mut deleted = String::with_capacity(2048);
        deleted.push('d');

        let step = ExecutionStep {
            phase: std::sync::Arc::from("P"),
            iteration: 0,
            step_type: Box::from("T"),
            timestamp,
            outcome: StepOutcome::Success {
                output: None,
                files_modified: Some(vec![file].into_boxed_slice()),
                exit_code: Some(0),
            },
            agent: Some(std::sync::Arc::from("A")),
            duration_secs: None,
            checkpoint_saved_at: Some(checkpoint_saved_at),
            git_commit_oid: Some(git_commit_oid),
            modified_files_detail: Some(ModifiedFilesDetail {
                added: Some(vec![added].into_boxed_slice()),
                modified: Some(vec![modified].into_boxed_slice()),
                deleted: Some(vec![deleted].into_boxed_slice()),
            }),
            prompt_used: Some(prompt_used),
            issues_summary: Some(IssuesSummary {
                found: 0,
                fixed: 0,
                description: Some(issues_desc),
            }),
        };

        state.add_execution_step(step, 1000);

        let bytes = super::estimate_execution_history_heap_size(&state);
        let expected = "P".len()
            + "T".len()
            + "t".len()
            + "A".len()
            + "f".len()
            + "c".len()
            + "g".len()
            + "p".len()
            + "i".len()
            + "a".len()
            + "m".len()
            + "d".len();

        assert_eq!(
            bytes, expected,
            "heap estimate should be a deterministic length-based proxy"
        );
    }

    #[test]
    fn test_memory_snapshot_captures_state() {
        let mut state = PipelineState::initial(100, 5);
        state.add_execution_step(
            ExecutionStep::new(
                "Development",
                0,
                "agent_invoked",
                StepOutcome::success(Some("output".to_string()), vec!["file.rs".to_string()]),
            ),
            1000,
        );

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
            state.add_execution_step(
                ExecutionStep::new(
                    "Development",
                    i,
                    "agent_invoked",
                    StepOutcome::success(
                        Some("output with sufficient content".to_string()),
                        vec!["file.rs".to_string()],
                    ),
                ),
                1000,
            );
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
