// Pipeline Initialization
//
// This module handles the initialization phase of the pipeline, including:
// - Context preparation and configuration
// - Run log context creation and checkpoint restoration
// - Early-exit conditions (dry-run, rebase-only, generate-commit-msg)
// - Template context setup
//
// Architecture:
//
// The initialization phase occurs before the main event loop. It:
// 1. Ensures required files and directories exist via AppEffectHandler
// 2. Creates or restores RunLogContext for per-run logging
// 3. Handles special modes (dry-run, rebase-only, commit generation)
// 4. Builds PipelineContext with all necessary dependencies
//
// Early Exit Modes:
//
// Several CLI flags cause early exit after preparation:
// - `--dry-run`: Displays pipeline configuration without executing
// - `--rebase-only`: Runs rebase operation without full pipeline
// - `--generate-commit-msg`: Generates commit message without pipeline
//
// These modes return `Ok(None)` to signal early exit to the caller.

/// Parameters for preparing the pipeline context.
///
/// Groups related parameters to avoid too many function arguments.
pub(super) struct PipelinePreparationParams<'a, H: effect::AppEffectHandler> {
    pub args: Args,
    pub config: crate::config::Config,
    pub registry: AgentRegistry,
    pub developer_agent: String,
    pub reviewer_agent: String,
    pub repo_root: std::path::PathBuf,
    pub logger: Logger,
    pub colors: Colors,
    pub executor: std::sync::Arc<dyn ProcessExecutor>,
    pub handler: &'a mut H,
    /// Workspace for explicit path resolution.
    ///
    /// Production code passes `Arc::new(WorkspaceFs::new(...))`.
    /// Tests can pass `Arc::new(MemoryWorkspace::new(...))`.
    pub workspace: std::sync::Arc<dyn crate::workspace::Workspace>,
}

struct PipelinePreparationState {
    args: Args,
    config: crate::config::Config,
    registry: AgentRegistry,
    developer_agent: String,
    reviewer_agent: String,
    repo_root: std::path::PathBuf,
    logger: Logger,
    colors: Colors,
    executor: std::sync::Arc<dyn ProcessExecutor>,
    workspace: std::sync::Arc<dyn crate::workspace::Workspace>,
}

impl PipelinePreparationState {
    fn build_pipeline_context(self, template_context: TemplateContext, run_log_context: crate::logging::RunLogContext) -> PipelineContext {
        let developer_display = self.registry.display_name(&self.developer_agent);
        let reviewer_display = self.registry.display_name(&self.reviewer_agent);

        PipelineContext {
            args: self.args,
            config: self.config,
            registry: self.registry,
            developer_agent: self.developer_agent,
            reviewer_agent: self.reviewer_agent,
            developer_display,
            reviewer_display,
            repo_root: self.repo_root,
            workspace: self.workspace,
            logger: self.logger,
            colors: self.colors,
            template_context,
            executor: self.executor,
            run_log_context,
        }
    }
}

fn create_resume_or_fresh_run_log_context(
    state: &PipelinePreparationState,
) -> anyhow::Result<crate::logging::RunLogContext> {
    use crate::checkpoint::{load_checkpoint_with_workspace, save_checkpoint_with_workspace};
    use crate::logging::RunLogContext;

    if !state.args.recovery.resume {
        return RunLogContext::new(&*state.workspace).context("Failed to create run log context");
    }

    let checkpoint = load_checkpoint_with_workspace(&*state.workspace)
        .context("Failed to load checkpoint for resume")?;

    if let Some(mut checkpoint) = checkpoint {
        if let Some(log_run_id) = checkpoint.log_run_id {
            return RunLogContext::from_checkpoint(&log_run_id, &*state.workspace)
                .context("Failed to restore run log context from checkpoint");
        }

        state
            .logger
            .warn("Checkpoint missing log_run_id field, generating new run log directory");
        let run_log_context =
            RunLogContext::new(&*state.workspace).context("Failed to create run log context")?;

        checkpoint.log_run_id = Some(run_log_context.run_id().to_string());
        save_checkpoint_with_workspace(&*state.workspace, &checkpoint).context(
            "Failed to update checkpoint with log_run_id. Log continuity requires this update to succeed. \
             Please check filesystem permissions and disk space, then retry.",
        )?;

        return Ok(run_log_context);
    }

    state.logger.warn(
        "No checkpoint file found (--resume flag was set). A fresh run directory has been created. \
         If you expected to resume from a previous run, please check that .agent/checkpoint.json exists.",
    );
    RunLogContext::new(&*state.workspace).context("Failed to create run log context")
}

