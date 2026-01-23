//! Application entrypoint and pipeline orchestration.
//!
//! This module exists to keep `src/main.rs` small and focused while preserving
//! the CLI surface and overall runtime behavior. It wires together:
//! - CLI/config parsing and plumbing commands
//! - Agent registry loading
//! - Repo setup and resume support
//! - Phase execution via `crate::phases`
//!
//! # Module Structure
//!
//! - [`config_init`]: Configuration loading and agent registry initialization
//! - [`plumbing`]: Low-level git operations (show/apply commit messages)
//! - [`validation`]: Agent validation and chain validation
//! - [`resume`]: Checkpoint resume functionality
//! - [`detection`]: Project stack detection
//! - [`finalization`]: Pipeline cleanup and finalization

pub mod config_init;
pub mod context;
pub mod detection;
pub mod event_loop;
pub mod finalization;
pub mod plumbing;
pub mod resume;
pub mod validation;

use crate::agents::AgentRegistry;
use crate::app::finalization::finalize_pipeline;
use crate::banner::print_welcome_banner;
use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};

use crate::checkpoint::{
    save_checkpoint, CheckpointBuilder, PipelineCheckpoint, PipelinePhase, RebaseState,
};
use crate::cli::{
    create_prompt_from_template, handle_diagnose, handle_dry_run, handle_list_agents,
    handle_list_available_agents, handle_list_providers, handle_show_baseline,
    handle_template_commands, prompt_template_selection, Args,
};
use crate::files::protection::monitoring::PromptMonitor;
use crate::files::{
    create_prompt_backup, ensure_files, make_prompt_read_only, reset_context_for_isolation,
    update_status, validate_prompt_md,
};
use crate::git_helpers::{
    abort_rebase, cleanup_orphaned_marker, continue_rebase, get_conflicted_files,
    get_default_branch, get_repo_root, get_start_commit_summary, is_main_or_master_branch,
    rebase_onto, require_git_repo, reset_start_commit, save_start_commit, start_agent_phase,
    RebaseResult,
};
use crate::logger::Colors;
use crate::logger::Logger;
use crate::phases::PhaseContext;
use crate::pipeline::{AgentPhaseGuard, Stats, Timer};
use crate::prompts::{get_stored_or_generate_prompt, template_context::TemplateContext};
use std::env;

use config_init::initialize_config;
use context::PipelineContext;
use detection::detect_project_stack;
use plumbing::{handle_apply_commit, handle_generate_commit_msg, handle_show_commit_msg};
use resume::{handle_resume_with_validation, offer_resume_if_checkpoint_exists};
use validation::{
    resolve_required_agents, validate_agent_chains, validate_agent_commands, validate_can_commit,
};

/// Main application entry point.
///
/// Orchestrates the entire Ralph pipeline:
/// 1. Configuration initialization
/// 2. Agent validation
/// 3. Plumbing commands (if requested)
/// 4. Development phase
/// 5. Review & fix phase
/// 6. Final validation
/// 7. Commit phase
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if any phase fails.
pub fn run(args: Args) -> anyhow::Result<()> {
    let colors = Colors::new();
    static LOGGER: std::sync::OnceLock<Logger> = std::sync::OnceLock::new();
    let logger = LOGGER.get_or_init(|| Logger::new(colors));

    // Initialize configuration and agent registry
    let Some(init_result) = initialize_config(&args, colors, &logger)? else {
        return Ok(()); // Early exit (--init/--init-global/--init-legacy)
    };

    let config_init::ConfigInitResult {
        config,
        registry,
        config_path,
        config_sources,
    } = init_result;

    // Resolve required agent names
    let validated = resolve_required_agents(&config)?;
    let developer_agent = validated.developer_agent;
    let reviewer_agent = validated.reviewer_agent;

    // Handle listing commands (these can run without git repo)
    if handle_listing_commands(&args, &registry, colors) {
        return Ok(());
    }

    // Handle --diagnose
    if args.recovery.diagnose {
        handle_diagnose(colors, &config, &registry, &config_path, &config_sources);
        return Ok(());
    }

    // Validate agent chains
    validate_agent_chains(&registry, colors);

    // Handle plumbing commands
    if handle_plumbing_commands(&args, &logger, colors)? {
        return Ok(());
    }

    // Validate agents and set up git repo and PROMPT.md
    let Some(repo_root) = validate_and_setup_agents(
        &config,
        &registry,
        &developer_agent,
        &reviewer_agent,
        &config_path,
        colors,
        &logger,
    )?
    else {
        return Ok(());
    };

    // Handle --rebase-only
    if args.rebase_flags.rebase_only {
        let template_context =
            TemplateContext::from_user_templates_dir(config.user_templates_dir().cloned());
        return handle_rebase_only(&args, &config, &template_context, &logger, colors);
    }

    // Prepare pipeline context or exit early
    (prepare_pipeline_or_exit(PipelinePreparationParams {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        logger,
        colors,
    })?)
    .map_or_else(|| Ok(()), |ctx| run_pipeline(&ctx))
}

/// Handles listing commands that don't require the full pipeline.
///
/// Returns `true` if a listing command was handled and we should exit.
fn handle_listing_commands(args: &Args, registry: &AgentRegistry, colors: Colors) -> bool {
    if args.agent_list.list_agents {
        handle_list_agents(registry);
        return true;
    }
    if args.agent_list.list_available_agents {
        handle_list_available_agents(registry);
        return true;
    }
    if args.provider_list.list_providers {
        handle_list_providers(colors);
        return true;
    }

    // Handle template commands
    let template_cmds = &args.template_commands;
    if template_cmds.init_templates_enabled()
        || template_cmds.validate
        || template_cmds.show.is_some()
        || template_cmds.list
        || template_cmds.list_all
        || template_cmds.variables.is_some()
        || template_cmds.render.is_some()
    {
        let _ = handle_template_commands(template_cmds, colors);
        return true;
    }

    false
}

