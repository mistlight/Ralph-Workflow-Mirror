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

pub mod config_init;
pub mod plumbing;
pub mod validation;

use crate::agents::AgentRegistry;
use crate::banner::{print_final_summary, print_welcome_banner};
use crate::cli::{
    handle_diagnose, handle_dry_run, handle_list_agents, handle_list_available_agents,
    handle_list_providers, Args,
};
use crate::colors::Colors;
use crate::config::Config;
use crate::git_helpers::{
    cleanup_orphaned_marker, get_repo_root, require_git_repo, reset_start_commit,
    save_start_commit, start_agent_phase,
};
use crate::guidelines::ReviewGuidelines;
use crate::language_detector::{detect_stack, ProjectStack};
use crate::phases::{run_development_phase, run_review_phase, PhaseContext};
use crate::pipeline::{AgentPhaseGuard, Stats};
use crate::timer::Timer;
use crate::utils::{
    clear_checkpoint, ensure_files, load_checkpoint, reset_context_for_isolation, save_checkpoint,
    update_status, validate_prompt_md, Logger, PipelineCheckpoint, PipelinePhase,
};
use std::env;
use std::process::Command;

use config_init::initialize_config;
use plumbing::{handle_apply_commit, handle_generate_commit_msg, handle_show_commit_msg};
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
    let init_result = match initialize_config(&args, &colors, &mut logger)? {
        Some(result) => result,
        None => return Ok(()), // Early exit (--init/--init-global/--init-legacy)
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
    if handle_listing_commands(&args, &registry, &colors) {
        return Ok(());
    }

    // Handle --diagnose
    if args.diagnose {
        handle_diagnose(&colors, &config, &registry, &config_path, &config_sources);
        return Ok(());
    }

    // Validate agent chains
    validate_agent_chains(&registry, &colors);

    // Handle plumbing commands (these need git repo but not full validation)
    if args.show_commit_msg {
        return handle_show_commit_msg();
    }
    if args.apply_commit {
        return handle_apply_commit(&logger, &colors);
    }
    if args.reset_start_commit {
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
                logger.error(&format!("Failed to reset starting commit: {}", e));
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
    ensure_files(config.isolation_mode)?;

    // Reset context for isolation mode
    if config.isolation_mode {
        reset_context_for_isolation(&logger)?;
    }

    logger = logger.with_log_file(".agent/logs/pipeline.log");

    // Handle --dry-run
    if args.dry_run {
        return handle_dry_run(
            &logger,
            &colors,
            &config,
            &developer_agent,
            &reviewer_agent,
            &repo_root,
        );
    }

    // Handle --generate-commit-msg
    if args.generate_commit_msg {
        return handle_generate_commit_msg(
            &config,
            &registry,
            &logger,
            &colors,
            &developer_agent,
            &reviewer_agent,
        );
    }

    // Run the full pipeline
    run_pipeline(
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        logger,
        colors,
    )
}

/// Handles listing commands that don't require the full pipeline.
///
/// Returns `true` if a listing command was handled and we should exit.
fn handle_listing_commands(args: &Args, registry: &AgentRegistry, colors: &Colors) -> bool {
    if args.list_agents {
        handle_list_agents(registry);
        return true;
    }
    if args.list_available_agents {
        handle_list_available_agents(registry);
        return true;
    }
    if args.list_providers {
        handle_list_providers(colors);
        return true;
    }
    false
}

/// Runs the full development/review/commit pipeline.
#[allow(clippy::too_many_arguments)]
fn run_pipeline(
    args: Args,
    config: Config,
    registry: AgentRegistry,
    developer_agent: String,
    reviewer_agent: String,
    repo_root: std::path::PathBuf,
    logger: Logger,
    colors: Colors,
) -> anyhow::Result<()> {
    // Handle --resume
    let resume_checkpoint = handle_resume(&args, &logger, &developer_agent, &reviewer_agent);

    // Set up git helpers
    let mut git_helpers = crate::git_helpers::GitHelpers::new();
    cleanup_orphaned_marker(&logger)?;
    start_agent_phase(&mut git_helpers)?;
    let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &logger);

    let mut timer = Timer::new();
    let mut stats = Stats::new();

    // Welcome banner
    print_welcome_banner(&colors, &developer_agent, &reviewer_agent);
    logger.info(&format!(
        "Working directory: {}{}{}",
        colors.cyan(),
        repo_root.display(),
        colors.reset()
    ));
    logger.info(&format!(
        "Commit message: {}{}{}",
        colors.cyan(),
        config.commit_msg,
        colors.reset()
    ));

    // Validate PROMPT.md early so we don't run a "review" against an ill-formed prompt.
    // In non-strict mode this is warning-only for missing sections, but still surfaced
    // loudly because it impacts the review workflow.
    let prompt_validation = validate_prompt_md(config.strict_validation);
    for err in &prompt_validation.errors {
        logger.error(err);
    }
    for warn in &prompt_validation.warnings {
        logger.warn(warn);
    }
    if !prompt_validation.is_valid() {
        anyhow::bail!("PROMPT.md validation errors");
    }

    // Detect project stack and generate review guidelines
    let (_project_stack, review_guidelines) =
        detect_project_stack(&config, &repo_root, &logger, &colors);

    if let Some(ref guidelines) = review_guidelines {
        logger.info(&format!(
            "Review guidelines: {}{}{}",
            colors.dim(),
            guidelines.summary(),
            colors.reset()
        ));
    }

    println!();

    // Create phase context
    let mut ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: &developer_agent,
        reviewer_agent: &reviewer_agent,
        review_guidelines: review_guidelines.as_ref(),
    };

    // Save the starting commit reference for incremental diff generation
    // This enables reviewers to see changes since pipeline start without git context
    //
    // If saving fails (e.g., due to filesystem issues), we log a warning but continue.
    // This may reduce incremental review quality (diffs may be empty after auto-commits).
    match save_start_commit() {
        Ok(()) => {
            if config.verbosity.is_debug() {
                logger.info("Saved starting commit for incremental diff generation");
            }
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to save starting commit: {}. \
                 Incremental diffs may be unavailable as a result.",
                e
            ));
            logger.info(
                "To fix this issue, ensure .agent directory is writable and you have a valid HEAD commit.",
            );
        }
    }

    // Run phases
    run_development(&mut ctx, &args, resume_checkpoint.as_ref())?;
    update_status("In progress.", config.isolation_mode)?;

    run_review_and_fix(&mut ctx, &args, resume_checkpoint.as_ref())?;
    update_status("In progress.", config.isolation_mode)?;

    run_final_validation(&ctx, resume_checkpoint.as_ref())?;

    // Commit phase
    finalize_pipeline(
        &mut agent_phase_guard,
        &logger,
        &colors,
        &config,
        &timer,
        &stats,
    )
}

