//! Pipeline checkpoint state and persistence.
//!
//! This module contains the checkpoint data structures and file operations
//! for saving and loading pipeline state.

use chrono::Local;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;

/// Default directory for Ralph's internal files.
const AGENT_DIR: &str = ".agent";

/// Default checkpoint file name.
const CHECKPOINT_FILE: &str = "checkpoint.json";

/// Current checkpoint format version.
///
/// Increment this when making breaking changes to the checkpoint format.
/// This allows for future migration logic if needed.
/// v1: Initial checkpoint format
/// v2: Added run_id, parent_run_id, resume_count, actual_developer_runs, actual_reviewer_runs
/// v3: Added execution_history, file_system_state for hardened resume
const CHECKPOINT_VERSION: u32 = 3;

/// Get the checkpoint file path.
///
/// By default, the checkpoint is stored in `.agent/checkpoint.json`
/// relative to the current working directory. This function provides
/// a single point of control for the checkpoint location, making it
/// easier to configure or override in the future if needed.
fn checkpoint_path() -> String {
    format!("{AGENT_DIR}/{CHECKPOINT_FILE}")
}

/// Calculate SHA-256 checksum of a file's contents.
///
/// Returns None if the file doesn't exist or cannot be read.
pub(crate) fn calculate_file_checksum(path: &Path) -> Option<String> {
    let content = fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Some(format!("{:x}", hasher.finalize()))
}

/// Snapshot of CLI arguments for exact restoration.
///
/// Captures all relevant CLI arguments so that resuming a pipeline
/// uses the exact same configuration as the original run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliArgsSnapshot {
    /// Number of developer iterations (-D flag)
    pub developer_iters: u32,
    /// Number of reviewer reviews (-R flag)
    pub reviewer_reviews: u32,
    /// Commit message for the final commit
    pub commit_msg: String,
    /// Review depth level (if specified)
    pub review_depth: Option<String>,
    /// Whether to skip automatic rebase
    pub skip_rebase: bool,
    /// Isolation mode: when false, NOTES.md and ISSUES.md persist between iterations
    /// Default is true for backward compatibility with v1/v2 checkpoints.
    #[serde(default = "default_isolation_mode")]
    pub isolation_mode: bool,
    /// Verbosity level (0=Quiet, 1=Normal, 2=Verbose, 3=Full, 4=Debug)
    /// Default is 2 (Verbose) for backward compatibility.
    #[serde(default = "default_verbosity")]
    pub verbosity: u8,
    /// Show streaming quality metrics at the end of agent output
    /// Default is false for backward compatibility.
    #[serde(default)]
    pub show_streaming_metrics: bool,
    /// JSON parser override for the reviewer agent (claude, codex, gemini, opencode, generic)
    #[serde(default)]
    pub reviewer_json_parser: Option<String>,
}

/// Default value for isolation_mode (true = isolation enabled).
fn default_isolation_mode() -> bool {
    true
}

/// Default value for verbosity (2 = Verbose).
fn default_verbosity() -> u8 {
    2
}

/// Builder for creating [`CliArgsSnapshot`] instances.
///
/// Provides a fluent interface for constructing CLI argument snapshots
/// without exceeding function argument limits.
pub struct CliArgsSnapshotBuilder {
    developer_iters: u32,
    reviewer_reviews: u32,
    commit_msg: String,
    review_depth: Option<String>,
    skip_rebase: bool,
    isolation_mode: bool,
    verbosity: u8,
    show_streaming_metrics: bool,
    reviewer_json_parser: Option<String>,
}

impl CliArgsSnapshotBuilder {
    /// Create a new builder with required fields.
    pub fn new(
        developer_iters: u32,
        reviewer_reviews: u32,
        commit_msg: String,
        review_depth: Option<String>,
        skip_rebase: bool,
        isolation_mode: bool,
    ) -> Self {
        Self {
            developer_iters,
            reviewer_reviews,
            commit_msg,
            review_depth,
            skip_rebase,
            isolation_mode,
            verbosity: 2,
            show_streaming_metrics: false,
            reviewer_json_parser: None,
        }
    }

    /// Set the verbosity level.
    pub fn verbosity(mut self, verbosity: u8) -> Self {
        self.verbosity = verbosity;
        self
    }

    /// Set whether to show streaming metrics.
    pub fn show_streaming_metrics(mut self, show: bool) -> Self {
        self.show_streaming_metrics = show;
        self
    }

    /// Set the reviewer JSON parser override.
    pub fn reviewer_json_parser(mut self, parser: Option<String>) -> Self {
        self.reviewer_json_parser = parser;
        self
    }

    /// Build the snapshot.
    pub fn build(self) -> CliArgsSnapshot {
        CliArgsSnapshot {
            developer_iters: self.developer_iters,
            reviewer_reviews: self.reviewer_reviews,
            commit_msg: self.commit_msg,
            review_depth: self.review_depth,
            skip_rebase: self.skip_rebase,
            isolation_mode: self.isolation_mode,
            verbosity: self.verbosity,
            show_streaming_metrics: self.show_streaming_metrics,
            reviewer_json_parser: self.reviewer_json_parser,
        }
    }
}