/// Handles plumbing commands that require git repo but not full validation.
///
/// Returns `Ok(true)` if a plumbing command was handled and we should exit.
/// Returns `Ok(false)` if we should continue to the main pipeline.
fn handle_plumbing_commands(args: &Args, logger: &Logger, colors: Colors) -> anyhow::Result<bool> {
    // Show commit message
    if args.commit_display.show_commit_msg {
        return handle_show_commit_msg().map(|()| true);
    }

    // Apply commit
    if args.commit_plumbing.apply_commit {
        return handle_apply_commit(logger, colors).map(|()| true);
    }

    // Reset start commit
    if args.commit_display.reset_start_commit {
        require_git_repo()?;
        let repo_root = get_repo_root()?;
        env::set_current_dir(&repo_root)?;

        return match reset_start_commit() {
            Ok(result) => {
                let short_oid = &result.oid[..8.min(result.oid.len())];
                if result.fell_back_to_head {
                    logger.success(&format!(
                        "Starting commit reference reset to current HEAD ({})",
                        short_oid
                    ));
                    logger.info("On main/master branch - using HEAD as baseline");
                } else if let Some(ref branch) = result.default_branch {
                    logger.success(&format!(
                        "Starting commit reference reset to merge-base with '{}' ({})",
                        branch, short_oid
                    ));
                    logger.info("Baseline set to common ancestor with default branch");
                } else {
                    logger.success(&format!("Starting commit reference reset ({})", short_oid));
                }
                logger.info(".agent/start_commit has been updated");
                Ok(true)
            }
            Err(e) => {
                logger.error(&format!("Failed to reset starting commit: {e}"));
                anyhow::bail!("Failed to reset starting commit");
            }
        };
    }

    // Show baseline state
    if args.commit_display.show_baseline {
        require_git_repo()?;
        let repo_root = get_repo_root()?;
        env::set_current_dir(&repo_root)?;

        return match handle_show_baseline() {
            Ok(()) => Ok(true),
            Err(e) => {
                logger.error(&format!("Failed to show baseline: {e}"));
                anyhow::bail!("Failed to show baseline");
            }
        };
    }

    Ok(false)
}

/// Parameters for preparing the pipeline context.
///
/// Groups related parameters to avoid too many function arguments.
struct PipelinePreparationParams {
    args: Args,
    config: crate::config::Config,
    registry: AgentRegistry,
    developer_agent: String,
    reviewer_agent: String,
    repo_root: std::path::PathBuf,
    logger: Logger,
    colors: Colors,
}

/// Prepares the pipeline context after agent validation.
///
/// Returns `Some(ctx)` if pipeline should run, or `None` if we should exit early.
fn prepare_pipeline_or_exit(
    params: PipelinePreparationParams,
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
    } = params;

    ensure_files(config.isolation_mode)?;

    // Reset context for isolation mode
    if config.isolation_mode {
        reset_context_for_isolation(&logger)?;
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
        )?;
        return Ok(None);
    }

    // Create template context for user template overrides
    let template_context =
        TemplateContext::from_user_templates_dir(config.user_templates_dir().cloned());

    // Handle --generate-commit-msg
    if args.commit_plumbing.generate_commit_msg {
        handle_generate_commit_msg(
            &config,
            &template_context,
            &registry,
            &logger,
            colors,
            &developer_agent,
            &reviewer_agent,
        )?;
        return Ok(None);
    }

    // Get display names before moving registry
    let developer_display = registry.display_name(&developer_agent);
    let reviewer_display = registry.display_name(&reviewer_agent);

    // Build pipeline context
    let ctx = PipelineContext {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        developer_display,
        reviewer_display,
        repo_root,
        logger,
        colors,
        template_context,
    };
    Ok(Some(ctx))
}

/// Validates agent commands and workflow capability, then sets up git repo and PROMPT.md.
///
/// Returns `Some(repo_root)` if setup succeeded and should continue.
/// Returns `None` if the user declined PROMPT.md creation (to exit early).
fn validate_and_setup_agents(
    config: &crate::config::Config,
    registry: &AgentRegistry,
    developer_agent: &str,
    reviewer_agent: &str,
    config_path: &std::path::Path,
    colors: Colors,
    logger: &Logger,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    // Validate agent commands exist
    validate_agent_commands(
        config,
        registry,
        developer_agent,
        reviewer_agent,
        config_path,
    )?;

    // Validate agents are workflow-capable
    validate_can_commit(
        config,
        registry,
        developer_agent,
        reviewer_agent,
        config_path,
    )?;

    // Set up git repo and working directory
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;

    // Set up PROMPT.md if needed (may return None to exit early)
    let should_continue = setup_git_and_prompt_file(config, colors, logger)?;
    if should_continue.is_none() {
        return Ok(None);
    }

    Ok(Some(repo_root))
}

/// In interactive mode, prompts to create PROMPT.md from a template before `ensure_files()`.
///
/// Returns `Ok(Some(()))` if setup succeeded and should continue.
/// Returns `Ok(None)` if the user declined PROMPT.md creation (to exit early).
fn setup_git_and_prompt_file(
    config: &crate::config::Config,
    colors: Colors,
    logger: &Logger,
) -> anyhow::Result<Option<()>> {
    let prompt_exists = std::path::Path::new("PROMPT.md").exists();

    // In interactive mode, prompt to create PROMPT.md from a template BEFORE ensure_files().
    // If the user declines (or we can't prompt), exit without creating a placeholder PROMPT.md.
    if config.behavior.interactive && !prompt_exists {
        if let Some(template_name) = prompt_template_selection(colors) {
            create_prompt_from_template(&template_name, colors)?;
            println!();
            logger.info(
                "PROMPT.md created. Please edit it with your task details, then run ralph again.",
            );
            logger.info(&format!(
                "Tip: Edit PROMPT.md, then run: ralph \"{}\"",
                config.commit_msg
            ));
            return Ok(None);
        }
        println!();
        logger.error("PROMPT.md not found in current directory.");
        logger.warn("PROMPT.md is required to run the Ralph pipeline.");
        println!();
        logger.info("To get started:");
        logger.info("  ralph --init                    # Smart setup wizard");
        logger.info("  ralph --init bug-fix             # Create from Work Guide");
        logger.info("  ralph --list-work-guides          # See all Work Guides");
        println!();
        return Ok(None);
    }

    // Non-interactive mode: show helpful error if PROMPT.md doesn't exist
    if !prompt_exists {
        logger.error("PROMPT.md not found in current directory.");
        logger.warn("PROMPT.md is required to run the Ralph pipeline.");
        println!();
        logger.info("Quick start:");
        logger.info("  ralph --init                    # Smart setup wizard");
        logger.info("  ralph --init bug-fix             # Create from Work Guide");
        logger.info("  ralph --list-work-guides          # See all Work Guides");
        println!();
        logger.info("Use -i flag for interactive mode to be prompted for template selection.");
        println!();
        return Ok(None);
    }

    Ok(Some(()))
}

