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
pub mod finalization;
pub mod plumbing;
pub mod resume;
pub mod validation;

use crate::agents::AgentRegistry;
use crate::app::finalization::finalize_pipeline;
use crate::app::resume::{phase_rank, should_run_from};
use crate::banner::print_welcome_banner;
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::cli::{
    create_prompt_from_template, handle_diagnose, handle_dry_run, handle_list_agents,
    handle_list_available_agents, handle_list_providers, prompt_template_selection, Args,
};
use crate::common::utils;
use crate::files::protection::monitoring::PromptMonitor;
use crate::files::{
    create_prompt_backup, ensure_files, make_prompt_read_only, reset_context_for_isolation,
    update_status, validate_prompt_md,
};
use crate::git_helpers::{
    cleanup_orphaned_marker, get_repo_root, require_git_repo, reset_start_commit,
    save_start_commit, start_agent_phase,
};
use crate::logger::Colors;
use crate::logger::Logger;
use crate::phases::{run_development_phase, run_review_phase, PhaseContext};
use crate::pipeline::{AgentPhaseGuard, Stats, Timer};
use std::env;
use std::process::Command;

use config_init::initialize_config;
use context::PipelineContext;
use detection::detect_project_stack;
use plumbing::{handle_apply_commit, handle_generate_commit_msg, handle_show_commit_msg};
use resume::handle_resume;
use validation::{
    resolve_required_agents, validate_agent_chains, validate_agent_commands, validate_can_commit,
};

/// Handle plumbing commands that need git repo but not full validation.
///
/// Returns `Ok(Some(()))` if a plumbing command was handled, `Ok(None)` if not.
fn handle_plumbing_commands(
    args: &Args,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<Option<()>> {
    if args.commit_display.show_commit_msg {
        handle_show_commit_msg()?;
        return Ok(Some(()));
    }
    if args.commit_plumbing.apply_commit {
        handle_apply_commit(logger, colors)?;
        return Ok(Some(()));
    }
    if args.commit_display.reset_start_commit {
        require_git_repo()?;
        let repo_root = get_repo_root()?;
        env::set_current_dir(&repo_root)?;

        match reset_start_commit() {
            Ok(()) => {
                logger.success("Starting commit reference reset to current HEAD");
                logger.info(".agent/start_commit has been updated");
                return Ok(Some(()));
            }
            Err(e) => {
                logger.error(&format!("Failed to reset starting commit: {e}"));
                anyhow::bail!("Failed to reset starting commit");
            }
        }
    }
    Ok(None)
}

/// Handle interactive mode prompt creation for missing PROMPT.md.
///
/// Returns `Ok(Some(()))` if prompt was created and we should exit, `Ok(None)` if not.
fn handle_interactive_prompt_creation(
    args: &Args,
    config: &crate::config::types::Config,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<Option<()>> {
    if !args.interactive || std::path::Path::new("PROMPT.md").exists() {
        return Ok(None);
    }

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
        return Ok(Some(()));
    }

    println!();
    logger.info("PROMPT.md is required to run the pipeline.");
    logger.info(
        "Create one with 'ralph --init-prompt <template>' (see: 'ralph --list-templates'), then rerun.",
    );
    Ok(Some(()))
}

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
    let mut logger = Logger::new(colors);

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

    // Get display names for UI/logging
    let developer_display = registry.display_name(&developer_agent);
    let reviewer_display = registry.display_name(&reviewer_agent);

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
    if handle_plumbing_commands(&args, &logger, colors)?.is_some() {
        return Ok(());
    }

    // Validate agent commands exist
    validate_agent_commands(
        &config,
        &registry,
        &developer_agent,
        &reviewer_agent,
        &config_path,
    )?;

    // Validate agents are workflow-capable
    validate_can_commit(
        &config,
        &registry,
        &developer_agent,
        &reviewer_agent,
        &config_path,
    )?;

    // Set up git repo and working directory
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;

    // Handle interactive mode prompt creation
    if handle_interactive_prompt_creation(&args, &config, &logger, colors)?.is_some() {
        return Ok(());
    }

    ensure_files(config.isolation_mode)?;

    // Reset context for isolation mode
    if config.isolation_mode {
        reset_context_for_isolation(&logger)?;
    }

    logger = logger.with_log_file(".agent/logs/pipeline.log");

    // Handle --dry-run
    if args.recovery.dry_run {
        return handle_dry_run(
            &logger,
            colors,
            &config,
            &developer_display,
            &reviewer_display,
            &repo_root,
        );
    }

    // Handle --generate-commit-msg
    if args.commit_plumbing.generate_commit_msg {
        return handle_generate_commit_msg(
            &config,
            &registry,
            &logger,
            colors,
            &developer_agent,
            &reviewer_agent,
        );
    }

    // Run the full pipeline
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
    };
    run_pipeline(&ctx)
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
    false
}