impl CliArgsSnapshot {
    /// Create a snapshot from CLI argument values.
    ///
    /// This is a convenience method for test code.
    /// For production code, use [`CliArgsSnapshotBuilder`] for better readability.
    #[cfg(test)]
    pub fn new(
        developer_iters: u32,
        reviewer_reviews: u32,
        commit_msg: String,
        review_depth: Option<String>,
        skip_rebase: bool,
        isolation_mode: bool,
        verbosity: u8,
        show_streaming_metrics: bool,
        reviewer_json_parser: Option<String>,
    ) -> Self {
        CliArgsSnapshotBuilder::new(
            developer_iters,
            reviewer_reviews,
            commit_msg,
            review_depth,
            skip_rebase,
            isolation_mode,
        )
        .verbosity(verbosity)
        .show_streaming_metrics(show_streaming_metrics)
        .reviewer_json_parser(reviewer_json_parser)
        .build()
    }
}

/// Snapshot of agent configuration.
///
/// Captures the complete agent configuration to ensure
/// the exact same agent behavior is used when resuming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigSnapshot {
    /// Agent name
    pub name: String,
    /// Agent command
    pub cmd: String,
    /// Output flag for JSON extraction
    pub output_flag: String,
    /// YOLO flag (if any)
    pub yolo_flag: Option<String>,
    /// Whether this agent can commit
    pub can_commit: bool,
    /// Model override (e.g., "-m opencode/glm-4.7-free")
    /// Default is None for backward compatibility with v1/v2 checkpoints.
    #[serde(default)]
    pub model_override: Option<String>,
    /// Provider override (e.g., "opencode", "anthropic")
    /// Default is None for backward compatibility with v1/v2 checkpoints.
    #[serde(default)]
    pub provider_override: Option<String>,
    /// Context level (0=minimal, 1=normal)
    /// Default is 1 (normal context) for backward compatibility with v1/v2 checkpoints.
    #[serde(default = "default_context_level")]
    pub context_level: u8,
}

/// Default value for context_level (1 = normal context).
fn default_context_level() -> u8 {
    1
}

impl AgentConfigSnapshot {
    /// Create a snapshot from agent configuration.
    pub fn new(
        name: String,
        cmd: String,
        output_flag: String,
        yolo_flag: Option<String>,
        can_commit: bool,
    ) -> Self {
        Self {
            name,
            cmd,
            output_flag,
            yolo_flag,
            can_commit,
            model_override: None,
            provider_override: None,
            context_level: default_context_level(),
        }
    }

    /// Set model override.
    pub fn with_model_override(mut self, model: Option<String>) -> Self {
        self.model_override = model;
        self
    }

    /// Set provider override.
    pub fn with_provider_override(mut self, provider: Option<String>) -> Self {
        self.provider_override = provider;
        self
    }

    /// Set context level.
    pub fn with_context_level(mut self, level: u8) -> Self {
        self.context_level = level;
        self
    }
}

/// Parameters for creating a new checkpoint.
///
/// Groups all the parameters needed to create a checkpoint, avoiding
/// functions with too many individual parameters.
pub struct CheckpointParams<'a> {
    /// Current pipeline phase
    pub phase: PipelinePhase,
    /// Current developer iteration number
    pub iteration: u32,
    /// Total developer iterations configured
    pub total_iterations: u32,
    /// Current reviewer pass number
    pub reviewer_pass: u32,
    /// Total reviewer passes configured
    pub total_reviewer_passes: u32,
    /// Display name of the developer agent
    pub developer_agent: &'a str,
    /// Display name of the reviewer agent
    pub reviewer_agent: &'a str,
    /// Snapshot of CLI arguments
    pub cli_args: CliArgsSnapshot,
    /// Snapshot of developer agent configuration
    pub developer_agent_config: AgentConfigSnapshot,
    /// Snapshot of reviewer agent configuration
    pub reviewer_agent_config: AgentConfigSnapshot,
    /// Current rebase state
    pub rebase_state: RebaseState,
    /// Git user name for commits (if overridden)
    pub git_user_name: Option<&'a str>,
    /// Git user email for commits (if overridden)
    pub git_user_email: Option<&'a str>,
    /// Unique identifier for this run (UUID v4)
    pub run_id: &'a str,
    /// Parent run ID if this is a resumed session
    pub parent_run_id: Option<&'a str>,
    /// Number of times this session has been resumed
    pub resume_count: u32,
    /// Actual completed developer iterations
    pub actual_developer_runs: u32,
    /// Actual completed reviewer passes
    pub actual_reviewer_runs: u32,
}

/// Rebase state tracking.
///
/// Tracks the state of rebase operations to enable
/// proper recovery from interruptions during rebase.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum RebaseState {
    /// Rebase not started yet
    #[default]
    NotStarted,
    /// Pre-development rebase in progress
    PreRebaseInProgress { upstream_branch: String },
    /// Pre-development rebase completed
    PreRebaseCompleted { commit_oid: String },
    /// Post-review rebase in progress
    PostRebaseInProgress { upstream_branch: String },
    /// Post-review rebase completed
    PostRebaseCompleted { commit_oid: String },
    /// Rebase has conflicts that need resolution
    HasConflicts { files: Vec<String> },
    /// Rebase failed
    Failed { error: String },
}

