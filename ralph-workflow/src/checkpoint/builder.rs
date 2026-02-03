//! Checkpoint builder for convenient checkpoint creation.
//!
//! This module provides a builder pattern for creating checkpoints
//! from various contexts in the pipeline.

use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::file_state::FileSystemState;
use crate::checkpoint::state::{
    calculate_file_checksum_with_workspace, AgentConfigSnapshot, CheckpointParams, CliArgsSnapshot,
    PipelineCheckpoint, PipelinePhase, RebaseState,
};
use crate::checkpoint::RunContext;
use crate::config::{Config, ReviewDepth};
use crate::executor::ProcessExecutor;
use crate::logger::Logger;
use crate::reducer::state::PromptInputsState;
use crate::workspace::Workspace;
use std::sync::Arc;

/// Builder for creating pipeline checkpoints.
///
/// Provides a convenient interface for capturing all necessary state
/// when creating checkpoints during pipeline execution.
///
/// # Example
///
/// ```ignore
/// let checkpoint = CheckpointBuilder::new()
///     .phase(PipelinePhase::Development, 3, 5)
///     .reviewer_pass(1, 2)
///     .capture_from_config(&ctx, &registry, "claude", "codex")
///     .build();
/// ```
pub struct CheckpointBuilder {
    phase: Option<PipelinePhase>,
    iteration: u32,
    total_iterations: u32,
    reviewer_pass: u32,
    total_reviewer_passes: u32,
    developer_agent: Option<String>,
    reviewer_agent: Option<String>,
    cli_args: Option<CliArgsSnapshot>,
    developer_agent_config: Option<AgentConfigSnapshot>,
    reviewer_agent_config: Option<AgentConfigSnapshot>,
    rebase_state: RebaseState,
    config_path: Option<std::path::PathBuf>,
    git_user_name: Option<String>,
    git_user_email: Option<String>,
    // Run context for tracking execution lineage and state
    run_context: Option<RunContext>,
    // Hardened resume fields
    execution_history: Option<ExecutionHistory>,
    prompt_history: Option<std::collections::HashMap<String, String>>,
    prompt_inputs: Option<PromptInputsState>,
    // Process executor for external process execution
    executor: Option<Arc<dyn ProcessExecutor>>,
}

impl Default for CheckpointBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointBuilder {
    /// Create a new checkpoint builder with default values.
    pub fn new() -> Self {
        Self {
            phase: None,
            iteration: 1,
            total_iterations: 1,
            reviewer_pass: 0,
            total_reviewer_passes: 0,
            developer_agent: None,
            reviewer_agent: None,
            cli_args: None,
            developer_agent_config: None,
            reviewer_agent_config: None,
            rebase_state: RebaseState::default(),
            config_path: None,
            git_user_name: None,
            git_user_email: None,
            run_context: None,
            execution_history: None,
            prompt_history: None,
            prompt_inputs: None,
            executor: None,
        }
    }

    /// Set the phase and iteration information.
    pub fn phase(mut self, phase: PipelinePhase, iteration: u32, total_iterations: u32) -> Self {
        self.phase = Some(phase);
        self.iteration = iteration;
        self.total_iterations = total_iterations;
        self
    }

    /// Set the reviewer pass information.
    pub fn reviewer_pass(mut self, pass: u32, total: u32) -> Self {
        self.reviewer_pass = pass;
        self.total_reviewer_passes = total;
        self
    }

    /// Set the agent names.
    pub fn agents(mut self, developer: &str, reviewer: &str) -> Self {
        self.developer_agent = Some(developer.to_string());
        self.reviewer_agent = Some(reviewer.to_string());
        self
    }

    /// Set the CLI arguments snapshot.
    pub fn cli_args(mut self, args: CliArgsSnapshot) -> Self {
        self.cli_args = Some(args);
        self
    }

    /// Set the developer agent configuration snapshot.
    pub fn developer_config(mut self, config: AgentConfigSnapshot) -> Self {
        self.developer_agent_config = Some(config);
        self
    }

    /// Set the reviewer agent configuration snapshot.
    pub fn reviewer_config(mut self, config: AgentConfigSnapshot) -> Self {
        self.reviewer_agent_config = Some(config);
        self
    }

    /// Set the rebase state.
    pub fn rebase_state(mut self, state: RebaseState) -> Self {
        self.rebase_state = state;
        self
    }

    /// Set the config path.
    pub fn config_path(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.config_path = path;
        self
    }

    /// Set the git user name and email.
    pub fn git_identity(mut self, name: Option<&str>, email: Option<&str>) -> Self {
        self.git_user_name = name.map(String::from);
        self.git_user_email = email.map(String::from);
        self
    }