/// Validate and protect PROMPT.md file.
fn setup_prompt_protection(
    strict_validation: bool,
    interactive: bool,
    logger: &Logger,
) -> anyhow::Result<()> {
    let prompt_validation = validate_prompt_md(strict_validation, interactive);
    for err in &prompt_validation.errors {
        logger.error(err);
    }
    for warn in &prompt_validation.warnings {
        logger.warn(warn);
    }
    if !prompt_validation.is_valid() {
        anyhow::bail!("PROMPT.md validation errors");
    }

    match create_prompt_backup() {
        Ok(None) => {}
        Ok(Some(warning)) => {
            logger.warn(&format!(
                "PROMPT.md backup created but: {warning}. Continuing anyway."
            ));
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to create PROMPT.md backup: {e}. Continuing anyway."
            ));
        }
    }

    match make_prompt_read_only() {
        None => {}
        Some(warning) => {
            logger.warn(&format!("{warning}. Continuing anyway."));
        }
    }

    Ok(())
}

/// Set up PROMPT.md monitoring for real-time deletion detection.
fn setup_prompt_monitor(
    logger: &Logger,
    verbosity: crate::config::types::Verbosity,
) -> Option<PromptMonitor> {
    match PromptMonitor::new() {
        Ok(mut monitor) => {
            if let Err(e) = monitor.start() {
                logger.warn(&format!(
                    "Failed to start PROMPT.md monitoring: {e}. Continuing anyway."
                ));
                None
            } else {
                if verbosity.is_debug() {
                    logger.info("Started real-time PROMPT.md monitoring");
                }
                Some(monitor)
            }
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to create PROMPT.md monitor: {e}. Continuing anyway."
            ));
            None
        }
    }
}

/// Print welcome banner and project information.
fn print_welcome_info(
    colors: Colors,
    developer_display: &str,
    reviewer_display: &str,
    repo_root: &std::path::Path,
    commit_msg: &str,
    logger: &Logger,
) {
    print_welcome_banner(colors, developer_display, reviewer_display);
    logger.info(&format!(
        "Working directory: {}{}{}",
        colors.cyan(),
        repo_root.display(),
        colors.reset()
    ));
    logger.info(&format!(
        "Commit message: {}{}{}",
        colors.cyan(),
        commit_msg,
        colors.reset()
    ));
}

/// Save the starting commit reference for incremental diff generation.
fn save_starting_commit(logger: &Logger, verbosity: crate::config::types::Verbosity) {
    match save_start_commit() {
        Ok(()) => {
            if verbosity.is_debug() {
                logger.info("Saved starting commit for incremental diff generation");
            }
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to save starting commit: {e}. \
                 Incremental diffs may be unavailable as a result."
            ));
            logger.info(
                "To fix this issue, ensure .agent directory is writable and you have a valid HEAD commit.",
            );
        }
    }
}