/// Pipeline phases for checkpoint tracking.
///
/// These phases represent the major stages of the Ralph pipeline.
/// Checkpoints are saved at phase boundaries to enable resume functionality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelinePhase {
    /// Rebase phase (synchronizing with upstream branch)
    Rebase,
    /// Planning phase (creating PLAN.md)
    Planning,
    /// Development/implementation phase
    Development,
    /// Review-fix cycles phase (N iterations of review + fix)
    Review,
    /// Fix phase (deprecated: kept for backward compatibility with old checkpoints)
    Fix,
    /// Verification review phase (deprecated: kept for backward compatibility with old checkpoints)
    ReviewAgain,
    /// Commit message generation
    CommitMessage,
    /// Final validation phase
    FinalValidation,
    /// Pipeline complete
    Complete,
    /// Before initial rebase
    PreRebase,
    /// During pre-rebase conflict resolution
    PreRebaseConflict,
    /// Before post-review rebase
    PostRebase,
    /// During post-review conflict resolution
    PostRebaseConflict,
    /// Pipeline was interrupted (e.g., by Ctrl+C)
    Interrupted,
}

impl std::fmt::Display for PipelinePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rebase => write!(f, "Rebase"),
            Self::Planning => write!(f, "Planning"),
            Self::Development => write!(f, "Development"),
            Self::Review => write!(f, "Review"),
            Self::Fix => write!(f, "Fix"),
            Self::ReviewAgain => write!(f, "Verification Review"),
            Self::CommitMessage => write!(f, "Commit Message Generation"),
            Self::FinalValidation => write!(f, "Final Validation"),
            Self::Complete => write!(f, "Complete"),
            Self::PreRebase => write!(f, "Pre-Rebase"),
            Self::PreRebaseConflict => write!(f, "Pre-Rebase Conflict"),
            Self::PostRebase => write!(f, "Post-Rebase"),
            Self::PostRebaseConflict => write!(f, "Post-Rebase Conflict"),
            Self::Interrupted => write!(f, "Interrupted"),
        }
    }
}

/// Enhanced pipeline checkpoint for resume functionality.
///
/// Contains comprehensive state needed to resume an interrupted pipeline
/// exactly where it left off, including CLI arguments, agent configurations,
/// rebase state, and file checksums for validation.
///
/// This is inspired by video game save states - capturing the complete
/// execution context to enable seamless recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCheckpoint {
    /// Checkpoint format version (for future compatibility)
    pub version: u32,

    // === Core pipeline state ===
    /// Current pipeline phase
    pub phase: PipelinePhase,
    /// Current iteration number (for developer iterations)
    pub iteration: u32,
    /// Total iterations configured
    pub total_iterations: u32,
    /// Current reviewer pass number
    pub reviewer_pass: u32,
    /// Total reviewer passes configured
    pub total_reviewer_passes: u32,

    // === Metadata ===
    /// Timestamp when checkpoint was saved
    pub timestamp: String,
    /// Developer agent display name
    pub developer_agent: String,
    /// Reviewer agent display name
    pub reviewer_agent: String,

    // === Enhanced state capture ===
    /// CLI argument snapshot
    pub cli_args: CliArgsSnapshot,
    /// Developer agent configuration snapshot
    pub developer_agent_config: AgentConfigSnapshot,
    /// Reviewer agent configuration snapshot
    pub reviewer_agent_config: AgentConfigSnapshot,
    /// Rebase state tracking
    pub rebase_state: RebaseState,

    // === Validation data ===
    /// Path to config file used for this run (if any)
    pub config_path: Option<String>,
    /// Checksum of config file (for validation on resume)
    pub config_checksum: Option<String>,
    /// Working directory when checkpoint was created
    pub working_dir: String,
    /// Checksum of PROMPT.md (for validation on resume)
    pub prompt_md_checksum: Option<String>,

    // === Additional state for exact restoration ===
    /// Git user name for commits (if overridden)
    pub git_user_name: Option<String>,
    /// Git user email for commits (if overridden)
    pub git_user_email: Option<String>,

    // === Run identification and lineage (v2+) ===
    /// Unique identifier for this run (UUID v4)
    pub run_id: String,
    /// Parent run ID if this is a resumed session
    pub parent_run_id: Option<String>,
    /// Number of times this session has been resumed
    pub resume_count: u32,

    // === Actual execution state (v2+) ===
    /// Actual number of developer iterations that completed
    pub actual_developer_runs: u32,
    /// Actual number of reviewer passes that completed
    pub actual_reviewer_runs: u32,

    // === Hardened resume state (v3+) ===
    /// Execution history tracking for idempotent recovery
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_history: Option<crate::checkpoint::ExecutionHistory>,
    /// File system state for validation on resume
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_system_state: Option<crate::checkpoint::FileSystemState>,
    /// Stored prompts used during this run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_history: Option<std::collections::HashMap<String, String>>,
}