    /// Set the process executor for external process execution.
    pub fn with_executor(mut self, executor: Arc<dyn ProcessExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Capture CLI arguments from a Config.
    pub fn capture_cli_args(mut self, config: &Config) -> Self {
        let review_depth_str = review_depth_to_string(config.review_depth);
        let snapshot = crate::checkpoint::state::CliArgsSnapshotBuilder::new(
            config.developer_iters,
            config.reviewer_reviews,
            review_depth_str,
            config.isolation_mode,
        )
        .verbosity(config.verbosity as u8)
        .show_streaming_metrics(config.show_streaming_metrics)
        .reviewer_json_parser(config.reviewer_json_parser.clone())
        .build();
        self.cli_args = Some(snapshot);
        self
    }

    /// Capture all configuration from a PhaseContext and AgentRegistry.
    ///
    /// This is a convenience method that captures CLI args and both agent configs.
    /// It takes a PhaseContext which provides access to config, registry, and agents.
    pub fn capture_from_context(
        mut self,
        config: &Config,
        registry: &AgentRegistry,
        developer_name: &str,
        reviewer_name: &str,
        logger: &Logger,
        run_context: &RunContext,
    ) -> Self {
        // Store run context (cloned for builder ownership)
        self.run_context = Some(run_context.clone());

        // Capture CLI args
        self = self.capture_cli_args(config);

        // Capture developer agent config
        if let Some(agent_config) = registry.resolve_config(developer_name) {
            let snapshot = AgentConfigSnapshot::new(
                developer_name.to_string(),
                agent_config.cmd.clone(),
                agent_config.output_flag.clone(),
                Some(agent_config.yolo_flag.clone()),
                agent_config.can_commit,
            )
            .with_model_override(config.developer_model.clone())
            .with_provider_override(config.developer_provider.clone())
            .with_context_level(config.developer_context);
            self.developer_agent_config = Some(snapshot);
            self.developer_agent = Some(developer_name.to_string());
        } else {
            logger.warn(&format!(
                "Developer agent '{}' not found in registry",
                developer_name
            ));
        }

        // Capture reviewer agent config
        if let Some(agent_config) = registry.resolve_config(reviewer_name) {
            let snapshot = AgentConfigSnapshot::new(
                reviewer_name.to_string(),
                agent_config.cmd.clone(),
                agent_config.output_flag.clone(),
                Some(agent_config.yolo_flag.clone()),
                agent_config.can_commit,
            )
            .with_model_override(config.reviewer_model.clone())
            .with_provider_override(config.reviewer_provider.clone())
            .with_context_level(config.reviewer_context);
            self.reviewer_agent_config = Some(snapshot);
            self.reviewer_agent = Some(reviewer_name.to_string());
        } else {
            logger.warn(&format!(
                "Reviewer agent '{}' not found in registry",
                reviewer_name
            ));
        }

        // Capture git identity
        self.git_user_name = config.git_user_name.clone();
        self.git_user_email = config.git_user_email.clone();

        self
    }

    /// Set the executor from a PhaseContext.
    ///
    /// This is a convenience method that extracts the executor_arc from PhaseContext
    /// and sets it for the checkpoint builder.
    pub fn with_executor_from_context(mut self, executor_arc: Arc<dyn ProcessExecutor>) -> Self {
        self.executor = Some(executor_arc);
        self
    }

    /// Attach execution history from a PhaseContext.
    ///
    /// This method captures the execution history from the phase context
    /// and attaches it to the checkpoint.
    pub fn with_execution_history(mut self, history: ExecutionHistory) -> Self {
        self.execution_history = Some(history);
        self
    }

    /// Set the entire prompt history from a HashMap.
    ///
    /// This is useful when transferring prompts from a PhaseContext.
    ///
    /// # Arguments
    ///
    /// * `history` - HashMap of prompt keys to prompt text
    pub fn with_prompt_history(
        mut self,
        history: std::collections::HashMap<String, String>,
    ) -> Self {
        self.prompt_history = if history.is_empty() {
            None
        } else {
            Some(history)
        };
        self
    }

    /// Attach reducer-managed prompt input materialization state.
    ///
    /// This is used by reducer-driven checkpointing so resumes can avoid repeating
    /// oversize handling that was already materialized for a given content id and
    /// consumer signature.
    pub fn with_prompt_inputs(mut self, prompt_inputs: PromptInputsState) -> Self {
        let is_empty = prompt_inputs.planning.is_none()
            && prompt_inputs.development.is_none()
            && prompt_inputs.review.is_none()
            && prompt_inputs.commit.is_none()
            && prompt_inputs.xsd_retry_last_output.is_none();
        self.prompt_inputs = if is_empty { None } else { Some(prompt_inputs) };
        self
    }

    /// Build the checkpoint without workspace.
    ///
    /// Returns None if required fields (phase, agent configs) are missing.
    /// Generates a new RunContext if not set.
    ///
    /// This method uses CWD-relative file operations for file state capture.
    /// For pipeline code where a workspace is available, prefer `build_with_workspace()`.
    pub fn build(self) -> Option<PipelineCheckpoint> {
        self.build_internal(None)
    }

    /// Build the checkpoint with workspace-aware file capture.
    ///
    /// Returns None if required fields (phase, agent configs) are missing.
    /// Generates a new RunContext if not set.
    ///
    /// This method uses the workspace abstraction for file state capture, which is
    /// the preferred approach for pipeline code. The workspace provides:
    /// - Explicit path resolution relative to repo root
    /// - Testability via `MemoryWorkspace` in tests
    pub fn build_with_workspace(self, workspace: &dyn Workspace) -> Option<PipelineCheckpoint> {
        self.build_internal(Some(workspace))
    }

    /// Internal build implementation that handles both workspace and non-workspace cases.
    fn build_internal(self, workspace: Option<&dyn Workspace>) -> Option<PipelineCheckpoint> {
        let phase = self.phase?;
        let developer_agent = self.developer_agent?;
        let reviewer_agent = self.reviewer_agent?;
        let cli_args = self.cli_args?;
        let developer_config = self.developer_agent_config?;
        let reviewer_config = self.reviewer_agent_config?;

        let git_user_name = self.git_user_name.as_deref();
        let git_user_email = self.git_user_email.as_deref();

        // Use provided run context or generate a new one
        let run_context = self.run_context.unwrap_or_default();

        let working_dir = workspace
            .map(|ws| ws.root().to_string_lossy().to_string())
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_default();

        let prompt_md_checksum = workspace.and_then(|ws| {
            calculate_file_checksum_with_workspace(ws, std::path::Path::new("PROMPT.md"))
        });

        let (config_path, config_checksum) = if let Some(path) = self.config_path {
            let path_string = path.to_string_lossy().to_string();
            let checksum = workspace.and_then(|ws| {
                let relative = path.strip_prefix(ws.root()).ok().unwrap_or(&path);
                calculate_file_checksum_with_workspace(ws, relative)
            });
            (Some(path_string), checksum)
        } else {
            (None, None)
        };

        let mut checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
            phase,
            iteration: self.iteration,
            total_iterations: self.total_iterations,
            reviewer_pass: self.reviewer_pass,
            total_reviewer_passes: self.total_reviewer_passes,
            developer_agent: &developer_agent,
            reviewer_agent: &reviewer_agent,
            cli_args,
            developer_agent_config: developer_config,
            reviewer_agent_config: reviewer_config,
            rebase_state: self.rebase_state,
            git_user_name,
            git_user_email,
            run_id: &run_context.run_id,
            parent_run_id: run_context.parent_run_id.as_deref(),
            resume_count: run_context.resume_count,
            actual_developer_runs: run_context.actual_developer_runs.max(self.iteration),
            actual_reviewer_runs: run_context.actual_reviewer_runs.max(self.reviewer_pass),
            working_dir,
            prompt_md_checksum,
            config_path,
            config_checksum,
        });

