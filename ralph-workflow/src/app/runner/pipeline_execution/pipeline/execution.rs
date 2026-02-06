// Pipeline execution functions.
//
// This module contains:
// - PipelinePreparationParams: Parameters for preparing the pipeline context
// - prepare_pipeline_or_exit: Prepares the pipeline context after agent validation
// - run_pipeline: Main pipeline execution via reducer event loop
// - run_pipeline_with_default_handler: Production entry point with MainEffectHandler
// - run_pipeline_with_effect_handler: Test entry point with custom effect handler

use anyhow::Context;

/// Parameters for preparing the pipeline context.
///
/// Groups related parameters to avoid too many function arguments.
struct PipelinePreparationParams<'a, H: effect::AppEffectHandler> {
    args: Args,
    config: crate::config::Config,
    registry: AgentRegistry,
    developer_agent: String,
    reviewer_agent: String,
    repo_root: std::path::PathBuf,
    logger: Logger,
    colors: Colors,
    executor: std::sync::Arc<dyn ProcessExecutor>,
    handler: &'a mut H,
    /// Workspace for explicit path resolution.
    ///
    /// Production code passes `Arc::new(WorkspaceFs::new(...))`.
    /// Tests can pass `Arc::new(MemoryWorkspace::new(...))`.
    workspace: std::sync::Arc<dyn crate::workspace::Workspace>,
}