/// Run pipeline phases with monitoring.
fn run_pipeline_phases(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext,
    prompt_monitor: &mut Option<PromptMonitor>,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> anyhow::Result<()> {
    run_development(phase_ctx, &ctx.args, resume_checkpoint)?;

    if let Some(ref mut monitor) = prompt_monitor {
        if monitor.check_and_restore() {
            ctx.logger
                .warn("PROMPT.md was deleted and restored during development phase");
        }
    }
    update_status("In progress.", ctx.config.isolation_mode)?;

    run_review_and_fix(phase_ctx, &ctx.args, resume_checkpoint)?;

    if let Some(ref mut monitor) = prompt_monitor {
        if monitor.check_and_restore() {
            ctx.logger
                .warn("PROMPT.md was deleted and restored during review phase");
        }
    }
    update_status("In progress.", ctx.config.isolation_mode)?;

    run_final_validation(phase_ctx, resume_checkpoint)?;

    Ok(())
}

/// Runs the full development/review/commit pipeline.
fn run_pipeline(ctx: &PipelineContext) -> anyhow::Result<()> {
    let resume_checkpoint = handle_resume(
        &ctx.args,
        &ctx.logger,
        &ctx.developer_display,
        &ctx.reviewer_display,
    );

    let mut git_helpers = crate::git_helpers::GitHelpers::new();
    cleanup_orphaned_marker(&ctx.logger)?;
    start_agent_phase(&mut git_helpers)?;
    let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &ctx.logger);

    print_welcome_info(
        ctx.colors,
        &ctx.developer_display,
        &ctx.reviewer_display,
        &ctx.repo_root,
        &ctx.config.commit_msg,
        &ctx.logger,
    );

    setup_prompt_protection(
        ctx.config.behavior.strict_validation,
        ctx.args.interactive,
        &ctx.logger,
    )?;

    let mut prompt_monitor = setup_prompt_monitor(&ctx.logger, ctx.config.verbosity);

    let (_project_stack, review_guidelines) =
        detect_project_stack(&ctx.config, &ctx.repo_root, &ctx.logger, ctx.colors);

    if let Some(ref guidelines) = review_guidelines {
        ctx.logger.info(&format!(
            "Review guidelines: {}{}{}",
            ctx.colors.dim(),
            guidelines.summary(),
            ctx.colors.reset()
        ));
    }

    println!();

    let mut timer = Timer::new();
    let mut stats = Stats::new();
    let mut phase_ctx = PhaseContext {
        config: &ctx.config,
        registry: &ctx.registry,
        logger: &ctx.logger,
        colors: &ctx.colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: &ctx.developer_agent,
        reviewer_agent: &ctx.reviewer_agent,
        review_guidelines: review_guidelines.as_ref(),
    };

    save_starting_commit(&ctx.logger, ctx.config.verbosity);

    run_pipeline_phases(
        ctx,
        &mut phase_ctx,
        &mut prompt_monitor,
        resume_checkpoint.as_ref(),
    )?;

    finalize_pipeline(
        &mut agent_phase_guard,
        &ctx.logger,
        ctx.colors,
        &ctx.config,
        &timer,
        &stats,
        prompt_monitor,
    );
    Ok(())
}

