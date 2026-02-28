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

fn serialize_option_boxed_string_slice_empty_if_none_field<S, V>(
    value: V,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    V: std::ops::Deref<Target = Option<Box<[String]>>>,
{
    let values = (*value).as_deref();
    serialize_option_boxed_string_slice_empty_if_none(values, serializer)
}

fn serialize_option_boxed_string_slice_empty_if_none<S>(
    value: Option<&[String]>,
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
            serialize_with = "serialize_option_boxed_string_slice_empty_if_none_field"
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
            serialize_with = "serialize_option_boxed_string_slice_empty_if_none_field"
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
        self.content.clone().or_else(|| {
            self.compressed_content
                .as_ref()
                .and_then(|compressed| decompress_data(compressed).ok())
        })
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
mod tests;