/// Runs the full development/review/commit pipeline using reducer-based event loop.
fn run_pipeline(ctx: &PipelineContext) -> anyhow::Result<()> {
    use crate::app::event_loop::{run_event_loop, EventLoopConfig};
    use crate::reducer::PipelineState;

    // First, offer interactive resume if checkpoint exists without --resume flag
    let resume_result = offer_resume_if_checkpoint_exists(
        &ctx.args,
        &ctx.config,
        &ctx.registry,
        &ctx.logger,
        &ctx.developer_agent,
        &ctx.reviewer_agent,
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
        ),
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
    let mut git_helpers = crate::git_helpers::GitHelpers::new();
    cleanup_orphaned_marker(&ctx.logger)?;
    start_agent_phase(&mut git_helpers)?;
    let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &ctx.logger);

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
    );

    // Ensure interrupt context is cleared on completion
    let _interrupt_guard = defer_clear_interrupt_context();

    // Determine if we should run rebase based on checkpoint or current args
    let should_run_rebase = if let Some(ref checkpoint) = resume_checkpoint {
        // Use checkpoint's skip_rebase value if it has meaningful cli_args
        if checkpoint.cli_args.developer_iters > 0 || checkpoint.cli_args.reviewer_reviews > 0 {
            !checkpoint.cli_args.skip_rebase
        } else {
            // Fallback to current args
            ctx.args.rebase_flags.with_rebase
        }
    } else {
        ctx.args.rebase_flags.with_rebase
    };

    // Run pre-development rebase (only if explicitly requested via --with-rebase)
    if should_run_rebase {
        run_initial_rebase(ctx, &mut phase_ctx, &run_context)?;
        // Update interrupt context after rebase
        update_interrupt_context_from_phase(
            &phase_ctx,
            PipelinePhase::Planning,
            config.developer_iters,
            config.reviewer_reviews,
            &run_context,
        );
    } else {
        // Save initial checkpoint when rebase is disabled
        if config.features.checkpoint_enabled && resume_checkpoint.is_none() {
            let builder = CheckpointBuilder::new()
                .phase(PipelinePhase::Planning, 0, config.developer_iters)
                .reviewer_pass(0, config.reviewer_reviews)
                .skip_rebase(true) // Rebase is disabled
                .capture_from_context(
                    &config,
                    &ctx.registry,
                    &ctx.developer_agent,
                    &ctx.reviewer_agent,
                    &ctx.logger,
                    &run_context,
                )
                .with_execution_history(phase_ctx.execution_history.clone())
                .with_prompt_history(phase_ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint(&checkpoint);
            }
        }
        // Update interrupt context after initial checkpoint
        update_interrupt_context_from_phase(
            &phase_ctx,
            PipelinePhase::Planning,
            config.developer_iters,
            config.reviewer_reviews,
            &run_context,
        );
    }

    // ============================================
    // RUN PIPELINE PHASES VIA REDUCER EVENT LOOP
    // ============================================

    // Initialize pipeline state
    let initial_state = if let Some(ref checkpoint) = resume_checkpoint {
        // Migrate from old checkpoint format to new reducer state
        PipelineState::from(checkpoint.clone())
    } else {
        // Create new initial state
        PipelineState::initial(config.developer_iters, config.reviewer_reviews)
    };

    // Configure event loop
    let event_loop_config = EventLoopConfig {
        max_iterations: 1000,
        enable_checkpointing: config.features.checkpoint_enabled,
    };

    // Clone execution_history and prompt_history BEFORE running event loop (to avoid borrow issues)
    let execution_history_before = phase_ctx.execution_history.clone();
    let prompt_history_before = phase_ctx.clone_prompt_history();

    // Run event loop in separate scope to release mutable borrow
    let loop_result = {
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop(phase_ctx_ref, Some(initial_state), event_loop_config)
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
        let skip_rebase = !ctx.args.rebase_flags.with_rebase;
        let builder = CheckpointBuilder::new()
            .phase(
                PipelinePhase::Complete,
                config.developer_iters,
                config.developer_iters,
            )
            .reviewer_pass(config.reviewer_reviews, config.reviewer_reviews)
            .skip_rebase(skip_rebase)
            .capture_from_context(
                &config,
                &ctx.registry,
                &ctx.developer_agent,
                &ctx.reviewer_agent,
                &ctx.logger,
                &run_context,
            );

        let builder = builder
            .with_execution_history(execution_history_before)
            .with_prompt_history(prompt_history_before);

        if let Some(checkpoint) = builder.build() {
            let _ = save_checkpoint(&checkpoint);
        }
    }

    // Post-pipeline operations
    check_prompt_restoration(ctx, &mut prompt_monitor, "event loop");
    update_status("In progress.", config.isolation_mode)?;

    // Commit phase
    finalize_pipeline(
        &mut agent_phase_guard,
        &ctx.logger,
        ctx.colors,
        &config,
        &timer,
        &stats,
        prompt_monitor,
    );
    Ok(())
}

/// Set up the interrupt context with initial pipeline state.
///
/// This function initializes the global interrupt context so that if
/// the user presses Ctrl+C, the interrupt handler can save a checkpoint.
fn setup_interrupt_context_for_pipeline(
    phase: PipelinePhase,
    total_iterations: u32,
    total_reviewer_passes: u32,
    execution_history: &crate::checkpoint::ExecutionHistory,
    prompt_history: &std::collections::HashMap<String, String>,
    run_context: &crate::checkpoint::RunContext,
) {
    use crate::interrupt::{set_interrupt_context, InterruptContext};

    // Determine initial iteration based on phase
    let (iteration, reviewer_pass) = match phase {
        PipelinePhase::Development => (1, 0),
        PipelinePhase::Review | PipelinePhase::Fix | PipelinePhase::ReviewAgain => {
            (total_iterations, 1)
        }
        PipelinePhase::PostRebase | PipelinePhase::CommitMessage => {
            (total_iterations, total_reviewer_passes)
        }
        _ => (0, 0),
    };

    let context = InterruptContext {
        phase,
        iteration,
        total_iterations,
        reviewer_pass,
        total_reviewer_passes,
        run_context: run_context.clone(),
        execution_history: execution_history.clone(),
        prompt_history: prompt_history.clone(),
    };

    set_interrupt_context(context);
}

/// Update the interrupt context from the current phase context.
///
/// This function should be called after each major phase to keep the
/// interrupt context up-to-date with the latest execution history.
fn update_interrupt_context_from_phase(
    phase_ctx: &crate::phases::PhaseContext,
    phase: PipelinePhase,
    total_iterations: u32,
    total_reviewer_passes: u32,
    run_context: &crate::checkpoint::RunContext,
) {
    use crate::interrupt::{set_interrupt_context, InterruptContext};

    // Determine current iteration based on phase
    let (iteration, reviewer_pass) = match phase {
        PipelinePhase::Development => {
            // Estimate iteration from actual runs
            let iter = run_context.actual_developer_runs.max(1);
            (iter, 0)
        }
        PipelinePhase::Review | PipelinePhase::Fix | PipelinePhase::ReviewAgain => {
            (total_iterations, run_context.actual_reviewer_runs.max(1))
        }
        PipelinePhase::PostRebase | PipelinePhase::CommitMessage => {
            (total_iterations, total_reviewer_passes)
        }
        _ => (0, 0),
    };

    let context = InterruptContext {
        phase,
        iteration,
        total_iterations,
        reviewer_pass,
        total_reviewer_passes,
        run_context: run_context.clone(),
        execution_history: phase_ctx.execution_history.clone(),
        prompt_history: phase_ctx.clone_prompt_history(),
    };

    set_interrupt_context(context);
}

