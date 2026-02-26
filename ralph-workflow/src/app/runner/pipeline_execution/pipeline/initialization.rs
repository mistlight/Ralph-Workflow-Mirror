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
    use crate::checkpoint::{load_checkpoint_with_workspace, save_checkpoint_with_workspace};
    use crate::logging::RunLogContext;

    let PipelinePreparationParams {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        mut logger,
        colors,
        executor,
        handler,
        workspace,
    } = params;

    // Ensure required files and directories exist via effects
    effectful::ensure_files_effectful(handler, config.isolation_mode)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Reset context for isolation mode via effects
    if config.isolation_mode {
        effectful::reset_context_for_isolation_effectful(handler)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    // Create run log context for per-run log directory
    // If resuming, continue with the same run_id from checkpoint; otherwise create new
    let run_log_context = if args.recovery.resume {
        // Try to load checkpoint to get log_run_id for resume continuity
        let checkpoint = load_checkpoint_with_workspace(&*workspace)
            .context("Failed to load checkpoint for resume")?;

        if let Some(mut checkpoint) = checkpoint {
            // Resume: continue logging to the same run directory using log_run_id
            if let Some(log_run_id) = checkpoint.log_run_id {
                RunLogContext::from_checkpoint(&log_run_id, &*workspace)
                    .context("Failed to restore run log context from checkpoint")?
            } else {
                // Older checkpoint without log_run_id field, generate new one
                logger
                    .warn("Checkpoint missing log_run_id field, generating new run log directory");
                let run_log_context =
                    RunLogContext::new(&*workspace).context("Failed to create run log context")?;

                // Update checkpoint with new log_run_id to ensure subsequent resumes continue with the same directory
                // This save MUST succeed for resume log continuity. If it fails, we error out rather than
                // proceeding with logs that will be fragmented on the next resume.
                checkpoint.log_run_id = Some(run_log_context.run_id().to_string());
                save_checkpoint_with_workspace(&*workspace, &checkpoint).context(
                    "Failed to update checkpoint with log_run_id. Log continuity requires this update to succeed. \
                     Please check filesystem permissions and disk space, then retry.",
                )?;

                run_log_context
            }
        } else {
            // No checkpoint found, but --resume was requested
            // This is handled later by resume validation, but we need a run context now
            logger.warn(
                "No checkpoint file found (--resume flag was set). A fresh run directory has been created. \
                 If you expected to resume from a previous run, please check that .agent/checkpoint.json exists.",
            );
            RunLogContext::new(&*workspace).context("Failed to create run log context")?
        }
    } else {
        // Fresh run: generate new run_id
        RunLogContext::new(&*workspace).context("Failed to create run log context")?
    };

    // Use per-run pipeline.log path via workspace (supports MemoryWorkspace in tests)
    logger = logger.with_workspace_log(
        std::sync::Arc::clone(&workspace),
        &run_log_context.pipeline_log().to_string_lossy(),
    );

    // Write run.json metadata
    let run_metadata = crate::logging::RunMetadata {
        run_id: run_log_context.run_id().to_string(),
        started_at_utc: chrono::Utc::now().to_rfc3339(),
        command: format!(
            "ralph {}",
            std::env::args().skip(1).collect::<Vec<_>>().join(" ")
        ),
        resume: args.recovery.resume,
        repo_root: repo_root.display().to_string(),
        ralph_version: env!("CARGO_PKG_VERSION").to_string(),
        pid: Some(std::process::id()),
        config_summary: None, // TODO: add public getters to Config if we want to include this
    };
    if let Err(e) = run_log_context.write_run_metadata(&*workspace, &run_metadata) {
        logger.warn(&format!("Failed to write run.json: {e}"));
    }

    // Handle --dry-run
    if args.recovery.dry_run {
        let developer_display = registry.display_name(&developer_agent);
        let reviewer_display = registry.display_name(&reviewer_agent);
        handle_dry_run(
            &logger,
            colors,
            &config,
            &developer_display,
            &reviewer_display,
            &repo_root,
            &*workspace,
        )?;
        return Ok(None);
    }

    // Create template context for user template overrides
    let template_context =
        TemplateContext::from_user_templates_dir(config.user_templates_dir().cloned());

    // Handle --rebase-only
    if args.rebase_flags.rebase_only {
        handle_rebase_only(
            &args,
            &config,
            &template_context,
            &logger,
            colors,
            std::sync::Arc::clone(&executor),
            &repo_root,
        )?;
        return Ok(None);
    }

    // Handle --generate-commit-msg
    if args.commit_plumbing.generate_commit_msg {
        handle_generate_commit_msg(&plumbing::CommitGenerationConfig {
            config: &config,
            template_context: &template_context,
            workspace: &*workspace,
            workspace_arc: std::sync::Arc::clone(&workspace),
            registry: &registry,
            logger: &logger,
            colors,
            developer_agent: &developer_agent,
            reviewer_agent: &reviewer_agent,
            executor: std::sync::Arc::clone(&executor),
        })?;
        return Ok(None);
    }

    // Get display names before moving registry
    let developer_display = registry.display_name(&developer_agent);
    let reviewer_display = registry.display_name(&reviewer_agent);

    // Build pipeline context (workspace was injected via params)
    let ctx = PipelineContext {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        developer_display,
        reviewer_display,
        repo_root,
        workspace,
        logger,
        colors,
        template_context,
        executor,
        run_log_context,
    };
    Ok(Some(ctx))
}