impl PipelineCheckpoint {
    /// Create a new checkpoint with comprehensive state capture.
    ///
    /// This is the main constructor for creating checkpoints during pipeline execution.
    /// It captures all necessary state to enable exact restoration of the pipeline.
    ///
    /// # Arguments
    ///
    /// * `params` - All checkpoint parameters bundled in a struct
    pub fn from_params(params: CheckpointParams<'_>) -> Self {
        // Get current working directory
        let working_dir = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Calculate PROMPT.md checksum if it exists
        let prompt_md_checksum = calculate_file_checksum(Path::new("PROMPT.md"));

        Self {
            version: CHECKPOINT_VERSION,
            phase: params.phase,
            iteration: params.iteration,
            total_iterations: params.total_iterations,
            reviewer_pass: params.reviewer_pass,
            total_reviewer_passes: params.total_reviewer_passes,
            timestamp: timestamp(),
            developer_agent: params.developer_agent.to_string(),
            reviewer_agent: params.reviewer_agent.to_string(),
            cli_args: params.cli_args,
            developer_agent_config: params.developer_agent_config,
            reviewer_agent_config: params.reviewer_agent_config,
            rebase_state: params.rebase_state,
            config_path: None,     // Will be set by caller if needed
            config_checksum: None, // Will be set by caller if needed
            working_dir,
            prompt_md_checksum,
            git_user_name: params.git_user_name.map(String::from),
            git_user_email: params.git_user_email.map(String::from),
            // New v2 fields
            run_id: params.run_id.to_string(),
            parent_run_id: params.parent_run_id.map(String::from),
            resume_count: params.resume_count,
            actual_developer_runs: params.actual_developer_runs,
            actual_reviewer_runs: params.actual_reviewer_runs,
            // New v3 fields - initialize as None, will be populated by caller
            execution_history: None,
            file_system_state: None,
            prompt_history: None,
        }
    }

    /// Get a human-readable description of the checkpoint.
    ///
    /// Returns a string describing the current phase and progress,
    /// suitable for display to the user when resuming.
    pub fn description(&self) -> String {
        match self.phase {
            PipelinePhase::Rebase => "Rebase in progress".to_string(),
            PipelinePhase::Planning => {
                format!(
                    "Planning phase, iteration {}/{}",
                    self.iteration, self.total_iterations
                )
            }
            PipelinePhase::Development => {
                format!(
                    "Development iteration {}/{}",
                    self.iteration, self.total_iterations
                )
            }
            PipelinePhase::Review => "Initial review".to_string(),
            PipelinePhase::Fix => "Applying fixes".to_string(),
            PipelinePhase::ReviewAgain => {
                format!(
                    "Verification review {}/{}",
                    self.reviewer_pass, self.total_reviewer_passes
                )
            }
            PipelinePhase::CommitMessage => "Commit message generation".to_string(),
            PipelinePhase::FinalValidation => "Final validation".to_string(),
            PipelinePhase::Complete => "Pipeline complete".to_string(),
            PipelinePhase::PreRebase => "Pre-development rebase".to_string(),
            PipelinePhase::PreRebaseConflict => "Pre-rebase conflict resolution".to_string(),
            PipelinePhase::PostRebase => "Post-review rebase".to_string(),
            PipelinePhase::PostRebaseConflict => "Post-rebase conflict resolution".to_string(),
            PipelinePhase::Interrupted => {
                format!(
                    "Interrupted during {} (iteration {}/{})",
                    if self.iteration > 0 {
                        "development"
                    } else {
                        "pipeline"
                    },
                    self.iteration,
                    self.total_iterations
                )
            }
        }
    }

    /// Set the config path and calculate its checksum.
    pub fn with_config(mut self, path: Option<std::path::PathBuf>) -> Self {
        if let Some(p) = path {
            self.config_path = Some(p.to_string_lossy().to_string());
            self.config_checksum = calculate_file_checksum(&p);
        }
        self
    }
}

