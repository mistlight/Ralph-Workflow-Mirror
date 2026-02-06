use super::run_id::RunId;
use crate::workspace::Workspace;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Context for managing per-run log directories and files.
///
/// Owns the run_id and provides path resolution for all logs.
/// All logs from a single pipeline invocation are grouped under
/// `.agent/logs-<run_id>/` for easy sharing and diagnosis.
pub struct RunLogContext {
    run_id: RunId,
    run_dir: PathBuf,
}

impl RunLogContext {
    /// Create a new RunLogContext with collision handling.
    ///
    /// Generates a new run_id and creates the run directory structure.
    /// If directory exists, tries collision counter variants (rare case
    /// of multiple runs starting in the same millisecond).
    ///
    /// Creates subdirectories:
    /// - `.agent/logs-<run_id>/agents/` for per-agent logs
    /// - `.agent/logs-<run_id>/provider/` for provider streaming logs
    /// - `.agent/logs-<run_id>/debug/` for future debug artifacts
    pub fn new(workspace: &dyn Workspace) -> Result<Self> {
        let mut run_id = RunId::new();
        let mut counter = 1u32;

        loop {
            let run_dir = PathBuf::from(format!(".agent/logs-{}", run_id));

            if !workspace.exists(&run_dir) {
                // Create run directory and subdirectories
                workspace
                    .create_dir_all(&run_dir)
                    .context("Failed to create run log directory")?;

                workspace
                    .create_dir_all(&run_dir.join("agents"))
                    .context("Failed to create agents log subdirectory")?;

                workspace
                    .create_dir_all(&run_dir.join("provider"))
                    .context("Failed to create provider log subdirectory")?;

                workspace
                    .create_dir_all(&run_dir.join("debug"))
                    .context("Failed to create debug log subdirectory")?;

                return Ok(Self { run_id, run_dir });
            }

            // Collision detected, try next counter
            if counter > 99 {
                anyhow::bail!("Too many collisions creating run log directory");
            }
            run_id = run_id.with_collision_counter(counter);
            counter += 1;
        }
    }

    /// Create a RunLogContext from an existing checkpoint (for resume).
    ///
    /// Uses the run_id from the checkpoint to continue logging to the
    /// same run directory. If the directory doesn't exist (e.g., deleted),
    /// it is recreated.
    pub fn from_checkpoint(run_id: &str, workspace: &dyn Workspace) -> Result<Self> {
        let run_id = RunId::from_checkpoint(run_id);
        let run_dir = PathBuf::from(format!(".agent/logs-{}", run_id));

        // Ensure directory exists (may have been deleted)
        if !workspace.exists(&run_dir) {
            workspace
                .create_dir_all(&run_dir)
                .context("Failed to recreate run log directory for resume")?;

            workspace
                .create_dir_all(&run_dir.join("agents"))
                .context("Failed to recreate agents log subdirectory for resume")?;

            workspace
                .create_dir_all(&run_dir.join("provider"))
                .context("Failed to recreate provider log subdirectory for resume")?;

            workspace
                .create_dir_all(&run_dir.join("debug"))
                .context("Failed to recreate debug log subdirectory for resume")?;
        }

        Ok(Self { run_id, run_dir })
    }

    /// Get a reference to the run ID.
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Get the run directory path (relative to workspace root).
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    /// Get the path to the pipeline log file.
    pub fn pipeline_log(&self) -> PathBuf {
        self.run_dir.join("pipeline.log")
    }

    /// Get the path to the event loop log file.
    pub fn event_loop_log(&self) -> PathBuf {
        self.run_dir.join("event_loop.log")
    }

    /// Get the path to the event loop trace file (crash-only).
    pub fn event_loop_trace(&self) -> PathBuf {
        self.run_dir.join("event_loop_trace.jsonl")
    }

    /// Get the path to an agent log file.
    ///
    /// # Arguments
    /// * `phase` - Phase name (e.g., "planning", "dev", "reviewer", "commit")
    /// * `index` - Invocation index within the phase (1-based)
    /// * `attempt` - Optional attempt number for retries (1-based, None for first attempt)
    ///
    /// # Returns
    /// Path like `.agent/logs-<run_id>/agents/planning_1.log` or
    /// `.agent/logs-<run_id>/agents/dev_2_a1.log` for retries.
    pub fn agent_log(&self, phase: &str, index: u32, attempt: Option<u32>) -> PathBuf {
        let filename = if let Some(a) = attempt {
            format!("{}_{}_a{}.log", phase, index, a)
        } else {
            format!("{}_{}.log", phase, index)
        };
        self.run_dir.join("agents").join(filename)
    }

    /// Get the path to a provider streaming log file.
    ///
    /// # Arguments
    /// * `name` - Provider log filename (e.g., "claude-stream_dev_1.jsonl")
    ///
    /// # Returns
    /// Path like `.agent/logs-<run_id>/provider/claude-stream_dev_1.jsonl`.
    pub fn provider_log(&self, name: &str) -> PathBuf {
        self.run_dir.join("provider").join(name)
    }

    /// Get the path to the run metadata file (run.json).
    pub fn run_metadata(&self) -> PathBuf {
        self.run_dir.join("run.json")
    }

