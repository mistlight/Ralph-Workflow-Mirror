// Pipeline execution functions.
//
// This module contains:
// - PipelinePreparationParams: Parameters for preparing the pipeline context
// - prepare_pipeline_or_exit: Prepares the pipeline context after agent validation
// - run_pipeline: Main pipeline execution via reducer event loop
// - run_pipeline_with_default_handler: Production entry point with MainEffectHandler
// - run_pipeline_with_effect_handler: Test entry point with custom effect handler

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

    logger = logger.with_log_file(".agent/logs/pipeline.log");

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
    #[cfg(not(feature = "test-utils"))]
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
    // Use workspace-aware versions when test-utils feature is enabled
    // to avoid real git operations that would cause test failures.
    let mut git_helpers = crate::git_helpers::GitHelpers::new();

    #[cfg(feature = "test-utils")]
    {
        use crate::git_helpers::{
            cleanup_orphaned_marker_with_workspace, create_marker_with_workspace,
        };
        // Use workspace-based operations that don't require real git
        cleanup_orphaned_marker_with_workspace(&*ctx.workspace, &ctx.logger)?;
        create_marker_with_workspace(&*ctx.workspace)?;
        // Skip hook installation and git wrapper in test mode
    }
    #[cfg(not(feature = "test-utils"))]
    {
        cleanup_orphaned_marker(&ctx.logger)?;
        start_agent_phase(&mut git_helpers)?;
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
    let (mut timer, mut stats) = (Timer::new(), Stats::new());
    let mut phase_ctx = create_phase_context_with_config(
        ctx,
        &config,
        &mut timer,
        &mut stats,
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
        // Migrate from old checkpoint format to new reducer state
        PipelineState::from(checkpoint.clone())
    } else {
        // Create new initial state
        PipelineState::initial(config.developer_iters, config.reviewer_reviews)
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
    // Under test-utils feature, use MockEffectHandler to avoid real git operations.
    #[cfg(feature = "test-utils")]
    let loop_result = {
        use crate::app::event_loop::run_event_loop_with_handler;
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        let mut handler = MockEffectHandler::new(initial_state.clone());
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop_with_handler(
            phase_ctx_ref,
            Some(initial_state),
            event_loop_config,
            &mut handler,
        )
    };
    #[cfg(not(feature = "test-utils"))]
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
        ctx.logger
            .success("Pipeline completed successfully via reducer event loop");
        ctx.logger.info(&format!(
            "Total events processed: {}",
            loop_result.events_processed
        ));
    } else {
        ctx.logger.warn("Pipeline exited without completion marker");
    }

    // Save Complete checkpoint before clearing (for idempotent resume)
    if config.features.checkpoint_enabled {
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
        &ctx.logger,
        ctx.colors,
        &config,
        finalization::RuntimeStats {
            timer: &timer,
            stats: &stats,
        },
        prompt_monitor,
        &*ctx.workspace,
    );
    Ok(())
}

/// Runs the pipeline with a custom effect handler for testing.
///
/// This function is only available with the `test-utils` feature and allows
/// injecting a `MockEffectHandler` to prevent real git operations during tests.
///
/// # Arguments
///
/// * `ctx` - Pipeline context
/// * `effect_handler` - Custom effect handler (e.g., `MockEffectHandler`)
///
/// # Type Parameters
///
/// * `H` - Effect handler type that implements `EffectHandler` and `StatefulHandler`
#[cfg(feature = "test-utils")]
pub fn run_pipeline_with_effect_handler<'ctx, H>(
    ctx: &PipelineContext,
    effect_handler: &mut H,
) -> anyhow::Result<()>
where
    H: crate::reducer::EffectHandler<'ctx> + crate::app::event_loop::StatefulHandler,
{
    use crate::app::event_loop::EventLoopConfig;
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
        restored_config
    } else {
        ctx.config.clone()
    };

    // Set up git helpers and agent phase
    // Use workspace-aware versions when test-utils feature is enabled
    // to avoid real git operations that would cause test failures.
    let mut git_helpers = crate::git_helpers::GitHelpers::new();

    #[cfg(feature = "test-utils")]
    {
        use crate::git_helpers::{
            cleanup_orphaned_marker_with_workspace, create_marker_with_workspace,
        };
        // Use workspace-based operations that don't require real git
        cleanup_orphaned_marker_with_workspace(&*ctx.workspace, &ctx.logger)?;
        create_marker_with_workspace(&*ctx.workspace)?;
        // Skip hook installation and git wrapper in test mode
    }
    #[cfg(not(feature = "test-utils"))]
    {
        cleanup_orphaned_marker(&ctx.logger)?;
        start_agent_phase(&mut git_helpers)?;
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
    let (mut timer, mut stats) = (Timer::new(), Stats::new());
    let mut phase_ctx = create_phase_context_with_config(
        ctx,
        &config,
        &mut timer,
        &mut stats,
        review_guidelines.as_ref(),
        &run_context,
        resume_checkpoint.as_ref(),
    );
    save_start_commit_or_warn(ctx);

    // Set up interrupt context for checkpoint saving on Ctrl+C
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

    // Initialize pipeline state
    let mut initial_state = if let Some(ref checkpoint) = resume_checkpoint {
        PipelineState::from(checkpoint.clone())
    } else {
        PipelineState::initial(config.developer_iters, config.reviewer_reviews)
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

    // Clone execution_history and prompt_history BEFORE running event loop
    let execution_history_before = phase_ctx.execution_history.clone();
    let prompt_history_before = phase_ctx.clone_prompt_history();

    // Run event loop with the provided handler
    effect_handler.update_state(initial_state.clone());
    let loop_result = {
        use crate::app::event_loop::run_event_loop_with_handler;
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop_with_handler(
            phase_ctx_ref,
            Some(initial_state),
            event_loop_config,
            effect_handler,
        )
    };

    // Handle event loop result
    let loop_result = loop_result?;
    if loop_result.completed {
        ctx.logger
            .success("Pipeline completed successfully via reducer event loop");
        ctx.logger.info(&format!(
            "Total events processed: {}",
            loop_result.events_processed
        ));
    } else {
        ctx.logger.warn("Pipeline exited without completion marker");
    }

    // Save Complete checkpoint before clearing (for idempotent resume)
    if config.features.checkpoint_enabled {
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
        &ctx.logger,
        ctx.colors,
        &config,
        finalization::RuntimeStats {
            timer: &timer,
            stats: &stats,
        },
        prompt_monitor,
        &*ctx.workspace,
    );
    Ok(())
}
