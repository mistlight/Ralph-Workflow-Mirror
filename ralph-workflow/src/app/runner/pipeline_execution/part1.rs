// Pipeline execution - Part 1: run_pipeline and run_pipeline_with_default_handler
// All imports are in this file to avoid duplication

use crate::app::{
    context::PipelineContext, detection::detect_project_stack, event_loop, finalization,
};
use crate::app::finalization::finalize_pipeline;
use crate::banner::print_welcome_banner;
use crate::checkpoint::{save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase};
use crate::files::update_status_with_workspace;
#[cfg(not(feature = "test-utils"))]
use crate::git_helpers::{cleanup_orphaned_marker, start_agent_phase};
use crate::pipeline::{AgentPhaseGuard, Stats, Timer};

use crate::app::resume::{handle_resume_with_validation, offer_resume_if_checkpoint_exists};

use super::{
    check_prompt_restoration, create_phase_context_with_config, defer_clear_interrupt_context,
    print_pipeline_info_with_config, print_review_guidelines, save_start_commit_or_warn,
    setup_interrupt_context_for_pipeline, setup_prompt_monitor,
    update_interrupt_context_from_phase, validate_prompt_and_setup_backup,
};

/// Runs the full development/review/commit pipeline using reducer-based event loop.
pub fn run_pipeline(ctx: &PipelineContext) -> anyhow::Result<()> {
    // Use MainEffectHandler for production
    run_pipeline_with_default_handler(ctx)
}

/// Runs the pipeline with the default MainEffectHandler.
///
/// This is the production entry point - it creates a MainEffectHandler internally.
pub fn run_pipeline_with_default_handler(ctx: &PipelineContext) -> anyhow::Result<()> {
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