/// Try to load a checkpoint, handling v1, v2, and v3 formats.
fn load_checkpoint_with_fallback(
    content: &str,
) -> Result<PipelineCheckpoint, Box<dyn std::error::Error>> {
    // Try v3 format first (current)
    match serde_json::from_str::<PipelineCheckpoint>(content) {
        Ok(mut checkpoint) => {
            // Accept v3 (current) or higher
            if checkpoint.version >= 3 {
                return Ok(checkpoint);
            }
            // v2 or v1 checkpoint parsed successfully as v3 struct - upgrade version
            // and add v3 defaults
            checkpoint.version = CHECKPOINT_VERSION;
            return Ok(checkpoint);
        }
        Err(_e) => {
            // First parse attempt failed, try v2 format below
        }
    }

    // If v3 struct parsing failed, try v2 format and migrate to v3
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct V2Checkpoint {
        version: u32,
        phase: PipelinePhase,
        iteration: u32,
        total_iterations: u32,
        reviewer_pass: u32,
        total_reviewer_passes: u32,
        timestamp: String,
        developer_agent: String,
        reviewer_agent: String,
        cli_args: CliArgsSnapshot,
        developer_agent_config: AgentConfigSnapshot,
        reviewer_agent_config: AgentConfigSnapshot,
        rebase_state: RebaseState,
        config_path: Option<String>,
        config_checksum: Option<String>,
        working_dir: String,
        prompt_md_checksum: Option<String>,
        git_user_name: Option<String>,
        git_user_email: Option<String>,
        run_id: String,
        parent_run_id: Option<String>,
        resume_count: u32,
        actual_developer_runs: u32,
        actual_reviewer_runs: u32,
    }

    if let Ok(v2) = serde_json::from_str::<V2Checkpoint>(content) {
        // Migrate v2 to v3: add new hardened resume fields (empty defaults)
        // Note: working_dir from v2 checkpoint is preserved
        return Ok(PipelineCheckpoint {
            version: CHECKPOINT_VERSION,
            phase: v2.phase,
            iteration: v2.iteration,
            total_iterations: v2.total_iterations,
            reviewer_pass: v2.reviewer_pass,
            total_reviewer_passes: v2.total_reviewer_passes,
            timestamp: v2.timestamp,
            developer_agent: v2.developer_agent,
            reviewer_agent: v2.reviewer_agent,
            cli_args: v2.cli_args,
            developer_agent_config: v2.developer_agent_config,
            reviewer_agent_config: v2.reviewer_agent_config,
            rebase_state: v2.rebase_state,
            config_path: v2.config_path,
            config_checksum: v2.config_checksum,
            working_dir: v2.working_dir,
            prompt_md_checksum: v2.prompt_md_checksum,
            git_user_name: v2.git_user_name,
            git_user_email: v2.git_user_email,
            run_id: v2.run_id,
            parent_run_id: v2.parent_run_id,
            resume_count: v2.resume_count,
            actual_developer_runs: v2.actual_developer_runs,
            actual_reviewer_runs: v2.actual_reviewer_runs,
            // New v3 fields - use empty defaults for migrated checkpoints
            execution_history: None,
            file_system_state: None,
            prompt_history: None,
        });
    }

    // Try v1 format and migrate to v3
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct V1Checkpoint {
        version: u32,
        phase: PipelinePhase,
        iteration: u32,
        total_iterations: u32,
        reviewer_pass: u32,
        total_reviewer_passes: u32,
        timestamp: String,
        developer_agent: String,
        reviewer_agent: String,
        cli_args: CliArgsSnapshot,
        developer_agent_config: AgentConfigSnapshot,
        reviewer_agent_config: AgentConfigSnapshot,
        rebase_state: RebaseState,
        config_path: Option<String>,
        config_checksum: Option<String>,
        working_dir: String,
        prompt_md_checksum: Option<String>,
        git_user_name: Option<String>,
        git_user_email: Option<String>,
    }

    if let Ok(v1) = serde_json::from_str::<V1Checkpoint>(content) {
        // Migrate v1 to v3: generate new run_id, set defaults for new fields
        let new_run_id = uuid::Uuid::new_v4().to_string();
        return Ok(PipelineCheckpoint {
            version: CHECKPOINT_VERSION,
            phase: v1.phase,
            iteration: v1.iteration,
            total_iterations: v1.total_iterations,
            reviewer_pass: v1.reviewer_pass,
            total_reviewer_passes: v1.total_reviewer_passes,
            timestamp: v1.timestamp,
            developer_agent: v1.developer_agent,
            reviewer_agent: v1.reviewer_agent,
            cli_args: v1.cli_args,
            developer_agent_config: v1.developer_agent_config,
            reviewer_agent_config: v1.reviewer_agent_config,
            rebase_state: v1.rebase_state,
            config_path: v1.config_path,
            config_checksum: v1.config_checksum,
            working_dir: v1.working_dir,
            prompt_md_checksum: v1.prompt_md_checksum,
            git_user_name: v1.git_user_name,
            git_user_email: v1.git_user_email,
            // New v2 fields - use defaults for migrated checkpoints
            run_id: new_run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: v1.iteration,
            actual_reviewer_runs: v1.reviewer_pass,
            // New v3 fields - use empty defaults for migrated checkpoints
            execution_history: None,
            file_system_state: None,
            prompt_history: None,
        });
    }

    // Try truly legacy format (pre-v1)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct LegacyCheckpoint {
        phase: PipelinePhase,
        iteration: u32,
        total_iterations: u32,
        reviewer_pass: u32,
        total_reviewer_passes: u32,
        timestamp: String,
        developer_agent: String,
        reviewer_agent: String,
    }

    if let Ok(legacy) = serde_json::from_str::<LegacyCheckpoint>(content) {
        let new_run_id = uuid::Uuid::new_v4().to_string();
        return Ok(PipelineCheckpoint {
            version: CHECKPOINT_VERSION,
            phase: legacy.phase,
            iteration: legacy.iteration,
            total_iterations: legacy.total_iterations,
            reviewer_pass: legacy.reviewer_pass,
            total_reviewer_passes: legacy.total_reviewer_passes,
            timestamp: legacy.timestamp,
            developer_agent: legacy.developer_agent.clone(),
            reviewer_agent: legacy.reviewer_agent.clone(),
            cli_args: CliArgsSnapshotBuilder::new(0, 0, String::new(), None, false, true).build(),
            developer_agent_config: AgentConfigSnapshot::new(
                legacy.developer_agent.clone(),
                String::new(),
                String::new(),
                None,
                false,
            ),
            reviewer_agent_config: AgentConfigSnapshot::new(
                legacy.reviewer_agent.clone(),
                String::new(),
                String::new(),
                None,
                false,
            ),
            rebase_state: RebaseState::default(),
            config_path: None,
            config_checksum: None,
            working_dir: String::new(),
            prompt_md_checksum: None,
            git_user_name: None,
            git_user_email: None,
            run_id: new_run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: legacy.iteration,
            actual_reviewer_runs: legacy.reviewer_pass,
            // New v3 fields - use empty defaults for migrated checkpoints
            execution_history: None,
            file_system_state: None,
            prompt_history: None,
        });
    }

    Err("Invalid checkpoint format".into())
}

