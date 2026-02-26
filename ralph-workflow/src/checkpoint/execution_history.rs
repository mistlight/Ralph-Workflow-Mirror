//! Execution history tracking for checkpoint state.
//!
//! This module provides structures for tracking the execution history of a pipeline,
//! enabling idempotent recovery and validation of state.

use crate::checkpoint::timestamp;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

fn deserialize_option_boxed_string_slice_none_if_empty<'de, D>(
    deserializer: D,
) -> Result<Option<Box<[String]>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<Vec<String>>::deserialize(deserializer)?;
    Ok(match opt {
        None => None,
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v.into_boxed_slice()),
    })
}

fn serialize_option_boxed_string_slice_empty_if_none<S>(
    value: &Option<Box<[String]>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;

    if let Some(values) = value {
        values.serialize(serializer)
    } else {
        let seq = serializer.serialize_seq(Some(0))?;
        seq.end()
    }
}

/// Outcome of an execution step.
///
/// # Memory Optimization
///
/// This enum uses Box<str> for string fields and Option<Box<[String]>> for
/// collections to reduce allocation overhead when fields are empty or small.
/// Vec<T> over-allocates capacity, while Box<[T]> uses exactly the needed space.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepOutcome {
    /// Step completed successfully
    Success {
        output: Option<Box<str>>,
        #[serde(
            default,
            deserialize_with = "deserialize_option_boxed_string_slice_none_if_empty",
            serialize_with = "serialize_option_boxed_string_slice_empty_if_none"
        )]
        files_modified: Option<Box<[String]>>,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    /// Step failed with error
    Failure {
        error: Box<str>,
        recoverable: bool,
        #[serde(default)]
        exit_code: Option<i32>,
        #[serde(
            default,
            deserialize_with = "deserialize_option_boxed_string_slice_none_if_empty",
            serialize_with = "serialize_option_boxed_string_slice_empty_if_none"
        )]
        signals: Option<Box<[String]>>,
    },
    /// Step partially completed (may need retry)
    Partial {
        completed: Box<str>,
        remaining: Box<str>,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    /// Step was skipped (e.g., already done)
    Skipped { reason: Box<str> },
}

impl StepOutcome {
    /// Create a Success outcome with default values.
    pub fn success(output: Option<String>, files_modified: Vec<String>) -> Self {
        Self::Success {
            output: output.map(String::into_boxed_str),
            files_modified: if files_modified.is_empty() {
                None
            } else {
                Some(files_modified.into_boxed_slice())
            },
            exit_code: Some(0),
        }
    }

    /// Create a Failure outcome with default values.
    #[must_use]
    pub fn failure(error: String, recoverable: bool) -> Self {
        Self::Failure {
            error: error.into_boxed_str(),
            recoverable,
            exit_code: None,
            signals: None,
        }
    }

    /// Create a Partial outcome with default values.
    #[must_use]
    pub fn partial(completed: String, remaining: String) -> Self {
        Self::Partial {
            completed: completed.into_boxed_str(),
            remaining: remaining.into_boxed_str(),
            exit_code: None,
        }
    }

    /// Create a Skipped outcome.
    #[must_use]
    pub fn skipped(reason: String) -> Self {
        Self::Skipped {
            reason: reason.into_boxed_str(),
        }
    }
}

/// Detailed information about files modified in a step.
///
/// # Memory Optimization
///
/// Uses `Option<Box<[String]>>` instead of `Vec<String>` to save memory:
/// - Empty collections use `None` instead of empty Vec (saves 24 bytes per field)
/// - Non-empty collections use `Box<[String]>` which is 16 bytes vs Vec's 24 bytes
/// - Total savings: up to 72 bytes per instance when all fields are empty
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifiedFilesDetail {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_option_boxed_string_slice_none_if_empty"
    )]
    pub added: Option<Box<[String]>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_option_boxed_string_slice_none_if_empty"
    )]
    pub modified: Option<Box<[String]>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_option_boxed_string_slice_none_if_empty"
    )]
    pub deleted: Option<Box<[String]>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A single execution step in the pipeline history.