fn configure_logger_for_run(
    state: &mut PipelinePreparationState,
    run_log_context: &crate::logging::RunLogContext,
) {
    let current_logger = std::mem::replace(&mut state.logger, Logger::new(state.colors));
    state.logger = current_logger.with_workspace_log(
        std::sync::Arc::clone(&state.workspace),
        &run_log_context.pipeline_log().to_string_lossy(),
    );
}

fn write_run_metadata_best_effort(
    state: &PipelinePreparationState,
    run_log_context: &crate::logging::RunLogContext,
) {
    let run_metadata = crate::logging::RunMetadata {
        run_id: run_log_context.run_id().to_string(),
        started_at_utc: chrono::Utc::now().to_rfc3339(),
        command: format!(
            "ralph {}",
            std::env::args().skip(1).collect::<Vec<_>>().join(" ")
        ),
        resume: state.args.recovery.resume,
        repo_root: state.repo_root.display().to_string(),
        ralph_version: env!("CARGO_PKG_VERSION").to_string(),
        pid: Some(std::process::id()),
        config_summary: None,
    };

    if let Err(e) = run_log_context.write_run_metadata(&*state.workspace, &run_metadata) {
        state.logger.warn(&format!("Failed to write run.json: {e}"));
    }
}

fn handle_early_exit_modes(
    state: &PipelinePreparationState,
    template_context: &TemplateContext,
) -> anyhow::Result<bool> {
    if state.args.recovery.dry_run {
        let developer_display = state.registry.display_name(&state.developer_agent);
        let reviewer_display = state.registry.display_name(&state.reviewer_agent);
        handle_dry_run(
            &state.logger,
            state.colors,
            &state.config,
            &developer_display,
            &reviewer_display,
            &state.repo_root,
            &*state.workspace,
        )?;
        return Ok(true);
    }

    if state.args.rebase_flags.rebase_only {
        handle_rebase_only(
            &state.args,
            &state.config,
            template_context,
            &state.logger,
            state.colors,
            &state.executor,
            &state.repo_root,
        )?;
        return Ok(true);
    }

    if state.args.commit_plumbing.generate_commit_msg {
        handle_generate_commit_msg(&plumbing::CommitGenerationConfig {
            config: &state.config,
            template_context,
            workspace: &*state.workspace,
            workspace_arc: std::sync::Arc::clone(&state.workspace),
            registry: &state.registry,
            logger: &state.logger,
            colors: state.colors,
            developer_agent: &state.developer_agent,
            reviewer_agent: &state.reviewer_agent,
            executor: std::sync::Arc::clone(&state.executor),
        })?;
        return Ok(true);
    }

    Ok(false)
}

/// Prepares the pipeline context after agent validation.
///
/// Returns `Some(ctx)` if pipeline should run, or `None` if we should exit early.
///
/// # Early Exit Conditions
///
/// Returns `None` (early exit) for:
/// - `--dry-run`: Displays configuration without executing
/// - `--rebase-only`: Runs rebase operation only
/// - `--generate-commit-msg`: Generates commit message only
///
/// # Run Log Context
///
/// Creates or restores per-run logging directory:
/// - Fresh run: Generates new `run_id` UUID
/// - Resume: Restores `log_run_id` from checkpoint for continuity
/// - Missing `log_run_id` in checkpoint: Generates new and updates checkpoint
///
/// # Errors
///
/// Returns error if:
/// - Required files/directories cannot be created
/// - Run log context creation fails
/// - Checkpoint save fails (when updating `log_run_id`)
pub(super) fn prepare_pipeline_or_exit<H: effect::AppEffectHandler>(
    params: PipelinePreparationParams<'_, H>,
) -> anyhow::Result<Option<PipelineContext>> {
    let PipelinePreparationParams {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        logger,
        colors,
        executor,
        handler,
        workspace,
    } = params;

    effectful::ensure_files_effectful(handler, config.isolation_mode)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if config.isolation_mode {
        effectful::reset_context_for_isolation_effectful(handler)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    let mut state = PipelinePreparationState {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        logger,
        colors,
        executor,
        workspace,
    };

    let run_log_context = create_resume_or_fresh_run_log_context(&state)?;
    configure_logger_for_run(&mut state, &run_log_context);
    write_run_metadata_best_effort(&state, &run_log_context);

    let template_context =
        TemplateContext::from_user_templates_dir(state.config.user_templates_dir().cloned());

    if handle_early_exit_modes(&state, &template_context)? {
        return Ok(None);
    }

    Ok(Some(
        state.build_pipeline_context(template_context, run_log_context),
    ))
}
