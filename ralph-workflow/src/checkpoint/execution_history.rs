//! Execution history tracking for checkpoint state.
//!
//! This module provides structures for tracking the execution history of a pipeline,
//! enabling idempotent recovery and validation of state.

use crate::checkpoint::timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Outcome of an execution step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepOutcome {
    /// Step completed successfully
    Success {
        /// Optional output data (for small outputs)
        output: Option<String>,
        /// Files modified by this step
        files_modified: Vec<String>,
    },
    /// Step failed with error
    Failure {
        /// Error message
        error: String,
        /// Whether this is recoverable
        recoverable: bool,
    },
    /// Step partially completed (may need retry)
    Partial {
        /// What was completed
        completed: String,
        /// What remains
        remaining: String,
    },
    /// Step was skipped (e.g., already done)
    Skipped {
        /// Reason for skipping
        reason: String,
    },
}

/// A single execution step in the pipeline history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    /// Phase this step belongs to
    pub phase: String,
    /// Iteration number (for development/review iterations)
    pub iteration: u32,
    /// Type of step (e.g., "review", "fix", "commit")
    pub step_type: String,
    /// When this step was executed (ISO 8601 format string)
    pub timestamp: String,
    /// Outcome of the step
    pub outcome: StepOutcome,
    /// Agent that executed this step
    pub agent: Option<String>,
    /// Duration in seconds (if available)
    pub duration_secs: Option<u64>,
}

impl ExecutionStep {
    /// Create a new execution step.
    pub fn new(phase: &str, iteration: u32, step_type: &str, outcome: StepOutcome) -> Self {
        Self {
            phase: phase.to_string(),
            iteration,
            step_type: step_type.to_string(),
            timestamp: timestamp(),
            outcome,
            agent: None,
            duration_secs: None,
        }
    }

    /// Set the agent that executed this step.
    pub fn with_agent(mut self, agent: &str) -> Self {
        self.agent = Some(agent.to_string());
        self
    }

    /// Set the duration of this step.
    pub fn with_duration(mut self, duration_secs: u64) -> Self {
        self.duration_secs = Some(duration_secs);
        self
    }

    /// Create a successful step outcome.
    pub fn success(files_modified: Vec<String>) -> StepOutcome {
        StepOutcome::Success {
            output: None,
            files_modified,
        }
    }

    /// Create a successful step outcome with output.
    pub fn success_with_output(output: String, files_modified: Vec<String>) -> StepOutcome {
        StepOutcome::Success {
            output: Some(output),
            files_modified,
        }
    }

    /// Create a failure outcome.
    pub fn failure(error: String, recoverable: bool) -> StepOutcome {
        StepOutcome::Failure { error, recoverable }
    }

    /// Create a partial outcome.
    pub fn partial(completed: String, remaining: String) -> StepOutcome {
        StepOutcome::Partial {
            completed,
            remaining,
        }
    }

    /// Create a skipped outcome.
    pub fn skipped(reason: String) -> StepOutcome {
        StepOutcome::Skipped { reason }
    }
}

/// Snapshot of a file's state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileSnapshot {
    /// Path to the file
    pub path: String,
    /// SHA-256 checksum of file contents
    pub checksum: String,
    /// File size in bytes
    pub size: u64,
    /// For small files (< 1KB), store full content
    pub content: Option<String>,
    /// Whether the file existed
    pub exists: bool,
}

impl FileSnapshot {
    /// Create a new file snapshot.
    pub fn new(path: &str, checksum: String, size: u64, exists: bool) -> Self {
        let content = if exists && size < 1024 {
            // For small files, read and store content
            std::fs::read_to_string(path).ok()
        } else {
            None
        };

        Self {
            path: path.to_string(),
            checksum,
            size,
            content,
            exists,
        }
    }

    /// Create a snapshot for a non-existent file.
    pub fn not_found(path: &str) -> Self {
        Self {
            path: path.to_string(),
            checksum: String::new(),
            size: 0,
            content: None,
            exists: false,
        }
    }