/// Helper to defer clearing interrupt context until function exit.
///
/// Uses a scope guard pattern to ensure the interrupt context is cleared
/// when the pipeline completes successfully, preventing an "interrupted"
/// checkpoint from being saved after normal completion.
fn defer_clear_interrupt_context() -> InterruptContextGuard {
    InterruptContextGuard
}

/// RAII guard for clearing interrupt context on drop.
///
/// Ensures the interrupt context is cleared when the guard is dropped,
/// preventing an "interrupted" checkpoint from being saved after normal
/// pipeline completion.
struct InterruptContextGuard;

impl Drop for InterruptContextGuard {
    fn drop(&mut self) {
        crate::interrupt::clear_interrupt_context();
    }
}

/// Validate PROMPT.md and set up backup/protection.
fn validate_prompt_and_setup_backup(ctx: &PipelineContext) -> anyhow::Result<()> {
    let prompt_validation =
        validate_prompt_md(ctx.config.behavior.strict_validation, ctx.args.interactive);
    for err in &prompt_validation.errors {
        ctx.logger.error(err);
    }
    for warn in &prompt_validation.warnings {
        ctx.logger.warn(warn);
    }
    if !prompt_validation.is_valid() {
        anyhow::bail!("PROMPT.md validation errors");
    }

    // Create a backup of PROMPT.md to protect against accidental deletion.
    match create_prompt_backup() {
        Ok(None) => {}
        Ok(Some(warning)) => {
            ctx.logger.warn(&format!(
                "PROMPT.md backup created but: {warning}. Continuing anyway."
            ));
        }
        Err(e) => {
            ctx.logger.warn(&format!(
                "Failed to create PROMPT.md backup: {e}. Continuing anyway."
            ));
        }
    }

    // Make PROMPT.md read-only to protect against accidental deletion.
    match make_prompt_read_only() {
        None => {}
        Some(warning) => {
            ctx.logger.warn(&format!("{warning}. Continuing anyway."));
        }
    }

    Ok(())
}

/// Set up PROMPT.md monitoring for deletion detection.
fn setup_prompt_monitor(ctx: &PipelineContext) -> Option<PromptMonitor> {
    match PromptMonitor::new() {
        Ok(mut monitor) => {
            if let Err(e) = monitor.start() {
                ctx.logger.warn(&format!(
                    "Failed to start PROMPT.md monitoring: {e}. Continuing anyway."
                ));
                None
            } else {
                if ctx.config.verbosity.is_debug() {
                    ctx.logger.info("Started real-time PROMPT.md monitoring");
                }
                Some(monitor)
            }
        }
        Err(e) => {
            ctx.logger.warn(&format!(
                "Failed to create PROMPT.md monitor: {e}. Continuing anyway."
            ));
            None
        }
    }
}

/// Print review guidelines if detected.
fn print_review_guidelines(
    ctx: &PipelineContext,
    review_guidelines: Option<&crate::guidelines::ReviewGuidelines>,
) {
    if let Some(guidelines) = review_guidelines {
        ctx.logger.info(&format!(
            "Review guidelines: {}{}{}",
            ctx.colors.dim(),
            guidelines.summary(),
            ctx.colors.reset()
        ));
    }
}

/// Create the phase context with a modified config (for resume restoration).
fn create_phase_context_with_config<'ctx>(
    ctx: &'ctx PipelineContext,
    config: &'ctx crate::config::Config,
    timer: &'ctx mut Timer,
    stats: &'ctx mut Stats,
    review_guidelines: Option<&'ctx crate::guidelines::ReviewGuidelines>,
    run_context: &'ctx crate::checkpoint::RunContext,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> PhaseContext<'ctx> {
    // Restore execution history and prompt history from checkpoint if available
    let (execution_history, prompt_history) = if let Some(checkpoint) = resume_checkpoint {
        let exec_history = checkpoint
            .execution_history
            .clone()
            .unwrap_or_else(crate::checkpoint::execution_history::ExecutionHistory::new);
        let prompt_hist = checkpoint.prompt_history.clone().unwrap_or_default();
        (exec_history, prompt_hist)
    } else {
        (
            crate::checkpoint::execution_history::ExecutionHistory::new(),
            std::collections::HashMap::new(),
        )
    };

    PhaseContext {
        config,
        registry: &ctx.registry,
        logger: &ctx.logger,
        colors: &ctx.colors,
        timer,
        stats,
        developer_agent: &ctx.developer_agent,
        reviewer_agent: &ctx.reviewer_agent,
        review_guidelines,
        template_context: &ctx.template_context,
        run_context: run_context.clone(),
        execution_history,
        prompt_history,
    }
}

/// Print pipeline info with a specific config.
fn print_pipeline_info_with_config(ctx: &PipelineContext, config: &crate::config::Config) {
    ctx.logger.info(&format!(
        "Working directory: {}{}{}",
        ctx.colors.cyan(),
        ctx.repo_root.display(),
        ctx.colors.reset()
    ));
    ctx.logger.info(&format!(
        "Commit message: {}{}{}",
        ctx.colors.cyan(),
        config.commit_msg,
        ctx.colors.reset()
    ));
}

/// Save starting commit or warn if it fails.
fn save_start_commit_or_warn(ctx: &PipelineContext) {
    match save_start_commit() {
        Ok(()) => {
            if ctx.config.verbosity.is_debug() {
                ctx.logger
                    .info("Saved starting commit for incremental diff generation");
            }
        }
        Err(e) => {
            ctx.logger.warn(&format!(
                "Failed to save starting commit: {e}. \
                 Incremental diffs may be unavailable as a result."
            ));
            ctx.logger.info(
                "To fix this issue, ensure .agent directory is writable and you have a valid HEAD commit.",
            );
        }
    }

    // Display start commit information to user
    match get_start_commit_summary() {
        Ok(summary) => {
            if ctx.config.verbosity.is_debug() || summary.commits_since > 5 || summary.is_stale {
                ctx.logger.info(&summary.format_compact());
                if summary.is_stale {
                    ctx.logger.warn(
                        "Start commit is stale. Consider running: ralph --reset-start-commit",
                    );
                } else if summary.commits_since > 5 {
                    ctx.logger
                        .info("Tip: Run 'ralph --show-baseline' for more details");
                }
            }
        }
        Err(e) => {
            // Only show error in debug mode since this is informational
            if ctx.config.verbosity.is_debug() {
                ctx.logger
                    .warn(&format!("Failed to get start commit summary: {e}"));
            }
        }
    }
}

/// Check for PROMPT.md restoration after a phase.
fn check_prompt_restoration(
    ctx: &PipelineContext,
    prompt_monitor: &mut Option<PromptMonitor>,
    phase: &str,
) {
    if let Some(ref mut monitor) = prompt_monitor {
        if monitor.check_and_restore() {
            ctx.logger.warn(&format!(
                "PROMPT.md was deleted and restored during {phase} phase"
            ));
        }
    }
}

