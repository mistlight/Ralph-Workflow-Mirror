//! Checkpoint builder for convenient checkpoint creation.
//!
//! This module provides a builder pattern for creating checkpoints
//! from various contexts in the pipeline.

use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::file_state::FileSystemState;
use crate::checkpoint::state::{
    AgentConfigSnapshot, CheckpointParams, CliArgsSnapshot, PipelineCheckpoint, PipelinePhase,
    RebaseState,
};
use crate::checkpoint::RunContext;
use crate::config::{Config, ReviewDepth};
use crate::logger::Logger;

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
    #[cfg(test)]
    pub fn agents(mut self, developer: &str, reviewer: &str) -> Self {
        self.developer_agent = Some(developer.to_string());
        self.reviewer_agent = Some(reviewer.to_string());
        self
    }

    /// Set the CLI arguments snapshot.
    #[cfg(test)]
    pub fn cli_args(mut self, args: CliArgsSnapshot) -> Self {
        self.cli_args = Some(args);
        self
    }

    /// Set the developer agent configuration snapshot.
    #[cfg(test)]
    pub fn developer_config(mut self, config: AgentConfigSnapshot) -> Self {
        self.developer_agent_config = Some(config);
        self
    }

    /// Set the reviewer agent configuration snapshot.
    #[cfg(test)]
    pub fn reviewer_config(mut self, config: AgentConfigSnapshot) -> Self {
        self.reviewer_agent_config = Some(config);
        self
    }

    /// Set the rebase state.
    #[cfg(test)]
    pub fn rebase_state(mut self, state: RebaseState) -> Self {
        self.rebase_state = state;
        self
    }

    /// Set the config path.
    #[cfg(test)]
    pub fn config_path(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.config_path = path;
        self
    }

    /// Set the git user name and email.
    #[cfg(test)]
    pub fn git_identity(mut self, name: Option<&str>, email: Option<&str>) -> Self {
        self.git_user_name = name.map(String::from);
        self.git_user_email = email.map(String::from);
        self
    }

    /// Capture CLI arguments from a Config.
    pub fn capture_cli_args(mut self, config: &Config) -> Self {
        let review_depth_str = review_depth_to_string(config.review_depth);

        let snapshot = CliArgsSnapshot::new(
            config.developer_iters,
            config.reviewer_reviews,
            config.commit_msg.clone(),
            review_depth_str,
            false, // skip_rebase - will be set from args if needed
        );
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

    /// Attach execution history from a PhaseContext.
    ///
    /// This method captures the execution history from the phase context
    /// and attaches it to the checkpoint.
    pub fn with_execution_history(mut self, history: ExecutionHistory) -> Self {
        self.execution_history = Some(history);
        self
    }

    /// Build the checkpoint.
    ///
    /// Returns None if required fields (phase, agent configs) are missing.
    /// Generates a new RunContext if not set.
    pub fn build(self) -> Option<PipelineCheckpoint> {
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
        });

        if let Some(path) = self.config_path {
            checkpoint = checkpoint.with_config(Some(path));
        }

        // Populate execution history
        checkpoint.execution_history = self.execution_history;

        // Populate prompt history
        checkpoint.prompt_history = self.prompt_history;

        // Capture and populate file system state
        checkpoint.file_system_state = Some(FileSystemState::capture_current());

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
mod tests {
    use super::*;

    #[test]
    fn test_builder_basic() {
        let cli_args = CliArgsSnapshot::new(5, 2, "test".into(), None, false);
        let dev_config =
            AgentConfigSnapshot::new("dev".into(), "cmd".into(), "-o".into(), None, true);
        let rev_config =
            AgentConfigSnapshot::new("rev".into(), "cmd".into(), "-o".into(), None, true);

        let checkpoint = CheckpointBuilder::new()
            .phase(PipelinePhase::Development, 2, 5)
            .reviewer_pass(1, 2)
            .agents("dev", "rev")
            .cli_args(cli_args)
            .developer_config(dev_config)
            .reviewer_config(rev_config)
            .build()
            .unwrap();

        assert_eq!(checkpoint.phase, PipelinePhase::Development);
        assert_eq!(checkpoint.iteration, 2);
        assert_eq!(checkpoint.total_iterations, 5);
        assert_eq!(checkpoint.reviewer_pass, 1);
        assert_eq!(checkpoint.total_reviewer_passes, 2);
    }

    #[test]
    fn test_builder_missing_required_field() {
        // Missing phase - should return None
        let result = CheckpointBuilder::new().build();
        assert!(result.is_none());
    }

    #[test]
    fn test_review_depth_to_string() {
        assert_eq!(
            review_depth_to_string(ReviewDepth::Standard),
            Some("standard".to_string())
        );
        assert_eq!(
            review_depth_to_string(ReviewDepth::Comprehensive),
            Some("comprehensive".to_string())
        );
        assert_eq!(
            review_depth_to_string(ReviewDepth::Security),
            Some("security".to_string())
        );
        assert_eq!(
            review_depth_to_string(ReviewDepth::Incremental),
            Some("incremental".to_string())
        );
    }
}