    /// Verify that the current file state matches this snapshot.
    pub fn verify(&self) -> bool {
        if !self.exists {
            return !std::path::Path::new(&self.path).exists();
        }

        let Ok(content) = std::fs::read(&self.path) else {
            return false;
        };

        if content.len() as u64 != self.size {
            return false;
        }

        let checksum =
            crate::checkpoint::state::calculate_file_checksum(std::path::Path::new(&self.path));

        match checksum {
            Some(actual) => actual == self.checksum,
            None => false,
        }
    }
}

/// Execution history tracking.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionHistory {
    /// All execution steps in order
    pub steps: Vec<ExecutionStep>,
    /// File snapshots for key files at checkpoint time
    pub file_snapshots: HashMap<String, FileSnapshot>,
}

impl ExecutionHistory {
    /// Create a new execution history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an execution step.
    pub fn add_step(&mut self, step: ExecutionStep) {
        self.steps.push(step);
    }

    /// Add a file snapshot.
    pub fn add_file_snapshot(&mut self, path: String, snapshot: FileSnapshot) {
        self.file_snapshots.insert(path, snapshot);
    }

    /// Get the last step of a given type.
    pub fn last_step_of_type(&self, step_type: &str) -> Option<&ExecutionStep> {
        self.steps.iter().rev().find(|s| s.step_type == step_type)
    }

    /// Get all steps for a specific phase.
    pub fn steps_for_phase(&self, phase: &str) -> Vec<&ExecutionStep> {
        self.steps.iter().filter(|s| s.phase == phase).collect()
    }

    /// Get the last completed iteration for a phase.
    pub fn last_completed_iteration(&self, phase: &str) -> Option<u32> {
        self.steps
            .iter()
            .filter(|s| s.phase == phase && matches!(s.outcome, StepOutcome::Success { .. }))
            .map(|s| s.iteration)
            .max()
    }

    /// Check if a step was already completed.
    pub fn is_step_completed(&self, phase: &str, iteration: u32, step_type: &str) -> bool {
        self.steps.iter().any(|s| {
            s.phase == phase
                && s.iteration == iteration
                && s.step_type == step_type
                && matches!(s.outcome, StepOutcome::Success { .. })
        })
    }

    /// Get a summary of completed work.
    pub fn summary(&self) -> String {
        let mut summary = String::from("Execution History:\n");

        for step in &self.steps {
            let status = match &step.outcome {
                StepOutcome::Success { .. } => "✓",
                StepOutcome::Failure { .. } => "✗",
                StepOutcome::Partial { .. } => "○",
                StepOutcome::Skipped { .. } => "⊘",
            };

            summary.push_str(&format!(
                "  {} {} (iteration {}): {}\n",
                status, step.phase, step.iteration, step.step_type
            ));
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_step_new() {
        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec!["test.txt".to_string()],
        };

        let step = ExecutionStep::new("Development", 1, "dev_run", outcome);

        assert_eq!(step.phase, "Development");
        assert_eq!(step.iteration, 1);
        assert_eq!(step.step_type, "dev_run");
        assert!(step.agent.is_none());
        assert!(step.duration_secs.is_none());
    }

    #[test]
    fn test_execution_step_with_agent() {
        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec![],
        };

        let step = ExecutionStep::new("Development", 1, "dev_run", outcome)
            .with_agent("claude")
            .with_duration(120);

        assert_eq!(step.agent, Some("claude".to_string()));
        assert_eq!(step.duration_secs, Some(120));
    }

    #[test]
    fn test_step_outcomes() {
        let success = ExecutionStep::success(vec!["file.rs".to_string()]);
        assert!(matches!(success, StepOutcome::Success { .. }));

        let failure = ExecutionStep::failure("error".to_string(), true);
        assert!(matches!(failure, StepOutcome::Failure { .. }));

        let partial = ExecutionStep::partial("done".to_string(), "todo".to_string());
        assert!(matches!(partial, StepOutcome::Partial { .. }));

        let skipped = ExecutionStep::skipped("reason".to_string());
        assert!(matches!(skipped, StepOutcome::Skipped { .. }));
    }