/// Handle --rebase-only flag.
///
/// This function performs a rebase to the default branch with AI conflict resolution and exits,
/// without running the full pipeline.
fn handle_rebase_only(
    _args: &Args,
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<()> {
    // Check if we're on main/master branch
    if is_main_or_master_branch()? {
        logger.warn("Already on main/master branch - rebasing on main is not recommended");
        logger.info("Tip: Use git worktrees to work on feature branches in parallel:");
        logger.info("  git worktree add ../feature-branch feature-branch");
        logger.info("This allows multiple AI agents to work on different features simultaneously.");
        logger.info("Proceeding with rebase anyway as requested...");
    }

    logger.header("Rebase to default branch", Colors::cyan);

    match run_rebase_to_default(logger, colors) {
        Ok(RebaseResult::Success) => {
            logger.success("Rebase completed successfully");
            Ok(())
        }
        Ok(RebaseResult::NoOp { reason }) => {
            logger.info(&format!("No rebase needed: {reason}"));
            Ok(())
        }
        Ok(RebaseResult::Failed(err)) => {
            logger.error(&format!("Rebase failed: {err}"));
            anyhow::bail!("Rebase failed: {err}")
        }
        Ok(RebaseResult::Conflicts(_conflicts)) => {
            // Get the actual conflicted files
            let conflicted_files = get_conflicted_files()?;
            if conflicted_files.is_empty() {
                logger.warn("Rebase reported conflicts but no conflicted files found");
                let _ = abort_rebase();
                return Ok(());
            }

            logger.warn(&format!(
                "Rebase resulted in {} conflict(s), attempting AI resolution",
                conflicted_files.len()
            ));

            // For --rebase-only, we don't have a full PhaseContext, so we use a wrapper
            match try_resolve_conflicts_without_phase_ctx(
                &conflicted_files,
                config,
                template_context,
                logger,
                colors,
            ) {
                Ok(true) => {
                    // Conflicts resolved, continue the rebase
                    logger.info("Continuing rebase after conflict resolution");
                    match continue_rebase() {
                        Ok(()) => {
                            logger.success("Rebase completed successfully after AI resolution");
                            Ok(())
                        }
                        Err(e) => {
                            logger.error(&format!("Failed to continue rebase: {e}"));
                            let _ = abort_rebase();
                            anyhow::bail!("Rebase failed after conflict resolution")
                        }
                    }
                }
                Ok(false) => {
                    // AI resolution failed
                    logger.error("AI conflict resolution failed, aborting rebase");
                    let _ = abort_rebase();
                    anyhow::bail!("Rebase conflicts could not be resolved by AI")
                }
                Err(e) => {
                    logger.error(&format!("Conflict resolution error: {e}"));
                    let _ = abort_rebase();
                    anyhow::bail!("Rebase conflict resolution failed: {e}")
                }
            }
        }
        Err(e) => {
            logger.error(&format!("Rebase failed: {e}"));
            anyhow::bail!("Rebase failed: {e}")
        }
    }
}

/// Run rebase to the default branch.
///
/// This function performs a rebase from the current branch to the
/// default branch (main/master). It handles all edge cases including:
/// - Already on main/master (proceeds with rebase attempt)
/// - Empty repository (returns `NoOp`)
/// - Upstream branch not found (error)
/// - Conflicts during rebase (returns `Conflicts` result)
///
/// # Returns
///
/// Returns `RebaseResult` indicating the outcome.
fn run_rebase_to_default(logger: &Logger, colors: Colors) -> std::io::Result<RebaseResult> {
    // Get the default branch
    let default_branch = get_default_branch()?;
    logger.info(&format!(
        "Rebasing onto {}{}{}",
        colors.cyan(),
        default_branch,
        colors.reset()
    ));

    // Perform the rebase
    rebase_onto(&default_branch)
}

/// Run initial rebase before development phase.
///
/// This function is called before the development phase starts to ensure
/// the feature branch is up-to-date with the default branch.
///
/// Uses a state machine for fault tolerance and automatic recovery from
/// interruptions or failures.
///
/// # Rebase Control
///
/// Rebase is only performed when both conditions are met:
/// - `--with-rebase` CLI flag is set (caller already checked this)
/// - `auto_rebase` config is enabled (checked here)
fn run_initial_rebase(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext,
    run_context: &crate::checkpoint::RunContext,
) -> anyhow::Result<()> {
    ctx.logger.header("Pre-development rebase", Colors::cyan);

    // Record execution step: pre-rebase started
    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_start",
        StepOutcome::success(None, vec![]),
    );
    phase_ctx.execution_history.add_step(step);

    // Save checkpoint at start of pre-rebase phase
    if ctx.config.features.checkpoint_enabled {
        let default_branch = get_default_branch().unwrap_or_else(|_| "main".to_string());
        let mut builder = CheckpointBuilder::new()
            .phase(PipelinePhase::PreRebase, 0, ctx.config.developer_iters)
            .reviewer_pass(0, ctx.config.reviewer_reviews)
            .capture_from_context(
                &ctx.config,
                &ctx.registry,
                &ctx.developer_agent,
                &ctx.reviewer_agent,
                &ctx.logger,
                run_context,
            );

        // Include prompt history and execution history for hardened resume
        builder = builder
            .with_execution_history(phase_ctx.execution_history.clone())
            .with_prompt_history(phase_ctx.clone_prompt_history());

        if let Some(mut checkpoint) = builder.build() {
            checkpoint.rebase_state = RebaseState::PreRebaseInProgress {
                upstream_branch: default_branch,
            };
            let _ = save_checkpoint(&checkpoint);
        }
    }

    match run_rebase_to_default(&ctx.logger, ctx.colors) {
        Ok(RebaseResult::Success) => {
            ctx.logger.success("Rebase completed successfully");
            // Record execution step: pre-rebase completed successfully
            let step = ExecutionStep::new(
                "PreRebase",
                0,
                "pre_rebase_complete",
                StepOutcome::success(None, vec![]),
            );
            phase_ctx.execution_history.add_step(step);

            // Save checkpoint after pre-rebase completes successfully
            if ctx.config.features.checkpoint_enabled {
                let builder = CheckpointBuilder::new()
                    .phase(PipelinePhase::Planning, 0, ctx.config.developer_iters)
                    .reviewer_pass(0, ctx.config.reviewer_reviews)
                    .skip_rebase(true) // Pre-rebase is done
                    .capture_from_context(
                        &ctx.config,
                        &ctx.registry,
                        &ctx.developer_agent,
                        &ctx.reviewer_agent,
                        &ctx.logger,
                        run_context,
                    )
                    .with_execution_history(phase_ctx.execution_history.clone())
                    .with_prompt_history(phase_ctx.clone_prompt_history());

                if let Some(checkpoint) = builder.build() {
                    let _ = save_checkpoint(&checkpoint);
                }
            }

            Ok(())
        }
        Ok(RebaseResult::NoOp { reason }) => {
            ctx.logger.info(&format!("No rebase needed: {reason}"));
            // Record execution step: pre-rebase skipped
            let step = ExecutionStep::new(
                "PreRebase",
                0,
                "pre_rebase_skipped",
                StepOutcome::skipped(reason.clone()),
            );
            phase_ctx.execution_history.add_step(step);

            // Save checkpoint after pre-rebase no-op
            if ctx.config.features.checkpoint_enabled {
                let builder = CheckpointBuilder::new()
                    .phase(PipelinePhase::Planning, 0, ctx.config.developer_iters)
                    .reviewer_pass(0, ctx.config.reviewer_reviews)
                    .skip_rebase(true) // Pre-rebase is done
                    .capture_from_context(
                        &ctx.config,
                        &ctx.registry,
                        &ctx.developer_agent,
                        &ctx.reviewer_agent,
                        &ctx.logger,
                        run_context,
                    )
                    .with_execution_history(phase_ctx.execution_history.clone())
                    .with_prompt_history(phase_ctx.clone_prompt_history());

                if let Some(checkpoint) = builder.build() {
                    let _ = save_checkpoint(&checkpoint);
                }
            }

            Ok(())
        }
        Ok(RebaseResult::Conflicts(_conflicts)) => {
            // Get the actual conflicted files
            let conflicted_files = get_conflicted_files()?;
            if conflicted_files.is_empty() {
                ctx.logger
                    .warn("Rebase reported conflicts but no conflicted files found");
                let _ = abort_rebase();
                return Ok(());
            }

            // Record execution step: pre-rebase conflicts detected
            let step = ExecutionStep::new(
                "PreRebase",
                0,
                "pre_rebase_conflict",
                StepOutcome::partial(
                    "Rebase started".to_string(),
                    format!("{} conflicts detected", conflicted_files.len()),
                ),
            );
            phase_ctx.execution_history.add_step(step);

            // Save checkpoint for conflict state
            if ctx.config.features.checkpoint_enabled {
                let mut builder = CheckpointBuilder::new()
                    .phase(
                        PipelinePhase::PreRebaseConflict,
                        0,
                        ctx.config.developer_iters,
                    )
                    .reviewer_pass(0, ctx.config.reviewer_reviews)
                    .capture_from_context(
                        &ctx.config,
                        &ctx.registry,
                        &ctx.developer_agent,
                        &ctx.reviewer_agent,
                        &ctx.logger,
                        run_context,
                    );

                // Include prompt history and execution history for hardened resume
                builder = builder
                    .with_execution_history(phase_ctx.execution_history.clone())
                    .with_prompt_history(phase_ctx.clone_prompt_history());

                if let Some(mut checkpoint) = builder.build() {
                    checkpoint.rebase_state = RebaseState::HasConflicts {
                        files: conflicted_files.clone(),
                    };
                    let _ = save_checkpoint(&checkpoint);
                }
            }

            ctx.logger.warn(&format!(
                "Rebase resulted in {} conflict(s), attempting AI resolution",
                conflicted_files.len()
            ));

            // Attempt to resolve conflicts with AI
            match try_resolve_conflicts_with_fallback(
                &conflicted_files,
                &ctx.config,
                &ctx.template_context,
                &ctx.logger,
                ctx.colors,
                phase_ctx,
                "PreRebase",
            ) {
                Ok(true) => {
                    // Conflicts resolved, continue the rebase
                    ctx.logger
                        .info("Continuing rebase after conflict resolution");
                    match continue_rebase() {
                        Ok(()) => {
                            ctx.logger
                                .success("Rebase completed successfully after AI resolution");
                            // Record execution step: conflicts resolved successfully
                            let step = ExecutionStep::new(
                                "PreRebase",
                                0,
                                "pre_rebase_resolution",
                                StepOutcome::success(None, vec![]),
                            );
                            phase_ctx.execution_history.add_step(step);

                            // Save checkpoint after pre-rebase conflict resolution completes
                            if ctx.config.features.checkpoint_enabled {
                                let builder = CheckpointBuilder::new()
                                    .phase(PipelinePhase::Planning, 0, ctx.config.developer_iters)
                                    .reviewer_pass(0, ctx.config.reviewer_reviews)
                                    .skip_rebase(true) // Pre-rebase is done
                                    .capture_from_context(
                                        &ctx.config,
                                        &ctx.registry,
                                        &ctx.developer_agent,
                                        &ctx.reviewer_agent,
                                        &ctx.logger,
                                        run_context,
                                    )
                                    .with_execution_history(phase_ctx.execution_history.clone())
                                    .with_prompt_history(phase_ctx.clone_prompt_history());

                                if let Some(checkpoint) = builder.build() {
                                    let _ = save_checkpoint(&checkpoint);
                                }
                            }

                            Ok(())
                        }
                        Err(e) => {
                            ctx.logger.warn(&format!("Failed to continue rebase: {e}"));
                            let _ = abort_rebase();
                            // Record execution step: resolution succeeded but continue failed
                            let step = ExecutionStep::new(
                                "PreRebase",
                                0,
                                "pre_rebase_resolution",
                                StepOutcome::partial(
                                    "Conflicts resolved by AI".to_string(),
                                    format!("Failed to continue rebase: {e}"),
                                ),
                            );
                            phase_ctx.execution_history.add_step(step);
                            Ok(()) // Continue anyway - conflicts were resolved
                        }
                    }
                }
                Ok(false) => {
                    // AI resolution failed
                    ctx.logger
                        .warn("AI conflict resolution failed, aborting rebase");
                    let _ = abort_rebase();
                    // Record execution step: resolution failed
                    let step = ExecutionStep::new(
                        "PreRebase",
                        0,
                        "pre_rebase_resolution",
                        StepOutcome::failure("AI conflict resolution failed".to_string(), true),
                    );
                    phase_ctx.execution_history.add_step(step);
                    Ok(()) // Continue pipeline - don't block on rebase failure
                }
                Err(e) => {
                    ctx.logger.error(&format!("Conflict resolution error: {e}"));
                    let _ = abort_rebase();
                    // Record execution step: resolution error
                    let step = ExecutionStep::new(
                        "PreRebase",
                        0,
                        "pre_rebase_resolution",
                        StepOutcome::failure(format!("Conflict resolution error: {e}"), true),
                    );
                    phase_ctx.execution_history.add_step(step);
                    Ok(()) // Continue pipeline
                }
            }
        }
        Ok(RebaseResult::Failed(err)) => {
            ctx.logger.error(&format!("Rebase failed: {err}"));
            let _ = abort_rebase();
            // Record execution step: rebase failed
            let step = ExecutionStep::new(
                "PreRebase",
                0,
                "pre_rebase_failed",
                StepOutcome::failure(format!("Rebase failed: {err}"), true),
            );
            phase_ctx.execution_history.add_step(step);
            Ok(()) // Continue pipeline despite failure
        }
        Err(e) => {
            ctx.logger
                .warn(&format!("Rebase failed, continuing without rebase: {e}"));
            // Record execution step: rebase error
            let step = ExecutionStep::new(
                "PreRebase",
                0,
                "pre_rebase_error",
                StepOutcome::failure(format!("Rebase error: {e}"), true),
            );
            phase_ctx.execution_history.add_step(step);
            Ok(())
        }
    }
}