    /// Write run.json metadata file.
    ///
    /// This should be called early in pipeline execution to record
    /// essential metadata for debugging and tooling.
    pub fn write_run_metadata(
        &self,
        workspace: &dyn Workspace,
        metadata: &RunMetadata,
    ) -> Result<()> {
        let path = self.run_metadata();
        let json =
            serde_json::to_string_pretty(metadata).context("Failed to serialize run metadata")?;
        workspace
            .write(&path, &json)
            .context("Failed to write run.json")
    }
}

/// Metadata recorded in run.json for each pipeline run.
///
/// This file is written at the start of each run to provide context
/// for debugging and tooling. It anchors the run with essential info
/// like command invocation, timestamps, and environment details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    /// Unique run identifier (matches directory name)
    pub run_id: String,

    /// Timestamp when run started (UTC, RFC3339)
    pub started_at_utc: String,

    /// Command as invoked by user (e.g., "ralph" or "ralph --resume")
    pub command: String,

    /// Whether this is a resumed session
    pub resume: bool,

    /// Absolute path to repository root
    pub repo_root: String,

    /// Ralph version (from Cargo.toml)
    pub ralph_version: String,

    /// Process ID (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,

    /// Configuration summary (non-secret metadata)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_summary: Option<ConfigSummary>,
}

/// Non-secret configuration summary for run.json.
///
/// Captures high-level config info useful for debugging without
/// exposing any sensitive data (API keys, tokens, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSummary {
    /// Developer agent name (if configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer_agent: Option<String>,

    /// Reviewer agent name (if configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer_agent: Option<String>,

    /// Total iterations configured
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_iterations: Option<u32>,

    /// Total reviewer passes configured
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_reviewer_passes: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceFs;
    use std::path::PathBuf;

    #[test]
    fn test_run_log_context_creation() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let ctx = RunLogContext::new(&workspace).unwrap();

        // Verify run directory exists
        assert!(workspace.exists(ctx.run_dir()));

        // Verify subdirectories exist
        assert!(workspace.exists(&ctx.run_dir().join("agents")));
        assert!(workspace.exists(&ctx.run_dir().join("provider")));
        assert!(workspace.exists(&ctx.run_dir().join("debug")));
    }

    #[test]
    fn test_run_log_context_path_resolution() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let ctx = RunLogContext::new(&workspace).unwrap();

        // Test pipeline log path
        let pipeline_log = ctx.pipeline_log();
        assert!(pipeline_log.ends_with("pipeline.log"));

        // Test event loop log path
        let event_loop_log = ctx.event_loop_log();
        assert!(event_loop_log.ends_with("event_loop.log"));

        // Test agent log path (no attempt)
        let agent_log = ctx.agent_log("planning", 1, None);
        assert!(agent_log.ends_with("agents/planning_1.log"));

        // Test agent log path (with attempt)
        let agent_log_retry = ctx.agent_log("dev", 2, Some(3));
        assert!(agent_log_retry.ends_with("agents/dev_2_a3.log"));

        // Test provider log path
        let provider_log = ctx.provider_log("claude-stream.jsonl");
        assert!(provider_log.ends_with("provider/claude-stream.jsonl"));
    }

    #[test]
    fn test_run_log_context_from_checkpoint() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let original_id = "2026-02-06_14-03-27.123Z";
        let ctx = RunLogContext::from_checkpoint(original_id, &workspace).unwrap();

        assert_eq!(ctx.run_id().as_str(), original_id);
        assert!(workspace.exists(ctx.run_dir()));
    }

    #[test]
    fn test_run_metadata_serialization() {
        let metadata = RunMetadata {
            run_id: "2026-02-06_14-03-27.123Z".to_string(),
            started_at_utc: "2026-02-06T14:03:27.123Z".to_string(),
            command: "ralph".to_string(),
            resume: false,
            repo_root: "/tmp/test".to_string(),
            ralph_version: "0.6.3".to_string(),
            pid: Some(12345),
            config_summary: Some(ConfigSummary {
                developer_agent: Some("claude".to_string()),
                reviewer_agent: Some("claude".to_string()),
                total_iterations: Some(3),
                total_reviewer_passes: Some(1),
            }),
        };

        let json = serde_json::to_string_pretty(&metadata).unwrap();
        assert!(json.contains("run_id"));
        assert!(json.contains("2026-02-06_14-03-27.123Z"));
        assert!(json.contains("ralph"));
    }

    #[test]
    fn test_write_run_metadata() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let ctx = RunLogContext::new(&workspace).unwrap();

        let metadata = RunMetadata {
            run_id: ctx.run_id().to_string(),
            started_at_utc: "2026-02-06T14:03:27.123Z".to_string(),
            command: "ralph".to_string(),
            resume: false,
            repo_root: tempdir.path().display().to_string(),
            ralph_version: "0.6.3".to_string(),
            pid: Some(12345),
            config_summary: None,
        };

        ctx.write_run_metadata(&workspace, &metadata).unwrap();

        // Verify file was written
        let json_path = ctx.run_metadata();
        assert!(workspace.exists(&json_path));

        // Verify content
        let content = workspace.read(&json_path).unwrap();
        assert!(content.contains(&ctx.run_id().to_string()));
    }
}
