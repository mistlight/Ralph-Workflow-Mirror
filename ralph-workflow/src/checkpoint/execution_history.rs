//! Execution history tracking for checkpoint state.
//!
//! This module provides structures for tracking the execution history of a pipeline,
//! enabling idempotent recovery and validation of state.

use crate::checkpoint::timestamp;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// Outcome of an execution step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepOutcome {
    /// Step completed successfully
    Success {
        output: Option<String>,
        files_modified: Vec<String>,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    /// Step failed with error
    Failure {
        error: String,
        recoverable: bool,
        #[serde(default)]
        exit_code: Option<i32>,
        #[serde(default)]
        signals: Vec<String>,
    },
    /// Step partially completed (may need retry)
    Partial {
        completed: String,
        remaining: String,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    /// Step was skipped (e.g., already done)
    Skipped { reason: String },
}

impl StepOutcome {
    /// Create a Success outcome with default values.
    pub fn success(output: Option<String>, files_modified: Vec<String>) -> Self {
        Self::Success {
            output,
            files_modified,
            exit_code: Some(0),
        }
    }

    /// Create a Failure outcome with default values.
    pub fn failure(error: String, recoverable: bool) -> Self {
        Self::Failure {
            error,
            recoverable,
            exit_code: None,
            signals: Vec::new(),
        }
    }

    /// Create a Partial outcome with default values.
    pub fn partial(completed: String, remaining: String) -> Self {
        Self::Partial {
            completed,
            remaining,
            exit_code: None,
        }
    }

    /// Create a Skipped outcome.
    pub fn skipped(reason: String) -> Self {
        Self::Skipped { reason }
    }
}

/// Detailed information about files modified in a step.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifiedFilesDetail {
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub modified: Vec<String>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

/// Summary of issues found and fixed during a step.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IssuesSummary {
    /// Number of issues found
    #[serde(default)]
    pub found: u32,
    /// Number of issues fixed
    #[serde(default)]
    pub fixed: u32,
    /// Description of issues (e.g., "3 clippy warnings, 2 test failures")
    #[serde(default)]
    pub description: Option<String>,
}

/// A single execution step in the pipeline history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// When a checkpoint was saved during this step (ISO 8601 format string)
    #[serde(default)]
    pub checkpoint_saved_at: Option<String>,
    /// Git commit OID created during this step (if any)
    #[serde(default)]
    pub git_commit_oid: Option<String>,
    /// Detailed information about files modified
    #[serde(default)]
    pub modified_files_detail: Option<ModifiedFilesDetail>,
    /// The prompt text used for this step (for deterministic replay)
    #[serde(default)]
    pub prompt_used: Option<String>,
    /// Issues summary (found and fixed counts)
    #[serde(default)]
    pub issues_summary: Option<IssuesSummary>,
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
            checkpoint_saved_at: None,
            git_commit_oid: None,
            modified_files_detail: None,
            prompt_used: None,
            issues_summary: None,
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

    /// Set the git commit OID created during this step.
    pub fn with_git_commit_oid(mut self, oid: &str) -> Self {
        self.git_commit_oid = Some(oid.to_string());
        self
    }
}

/// Default threshold for storing file content in snapshots (10KB).
///
/// Files smaller than this threshold will have their full content stored
/// in the checkpoint for automatic recovery on resume.
const DEFAULT_CONTENT_THRESHOLD: u64 = 10 * 1024;

/// Maximum file size that will be compressed in snapshots (100KB).
///
/// Files between DEFAULT_CONTENT_THRESHOLD and this size that are key files
/// (PROMPT.md, PLAN.md, ISSUES.md) will be compressed before storing.
const MAX_COMPRESS_SIZE: u64 = 100 * 1024;

/// Snapshot of a file's state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FileSnapshot {
    /// Path to the file
    pub path: String,
    /// SHA-256 checksum of file contents
    pub checksum: String,
    /// File size in bytes
    pub size: u64,
    /// For small files (< 10KB by default), store full content
    pub content: Option<String>,
    /// Compressed content (base64-encoded gzip) for larger key files
    pub compressed_content: Option<String>,
    /// Whether the file existed
    pub exists: bool,
}

impl FileSnapshot {
    /// Create a new file snapshot with the default content threshold (10KB).
    ///
    /// This version does not capture file content (content and compressed_content will be None).
    /// Use `from_workspace` to create a snapshot with content from a workspace.
    pub fn new(path: &str, checksum: String, size: u64, exists: bool) -> Self {
        Self {
            path: path.to_string(),
            checksum,
            size,
            content: None,
            compressed_content: None,
            exists,
        }
    }