    #[test]
    fn test_file_snapshot() {
        let snapshot = FileSnapshot::new("test.txt", "abc123".to_string(), 100, true);

        assert_eq!(snapshot.path, "test.txt");
        assert_eq!(snapshot.checksum, "abc123");
        assert_eq!(snapshot.size, 100);
        assert!(snapshot.exists);
    }

    #[test]
    fn test_file_snapshot_not_found() {
        let snapshot = FileSnapshot::not_found("missing.txt");

        assert_eq!(snapshot.path, "missing.txt");
        assert!(!snapshot.exists);
        assert_eq!(snapshot.size, 0);
    }

    #[test]
    fn test_execution_history_add_step() {
        let mut history = ExecutionHistory::new();
        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec![],
        };

        let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
        history.add_step(step);

        assert_eq!(history.steps.len(), 1);
    }

    #[test]
    fn test_execution_history_last_step_of_type() {
        let mut history = ExecutionHistory::new();

        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec![],
        };

        history.add_step(
            ExecutionStep::new("Development", 1, "dev_run", outcome.clone()).with_agent("agent1"),
        );
        history.add_step(
            ExecutionStep::new("Development", 2, "dev_run", outcome.clone()).with_agent("agent2"),
        );

        let last = history.last_step_of_type("dev_run");
        assert!(last.is_some());
        assert_eq!(last.unwrap().agent, Some("agent2".to_string()));
    }

    #[test]
    fn test_execution_history_is_step_completed() {
        let mut history = ExecutionHistory::new();

        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec![],
        };

        history.add_step(ExecutionStep::new("Development", 1, "dev_run", outcome));

        assert!(history.is_step_completed("Development", 1, "dev_run"));
        assert!(!history.is_step_completed("Development", 2, "dev_run"));
        assert!(!history.is_step_completed("Review", 1, "dev_run"));
    }

    #[test]
    fn test_execution_history_last_completed_iteration() {
        let mut history = ExecutionHistory::new();

        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec![],
        };

        history.add_step(ExecutionStep::new(
            "Development",
            1,
            "dev_run",
            outcome.clone(),
        ));
        history.add_step(ExecutionStep::new(
            "Development",
            2,
            "dev_run",
            outcome.clone(),
        ));
        history.add_step(ExecutionStep::new("Development", 3, "dev_run", outcome));

        assert_eq!(history.last_completed_iteration("Development"), Some(3));
        assert_eq!(history.last_completed_iteration("Review"), None);
    }

    #[test]
    fn test_execution_history_summary() {
        let mut history = ExecutionHistory::new();

        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec![],
        };

        history.add_step(ExecutionStep::new("Development", 1, "dev_run", outcome));

        let summary = history.summary();
        assert!(summary.contains("Execution History:"));
        assert!(summary.contains("Development"));
        assert!(summary.contains("✓"));
    }

    #[test]
    fn test_execution_history_add_file_snapshot() {
        let mut history = ExecutionHistory::new();
        let snapshot = FileSnapshot::new("test.txt", "abc123".to_string(), 100, true);

        history.add_file_snapshot("test.txt".to_string(), snapshot);

        assert_eq!(history.file_snapshots.len(), 1);
        assert!(history.file_snapshots.contains_key("test.txt"));
    }

    #[test]
    fn test_execution_history_steps_for_phase() {
        let mut history = ExecutionHistory::new();

        let outcome = StepOutcome::Success {
            output: None,
            files_modified: vec![],
        };

        history.add_step(ExecutionStep::new(
            "Development",
            1,
            "dev_run",
            outcome.clone(),
        ));
        history.add_step(ExecutionStep::new("Review", 1, "review", outcome.clone()));
        history.add_step(ExecutionStep::new("Development", 2, "dev_run", outcome));

        let dev_steps = history.steps_for_phase("Development");
        assert_eq!(dev_steps.len(), 2);

        let review_steps = history.steps_for_phase("Review");
        assert_eq!(review_steps.len(), 1);
    }
}