/// Prepares the pipeline context after agent validation.
///
/// Returns `Some(ctx)` if pipeline should run, or `None` if we should exit early.
fn prepare_pipeline_or_exit<H: effect::AppEffectHandler>(
    params: PipelinePreparationParams<'_, H>,
) -> anyhow::Result<Option<PipelineContext>> {
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
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Reset context for isolation mode via effects
    if config.isolation_mode {
        effectful::reset_context_for_isolation_effectful(handler)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    // Create run log context for per-run log directory
    // If resuming, continue with the same run_id from checkpoint; otherwise create new
    use crate::logging::RunLogContext;
    let run_log_context = if args.recovery.resume {
        // Try to load checkpoint to get run_id for resume continuity
        use crate::checkpoint::load_checkpoint_with_workspace;
        let checkpoint = load_checkpoint_with_workspace(&*workspace)
            .context("Failed to load checkpoint for resume")?;

        if let Some(checkpoint) = checkpoint {
            // Resume: continue logging to the same run directory
            RunLogContext::from_checkpoint(&checkpoint.run_id, &*workspace)
                .context("Failed to restore run log context from checkpoint")?
        } else {
            // No checkpoint found, but --resume was requested
            // This is handled later by resume validation, but we need a run context now
            RunLogContext::new(&*workspace).context("Failed to create run log context")?
        }
    } else {
        // Fresh run: generate new run_id
        RunLogContext::new(&*workspace).context("Failed to create run log context")?
    };

    // Use per-run pipeline.log path via workspace (supports MemoryWorkspace in tests)
    logger = logger.with_workspace_log(
        std::sync::Arc::clone(&workspace),
        run_log_context.pipeline_log().to_str().unwrap(),
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
        logger.warn(&format!("Failed to write run.json: {}", e));
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
        handle_generate_commit_msg(plumbing::CommitGenerationConfig {
            config: &config,
            template_context: &template_context,
            workspace: &*workspace,
            registry: &registry,
            logger: &logger,
            colors,
            developer_agent: &developer_agent,
            _reviewer_agent: &reviewer_agent,
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

/// Runs the full development/review/commit pipeline using reducer-based event loop.
fn run_pipeline(ctx: &PipelineContext) -> anyhow::Result<()> {
    // Use MainEffectHandler for production
    run_pipeline_with_default_handler(ctx)
}

/// Runs the pipeline with the default MainEffectHandler.
///
/// This is the production entry point - it creates a MainEffectHandler internally.
fn run_pipeline_with_default_handler(ctx: &PipelineContext) -> anyhow::Result<()> {
    use crate::app::event_loop::EventLoopConfig;
    use crate::reducer::MainEffectHandler;
    use crate::reducer::PipelineState;

    // First, offer interactive resume if checkpoint exists without --resume flag
    let resume_result = offer_resume_if_checkpoint_exists(
        &ctx.args,
        &ctx.config,
        &ctx.registry,
        &ctx.logger,
        &ctx.developer_agent,
        &ctx.reviewer_agent,
        &*ctx.workspace,
    );

    // If interactive resume didn't happen, check for --resume flag
    let resume_result = match resume_result {
        Some(result) => Some(result),
        None => handle_resume_with_validation(
            &ctx.args,
            &ctx.config,
            &ctx.registry,
            &ctx.logger,
            &ctx.developer_display,
            &ctx.reviewer_display,
            &*ctx.workspace,
        )?,
    };

    let resume_checkpoint = resume_result.map(|r| r.checkpoint);

    // Create run context - either new or from checkpoint
    let run_context = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::RunContext;
        RunContext::from_checkpoint(checkpoint)
    } else {
        use crate::checkpoint::RunContext;
        RunContext::new()
    };

    // Apply checkpoint configuration restoration if resuming
    let config = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::apply_checkpoint_to_config;
        let mut restored_config = ctx.config.clone();
        apply_checkpoint_to_config(&mut restored_config, checkpoint);
        ctx.logger.info("Restored configuration from checkpoint:");
        if checkpoint.cli_args.developer_iters > 0 {
            ctx.logger.info(&format!(
                "  Developer iterations: {} (from checkpoint)",
                checkpoint.cli_args.developer_iters
            ));
        }
        if checkpoint.cli_args.reviewer_reviews > 0 {
            ctx.logger.info(&format!(
                "  Reviewer passes: {} (from checkpoint)",
                checkpoint.cli_args.reviewer_reviews
            ));
        }
        restored_config
    } else {
        ctx.config.clone()
    };

    // Restore environment variables from checkpoint if resuming
    if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::restore::restore_environment_from_checkpoint;
        let restored_count = restore_environment_from_checkpoint(checkpoint);
        if restored_count > 0 {
            ctx.logger.info(&format!(
                "  Restored {} environment variable(s) from checkpoint",
                restored_count
            ));
        }
    }

    // Set up git helpers and agent phase
    // Use workspace-aware marker operations; then attempt hook install / wrapper.
    // This is best-effort: failures must not terminate the pipeline.
    let mut git_helpers = crate::git_helpers::GitHelpers::new();

    // Marker cleanup/creation are filesystem concerns; use Workspace so tests can run
    // with MemoryWorkspace, and production stays consistent.
    if let Err(err) =
        crate::git_helpers::cleanup_orphaned_marker_with_workspace(&*ctx.workspace, &ctx.logger)
    {
        ctx.logger
            .warn(&format!("Failed to cleanup orphaned marker: {err}"));
    }
    if let Err(err) = crate::git_helpers::create_marker_with_workspace(&*ctx.workspace) {
        ctx.logger
            .warn(&format!("Failed to create agent phase marker: {err}"));
    }

    // Hook install / wrapper require a real repo; treat as best-effort.
    if let Err(err) = crate::git_helpers::cleanup_orphaned_marker(&ctx.logger) {
        ctx.logger.warn(&format!(
            "Failed to cleanup orphaned marker via git helpers: {err}"
        ));
    }
    if let Err(err) = crate::git_helpers::start_agent_phase(&mut git_helpers) {
        ctx.logger
            .warn(&format!("Failed to start agent phase: {err}"));
    }

    let mut agent_phase_guard =
        AgentPhaseGuard::new(&mut git_helpers, &ctx.logger, &*ctx.workspace);

    // Print welcome banner and validate PROMPT.md
    print_welcome_banner(ctx.colors, &ctx.developer_display, &ctx.reviewer_display);
    print_pipeline_info_with_config(ctx, &config);
    validate_prompt_and_setup_backup(ctx)?;

    // Set up PROMPT.md monitoring
    let mut prompt_monitor = setup_prompt_monitor(ctx);

    // Detect project stack and review guidelines
    let (_project_stack, review_guidelines) =
        detect_project_stack(&config, &ctx.repo_root, &ctx.logger, ctx.colors);

    print_review_guidelines(ctx, review_guidelines.as_ref());
    println!();

    // Create phase context and save starting commit
    let mut timer = Timer::new();
    let mut phase_ctx = create_phase_context_with_config(
        ctx,
        &config,
        &mut timer,
        review_guidelines.as_ref(),
        &run_context,
        resume_checkpoint.as_ref(),
    );
    save_start_commit_or_warn(ctx);

    // Set up interrupt context for checkpoint saving on Ctrl+C
    // This must be done after phase_ctx is created
    let initial_phase = if let Some(ref checkpoint) = resume_checkpoint {
        checkpoint.phase
    } else {
        PipelinePhase::Planning
    };
    setup_interrupt_context_for_pipeline(
        initial_phase,
        config.developer_iters,
        config.reviewer_reviews,
        &phase_ctx.execution_history,
        &phase_ctx.prompt_history,
        &run_context,
        std::sync::Arc::clone(&ctx.workspace),
    );

    // Ensure interrupt context is cleared on completion
    let _interrupt_guard = defer_clear_interrupt_context();

    // Determine if we should run rebase based on current args only.
    let should_run_rebase = ctx.args.rebase_flags.with_rebase;

    // Update interrupt context before entering the reducer event loop.
    update_interrupt_context_from_phase(
        &phase_ctx,
        initial_phase,
        config.developer_iters,
        config.reviewer_reviews,
        &run_context,
        std::sync::Arc::clone(&ctx.workspace),
    );

    // ============================================
    // RUN PIPELINE PHASES VIA REDUCER EVENT LOOP
    // ============================================

    // Initialize pipeline state
    let mut initial_state = if let Some(ref checkpoint) = resume_checkpoint {
        // Restore progress from checkpoint, but keep budgets/limits config-driven.
        // Initialize a config-aware base state first, then overlay checkpoint progress.
        let mut base_state = crate::app::event_loop::create_initial_state_with_config(&phase_ctx);
        let migrated: PipelineState = checkpoint.clone().into();

        base_state.phase = migrated.phase;
        base_state.iteration = migrated.iteration;
        base_state.total_iterations = migrated.total_iterations;
        base_state.reviewer_pass = migrated.reviewer_pass;
        base_state.total_reviewer_passes = migrated.total_reviewer_passes;
        base_state.rebase = migrated.rebase;
        base_state.execution_history = migrated.execution_history;
        base_state.prompt_inputs = migrated.prompt_inputs;
        base_state.metrics = migrated.metrics;

        base_state
    } else {
        // Create new initial state with config-derived continuation limits.
        crate::app::event_loop::create_initial_state_with_config(&phase_ctx)
    };

    if should_run_rebase {
        if matches!(
            initial_state.rebase,
            crate::reducer::state::RebaseState::NotStarted
        ) {
            let default_branch =
                crate::git_helpers::get_default_branch().unwrap_or_else(|_| "main".to_string());
            initial_state.rebase = crate::reducer::state::RebaseState::InProgress {
                original_head: "HEAD".to_string(),
                target_branch: default_branch,
            };
        }
    } else if matches!(
        initial_state.rebase,
        crate::reducer::state::RebaseState::NotStarted
    ) {
        initial_state.rebase = crate::reducer::state::RebaseState::Skipped;
    }

    // Configure event loop
    let event_loop_config = EventLoopConfig {
        max_iterations: event_loop::MAX_EVENT_LOOP_ITERATIONS,
    };

    // Clone execution_history and prompt_history BEFORE running event loop (to avoid borrow issues)
    let execution_history_before = phase_ctx.execution_history.clone();
    let prompt_history_before = phase_ctx.clone_prompt_history();

    // Create effect handler and run event loop.
    let loop_result = {
        use crate::app::event_loop::run_event_loop_with_handler;
        let mut handler = MainEffectHandler::new(initial_state.clone());
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop_with_handler(
            phase_ctx_ref,
            Some(initial_state),
            event_loop_config,
            &mut handler,
        )
    };

    // Handle event loop result
    let loop_result = loop_result?;
    if loop_result.completed {
        match loop_result.final_phase {
            crate::reducer::event::PipelinePhase::Complete => {
                ctx.logger
                    .success("Pipeline completed successfully via reducer event loop");
            }
            crate::reducer::event::PipelinePhase::Interrupted => {
                ctx.logger
                    .info("Pipeline completed with Interrupted phase (failure handled)");
                ctx.logger.info(
                    "Completion marker was written during failure handling. \
                     External orchestration can detect termination via .agent/tmp/completion_marker"
                );
            }
            _ => {
                ctx.logger
                    .success("Pipeline completed via reducer event loop");
            }
        }
        ctx.logger.info(&format!(
            "Total events processed: {}",
            loop_result.events_processed
        ));
    } else {
        ctx.logger
            .error("⚠️  EXCEPTIONAL: Pipeline exited without normal completion");
        ctx.logger.warn(&format!(
            "This indicates a bug in the event loop or reducer. \
             Expected final phase: Complete or Interrupted+checkpoint. \
             Actual: completed=false, final_phase={:?}, events_processed={}",
            loop_result.final_phase, loop_result.events_processed
        ));

        // If we exited from AwaitingDevFix without completing, this is the specific bug
        // we're trying to fix - log it explicitly with state details
        if matches!(
            loop_result.final_phase,
            crate::reducer::event::PipelinePhase::AwaitingDevFix
        ) {
            ctx.logger.error(
                "BUG DETECTED: Event loop exited from AwaitingDevFix without completing dev-fix flow. \
                 This should transition to Interrupted and save checkpoint. \
                 Check: Was TriggerDevFixFlow executed? Was completion marker written? \
                 See .agent/tmp/event_loop_trace.jsonl for execution trace."
            );
        }

        // DEFENSIVE: Emit completion marker for orchestration
        // This ensures external systems can detect termination even if the
        // event loop exited unexpectedly before SaveCheckpoint was processed.
        write_defensive_completion_marker(&*ctx.workspace, &ctx.logger, loop_result.final_phase);
    }

    // Save Complete checkpoint before clearing (for idempotent resume)
    if config.features.checkpoint_enabled
        && should_write_complete_checkpoint(loop_result.final_phase)
    {
        let builder = CheckpointBuilder::new()
            .phase(
                PipelinePhase::Complete,
                config.developer_iters,
                config.developer_iters,
            )
            .reviewer_pass(config.reviewer_reviews, config.reviewer_reviews)
            .capture_from_context(
                &config,
                &ctx.registry,
                &ctx.developer_agent,
                &ctx.reviewer_agent,
                &ctx.logger,
                &run_context,
            )
            .with_executor_from_context(std::sync::Arc::clone(&ctx.executor));

        let builder = builder
            .with_execution_history(execution_history_before)
            .with_prompt_history(prompt_history_before);

        if let Some(checkpoint) = builder.build_with_workspace(&*ctx.workspace) {
            let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
        }
    }

    // Post-pipeline operations
    check_prompt_restoration(ctx, &mut prompt_monitor, "event loop");
    update_status_with_workspace(&*ctx.workspace, "In progress.", config.isolation_mode)?;

    // Commit phase
    finalize_pipeline(
        &mut agent_phase_guard,
        crate::app::finalization::FinalizeContext {
            logger: &ctx.logger,
            colors: ctx.colors,
            config: &config,
            timer: &timer,
            workspace: &*ctx.workspace,
        },
        &loop_result.final_state,
        prompt_monitor,
    );
    Ok(())
}

fn write_defensive_completion_marker(
    workspace: &dyn crate::workspace::Workspace,
    logger: &Logger,
    final_phase: crate::reducer::event::PipelinePhase,
) -> bool {
    if let Err(err) = workspace.create_dir_all(std::path::Path::new(".agent/tmp")) {
        logger.error(&format!(
            "Failed to create completion marker directory: {err}"
        ));
        return false;
    }

    let marker_path = std::path::Path::new(".agent/tmp/completion_marker");
    let content = format!(
        "failure\nEvent loop exited without normal completion (final_phase={:?})",
        final_phase
    );
    if let Err(err) = workspace.write(marker_path, &content) {
        logger.error(&format!(
            "Failed to write defensive completion marker: {err}"
        ));
        return false;
    }

    logger.info("Defensive completion marker written: failure");
    true
}

#[cfg(test)]
mod execution_tests {
    use super::write_defensive_completion_marker;
    use crate::logger::{Colors, Logger};
    use crate::workspace::{DirEntry, MemoryWorkspace, Workspace};
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Debug)]
    struct TrackingWorkspace {
        inner: MemoryWorkspace,
        tmp_created: Mutex<bool>,
    }

    impl TrackingWorkspace {
        fn new() -> Self {
            Self {
                inner: MemoryWorkspace::new(PathBuf::from("/test/repo")),
                tmp_created: Mutex::new(false),
            }
        }

        fn tmp_created(&self) -> bool {
            *self.tmp_created.lock().unwrap()
        }
    }

    impl Workspace for TrackingWorkspace {
        fn root(&self) -> &Path {
            self.inner.root()
        }

        fn read(&self, relative: &Path) -> io::Result<String> {
            self.inner.read(relative)
        }

        fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
            self.inner.read_bytes(relative)
        }

        fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
            self.inner.write(relative, content)
        }

        fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            self.inner.write_bytes(relative, content)
        }

        fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            self.inner.append_bytes(relative, content)
        }

        fn exists(&self, relative: &Path) -> bool {
            self.inner.exists(relative)
        }

        fn is_file(&self, relative: &Path) -> bool {
            self.inner.is_file(relative)
        }

        fn is_dir(&self, relative: &Path) -> bool {
            self.inner.is_dir(relative)
        }

        fn remove(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove(relative)
        }

        fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_if_exists(relative)
        }

        fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_dir_all(relative)
        }

        fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_dir_all_if_exists(relative)
        }

        fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
            if relative == Path::new(".agent/tmp") {
                *self.tmp_created.lock().unwrap() = true;
            }
            self.inner.create_dir_all(relative)
        }

        fn read_dir(&self, relative: &Path) -> io::Result<Vec<DirEntry>> {
            self.inner.read_dir(relative)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.inner.rename(from, to)
        }

        fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
            self.inner.write_atomic(relative, content)
        }

        fn set_readonly(&self, relative: &Path) -> io::Result<()> {
            self.inner.set_readonly(relative)
        }

        fn set_writable(&self, relative: &Path) -> io::Result<()> {
            self.inner.set_writable(relative)
        }
    }

    #[test]
    fn test_defensive_completion_marker_creates_tmp_dir() {
        let workspace = TrackingWorkspace::new();
        let logger = Logger::new(Colors { enabled: false });

        let wrote = write_defensive_completion_marker(
            &workspace,
            &logger,
            crate::reducer::event::PipelinePhase::AwaitingDevFix,
        );

        assert!(wrote, "marker write should succeed");
        assert!(
            workspace.tmp_created(),
            "should create .agent/tmp before writing marker"
        );
        assert!(
            workspace.exists(Path::new(".agent/tmp/completion_marker")),
            "completion marker should exist after defensive write"
        );
    }
}