    /// Create a file snapshot from a workspace using the default content threshold (10KB).
    ///
    /// Files smaller than 10KB will have their content stored.
    /// Key files (PROMPT.md, PLAN.md, ISSUES.md, NOTES.md) may be compressed if they
    /// are between 10KB and 100KB.
    pub fn from_workspace_default(
        workspace: &dyn Workspace,
        path: &str,
        checksum: String,
        size: u64,
        exists: bool,
    ) -> Self {
        Self::from_workspace(
            workspace,
            path,
            checksum,
            size,
            exists,
            DEFAULT_CONTENT_THRESHOLD,
        )
    }

    /// Create a file snapshot from a workspace, optionally capturing content.
    ///
    /// Files smaller than `max_size` bytes will have their content stored.
    /// Key files (PROMPT.md, PLAN.md, ISSUES.md, NOTES.md) may be compressed if they
    /// are between max_size and MAX_COMPRESS_SIZE.
    pub fn from_workspace(
        workspace: &dyn Workspace,
        path: &str,
        checksum: String,
        size: u64,
        exists: bool,
        max_size: u64,
    ) -> Self {
        let mut content = None;
        let mut compressed_content = None;

        if exists {
            let is_key_file = path.contains("PROMPT.md")
                || path.contains("PLAN.md")
                || path.contains("ISSUES.md")
                || path.contains("NOTES.md");

            let path_ref = Path::new(path);

            if size < max_size {
                // For small files, read and store content directly
                content = workspace.read(path_ref).ok();
            } else if is_key_file && size < MAX_COMPRESS_SIZE {
                // For larger key files, compress the content
                if let Ok(data) = workspace.read_bytes(path_ref) {
                    compressed_content = compress_data(&data).ok();
                }
            }
        }

        Self {
            path: path.to_string(),
            checksum,
            size,
            content,
            compressed_content,
            exists,
        }
    }

    /// Get the file content, decompressing if necessary.
    pub fn get_content(&self) -> Option<String> {
        if let Some(ref content) = self.content {
            Some(content.clone())
        } else if let Some(ref compressed) = self.compressed_content {
            decompress_data(compressed).ok()
        } else {
            None
        }
    }

    /// Create a snapshot for a non-existent file.
    pub fn not_found(path: &str) -> Self {
        Self {
            path: path.to_string(),
            checksum: String::new(),
            size: 0,
            content: None,
            compressed_content: None,
            exists: false,
        }
    }

    /// Verify that the current file state matches this snapshot using a workspace.
    pub fn verify_with_workspace(&self, workspace: &dyn Workspace) -> bool {
        let path = Path::new(&self.path);

        if !self.exists {
            return !workspace.exists(path);
        }

        let Ok(content) = workspace.read_bytes(path) else {
            return false;
        };

        if content.len() as u64 != self.size {
            return false;
        }

        let checksum = crate::checkpoint::state::calculate_checksum_from_bytes(&content);
        checksum == self.checksum
    }
}

/// Compress data using gzip and encode as base64.
///
/// This is used to store larger file content in checkpoints without
/// bloating the checkpoint file size too much.
fn compress_data(data: &[u8]) -> Result<String, std::io::Error> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    let compressed = encoder.finish()?;

    Ok(STANDARD.encode(&compressed))
}

/// Decompress data that was compressed with compress_data.
fn decompress_data(encoded: &str) -> Result<String, std::io::Error> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use flate2::read::GzDecoder;
    use std::io::Read;

    let compressed = STANDARD.decode(encoded).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Base64 decode error: {}", e),
        )
    })?;

    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;

    String::from_utf8(decompressed).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("UTF-8 decode error: {}", e),
        )
    })
}

/// Execution history tracking.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ExecutionHistory {
    /// All execution steps in order
    pub steps: VecDeque<ExecutionStep>,
    /// File snapshots for key files at checkpoint time
    pub file_snapshots: HashMap<String, FileSnapshot>,
}