/// Runs the development phase.
fn run_development(
    ctx: &mut PhaseContext,
    args: &Args,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> anyhow::Result<()> {
    ctx.logger
        .header("PHASE 1: Development", crate::logger::Colors::blue);

    let resume_phase = resume_checkpoint.map(|c| c.phase);
    let resume_rank = resume_phase.map(phase_rank);

    if resume_rank.is_some_and(|rank| rank >= phase_rank(PipelinePhase::Review)) {
        ctx.logger
            .info("Skipping development phase (checkpoint indicates it already completed)");
        return Ok(());
    }

    if !should_run_from(PipelinePhase::Planning, resume_checkpoint) {
        ctx.logger
            .info("Skipping development phase (resuming from a later checkpoint phase)");
        return Ok(());
    }

    let start_iter = match resume_phase {
        Some(PipelinePhase::Planning | PipelinePhase::Development) => resume_checkpoint
            .map_or(1, |c| c.iteration)
            .clamp(1, ctx.config.developer_iters),
        _ => 1,
    };

    let resuming_from_development =
        args.recovery.resume && resume_phase == Some(PipelinePhase::Development);
    let development_result = run_development_phase(ctx, start_iter, resuming_from_development)?;

    if development_result.had_errors {
        ctx.logger
            .warn("Development phase completed with non-fatal errors");
    }

    Ok(())
}

/// Runs the review and fix phase.
fn run_review_and_fix(
    ctx: &mut PhaseContext,
    _args: &Args,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> anyhow::Result<()> {
    ctx.logger
        .header("PHASE 2: Review & Fix", crate::logger::Colors::magenta);

    let resume_phase = resume_checkpoint.map(|c| c.phase);

    // Check if we should run any reviewer phase
    let run_any_reviewer_phase = should_run_from(PipelinePhase::Review, resume_checkpoint)
        || should_run_from(PipelinePhase::Fix, resume_checkpoint)
        || should_run_from(PipelinePhase::ReviewAgain, resume_checkpoint)
        || should_run_from(PipelinePhase::CommitMessage, resume_checkpoint);

    let should_run_review_phase = should_run_from(PipelinePhase::Review, resume_checkpoint)
        || resume_phase == Some(PipelinePhase::Fix)
        || resume_phase == Some(PipelinePhase::ReviewAgain);

    if should_run_review_phase && ctx.config.reviewer_reviews > 0 {
        let start_pass = match resume_phase {
            Some(PipelinePhase::Review | PipelinePhase::Fix | PipelinePhase::ReviewAgain) => {
                resume_checkpoint
                    .map_or(1, |c| c.reviewer_pass)
                    .clamp(1, ctx.config.reviewer_reviews.max(1))
            }
            _ => 1,
        };

        let review_result = run_review_phase(ctx, start_pass)?;
        if review_result.completed_early {
            ctx.logger
                .success("Review phase completed early (no issues found)");
        }
    } else if run_any_reviewer_phase && ctx.config.reviewer_reviews == 0 {
        ctx.logger
            .info("Skipping review phase (reviewer_reviews=0)");
    } else if run_any_reviewer_phase {
        ctx.logger
            .info("Skipping review-fix cycles (resuming from a later checkpoint phase)");
    }

    // Note: The old dedicated commit phase has been removed.
    // Commits now happen automatically per-iteration during development and per-cycle during review.

    Ok(())
}

/// Runs final validation if configured.
fn run_final_validation(
    ctx: &PhaseContext,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> anyhow::Result<()> {
    let Some(ref full_cmd) = ctx.config.full_check_cmd else {
        return Ok(());
    };

    if !should_run_from(PipelinePhase::FinalValidation, resume_checkpoint) {
        ctx.logger
            .header("PHASE 3: Final Validation", crate::logger::Colors::yellow);
        ctx.logger
            .info("Skipping final validation (resuming from a later checkpoint phase)");
        return Ok(());
    }

    let argv = utils::split_command(full_cmd)
        .map_err(|e| anyhow::anyhow!("FULL_CHECK_CMD parse error: {e}"))?;
    if argv.is_empty() {
        ctx.logger
            .warn("FULL_CHECK_CMD is empty; skipping final validation");
        return Ok(());
    }

    if ctx.config.features.checkpoint_enabled {
        let _ = save_checkpoint(&PipelineCheckpoint::new(
            PipelinePhase::FinalValidation,
            ctx.config.developer_iters,
            ctx.config.developer_iters,
            ctx.config.reviewer_reviews,
            ctx.config.reviewer_reviews,
            ctx.developer_agent,
            ctx.reviewer_agent,
        ));
    }

    ctx.logger
        .header("PHASE 3: Final Validation", crate::logger::Colors::yellow);
    let display_cmd = utils::format_argv_for_log(&argv);
    ctx.logger.info(&format!(
        "Running full check: {}{}{}",
        ctx.colors.dim(),
        display_cmd,
        ctx.colors.reset()
    ));

    let Some((program, arguments)) = argv.split_first() else {
        ctx.logger
            .error("FULL_CHECK_CMD is empty after parsing; skipping final validation");
        return Ok(());
    };
    let status = Command::new(program).args(arguments).status()?;

    if status.success() {
        ctx.logger.success("Full check passed");
    } else {
        ctx.logger.error("Full check failed");
        anyhow::bail!("Full check failed");
    }

    Ok(())
}
