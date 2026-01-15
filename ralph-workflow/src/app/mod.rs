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
    abort_rebase, cleanup_orphaned_marker, get_default_branch, get_repo_root,
    is_main_or_master_branch, rebase_onto, require_git_repo, reset_start_commit, save_start_commit,
    start_agent_phase, RebaseResult,
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

    // Handle plumbing commands (these need git repo but not full validation)
    if args.commit_display.show_commit_msg {
        return handle_show_commit_msg();
    }
    if args.commit_plumbing.apply_commit {
        return handle_apply_commit(&logger, colors);
    }
    if args.commit_display.reset_start_commit {
        require_git_repo()?;
        let repo_root = get_repo_root()?;
        env::set_current_dir(&repo_root)?;

        match reset_start_commit() {
            Ok(()) => {
                logger.success("Starting commit reference reset to current HEAD");
                logger.info(".agent/start_commit has been updated");
                return Ok(());
            }
            Err(e) => {
                logger.error(&format!("Failed to reset starting commit: {e}"));
                anyhow::bail!("Failed to reset starting commit");
            }
        }
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

    // Handle --rebase-only
    if args.rebase_flags.rebase_only {
        return handle_rebase_only(&args, &config, &logger, colors);
    }

    // In interactive mode, prompt to create PROMPT.md from a template BEFORE ensure_files().
    // If the user declines (or we can't prompt), exit without creating a placeholder PROMPT.md.
    if config.behavior.interactive && !std::path::Path::new("PROMPT.md").exists() {
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
            return Ok(());
        }
        println!();
        logger.info("PROMPT.md is required to run the pipeline.");
        logger.info(
            "Create one with 'ralph --init-prompt <template>' (see: 'ralph --list-templates'), then rerun.",
        );
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

/// Runs the full development/review/commit pipeline.
fn run_pipeline(ctx: &PipelineContext) -> anyhow::Result<()> {
    // Handle --resume
    let resume_checkpoint = handle_resume(
        &ctx.args,
        &ctx.logger,
        &ctx.developer_display,
        &ctx.reviewer_display,
    );

    // Set up git helpers and agent phase
    let mut git_helpers = crate::git_helpers::GitHelpers::new();
    cleanup_orphaned_marker(&ctx.logger)?;
    start_agent_phase(&mut git_helpers)?;
    let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &ctx.logger);

    // Print welcome banner and validate PROMPT.md
    print_welcome_banner(ctx.colors, &ctx.developer_display, &ctx.reviewer_display);
    print_pipeline_info(ctx);
    validate_prompt_and_setup_backup(ctx)?;

    // Set up PROMPT.md monitoring
    let mut prompt_monitor = setup_prompt_monitor(ctx);

    // Detect project stack and review guidelines
    let (_project_stack, review_guidelines) =
        detect_project_stack(&ctx.config, &ctx.repo_root, &ctx.logger, ctx.colors);

    print_review_guidelines(ctx, review_guidelines.as_ref());
    println!();

    // Create phase context and save starting commit
    let (mut timer, mut stats) = (Timer::new(), Stats::new());
    let mut phase_ctx =
        create_phase_context(ctx, &mut timer, &mut stats, review_guidelines.as_ref());
    save_start_commit_or_warn(ctx);

    // Run pre-development rebase
    run_initial_rebase(&ctx.args, &ctx.config, &ctx.logger, ctx.colors)?;

    // Run pipeline phases
    run_development(&mut phase_ctx, &ctx.args, resume_checkpoint.as_ref())?;
    check_prompt_restoration(ctx, &mut prompt_monitor, "development");
    update_status("In progress.", ctx.config.isolation_mode)?;

    run_review_and_fix(&mut phase_ctx, &ctx.args, resume_checkpoint.as_ref())?;
    check_prompt_restoration(ctx, &mut prompt_monitor, "review");

    // Run post-review rebase
    run_post_review_rebase(&ctx.args, &ctx.config, &ctx.logger, ctx.colors)?;

    update_status("In progress.", ctx.config.isolation_mode)?;

    run_final_validation(&phase_ctx, resume_checkpoint.as_ref())?;

    // Commit phase
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

/// Print pipeline information (working directory and commit message).
fn print_pipeline_info(ctx: &PipelineContext) {
    ctx.logger.info(&format!(
        "Working directory: {}{}{}",
        ctx.colors.cyan(),
        ctx.repo_root.display(),
        ctx.colors.reset()
    ));
    ctx.logger.info(&format!(
        "Commit message: {}{}{}",
        ctx.colors.cyan(),
        ctx.config.commit_msg,
        ctx.colors.reset()
    ));
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

/// Create the phase context for running pipeline phases.
fn create_phase_context<'ctx>(
    ctx: &'ctx PipelineContext,
    timer: &'ctx mut Timer,
    stats: &'ctx mut Stats,
    review_guidelines: Option<&'ctx crate::guidelines::ReviewGuidelines>,
) -> PhaseContext<'ctx> {
    PhaseContext {
        config: &ctx.config,
        registry: &ctx.registry,
        logger: &ctx.logger,
        colors: &ctx.colors,
        timer,
        stats,
        developer_agent: &ctx.developer_agent,
        reviewer_agent: &ctx.reviewer_agent,
        review_guidelines,
    }
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

/// Handle --rebase-only flag.
///
/// This function performs a rebase to the default branch and exits,
/// without running the full pipeline.
fn handle_rebase_only(
    args: &Args,
    config: &crate::config::Config,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<()> {
    // Check if rebase is enabled
    if !args.rebase_flags.skip_rebase && !config.features.auto_rebase_enabled {
        logger.info("Auto-rebase is disabled in configuration");
        return Ok(());
    }

    if args.rebase_flags.skip_rebase {
        logger.info("--skip-rebase flag set, skipping rebase");
        return Ok(());
    }

    logger.header("Rebase to default branch", Colors::cyan);

    match run_rebase_to_default(logger, colors) {
        Ok(RebaseResult::Success) => {
            logger.success("Rebase completed successfully");
            Ok(())
        }
        Ok(RebaseResult::NoOp) => {
            logger.info("No rebase needed (already up-to-date or on main branch)");
            Ok(())
        }
        Ok(RebaseResult::Conflicts(conflicts)) => {
            logger.warn("Rebase resulted in conflicts:");
            for conflict in conflicts {
                logger.error(&format!("  - {conflict}"));
            }
            logger.info("Please resolve conflicts manually and run 'ralph --rebase-only' again");
            anyhow::bail!("Rebase conflicts need manual resolution")
        }
        Err(e) => {
            logger.error(&format!("Rebase failed: {e}"));
            anyhow::bail!("Rebase failed: {e}")
        }
    }
}

/// Run rebase to the default branch.
///
/// This function performs a rebase from the current feature branch to the
/// default branch (main/master). It handles all edge cases including:
/// - Already on main/master (skips rebase)
/// - Empty repository (skips rebase)
/// - Upstream branch not found (error)
/// - Conflicts during rebase (returns Conflicts result)
///
/// # Returns
///
/// Returns `RebaseResult` indicating the outcome.
fn run_rebase_to_default(logger: &Logger, colors: Colors) -> std::io::Result<RebaseResult> {
    // Check if we're on main/master
    if is_main_or_master_branch()? {
        logger.info("Already on default branch, skipping rebase");
        return Ok(RebaseResult::NoOp);
    }

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
fn run_initial_rebase(
    args: &Args,
    config: &crate::config::Config,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<()> {
    // Check if auto-rebase is enabled and not skipped
    if args.rebase_flags.skip_rebase || !config.features.auto_rebase_enabled {
        return Ok(());
    }

    logger.header("Pre-development rebase", Colors::cyan);

    match run_rebase_to_default(logger, colors) {
        Ok(RebaseResult::Success) => {
            logger.success("Rebase completed successfully");
            Ok(())
        }
        Ok(RebaseResult::NoOp) => {
            logger.info("No rebase needed (already up-to-date or on main branch)");
            Ok(())
        }
        Ok(RebaseResult::Conflicts(conflicts)) => {
            logger.warn("Rebase resulted in conflicts:");
            for conflict in conflicts {
                logger.error(&format!("  - {conflict}"));
            }
            logger.info("Aborting rebase to allow manual resolution");
            let _ = abort_rebase();
            anyhow::bail!(
                "Rebase conflicts detected. Please resolve conflicts manually and run again with --skip-rebase"
            )
        }
        Err(e) => {
            logger.warn(&format!("Rebase failed, continuing without rebase: {e}"));
            Ok(())
        }
    }
}

/// Run post-review rebase after review phase.
///
/// This function is called after the review phase completes to ensure
/// the feature branch is still up-to-date with the default branch.
fn run_post_review_rebase(
    args: &Args,
    config: &crate::config::Config,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<()> {
    // Check if auto-rebase is enabled and not skipped
    if args.rebase_flags.skip_rebase || !config.features.auto_rebase_enabled {
        return Ok(());
    }

    logger.header("Post-review rebase", Colors::cyan);

    match run_rebase_to_default(logger, colors) {
        Ok(RebaseResult::Success) => {
            logger.success("Rebase completed successfully");
            Ok(())
        }
        Ok(RebaseResult::NoOp) => {
            logger.info("No rebase needed (already up-to-date or on main branch)");
            Ok(())
        }
        Ok(RebaseResult::Conflicts(conflicts)) => {
            logger.warn("Rebase resulted in conflicts:");
            for conflict in conflicts {
                logger.error(&format!("  - {conflict}"));
            }
            logger.info("Aborting rebase to allow manual resolution");
            let _ = abort_rebase();
            anyhow::bail!(
                "Rebase conflicts detected. Please resolve conflicts manually and run again with --skip-rebase"
            )
        }
        Err(e) => {
            logger.warn(&format!("Rebase failed, continuing without rebase: {e}"));
            Ok(())
        }
    }
}