impl ExecutionHistory {
    /// Create a new execution history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an execution step WITHOUT bounding (DEPRECATED - use add_step_bounded instead).
    ///
    /// **WARNING:** This method allows unbounded growth and should NOT be used
    /// in production code. Use `add_step_bounded(step, limit)` instead.
    ///
    /// This method exists only for backward compatibility during migration.
    /// All production code should use the bounded version to prevent memory leaks.
    ///
    /// # Migration
    ///
    /// Replace:
    /// ```ignore
    /// history.add_step(step);
    /// ```
    ///
    /// With:
    /// ```ignore
    /// history.add_step_bounded(step, ctx.config.execution_history_limit);
    /// ```
    #[deprecated(
        since = "0.7.3",
        note = "Use add_step_bounded(step, limit) to prevent unbounded memory growth"
    )]
    pub fn add_step(&mut self, step: ExecutionStep) {
        // Unbounded behavior - kept only for backward compatibility
        // All production callsites have been migrated to add_step_bounded
        self.steps.push_back(step);
    }

    /// Add an execution step with explicit bounding (preferred method).
    ///
    /// This is the preferred method that enforces bounded memory growth.
    /// Use this instead of `add_step()` to prevent unbounded growth.
    pub fn add_step_bounded(&mut self, step: ExecutionStep, limit: usize) {
        self.steps.push_back(step);

        // Enforce limit by dropping oldest entries.
        // VecDeque::pop_front is O(1) amortized and avoids repeated memmoves.
        while self.steps.len() > limit {
            self.steps.pop_front();
        }
    }

    /// Clone this execution history while enforcing a hard step limit.
    ///
    /// This is intended for resume paths where a legacy checkpoint may contain an
    /// oversized `steps` buffer. Cloning only the tail avoids allocating memory
    /// proportional to the checkpoint's full history.
    pub fn clone_bounded(&self, limit: usize) -> Self {
        if limit == 0 {
            return Self {
                steps: VecDeque::new(),
                file_snapshots: self.file_snapshots.clone(),
            };
        }

        let len = self.steps.len();
        if len <= limit {
            return self.clone();
        }

        let keep_from = len - limit;
        let mut steps = VecDeque::with_capacity(limit);
        steps.extend(self.steps.iter().skip(keep_from).cloned());
        Self {
            steps,
            file_snapshots: self.file_snapshots.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_step_new() {
        let outcome = StepOutcome::success(None, vec!["test.txt".to_string()]);
        let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
        assert_eq!(step.phase, "Development");
        assert_eq!(step.iteration, 1);
        assert_eq!(step.step_type, "dev_run");
        assert!(step.agent.is_none());
        assert!(step.duration_secs.is_none());
        // Verify new fields are None by default
        assert!(step.git_commit_oid.is_none());
        assert!(step.modified_files_detail.is_none());
        assert!(step.prompt_used.is_none());
        assert!(step.issues_summary.is_none());
    }

    #[test]
    fn test_execution_step_with_agent() {
        let outcome = StepOutcome::success(None, vec![]);
        let step = ExecutionStep::new("Development", 1, "dev_run", outcome)
            .with_agent("claude")
            .with_duration(120);
        assert_eq!(step.agent, Some("claude".to_string()));
        assert_eq!(step.duration_secs, Some(120));
    }

    #[test]
    fn test_execution_step_new_fields_default() {
        let outcome = StepOutcome::success(None, vec![]);
        let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
        // Verify new fields are None by default
        assert!(step.git_commit_oid.is_none());
        assert!(step.modified_files_detail.is_none());
        assert!(step.prompt_used.is_none());
        assert!(step.issues_summary.is_none());
    }

    #[test]
    fn test_modified_files_detail_default() {
        let detail = ModifiedFilesDetail::default();
        assert!(detail.added.is_empty());
        assert!(detail.modified.is_empty());
        assert!(detail.deleted.is_empty());
    }

    #[test]
    fn test_issues_summary_default() {
        let summary = IssuesSummary::default();
        assert_eq!(summary.found, 0);
        assert_eq!(summary.fixed, 0);
        assert!(summary.description.is_none());
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
        let outcome = StepOutcome::success(None, vec![]);
        let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
        history.add_step_bounded(step, 1000);
        assert_eq!(history.steps.len(), 1);
        assert_eq!(history.steps[0].phase, "Development");
        assert_eq!(history.steps[0].iteration, 1);
    }

    #[test]
    fn test_execution_step_serialization_with_new_fields() {
        // Create a step with new fields via JSON to test backward compatibility
        let json_str = r#"{"phase":"Review","iteration":1,"step_type":"review","timestamp":"2025-01-20 12:00:00","outcome":{"Success":{"output":null,"files_modified":[],"exit_code":0}},"agent":null,"duration_secs":null,"checkpoint_saved_at":null,"git_commit_oid":"abc123","modified_files_detail":{"added":["a.rs"],"modified":[],"deleted":[]},"prompt_used":"Fix issues","issues_summary":{"found":2,"fixed":2,"description":"All fixed"}}"#;
        let deserialized: ExecutionStep = serde_json::from_str(json_str).unwrap();
        assert_eq!(deserialized.git_commit_oid, Some("abc123".to_string()));
        assert_eq!(
            deserialized.modified_files_detail.as_ref().unwrap().added,
            vec!["a.rs"]
        );
        assert_eq!(deserialized.prompt_used, Some("Fix issues".to_string()));
        assert_eq!(deserialized.issues_summary.as_ref().unwrap().found, 2);
    }
}