///
/// # Memory Optimization
///
/// This struct uses Arc<str> for `phase` and `agent` fields to reduce memory
/// usage through string interning. Phase names and agent names are repeated
/// frequently across execution history entries, so sharing allocations via
/// Arc<str> significantly reduces heap usage.
///
/// Serialization/deserialization is backward-compatible - Arc<str> is serialized
/// as a regular string and can be deserialized from both old (String) and new
/// (Arc<str>) checkpoint formats.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionStep {
    /// Phase this step belongs to (interned via Arc<str>)
    pub phase: Arc<str>,
    /// Iteration number (for development/review iterations)
    pub iteration: u32,
    /// Type of step (e.g., "review", "fix", "commit")
    pub step_type: Box<str>,
    /// When this step was executed (ISO 8601 format string)
    pub timestamp: String,
    /// Outcome of the step
    pub outcome: StepOutcome,
    /// Agent that executed this step (interned via Arc<str>)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Arc<str>>,
    /// Duration in seconds (if available)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    /// When a checkpoint was saved during this step (ISO 8601 format string)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_saved_at: Option<String>,
    /// Git commit OID created during this step (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit_oid: Option<String>,
    /// Detailed information about files modified
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_files_detail: Option<ModifiedFilesDetail>,
    /// The prompt text used for this step (for deterministic replay)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_used: Option<String>,
    /// Issues summary (found and fixed counts)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues_summary: Option<IssuesSummary>,
}