/// Get current timestamp in "YYYY-MM-DD HH:MM:SS" format.
pub fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Save a pipeline checkpoint to disk.
///
/// Writes the checkpoint atomically by writing to a temp file first,
/// then renaming to the final path. This prevents corruption if the
/// process is interrupted during the write.
///
/// # Errors
///
/// Returns an error if serialization fails or the file cannot be written.
pub fn save_checkpoint(checkpoint: &PipelineCheckpoint) -> io::Result<()> {
    let json = serde_json::to_string_pretty(checkpoint).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize checkpoint: {e}"),
        )
    })?;

    // Ensure the .agent directory exists before attempting to write
    fs::create_dir_all(AGENT_DIR)?;

    // Write atomically by writing to temp file then renaming
    let checkpoint_path_str = checkpoint_path();
    let temp_path = format!("{checkpoint_path_str}.tmp");

    // Ensure temp file is cleaned up even if write or rename fails
    let write_result = fs::write(&temp_path, &json);
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return write_result;
    }

    let rename_result = fs::rename(&temp_path, &checkpoint_path_str);
    if rename_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return rename_result;
    }

    Ok(())
}

/// Load an existing checkpoint if one exists.
///
/// Returns `Ok(Some(checkpoint))` if a valid checkpoint was loaded,
/// `Ok(None)` if no checkpoint file exists, or an error if the file
/// exists but cannot be parsed.
///
/// # Errors
///
/// Returns an error if the checkpoint file exists but cannot be read
/// or contains invalid JSON.
///
/// # Note
///
/// This function handles both new format (v1) and legacy checkpoints
/// for backward compatibility. Legacy checkpoints are migrated to the
/// new format automatically.
pub fn load_checkpoint() -> io::Result<Option<PipelineCheckpoint>> {
    let checkpoint = checkpoint_path();
    let path = Path::new(&checkpoint);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let loaded_checkpoint = load_checkpoint_with_fallback(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse checkpoint: {e}"),
        )
    })?;

    Ok(Some(loaded_checkpoint))
}

