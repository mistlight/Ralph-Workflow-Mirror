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
    /// Logging run_id (timestamp-based) for per-run log directory
    /// Format: YYYY-MM-DD_HH-mm-ss.SSSZ[-NN]
    /// This is separate from run_id which is a UUID v4
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_run_id: Option<String>,

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
    /// Environment snapshot for idempotent recovery
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_snapshot: Option<EnvironmentSnapshot>,

    /// Reducer-managed prompt input materialization state.
    ///
    /// This allows resumed pipelines to avoid re-materializing oversize inputs
    /// (and re-emitting oversize warnings) when the underlying content and
    /// consumer signature are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_inputs: Option<crate::reducer::state::PromptInputsState>,
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
            config_path: params.config_path,
            config_checksum: params.config_checksum,
            working_dir: params.working_dir,
            prompt_md_checksum: params.prompt_md_checksum,
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
            env_snapshot: None,
            prompt_inputs: None,
            log_run_id: None,
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
            PipelinePhase::Review => {
                if self.reviewer_pass > 0 {
                    format!(
                        "Verification review {}/{}",
                        self.reviewer_pass, self.total_reviewer_passes
                    )
                } else {
                    "Initial review".to_string()
                }
            }
            PipelinePhase::CommitMessage => "Commit message generation".to_string(),
            PipelinePhase::FinalValidation => "Final validation".to_string(),
            PipelinePhase::Complete => "Pipeline complete".to_string(),
            PipelinePhase::PreRebase => "Pre-development rebase".to_string(),
            PipelinePhase::PreRebaseConflict => "Pre-rebase conflict resolution".to_string(),
            PipelinePhase::PostRebase => "Post-review rebase".to_string(),
            PipelinePhase::PostRebaseConflict => "Post-rebase conflict resolution".to_string(),
            PipelinePhase::AwaitingDevFix => {
                "Awaiting development agent to fix pipeline failure".to_string()
            }
            PipelinePhase::Interrupted => {
                // Provide more detailed information for interrupted state
                let mut parts = vec!["Interrupted".to_string()];

                // Add context about what phase was interrupted
                if self.iteration > 0 && self.iteration < self.total_iterations {
                    parts.push(format!(
                        "during development (iteration {}/{})",
                        self.iteration, self.total_iterations
                    ));
                } else if self.iteration >= self.total_iterations {
                    if self.reviewer_pass > 0 {
                        parts.push(format!(
                            "during review (pass {}/{})",
                            self.reviewer_pass, self.total_reviewer_passes
                        ));
                    } else {
                        parts.push("after development phase".to_string());
                    }
                } else {
                    parts.push("during pipeline initialization".to_string());
                }

                parts.join(" ")
            }
        }
    }
}

/// Get current timestamp in "YYYY-MM-DD HH:MM:SS" format.
pub fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