        // Populate execution history
        checkpoint.execution_history = self.execution_history;

        // Populate prompt history
        checkpoint.prompt_history = self.prompt_history;

        // Populate reducer prompt input materialization state
        checkpoint.prompt_inputs = self.prompt_inputs;

        // Capture and populate file system state
        // Use workspace-based capture when workspace is available (pipeline code),
        // fall back to CWD-based capture when not (CLI layer code).
        let executor_ref = self.executor.as_ref().map(|e| e.as_ref());
        checkpoint.file_system_state = if let Some(ws) = workspace {
            executor_ref.map(|executor| FileSystemState::capture_with_workspace(ws, executor))
        } else {
            Some(FileSystemState::capture_with_optional_executor_impl(
                executor_ref,
            ))
        };

        // Capture and populate environment snapshot
        checkpoint.env_snapshot =
            Some(crate::checkpoint::state::EnvironmentSnapshot::capture_current());

        Some(checkpoint)
    }
}

/// Convert ReviewDepth to a string representation.
fn review_depth_to_string(depth: ReviewDepth) -> Option<String> {
    match depth {
        ReviewDepth::Standard => Some("standard".to_string()),
        ReviewDepth::Comprehensive => Some("comprehensive".to_string()),
        ReviewDepth::Security => Some("security".to_string()),
        ReviewDepth::Incremental => Some("incremental".to_string()),
    }
}

#[cfg(test)]
mod tests;