/// Delete the checkpoint file.
///
/// Called on successful pipeline completion to clean up the checkpoint.
/// Does nothing if the checkpoint file doesn't exist.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be deleted.
pub fn clear_checkpoint() -> io::Result<()> {
    let checkpoint = checkpoint_path();
    let path = Path::new(&checkpoint);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Check if a checkpoint exists.
///
/// Returns `true` if a checkpoint file exists, `false` otherwise.
pub fn checkpoint_exists() -> bool {
    Path::new(&checkpoint_path()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::with_temp_cwd;

    /// Helper function to create a checkpoint for testing.
    fn make_test_checkpoint(phase: PipelinePhase, iteration: u32) -> PipelineCheckpoint {
        let cli_args = CliArgsSnapshot::new(
            5,
            2,
            "test commit".to_string(),
            None,
            false,
            true,
            2,
            false,
            None,
        );
        let dev_config =
            AgentConfigSnapshot::new("claude".into(), "cmd".into(), "-o".into(), None, true);
        let rev_config =
            AgentConfigSnapshot::new("codex".into(), "cmd".into(), "-o".into(), None, true);
        let run_id = uuid::Uuid::new_v4().to_string();
        PipelineCheckpoint::from_params(CheckpointParams {
            phase,
            iteration,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args,
            developer_agent_config: dev_config,
            reviewer_agent_config: rev_config,
            rebase_state: RebaseState::default(),
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: iteration,
            actual_reviewer_runs: 0,
        })
    }

    #[test]
    fn test_timestamp_format() {
        let ts = timestamp();
        assert!(ts.contains('-'));
        assert!(ts.contains(':'));
        assert_eq!(ts.len(), 19);
    }

    #[test]
    fn test_pipeline_phase_display() {
        assert_eq!(format!("{}", PipelinePhase::Rebase), "Rebase");
        assert_eq!(format!("{}", PipelinePhase::Planning), "Planning");
        assert_eq!(format!("{}", PipelinePhase::Development), "Development");
        assert_eq!(format!("{}", PipelinePhase::Review), "Review");
        assert_eq!(format!("{}", PipelinePhase::Fix), "Fix");
        assert_eq!(
            format!("{}", PipelinePhase::ReviewAgain),
            "Verification Review"
        );
        assert_eq!(
            format!("{}", PipelinePhase::CommitMessage),
            "Commit Message Generation"
        );
        assert_eq!(
            format!("{}", PipelinePhase::FinalValidation),
            "Final Validation"
        );
        assert_eq!(format!("{}", PipelinePhase::Complete), "Complete");
        assert_eq!(format!("{}", PipelinePhase::PreRebase), "Pre-Rebase");
        assert_eq!(
            format!("{}", PipelinePhase::PreRebaseConflict),
            "Pre-Rebase Conflict"
        );
        assert_eq!(format!("{}", PipelinePhase::PostRebase), "Post-Rebase");
        assert_eq!(
            format!("{}", PipelinePhase::PostRebaseConflict),
            "Post-Rebase Conflict"
        );
        assert_eq!(format!("{}", PipelinePhase::Interrupted), "Interrupted");
    }

    #[test]
    fn test_checkpoint_from_params() {
        let cli_args =
            CliArgsSnapshot::new(5, 2, "test".to_string(), None, false, true, 2, false, None);
        let dev_config =
            AgentConfigSnapshot::new("claude".into(), "cmd".into(), "-o".into(), None, true);
        let rev_config =
            AgentConfigSnapshot::new("codex".into(), "cmd".into(), "-o".into(), None, true);
        let run_id = uuid::Uuid::new_v4().to_string();
        let checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args,
            developer_agent_config: dev_config,
            reviewer_agent_config: rev_config,
            rebase_state: RebaseState::default(),
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: 2,
            actual_reviewer_runs: 0,
        });

        assert_eq!(checkpoint.phase, PipelinePhase::Development);
        assert_eq!(checkpoint.iteration, 2);
        assert_eq!(checkpoint.total_iterations, 5);
        assert_eq!(checkpoint.reviewer_pass, 0);
        assert_eq!(checkpoint.total_reviewer_passes, 2);
        assert_eq!(checkpoint.developer_agent, "claude");
        assert_eq!(checkpoint.reviewer_agent, "codex");
        assert_eq!(checkpoint.version, CHECKPOINT_VERSION);
        assert!(!checkpoint.timestamp.is_empty());
        assert_eq!(checkpoint.run_id, run_id);
        assert_eq!(checkpoint.resume_count, 0);
        assert_eq!(checkpoint.actual_developer_runs, 2);
        assert!(checkpoint.parent_run_id.is_none());
    }

    #[test]
    fn test_checkpoint_description() {
        let checkpoint = make_test_checkpoint(PipelinePhase::Development, 3);
        assert_eq!(checkpoint.description(), "Development iteration 3/5");

        let run_id = uuid::Uuid::new_v4().to_string();
        let checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
            phase: PipelinePhase::ReviewAgain,
            iteration: 5,
            total_iterations: 5,
            reviewer_pass: 2,
            total_reviewer_passes: 3,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args: CliArgsSnapshot::new(
                5,
                3,
                "test".to_string(),
                None,
                false,
                true,
                2,
                false,
                None,
            ),
            developer_agent_config: AgentConfigSnapshot::new(
                "claude".into(),
                "cmd".into(),
                "-o".into(),
                None,
                true,
            ),
            reviewer_agent_config: AgentConfigSnapshot::new(
                "codex".into(),
                "cmd".into(),
                "-o".into(),
                None,
                true,
            ),
            rebase_state: RebaseState::default(),
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: 5,
            actual_reviewer_runs: 2,
        });
        assert_eq!(checkpoint.description(), "Verification review 2/3");
    }

    #[test]
    fn test_checkpoint_save_load() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let checkpoint = make_test_checkpoint(PipelinePhase::Review, 5);

            save_checkpoint(&checkpoint).unwrap();
            assert!(checkpoint_exists());

            let loaded = load_checkpoint()
                .unwrap()
                .expect("checkpoint should exist after save_checkpoint");
            assert_eq!(loaded.phase, PipelinePhase::Review);
            assert_eq!(loaded.iteration, 5);
            assert_eq!(loaded.developer_agent, "claude");
            assert_eq!(loaded.reviewer_agent, "codex");
            assert_eq!(loaded.version, CHECKPOINT_VERSION);
        });
    }

    #[test]
    fn test_checkpoint_clear() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let checkpoint = make_test_checkpoint(PipelinePhase::Development, 1);

            save_checkpoint(&checkpoint).unwrap();
            assert!(checkpoint_exists());

            clear_checkpoint().unwrap();
            assert!(!checkpoint_exists());
        });
    }

    #[test]
    fn test_load_checkpoint_nonexistent() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let result = load_checkpoint().unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_checkpoint_serialization() {
        let run_id = uuid::Uuid::new_v4().to_string();
        let checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
            phase: PipelinePhase::Fix,
            iteration: 3,
            total_iterations: 5,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            developer_agent: "aider",
            reviewer_agent: "opencode",
            cli_args: CliArgsSnapshot::new(
                5,
                2,
                "fix".to_string(),
                Some("standard".into()),
                false,
                true,
                2,
                false,
                None,
            ),
            developer_agent_config: AgentConfigSnapshot::new(
                "aider".into(),
                "aider".into(),
                "-o".into(),
                Some("--yes".into()),
                true,
            ),
            reviewer_agent_config: AgentConfigSnapshot::new(
                "opencode".into(),
                "opencode".into(),
                "-o".into(),
                None,
                false,
            ),
            rebase_state: RebaseState::PreRebaseCompleted {
                commit_oid: "abc123".into(),
            },
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: 3,
            actual_reviewer_runs: 1,
        });

        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains("Fix"));
        assert!(json.contains("aider"));
        assert!(json.contains("opencode"));
        assert!(json.contains("\"version\":"));

        let deserialized: PipelineCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.phase, checkpoint.phase);
        assert_eq!(deserialized.iteration, checkpoint.iteration);
        assert_eq!(deserialized.cli_args.developer_iters, 5);
        assert_eq!(deserialized.cli_args.commit_msg, "fix");
        assert!(matches!(
            deserialized.rebase_state,
            RebaseState::PreRebaseCompleted { .. }
        ));
        assert_eq!(deserialized.run_id, run_id);
        assert_eq!(deserialized.actual_developer_runs, 3);
        assert_eq!(deserialized.actual_reviewer_runs, 1);
    }

    #[test]
    fn test_cli_args_snapshot() {
        let snapshot = CliArgsSnapshot::new(
            10,
            3,
            "feat: new feature".to_string(),
            Some("comprehensive".into()),
            true,
            true,
            3,
            true,
            Some("claude".to_string()),
        );

        assert_eq!(snapshot.developer_iters, 10);
        assert_eq!(snapshot.reviewer_reviews, 3);
        assert_eq!(snapshot.commit_msg, "feat: new feature");
        assert_eq!(snapshot.review_depth, Some("comprehensive".to_string()));
        assert!(snapshot.skip_rebase);
        assert!(snapshot.isolation_mode);
        assert_eq!(snapshot.verbosity, 3);
        assert!(snapshot.show_streaming_metrics);
        assert_eq!(snapshot.reviewer_json_parser, Some("claude".to_string()));
    }

    #[test]
    fn test_agent_config_snapshot() {
        let config = AgentConfigSnapshot::new(
            "test-agent".into(),
            "/usr/bin/test".into(),
            "--output".into(),
            Some("--yolo".into()),
            false,
        );

        assert_eq!(config.name, "test-agent");
        assert_eq!(config.cmd, "/usr/bin/test");
        assert_eq!(config.output_flag, "--output");
        assert_eq!(config.yolo_flag, Some("--yolo".to_string()));
        assert!(!config.can_commit);
    }

    #[test]
    fn test_rebase_state() {
        let state = RebaseState::PreRebaseInProgress {
            upstream_branch: "main".into(),
        };
        assert!(matches!(state, RebaseState::PreRebaseInProgress { .. }));

        let state = RebaseState::Failed {
            error: "conflict".into(),
        };
        assert!(matches!(state, RebaseState::Failed { .. }));
    }

    #[test]
    fn test_calculate_file_checksum() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            // Create a test file in current directory (temp cwd)
            let test_file = Path::new("test.txt");
            fs::write(&test_file, "test content").unwrap();

            let checksum1 = calculate_file_checksum(test_file);
            assert!(checksum1.is_some());

            // Same content should give same checksum
            let checksum2 = calculate_file_checksum(test_file);
            assert_eq!(checksum1, checksum2);

            // Different content should give different checksum
            fs::write(&test_file, "different content").unwrap();
            let checksum3 = calculate_file_checksum(test_file);
            assert_ne!(checksum1, checksum3);

            // Non-existent file should return None
            let nonexistent = Path::new("nonexistent.txt");
            assert!(calculate_file_checksum(nonexistent).is_none());
        });
    }

    #[test]
    fn test_load_checkpoint_preserves_working_dir() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let json = r#"{
                "version": 1,
                "phase": "Development",
                "iteration": 1,
                "total_iterations": 1,
                "reviewer_pass": 0,
                "total_reviewer_passes": 0,
                "timestamp": "2024-01-01 12:00:00",
                "developer_agent": "test-agent",
                "reviewer_agent": "test-agent",
                "cli_args": {
                    "developer_iters": 1,
                    "reviewer_reviews": 0,
                    "commit_msg": "",
                    "review_depth": null,
                    "skip_rebase": false
                },
                "developer_agent_config": {
                    "name": "test-agent",
                    "cmd": "echo",
                    "output_flag": "",
                    "yolo_flag": null,
                    "can_commit": false,
                    "model_override": null,
                    "provider_override": null,
                    "context_level": 1
                },
                "reviewer_agent_config": {
                    "name": "test-agent",
                    "cmd": "echo",
                    "output_flag": "",
                    "yolo_flag": null,
                    "can_commit": false,
                    "model_override": null,
                    "provider_override": null,
                    "context_level": 1
                },
                "rebase_state": "NotStarted",
                "config_path": null,
                "config_checksum": null,
                "working_dir": "/some/other/directory",
                "prompt_md_checksum": null,
                "git_user_name": null,
                "git_user_email": null
            }"#;

            fs::write(".agent/checkpoint.json", json).unwrap();

            let loaded = load_checkpoint().unwrap().expect("should load checkpoint");
            assert_eq!(
                loaded.working_dir, "/some/other/directory",
                "working_dir should be preserved from JSON"
            );
        });
    }
}