/// Handles the --resume flag and loads checkpoint if applicable.
fn handle_resume(
    args: &Args,
    logger: &Logger,
    developer_agent: &str,
    reviewer_agent: &str,
) -> Option<PipelineCheckpoint> {
    if !args.resume {
        return None;
    }

    match load_checkpoint() {
        Ok(Some(checkpoint)) => {
            logger.header("RESUME: Loading Checkpoint", |c| c.yellow());
            logger.info(&format!("Resuming from: {}", checkpoint.description()));
            logger.info(&format!("Checkpoint saved at: {}", checkpoint.timestamp));

            // Verify agents match
            if checkpoint.developer_agent != developer_agent {
                logger.warn(&format!(
                    "Developer agent changed: {} -> {}",
                    checkpoint.developer_agent, developer_agent
                ));
            }
            if checkpoint.reviewer_agent != reviewer_agent {
                logger.warn(&format!(
                    "Reviewer agent changed: {} -> {}",
                    checkpoint.reviewer_agent, reviewer_agent
                ));
            }

            Some(checkpoint)
        }
        Ok(None) => {
            logger.warn("No checkpoint found. Starting fresh pipeline...");
            None
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to load checkpoint (starting fresh): {}",
                e
            ));
            None
        }
    }
}

/// Detects project stack and generates review guidelines.
fn detect_project_stack(
    config: &Config,
    repo_root: &std::path::Path,
    logger: &Logger,
    colors: &Colors,
) -> (Option<ProjectStack>, Option<ReviewGuidelines>) {
    if !config.auto_detect_stack {
        return (None, None);
    }

    match detect_stack(repo_root) {
        Ok(stack) => {
            logger.info(&format!(
                "Detected stack: {}{}{}",
                colors.cyan(),
                stack.summary(),
                colors.reset()
            ));
            let guidelines = ReviewGuidelines::for_stack(&stack);
            (Some(stack), Some(guidelines))
        }
        Err(e) => {
            logger.warn(&format!("Could not detect project stack: {}", e));
            (None, None)
        }
    }
}