impl ExecutionStep {
    /// Create a new execution step.
    ///
    /// # Performance Note
    ///
    /// For optimal memory usage, use `new_with_pool` to intern repeated phase
    /// and agent names via a `StringPool`. This constructor creates new Arc<str>
    /// allocations for each call.
    #[must_use]
    pub fn new(phase: &str, iteration: u32, step_type: &str, outcome: StepOutcome) -> Self {
        Self {
            phase: Arc::from(phase),
            iteration,
            step_type: Box::from(step_type),
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

    /// Create a new execution step using a `StringPool` for interning.
    ///
    /// This is the preferred constructor when creating many `ExecutionSteps`,
    /// as it reduces memory usage by sharing allocations for repeated phase
    /// and agent names.
    pub fn new_with_pool(
        phase: &str,
        iteration: u32,
        step_type: &str,
        outcome: StepOutcome,
        pool: &mut crate::checkpoint::StringPool,
    ) -> Self {
        Self {
            phase: pool.intern_str(phase),
            iteration,
            step_type: Box::from(step_type),
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
    #[must_use]
    pub fn with_agent(mut self, agent: &str) -> Self {
        self.agent = Some(Arc::from(agent));
        self
    }

    /// Set the agent using a `StringPool` for interning.
    #[must_use]
    pub fn with_agent_pooled(
        mut self,
        agent: &str,
        pool: &mut crate::checkpoint::StringPool,
    ) -> Self {
        self.agent = Some(pool.intern_str(agent));
        self
    }

    /// Set the duration of this step.
    #[must_use]
    pub const fn with_duration(mut self, duration_secs: u64) -> Self {
        self.duration_secs = Some(duration_secs);
        self
    }

    /// Set the git commit OID created during this step.
    #[must_use]
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
/// Files between `DEFAULT_CONTENT_THRESHOLD` and this size that are key files
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
    /// This version does not capture file content (content and `compressed_content` will be None).
    /// Use `from_workspace` to create a snapshot with content from a workspace.
    #[must_use]
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
    /// are between `max_size` and `MAX_COMPRESS_SIZE`.
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
    #[must_use]
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
    #[must_use]
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

const MAX_DECOMPRESSED_SNAPSHOT_BYTES: usize = 1024 * 1024;

/// Decompress data that was compressed with `compress_data`.
fn decompress_data(encoded: &str) -> Result<String, std::io::Error> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use flate2::read::GzDecoder;
    use std::io::Read;

    let compressed = STANDARD.decode(encoded).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Base64 decode error: {e}"),
        )
    })?;

    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut decompressed = Vec::new();
    let mut buf = [0u8; 8 * 1024];

    loop {
        let n = decoder.read(&mut buf)?;
        if n == 0 {
            break;
        }

        if decompressed.len().saturating_add(n) > MAX_DECOMPRESSED_SNAPSHOT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Decompressed payload exceeds max size ({MAX_DECOMPRESSED_SNAPSHOT_BYTES} bytes)"
                ),
            ));
        }

        decompressed.extend_from_slice(&buf[..n]);
    }

    String::from_utf8(decompressed).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("UTF-8 decode error: {e}"),
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
    /// Execution history must be bounded.
    ///
    /// The historical unbounded `add_step` API is intentionally not available in
    /// non-test builds to avoid reintroducing unbounded growth.
    ///
    /// ```compile_fail
    /// use ralph_workflow::checkpoint::ExecutionHistory;
    /// use ralph_workflow::checkpoint::execution_history::{ExecutionStep, StepOutcome};
    ///
    /// let mut history = ExecutionHistory::new();
    /// let step = ExecutionStep::new("Development", 0, "dev_run", StepOutcome::success(None, vec![]));
    ///
    /// // Unbounded push is not part of the public API.
    /// history.add_step(step);
    /// ```
    /// Create a new execution history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an execution step with explicit bounding (preferred method).
    ///
    /// This is the preferred method that enforces bounded memory growth.
    /// Use this to prevent unbounded growth.
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
    #[must_use]
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
        assert_eq!(&*step.phase, "Development");
        assert_eq!(step.iteration, 1);
        assert_eq!(&*step.step_type, "dev_run");
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
        assert_eq!(step.agent.as_deref(), Some("claude"));
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
        assert!(detail.added.is_none());
        assert!(detail.modified.is_none());
        assert!(detail.deleted.is_none());
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
    fn test_decompress_data_rejects_oversized_payload() {
        // Safety invariant: checkpoint resume must not allow decompression bombs.
        // We enforce an upper bound on decompressed payload size.
        let max_bytes = 1024 * 1024;
        let data = "a".repeat(max_bytes + 1);
        let encoded = compress_data(data.as_bytes()).unwrap();

        let err = decompress_data(&encoded).expect_err("oversized payload should be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_execution_history_add_step_bounded() {
        let mut history = ExecutionHistory::new();
        let outcome = StepOutcome::success(None, vec![]);
        let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
        history.add_step_bounded(step, 1000);
        assert_eq!(history.steps.len(), 1);
        assert_eq!(&*history.steps[0].phase, "Development");
        assert_eq!(history.steps[0].iteration, 1);
    }

    #[test]
    fn test_execution_step_serialization_omits_none_option_fields() {
        let outcome = StepOutcome::success(None, vec![]);
        let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
        let json = serde_json::to_string(&step).unwrap();

        assert!(!json.contains("\"agent\":null"));
        assert!(!json.contains("\"duration_secs\":null"));
        assert!(!json.contains("\"checkpoint_saved_at\":null"));
        assert!(!json.contains("\"git_commit_oid\":null"));
        assert!(!json.contains("\"modified_files_detail\":null"));
        assert!(!json.contains("\"prompt_used\":null"));
        assert!(!json.contains("\"issues_summary\":null"));
    }

    #[test]
    fn test_execution_step_serialization_with_new_fields() {
        // Create a step with new fields via JSON to test backward compatibility
        let json_str = r#"{"phase":"Review","iteration":1,"step_type":"review","timestamp":"2025-01-20 12:00:00","outcome":{"Success":{"output":null,"files_modified":[],"exit_code":0}},"agent":null,"duration_secs":null,"checkpoint_saved_at":null,"git_commit_oid":"abc123","modified_files_detail":{"added":["a.rs"],"modified":[],"deleted":[]},"prompt_used":"Fix issues","issues_summary":{"found":2,"fixed":2,"description":"All fixed"}}"#;
        let deserialized: ExecutionStep = serde_json::from_str(json_str).unwrap();
        assert_eq!(deserialized.git_commit_oid, Some("abc123".to_string()));
        let added = deserialized
            .modified_files_detail
            .as_ref()
            .unwrap()
            .added
            .as_ref()
            .unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0], "a.rs");

        // Empty arrays in legacy JSON should preserve the None-for-empty canonical form.
        let detail = deserialized.modified_files_detail.as_ref().unwrap();
        assert!(detail.modified.is_none());
        assert!(detail.deleted.is_none());
        assert_eq!(deserialized.prompt_used, Some("Fix issues".to_string()));
        assert_eq!(deserialized.issues_summary.as_ref().unwrap().found, 2);
    }

    #[test]
    fn test_execution_step_with_string_pool() {
        use crate::checkpoint::StringPool;

        let mut pool = StringPool::new();
        let outcome = StepOutcome::success(None, vec![]);

        // Create multiple steps with the same phase and agent
        let step1 =
            ExecutionStep::new_with_pool("Development", 1, "dev_run", outcome.clone(), &mut pool)
                .with_agent_pooled("claude", &mut pool);
        let step2 = ExecutionStep::new_with_pool("Development", 2, "dev_run", outcome, &mut pool)
            .with_agent_pooled("claude", &mut pool);

        // Verify string pool deduplication works
        assert!(Arc::ptr_eq(&step1.phase, &step2.phase));
        assert!(Arc::ptr_eq(
            step1.agent.as_ref().unwrap(),
            step2.agent.as_ref().unwrap()
        ));

        // Verify content is correct
        assert_eq!(&*step1.phase, "Development");
        assert_eq!(&*step2.phase, "Development");
        assert_eq!(step1.agent.as_deref(), Some("claude"));
        assert_eq!(step2.agent.as_deref(), Some("claude"));
    }

    #[test]
    fn test_execution_step_memory_optimization() {
        use crate::checkpoint::StringPool;

        let mut pool = StringPool::new();
        let outcome = StepOutcome::success(None, vec![]);

        // Create step with string pool
        let step = ExecutionStep::new_with_pool("Development", 1, "dev_run", outcome, &mut pool)
            .with_agent_pooled("claude", &mut pool);

        // Arc<str> and Box<str> should use len() not capacity()
        let phase_size = step.phase.len();
        let step_type_size = step.step_type.len();
        let agent_size = step.agent.as_ref().map_or(0, |s| s.len());

        // Verify sizes are reasonable
        assert_eq!(phase_size, "Development".len());
        assert_eq!(step_type_size, "dev_run".len());
        assert_eq!(agent_size, "claude".len());

        // Total size should be less than String capacity-based approach
        let optimized_size = phase_size + step_type_size + agent_size;
        assert!(optimized_size < 100); // Reasonable upper bound
    }

    #[test]
    fn test_execution_step_serialization_roundtrip() {
        use crate::checkpoint::StringPool;

        let mut pool = StringPool::new();
        let outcome =
            StepOutcome::success(Some("output".to_string()), vec!["file.txt".to_string()]);

        let step = ExecutionStep::new_with_pool("Development", 1, "dev_run", outcome, &mut pool)
            .with_agent_pooled("claude", &mut pool)
            .with_duration(120);

        // Serialize to JSON
        let json = serde_json::to_string(&step).unwrap();

        // Deserialize back
        let deserialized: ExecutionStep = serde_json::from_str(&json).unwrap();

        // Verify all fields match
        assert_eq!(&*step.phase, &*deserialized.phase);
        assert_eq!(step.iteration, deserialized.iteration);
        assert_eq!(&*step.step_type, &*deserialized.step_type);
        assert_eq!(step.agent.as_deref(), deserialized.agent.as_deref());
        assert_eq!(step.duration_secs, deserialized.duration_secs);
        assert_eq!(step.outcome, deserialized.outcome);
    }

    #[test]
    fn test_execution_step_backward_compatible_deserialization() {
        // Old checkpoint format with String fields
        let old_json = r#"{
            "phase": "Development",
            "iteration": 1,
            "step_type": "dev_run",
            "timestamp": "2025-01-20 12:00:00",
            "outcome": {"Success": {"output": null, "files_modified": [], "exit_code": 0}},
            "agent": "claude",
            "duration_secs": 120
        }"#;

        // Should deserialize successfully into new Arc<str> format
        let step: ExecutionStep = serde_json::from_str(old_json).unwrap();

        assert_eq!(&*step.phase, "Development");
        assert_eq!(step.iteration, 1);
        assert_eq!(&*step.step_type, "dev_run");
        assert_eq!(step.agent.as_deref(), Some("claude"));
        assert_eq!(step.duration_secs, Some(120));
    }

    #[test]
    fn test_step_outcome_success_with_empty_files_uses_none() {
        // Empty files_modified should use None instead of empty Vec
        let outcome = StepOutcome::success(None, vec![]);

        match outcome {
            StepOutcome::Success { files_modified, .. } => {
                assert!(files_modified.is_none(), "Empty files should be None");
            }
            _ => panic!("Expected Success variant"),
        }
    }

    #[test]
    fn test_step_outcome_success_with_files_uses_boxed_slice() {
        // Non-empty files_modified should use Box<[String]>
        let files = vec!["file1.txt".to_string(), "file2.txt".to_string()];
        let outcome = StepOutcome::success(None, files);

        match outcome {
            StepOutcome::Success { files_modified, .. } => {
                let files = files_modified.expect("Files should be present");
                assert_eq!(files.len(), 2);
                assert_eq!(files[0], "file1.txt");
                assert_eq!(files[1], "file2.txt");
            }
            _ => panic!("Expected Success variant"),
        }
    }

    #[test]
    fn test_step_outcome_failure_with_no_signals_uses_none() {
        // Failure without signals should use None
        let outcome = StepOutcome::failure("error message".to_string(), true);

        match outcome {
            StepOutcome::Failure { signals, .. } => {
                assert!(signals.is_none(), "Empty signals should be None");
            }
            _ => panic!("Expected Failure variant"),
        }
    }

    #[test]
    fn test_step_outcome_uses_box_str_for_strings() {
        // Verify that Box<str> is used for string fields
        let outcome = StepOutcome::failure("test error".to_string(), false);

        match outcome {
            StepOutcome::Failure { error, .. } => {
                assert_eq!(&*error, "test error");
                // Box<str> uses exactly the needed space
                assert_eq!(error.len(), "test error".len());
            }
            _ => panic!("Expected Failure variant"),
        }
    }

    #[test]
    fn test_step_outcome_constructors_preserve_large_string_content() {
        // StepOutcome constructors accept owned String inputs and store them as Box<str>.
        // Allocation reuse is an optimization and is not guaranteed by Rust toolchains or
        // allocators, so this test asserts only semantic correctness.

        // Large strings avoid any small-string/allocator-size quirks.
        let make_string = |byte: u8| -> String {
            let bytes = vec![byte; 1024];
            String::from_utf8(bytes).expect("valid utf8")
        };

        // failure()
        let s = make_string(b'e');
        let s_expected = s.clone();
        let outcome = StepOutcome::failure(s, true);
        match outcome {
            StepOutcome::Failure { error, .. } => {
                assert_eq!(&*error, s_expected);
                assert_eq!(error.len(), s_expected.len());
            }
            _ => panic!("Expected Failure variant"),
        }

        // partial()
        let completed = make_string(b'c');
        let completed_expected = completed.clone();
        let remaining = make_string(b'r');
        let remaining_expected = remaining.clone();
        let outcome = StepOutcome::partial(completed, remaining);
        match outcome {
            StepOutcome::Partial {
                completed,
                remaining,
                ..
            } => {
                assert_eq!(&*completed, completed_expected);
                assert_eq!(completed.len(), completed_expected.len());
                assert_eq!(&*remaining, remaining_expected);
                assert_eq!(remaining.len(), remaining_expected.len());
            }
            _ => panic!("Expected Partial variant"),
        }

        // skipped()
        let reason = make_string(b's');
        let reason_expected = reason.clone();
        let outcome = StepOutcome::skipped(reason);
        match outcome {
            StepOutcome::Skipped { reason } => {
                assert_eq!(&*reason, reason_expected);
                assert_eq!(reason.len(), reason_expected.len());
            }
            _ => panic!("Expected Skipped variant"),
        }

        // success(Some(output), empty files)
        let output = make_string(b'o');
        let output_expected = output.clone();
        let outcome = StepOutcome::success(Some(output), vec![]);
        match outcome {
            StepOutcome::Success {
                output: Some(output),
                ..
            } => {
                assert_eq!(&*output, output_expected);
                assert_eq!(output.len(), output_expected.len());
            }
            _ => panic!("Expected Success variant with output"),
        }
    }

    #[test]
    fn test_step_outcome_partial_uses_box_str() {
        let outcome = StepOutcome::partial("done".to_string(), "remaining".to_string());

        match outcome {
            StepOutcome::Partial {
                completed,
                remaining,
                ..
            } => {
                assert_eq!(&*completed, "done");
                assert_eq!(&*remaining, "remaining");
                // Verify Box<str> efficiency
                assert_eq!(completed.len(), "done".len());
                assert_eq!(remaining.len(), "remaining".len());
            }
            _ => panic!("Expected Partial variant"),
        }
    }

    #[test]
    fn test_step_outcome_skipped_uses_box_str() {
        let outcome = StepOutcome::skipped("already done".to_string());

        match outcome {
            StepOutcome::Skipped { reason } => {
                assert_eq!(&*reason, "already done");
                assert_eq!(reason.len(), "already done".len());
            }
            _ => panic!("Expected Skipped variant"),
        }
    }

    #[test]
    fn test_step_outcome_serialization_with_empty_collections() {
        // Test that empty collections serialize correctly
        let outcome = StepOutcome::success(None, vec![]);
        let json = serde_json::to_string(&outcome).unwrap();

        // Deserialize back
        let deserialized: StepOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);

        // Verify None is preserved
        match deserialized {
            StepOutcome::Success { files_modified, .. } => {
                assert!(files_modified.is_none());
            }
            _ => panic!("Expected Success variant"),
        }
    }

    #[test]
    fn test_step_outcome_backward_compatibility_with_empty_vec() {
        // Old checkpoints may have empty Vec serialized as []
        let old_json = r#"{"Success":{"output":null,"files_modified":[],"exit_code":0}}"#;
        let outcome: StepOutcome = serde_json::from_str(old_json).unwrap();

        // Canonical form: treat empty arrays as None to preserve the
        // None-for-empty optimization when resaving a legacy checkpoint.
        match outcome {
            StepOutcome::Success {
                ref files_modified, ..
            } => {
                assert!(
                    files_modified.is_none(),
                    "expected empty legacy array to deserialize as None"
                );
            }
            _ => panic!("Expected Success variant"),
        }

        // Round-trip should preserve the on-disk shape for compatibility.
        let json = serde_json::to_string(&outcome).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value.get("Success").and_then(|v| v.get("files_modified")),
            Some(&serde_json::Value::Array(vec![])),
            "expected serialization to use [] (not null) for compatibility"
        );
    }

    #[test]
    fn test_step_outcome_failure_signals_serialize_as_empty_array_when_none() {
        let outcome = StepOutcome::failure("boom".to_string(), true);
        let json = serde_json::to_string(&outcome).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value.get("Failure").and_then(|v| v.get("signals")),
            Some(&serde_json::Value::Array(vec![])),
            "expected serialization to use [] (not null) for signals"
        );
    }

    #[test]
    fn test_modified_files_detail_legacy_empty_arrays_deserialize_to_none() {
        let legacy = r#"{"added":[],"modified":[],"deleted":[]}"#;
        let detail: ModifiedFilesDetail = serde_json::from_str(legacy).unwrap();
        assert!(detail.added.is_none());
        assert!(detail.modified.is_none());
        assert!(detail.deleted.is_none());

        // Round-trip should omit empty fields.
        let json = serde_json::to_string(&detail).unwrap();
        assert_eq!(json, "{}", "expected empty fields to be omitted");
    }

    #[test]
    fn test_step_outcome_memory_efficiency_vs_vec() {
        // Demonstrate memory efficiency of Box<str> and Option<Box<[T]>>
        // Vec<T> over-allocates capacity, Box<[T]> uses exact size

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
                // Box<str> uses exact size
                let output_str = output.expect("Output should be present");
                assert_eq!(output_str.len(), "output".len());

                // Box<[String]> uses exact size (no excess capacity)
                let files = files_modified.expect("Files should be present");
                assert_eq!(files.len(), 2);
            }
            _ => panic!("Expected Success variant"),
        }
    }
}