/// Result type for conflict resolution attempts.
///
/// Represents the different ways conflict resolution can succeed or fail.
enum ConflictResolutionResult {
    /// Agent provided JSON output with resolved file contents
    WithJson(String),
    /// Agent resolved conflicts by editing files directly (no JSON output)
    FileEditsOnly,
    /// Resolution failed completely
    Failed,
}

/// Attempt to resolve rebase conflicts with AI fallback.
///
/// This function accepts `PhaseContext` to capture prompts and track
/// execution history for hardened resume functionality.
fn try_resolve_conflicts_with_fallback(
    conflicted_files: &[String],
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
    phase_ctx: &mut PhaseContext<'_>,
    phase: &str,
) -> anyhow::Result<bool> {
    if conflicted_files.is_empty() {
        return Ok(false);
    }

    logger.info(&format!(
        "Attempting AI conflict resolution for {} file(s)",
        conflicted_files.len()
    ));

    let conflicts = collect_conflict_info_or_error(conflicted_files, logger)?;

    // Use stored_or_generate pattern for hardened resume
    // On resume, use the exact same prompt that was used before
    let prompt_key = format!("{}_conflict_resolution", phase.to_lowercase());
    let (resolution_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &phase_ctx.prompt_history, || {
            build_resolution_prompt(&conflicts, template_context)
        });

    // Capture the resolution prompt for deterministic resume (only if newly generated)
    if !was_replayed {
        phase_ctx.capture_prompt(&prompt_key, &resolution_prompt);
    } else {
        logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    match run_ai_conflict_resolution(&resolution_prompt, config, logger, colors) {
        Ok(ConflictResolutionResult::WithJson(resolved_content)) => {
            // Agent provided JSON output - attempt to parse and write files
            // JSON is optional for verification - LibGit2 state is authoritative
            match parse_and_validate_resolved_files(&resolved_content, logger) {
                Ok(resolved_files) => {
                    write_resolved_files(&resolved_files, logger)?;
                }
                Err(_) => {
                    // JSON parsing failed - this is expected and normal
                    // We verify conflicts via LibGit2 state, not JSON parsing
                    // Continue to LibGit2 verification below
                }
            }

            // Verify all conflicts are resolved via LibGit2 (authoritative source)
            let remaining_conflicts = get_conflicted_files()?;
            if remaining_conflicts.is_empty() {
                Ok(true)
            } else {
                logger.warn(&format!(
                    "{} conflicts remain after AI resolution",
                    remaining_conflicts.len()
                ));
                Ok(false)
            }
        }
        Ok(ConflictResolutionResult::FileEditsOnly) => {
            // Agent resolved conflicts by editing files directly
            logger.info("Agent resolved conflicts via file edits (no JSON output)");

            // Verify all conflicts are resolved
            let remaining_conflicts = get_conflicted_files()?;
            if remaining_conflicts.is_empty() {
                logger.success("All conflicts resolved via file edits");
                Ok(true)
            } else {
                logger.warn(&format!(
                    "{} conflicts remain after AI resolution",
                    remaining_conflicts.len()
                ));
                Ok(false)
            }
        }
        Ok(ConflictResolutionResult::Failed) => {
            logger.warn("AI conflict resolution failed");
            logger.info("Attempting to continue rebase anyway...");

            // Try to continue rebase - user may have manually resolved conflicts
            match crate::git_helpers::continue_rebase() {
                Ok(()) => {
                    logger.info("Successfully continued rebase");
                    Ok(true)
                }
                Err(rebase_err) => {
                    logger.warn(&format!("Failed to continue rebase: {rebase_err}"));
                    Ok(false) // Conflicts remain
                }
            }
        }
        Err(e) => {
            logger.warn(&format!("AI conflict resolution failed: {e}"));
            logger.info("Attempting to continue rebase anyway...");

            // Try to continue rebase - user may have manually resolved conflicts
            match crate::git_helpers::continue_rebase() {
                Ok(()) => {
                    logger.info("Successfully continued rebase");
                    Ok(true)
                }
                Err(rebase_err) => {
                    logger.warn(&format!("Failed to continue rebase: {rebase_err}"));
                    Ok(false) // Conflicts remain
                }
            }
        }
    }
}

/// Wrapper for conflict resolution without PhaseContext.
///
/// This is used for --rebase-only mode where we don't have a full pipeline context.
/// It creates a minimal PhaseContext for the conflict resolution call.
fn try_resolve_conflicts_without_phase_ctx(
    conflicted_files: &[String],
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<bool> {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::execution_history::ExecutionHistory;
    use crate::checkpoint::RunContext;
    use crate::pipeline::{Stats, Timer};

    // Create minimal PhaseContext for conflict resolution
    let registry = AgentRegistry::new()?;
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let reviewer_agent = config.reviewer_agent.as_deref().unwrap_or("codex");
    let developer_agent = config.developer_agent.as_deref().unwrap_or("codex");

    let mut phase_ctx = PhaseContext {
        config,
        registry: &registry,
        logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent,
        reviewer_agent,
        review_guidelines: None,
        template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
    };

    try_resolve_conflicts_with_fallback(
        conflicted_files,
        config,
        template_context,
        logger,
        colors,
        &mut phase_ctx,
        "RebaseOnly",
    )
}

/// Collect conflict information from conflicted files.
fn collect_conflict_info_or_error(
    conflicted_files: &[String],
    logger: &Logger,
) -> anyhow::Result<std::collections::HashMap<String, crate::prompts::FileConflict>> {
    use crate::prompts::collect_conflict_info;

    let conflicts = match collect_conflict_info(conflicted_files) {
        Ok(c) => c,
        Err(e) => {
            logger.error(&format!("Failed to collect conflict info: {e}"));
            anyhow::bail!("Failed to collect conflict info");
        }
    };
    Ok(conflicts)
}

/// Build the conflict resolution prompt from context files.
fn build_resolution_prompt(
    conflicts: &std::collections::HashMap<String, crate::prompts::FileConflict>,
    template_context: &TemplateContext,
) -> String {
    build_enhanced_resolution_prompt(conflicts, None::<()>, template_context)
        .unwrap_or_else(|_| String::new())
}

/// Build the conflict resolution prompt.
///
/// This function uses the standard conflict resolution prompt.
fn build_enhanced_resolution_prompt(
    conflicts: &std::collections::HashMap<String, crate::prompts::FileConflict>,
    _branch_info: Option<()>, // Kept for API compatibility, currently unused
    template_context: &TemplateContext,
) -> anyhow::Result<String> {
    use std::fs;

    let prompt_md_content = fs::read_to_string("PROMPT.md").ok();
    let plan_content = fs::read_to_string(".agent/PLAN.md").ok();

    // Use standard prompt
    Ok(
        crate::prompts::build_conflict_resolution_prompt_with_context(
            template_context,
            conflicts,
            prompt_md_content.as_deref(),
            plan_content.as_deref(),
        ),
    )
}

/// Run AI agent to resolve conflicts with fallback mechanism.
///
/// Returns `ConflictResolutionResult` indicating whether the agent provided
/// JSON output, resolved conflicts via file edits, or failed completely.
fn run_ai_conflict_resolution(
    resolution_prompt: &str,
    config: &crate::config::Config,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<ConflictResolutionResult> {
    use crate::agents::AgentRegistry;
    use crate::files::result_extraction::extract_last_result;
    use crate::pipeline::{
        run_with_fallback_and_validator, FallbackConfig, OutputValidator, PipelineRuntime,
    };
    use std::io;
    use std::path::Path;

    // Note: log_dir is used as a prefix for log file names, not as a directory.
    // The actual log files will be created in .agent/logs/ with names like:
    // .agent/logs/rebase_conflict_resolution_ccs-glm_0.log
    let log_dir = ".agent/logs/rebase_conflict_resolution";

    let registry = AgentRegistry::new()?;
    let reviewer_agent = config.reviewer_agent.as_deref().unwrap_or("codex");

    let mut runtime = PipelineRuntime {
        timer: &mut crate::pipeline::Timer::new(),
        logger,
        colors: &colors,
        config,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Output validator: checks if agent produced valid output OR resolved conflicts
    // Agents may edit files without returning JSON, so we verify conflicts are resolved.
    let validate_output: OutputValidator = |log_dir_path: &Path,
                                            validation_logger: &crate::logger::Logger|
     -> io::Result<bool> {
        match extract_last_result(log_dir_path) {
            Ok(Some(_)) => {
                // Valid JSON output exists
                Ok(true)
            }
            Ok(None) => {
                // No JSON output - check if conflicts were resolved anyway
                // (agent may have edited files without returning JSON)
                match crate::git_helpers::get_conflicted_files() {
                    Ok(conflicts) if conflicts.is_empty() => {
                        validation_logger
                            .info("Agent resolved conflicts without JSON output (file edits only)");
                        Ok(true) // Conflicts resolved, consider success
                    }
                    Ok(conflicts) => {
                        validation_logger.warn(&format!(
                            "{} conflict(s) remain unresolved",
                            conflicts.len()
                        ));
                        Ok(false) // Conflicts remain
                    }
                    Err(e) => {
                        validation_logger.warn(&format!("Failed to check for conflicts: {e}"));
                        Ok(false) // Error checking conflicts
                    }
                }
            }
            Err(e) => {
                validation_logger.warn(&format!("Output validation check failed: {e}"));
                Ok(false) // Treat validation errors as missing output
            }
        }
    };

    let mut fallback_config = FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "conflict resolution",
        prompt: resolution_prompt,
        logfile_prefix: log_dir,
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: reviewer_agent,
        output_validator: Some(validate_output),
    };

    let exit_code = run_with_fallback_and_validator(&mut fallback_config)?;

    if exit_code != 0 {
        return Ok(ConflictResolutionResult::Failed);
    }

    // Check if conflicts are resolved after agent run
    // The validator already checked this, but we verify again to determine the result type
    let remaining_conflicts = crate::git_helpers::get_conflicted_files()?;

    if remaining_conflicts.is_empty() {
        // Conflicts are resolved - check if agent provided JSON output
        match extract_last_result(Path::new(log_dir)) {
            Ok(Some(content)) => {
                logger.info("Agent provided JSON output with resolved files");
                Ok(ConflictResolutionResult::WithJson(content))
            }
            Ok(None) => {
                logger.info("Agent resolved conflicts via file edits (no JSON output)");
                Ok(ConflictResolutionResult::FileEditsOnly)
            }
            Err(e) => {
                // Extraction failed but conflicts are resolved - treat as file edits only
                logger.warn(&format!(
                    "Failed to extract JSON output but conflicts are resolved: {e}"
                ));
                Ok(ConflictResolutionResult::FileEditsOnly)
            }
        }
    } else {
        logger.warn(&format!(
            "{} conflict(s) remain after agent attempted resolution",
            remaining_conflicts.len()
        ));
        Ok(ConflictResolutionResult::Failed)
    }
}

/// Parse and validate the resolved files from AI output.
///
/// JSON parsing failures are expected and handled gracefully - LibGit2 state
/// is used for verification, not JSON output. This function only parses the
/// JSON to write resolved files if available.
fn parse_and_validate_resolved_files(
    resolved_content: &str,
    logger: &Logger,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let json: serde_json::Value = serde_json::from_str(resolved_content).map_err(|_e| {
        // Agent did not provide JSON output - fall back to LibGit2 verification
        // This is expected and normal, not an error condition
        anyhow::anyhow!("Agent did not provide JSON output (will verify via Git state)")
    })?;

    let resolved_files = match json.get("resolved_files") {
        Some(v) if v.is_object() => v.as_object().unwrap(),
        _ => {
            logger.info("Agent output missing 'resolved_files' object");
            anyhow::bail!("Agent output missing 'resolved_files' object");
        }
    };

    if resolved_files.is_empty() {
        logger.info("No resolved files in JSON output");
        anyhow::bail!("No files were resolved by the agent");
    }

    Ok(resolved_files.clone())
}

/// Write resolved files to disk and stage them.
fn write_resolved_files(
    resolved_files: &serde_json::Map<String, serde_json::Value>,
    logger: &Logger,
) -> anyhow::Result<usize> {
    use std::fs;

    let mut files_written = 0;
    for (path, content) in resolved_files {
        if let Some(content_str) = content.as_str() {
            fs::write(path, content_str).map_err(|e| {
                logger.error(&format!("Failed to write {path}: {e}"));
                anyhow::anyhow!("Failed to write {path}: {e}")
            })?;
            logger.info(&format!("Resolved and wrote: {path}"));
            files_written += 1;
            // Stage the resolved file
            if let Err(e) = crate::git_helpers::git_add_all() {
                logger.warn(&format!("Failed to stage {path}: {e}"));
            }
        }
    }

    logger.success(&format!("Successfully resolved {files_written} file(s)"));
    Ok(files_written)
}