/// Helper to get phase rank for resume logic.
fn phase_rank(p: PipelinePhase) -> u8 {
    match p {
        PipelinePhase::Planning => 0,
        PipelinePhase::Development => 1,
        PipelinePhase::Review => 2,
        PipelinePhase::Fix => 3,
        PipelinePhase::ReviewAgain => 4,
        PipelinePhase::CommitMessage => 5,
        PipelinePhase::FinalValidation => 6,
        PipelinePhase::Complete => 7,
    }
}

/// Determines if a phase should run based on resume checkpoint.
fn should_run_from(phase: PipelinePhase, resume_checkpoint: Option<&PipelineCheckpoint>) -> bool {
    match resume_checkpoint {
        None => true,
        Some(checkpoint) => phase_rank(phase) >= phase_rank(checkpoint.phase),
    }
}

/// Runs the development phase.
fn run_development(
    ctx: &mut PhaseContext,
    args: &Args,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> anyhow::Result<()> {
    ctx.logger.header("PHASE 1: Development", |c| c.blue());

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
            .map(|c| c.iteration)
            .unwrap_or(1)
            .clamp(1, ctx.config.developer_iters),
        _ => 1,
    };

    let resuming_from_development = args.resume && resume_phase == Some(PipelinePhase::Development);
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
    ctx.logger.header("PHASE 2: Review & Fix", |c| c.magenta());

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
                    .map(|c| c.reviewer_pass)
                    .unwrap_or(1)
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
            .header("PHASE 3: Final Validation", |c| c.yellow());
        ctx.logger
            .info("Skipping final validation (resuming from a later checkpoint phase)");
        return Ok(());
    }

    let argv = crate::utils::split_command(full_cmd)
        .map_err(|e| anyhow::anyhow!("FULL_CHECK_CMD parse error: {}", e))?;
    if argv.is_empty() {
        ctx.logger
            .warn("FULL_CHECK_CMD is empty; skipping final validation");
        return Ok(());
    }

    if ctx.config.checkpoint_enabled {
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
        .header("PHASE 3: Final Validation", |c| c.yellow());
    let display_cmd = crate::utils::format_argv_for_log(&argv);
    ctx.logger.info(&format!(
        "Running full check: {}{}{}",
        ctx.colors.dim(),
        display_cmd,
        ctx.colors.reset()
    ));

    let (program, args) = argv
        .split_first()
        .expect("argv is non-empty after empty check");
    let status = Command::new(program).args(args).status()?;

    if status.success() {
        ctx.logger.success("Full check passed");
    } else {
        ctx.logger.error("Full check failed");
        anyhow::bail!("Full check failed");
    }

    Ok(())
}

/// Finalizes the pipeline: cleans up and prints summary.
///
/// Commits now happen per-iteration during development and per-cycle during review,
/// so this function only handles cleanup and final summary.
fn finalize_pipeline(
    agent_phase_guard: &mut AgentPhaseGuard,
    logger: &Logger,
    colors: &Colors,
    config: &Config,
    timer: &Timer,
    stats: &Stats,
) -> anyhow::Result<()> {
    // End agent phase and clean up
    crate::git_helpers::end_agent_phase()?;
    crate::git_helpers::disable_git_wrapper(agent_phase_guard.git_helpers);
    if let Err(err) = crate::git_helpers::uninstall_hooks(logger) {
        logger.warn(&format!("Failed to uninstall Ralph hooks: {}", err));
    }

    // Note: Individual commits were created per-iteration during development
    // and per-cycle during review. The final commit phase has been removed.

    // Final summary
    print_final_summary(colors, config, timer, stats, logger);

    if config.checkpoint_enabled {
        if let Err(err) = clear_checkpoint() {
            logger.warn(&format!("Failed to clear checkpoint: {}", err));
        }
    }

    agent_phase_guard.disarm();
    Ok(())
}
