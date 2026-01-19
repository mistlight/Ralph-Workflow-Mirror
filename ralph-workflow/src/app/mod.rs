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
    handle_list_available_agents, handle_list_providers, handle_show_baseline,
    handle_template_commands, prompt_template_selection, Args,
};
use crate::common::utils;
use crate::files::protection::monitoring::PromptMonitor;
use crate::files::{
    create_prompt_backup, ensure_files, make_prompt_read_only, reset_context_for_isolation,
    update_status, validate_prompt_md,
};
use crate::git_helpers::{
    abort_rebase, cleanup_orphaned_marker, continue_rebase, get_conflicted_files,
    get_default_branch, get_repo_root, get_start_commit_summary, is_main_or_master_branch,
    rebase_onto, require_git_repo, reset_start_commit, save_start_commit, start_agent_phase,
    RebaseResult, RebaseStateMachine,
};
use crate::logger::Colors;
use crate::logger::Logger;
use crate::phases::{run_development_phase, run_review_phase, PhaseContext};
use crate::pipeline::{AgentPhaseGuard, Stats, Timer};
use crate::prompts::template_context::TemplateContext;
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
    let logger = Logger::new(colors);

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
            Ok(()) => {
                logger.success("Starting commit reference reset to current HEAD");
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

    // Run pre-development rebase (only if explicitly requested via --with-rebase)
    if ctx.args.rebase_flags.with_rebase {
        run_initial_rebase(
            &ctx.args,
            &ctx.config,
            &ctx.template_context,
            &ctx.logger,
            ctx.colors,
        )?;
    }

    // Run pipeline phases
    run_development(&mut phase_ctx, &ctx.args, resume_checkpoint.as_ref())?;
    check_prompt_restoration(ctx, &mut prompt_monitor, "development");
    update_status("In progress.", ctx.config.isolation_mode)?;

    run_review_and_fix(&mut phase_ctx, &ctx.args, resume_checkpoint.as_ref())?;
    check_prompt_restoration(ctx, &mut prompt_monitor, "review");

    // Run post-review rebase (only if explicitly requested via --with-rebase)
    if ctx.args.rebase_flags.with_rebase {
        run_post_review_rebase(
            &ctx.args,
            &ctx.config,
            &ctx.template_context,
            &ctx.logger,
            ctx.colors,
        )?;
    }

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
        template_context: &ctx.template_context,
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

            // Attempt to resolve conflicts with AI
            match try_resolve_conflicts_with_fallback(
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
/// - `auto_rebase` config is enabled (default: true)
/// - `--skip-rebase` CLI flag is not set
fn run_initial_rebase(
    args: &Args,
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<()> {
    // Check if rebase is disabled via config or CLI flag
    if !config.features.auto_rebase || args.rebase_flags.skip_rebase {
        logger.info("Rebase disabled via config or --skip-rebase flag");
        return Ok(());
    }

    logger.header("Pre-development rebase", Colors::cyan);

    // Get the default branch for rebasing
    let default_branch = get_default_branch()?;

    // Try to load an existing state machine or create a new one
    let mut state_machine: RebaseStateMachine =
        match RebaseStateMachine::load_or_create(default_branch.clone()) {
            Ok(mut machine) => {
                // Set max recovery attempts from config when creating a new machine
                // (loaded machines already have their checkpoint state)
                if machine.phase() == &crate::git_helpers::RebasePhase::NotStarted {
                    machine =
                        machine.with_max_recovery_attempts(config.features.max_recovery_attempts);
                }
                machine
            }
            Err(e) => {
                logger.warn(&format!("Failed to load rebase state machine: {e}"));
                // Fall back to basic rebase without state machine
                return run_fallback_rebase(logger, colors, config, template_context);
            }
        };

    // Check if we're resuming from an interrupted rebase
    let phase = state_machine.phase().clone();
    if phase != crate::git_helpers::RebasePhase::NotStarted {
        logger.info(&format!("Resuming rebase from phase: {:?}", phase));
    }

    // Run rebase with state machine
    match run_rebase_with_state_machine(
        &mut state_machine,
        logger,
        colors,
        config,
        template_context,
    ) {
        Ok(RebaseResult::Success) => {
            logger.success("Rebase completed successfully");
            // Clear checkpoint on success
            let _ = state_machine.clear_checkpoint();
            Ok(())
        }
        Ok(RebaseResult::NoOp { reason }) => {
            logger.info(&format!("No rebase needed: {reason}"));
            // Clear checkpoint on no-op
            let _ = state_machine.clear_checkpoint();
            Ok(())
        }
        Ok(RebaseResult::Conflicts(_conflicts)) => {
            // Conflicts were resolved during state machine processing
            logger.success("Rebase completed successfully after conflict resolution");
            let _ = state_machine.clear_checkpoint();
            Ok(())
        }
        Ok(RebaseResult::Failed(err)) => {
            logger.error(&format!("Rebase failed: {err}"));
            anyhow::bail!("Rebase failed: {err}")
        }
        Err(e) => {
            logger.warn(&format!("Rebase failed, continuing without rebase: {e}"));
            // Don't abort - continue pipeline
            Ok(())
        }
    }
}

/// Run post-review rebase after review phase.
///
/// This function is called after the review phase completes to ensure
/// the feature branch is still up-to-date with the default branch.
///
/// Uses a state machine for fault tolerance and automatic recovery from
/// interruptions or failures.
///
/// # Rebase Control
///
/// Rebase is only performed when both conditions are met:
/// - `auto_rebase` config is enabled (default: true)
/// - `--skip-rebase` CLI flag is not set
fn run_post_review_rebase(
    args: &Args,
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<()> {
    // Check if rebase is disabled via config or CLI flag
    if !config.features.auto_rebase || args.rebase_flags.skip_rebase {
        logger.info("Rebase disabled via config or --skip-rebase flag");
        return Ok(());
    }

    logger.header("Post-review rebase", Colors::cyan);

    // Get the default branch for rebasing
    let default_branch = get_default_branch()?;

    // Try to load an existing state machine or create a new one
    let mut state_machine: RebaseStateMachine =
        match RebaseStateMachine::load_or_create(default_branch.clone()) {
            Ok(mut machine) => {
                // Set max recovery attempts from config when creating a new machine
                // (loaded machines already have their checkpoint state)
                if machine.phase() == &crate::git_helpers::RebasePhase::NotStarted {
                    machine =
                        machine.with_max_recovery_attempts(config.features.max_recovery_attempts);
                }
                machine
            }
            Err(e) => {
                logger.warn(&format!("Failed to load rebase state machine: {e}"));
                // Fall back to basic rebase without state machine
                return run_fallback_rebase(logger, colors, config, template_context);
            }
        };

    // Check if we're resuming from an interrupted rebase
    let phase = state_machine.phase().clone();
    if phase != crate::git_helpers::RebasePhase::NotStarted {
        logger.info(&format!("Resuming rebase from phase: {:?}", phase));
    }

    // Run rebase with state machine
    match run_rebase_with_state_machine(
        &mut state_machine,
        logger,
        colors,
        config,
        template_context,
    ) {
        Ok(RebaseResult::Success) => {
            logger.success("Rebase completed successfully");
            // Clear checkpoint on success
            let _ = state_machine.clear_checkpoint();
            Ok(())
        }
        Ok(RebaseResult::NoOp { reason }) => {
            logger.info(&format!("No rebase needed: {reason}"));
            // Clear checkpoint on no-op
            let _ = state_machine.clear_checkpoint();
            Ok(())
        }
        Ok(RebaseResult::Conflicts(_conflicts)) => {
            // Conflicts were resolved during state machine processing
            logger.success("Rebase completed successfully after conflict resolution");
            let _ = state_machine.clear_checkpoint();
            Ok(())
        }
        Ok(RebaseResult::Failed(err)) => {
            logger.error(&format!("Rebase failed: {err}"));
            anyhow::bail!("Rebase failed: {err}")
        }
        Err(e) => {
            logger.warn(&format!("Rebase failed, continuing without rebase: {e}"));
            // Don't abort - continue pipeline
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
/// This is a helper function that creates a minimal `PhaseContext`
/// for conflict resolution without requiring full pipeline state.
fn try_resolve_conflicts_with_fallback(
    conflicted_files: &[String],
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<bool> {
    if conflicted_files.is_empty() {
        return Ok(false);
    }

    logger.info(&format!(
        "Attempting AI conflict resolution for {} file(s)",
        conflicted_files.len()
    ));

    let conflicts = collect_conflict_info_or_error(conflicted_files, logger)?;
    let resolution_prompt = build_resolution_prompt(&conflicts, template_context);

    match run_ai_conflict_resolution(&resolution_prompt, config, logger, colors) {
        Ok(ConflictResolutionResult::WithJson(resolved_content)) => {
            // Agent provided JSON output - attempt to parse and write files
            // JSON is optional for verification - LibGit2 state is authoritative
            match parse_and_validate_resolved_files(&resolved_content, logger) {
                Ok(resolved_files) => {
                    write_resolved_files(&resolved_files, logger)?;
                }
                Err(json_err) => {
                    // JSON parsing failed - this is NOT a verification failure
                    // We verify conflicts via LibGit2 state, not JSON parsing
                    logger.info(&format!(
                        "JSON output unavailable ({}), verifying via LibGit2 state...",
                        json_err
                    ));
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
    build_enhanced_resolution_prompt(conflicts, None, template_context)
        .unwrap_or_else(|_| String::new())
}

/// Build the enhanced conflict resolution prompt with optional branch info.
///
/// This function uses the enhanced prompt builder when branch info is available,
/// falling back to the standard prompt when it's not.
fn build_enhanced_resolution_prompt(
    conflicts: &std::collections::HashMap<String, crate::prompts::FileConflict>,
    branch_info: Option<&crate::prompts::BranchInfo>,
    template_context: &TemplateContext,
) -> anyhow::Result<String> {
    use std::fs;

    let prompt_md_content = fs::read_to_string("PROMPT.md").ok();
    let plan_content = fs::read_to_string(".agent/PLAN.md").ok();

    // Use enhanced prompt with branch info if available
    if let Some(info) = branch_info {
        Ok(crate::prompts::build_enhanced_conflict_resolution_prompt(
            template_context,
            conflicts,
            Some(info),
            prompt_md_content.as_deref(),
            plan_content.as_deref(),
        ))
    } else {
        // Fall back to standard prompt
        Ok(
            crate::prompts::build_conflict_resolution_prompt_with_context(
                template_context,
                conflicts,
                prompt_md_content.as_deref(),
                plan_content.as_deref(),
            ),
        )
    }
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
fn parse_and_validate_resolved_files(
    resolved_content: &str,
    logger: &Logger,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let json: serde_json::Value = serde_json::from_str(resolved_content).map_err(|e| {
        logger.error(&format!("Failed to parse agent output as JSON: {e}"));
        anyhow::anyhow!("Failed to parse agent output as JSON")
    })?;

    let resolved_files = match json.get("resolved_files") {
        Some(v) if v.is_object() => v.as_object().unwrap(),
        _ => {
            logger.error("Agent output missing 'resolved_files' object");
            anyhow::bail!("Agent output missing 'resolved_files' object");
        }
    };

    if resolved_files.is_empty() {
        logger.error("No files were resolved by the agent");
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

/// Run rebase with fault tolerance using state machine.
///
/// This function performs a rebase with automatic recovery from
/// interruptions and failures. It uses the state machine to track
/// progress and can resume from checkpoints.
///
/// # Arguments
///
/// * `state_machine` - Mutable reference to the rebase state machine
/// * `logger` - Logger for output
/// * `colors` - Color formatting
/// * `config` - Application configuration
/// * `template_context` - Template context for prompts
///
/// # Returns
///
/// Returns `Ok(RebaseResult)` indicating the outcome, or an error.
fn run_rebase_with_state_machine(
    state_machine: &mut RebaseStateMachine,
    logger: &Logger,
    colors: Colors,
    config: &crate::config::Config,
    template_context: &TemplateContext,
) -> anyhow::Result<RebaseResult> {
    use crate::git_helpers::{detect_concurrent_git_operations, RebaseLock, RebasePhase};

    let upstream_branch = state_machine.upstream_branch().to_string();
    logger.info(&format!(
        "Rebasing onto {}{}{}",
        colors.cyan(),
        upstream_branch,
        colors.reset()
    ));

    // Transition to pre-rebase check
    state_machine.transition_to(RebasePhase::PreRebaseCheck)?;

    // Log current checkpoint state for debugging
    let checkpoint = state_machine.checkpoint();
    logger.info(&format!(
        "Rebase checkpoint: upstream={}, phase={:?}, error_count={}",
        checkpoint.upstream_branch, checkpoint.phase, checkpoint.error_count
    ));

    // Acquire rebase lock to prevent concurrent rebases
    let _lock =
        RebaseLock::new().map_err(|e| anyhow::anyhow!("Failed to acquire rebase lock: {e}"))?;

    // Validate Git repository state before starting rebase
    if let Err(e) = crate::git_helpers::validate_git_state() {
        logger.warn(&format!("Git state validation failed: {e}"));
        // Try to clean up any stale state that might be causing issues
        let _ = crate::git_helpers::cleanup_stale_rebase_state();
    }

    // Check for concurrent Git operations that would block rebase
    if let Ok(Some(operation)) = detect_concurrent_git_operations() {
        let operation_desc = operation.description();
        logger.warn(&format!(
            "Cannot start rebase: {operation_desc} already in progress"
        ));
        return Ok(RebaseResult::Failed(
            crate::git_helpers::RebaseErrorKind::ConcurrentOperation {
                operation: operation_desc,
            },
        ));
    }

    // Perform pre-rebase validation
    // This checks for Category 1 failure modes before attempting the rebase
    if let Err(e) = crate::git_helpers::validate_rebase_preconditions() {
        logger.warn(&format!("Pre-rebase validation failed: {e}"));
        state_machine.record_error(format!("Pre-rebase validation failed: {e}"));
        // Return NoOp as this is not a transient error
        return Ok(RebaseResult::NoOp {
            reason: format!("Pre-rebase validation failed: {e}"),
        });
    }

    // Perform the rebase operation
    state_machine.transition_to(RebasePhase::RebaseInProgress)?;

    // Get the rebase result and handle each case
    match rebase_onto(&upstream_branch) {
        Ok(RebaseResult::Success) => {
            // Perform post-rebase validation
            state_machine.transition_to(RebasePhase::RebaseComplete)?;
            if let Err(e) = crate::git_helpers::validate_post_rebase_state() {
                logger.warn(&format!("Post-rebase validation failed: {e}"));
                state_machine.record_error(format!("Post-rebase validation failed: {e}"));
                // Still return success since the rebase itself succeeded
                // The validation warning is informational
            }
            Ok(RebaseResult::Success)
        }
        Ok(RebaseResult::NoOp { reason }) => {
            state_machine.transition_to(RebasePhase::RebaseComplete)?;
            Ok(RebaseResult::NoOp { reason })
        }
        Ok(RebaseResult::Conflicts(files)) => {
            state_machine.transition_to(RebasePhase::ConflictDetected)?;
            for file in &files {
                state_machine.record_conflict(file.clone());
            }

            logger.warn(&format!(
                "Rebase resulted in {} conflict(s), attempting AI resolution",
                state_machine.unresolved_conflict_count()
            ));

            // Attempt to resolve conflicts with AI
            let resolution_result = try_resolve_conflicts_with_state_machine(
                state_machine,
                config,
                template_context,
                logger,
                colors,
            )?;

            if resolution_result {
                // Verify all conflicts are resolved
                if state_machine.all_conflicts_resolved() {
                    // Conflicts resolved, continue the rebase
                    state_machine.transition_to(RebasePhase::CompletingRebase)?;
                    logger.info("Continuing rebase after conflict resolution");
                    match continue_rebase() {
                        Ok(()) => {
                            // Perform post-rebase validation
                            if let Err(e) = crate::git_helpers::validate_post_rebase_state() {
                                logger.warn(&format!("Post-rebase validation failed: {e}"));
                                state_machine
                                    .record_error(format!("Post-rebase validation failed: {e}"));
                                // Still return success since the rebase itself succeeded
                            }
                            state_machine.transition_to(RebasePhase::RebaseComplete)?;
                            Ok(RebaseResult::Success)
                        }
                        Err(e) => {
                            state_machine.record_error(format!("Failed to continue rebase: {e}"));
                            logger.warn(&format!("Failed to continue rebase: {e}"));
                            let _ = state_machine.transition_to(RebasePhase::RebaseAborted);
                            let _ = abort_rebase();
                            Ok(RebaseResult::Failed(
                                crate::git_helpers::RebaseErrorKind::ReferenceUpdateFailed {
                                    reason: format!("Failed to continue: {e}"),
                                },
                            ))
                        }
                    }
                } else {
                    // Not all conflicts were resolved
                    let remaining = state_machine.unresolved_conflict_count();
                    state_machine
                        .record_error(format!("AI resolution left {remaining} conflict(s)"));
                    logger.warn(&format!(
                        "AI resolution left {remaining} conflict(s) unresolved"
                    ));
                    let _ = state_machine.transition_to(RebasePhase::RebaseAborted);
                    let _ = abort_rebase();
                    Ok(RebaseResult::Failed(
                        crate::git_helpers::RebaseErrorKind::ContentConflict { files },
                    ))
                }
            } else {
                // AI resolution failed
                state_machine.record_error("AI conflict resolution failed".to_string());
                logger.warn("AI conflict resolution failed, aborting rebase");
                let _ = state_machine.transition_to(RebasePhase::RebaseAborted);
                let _ = abort_rebase();
                Ok(RebaseResult::Failed(
                    crate::git_helpers::RebaseErrorKind::ContentConflict { files },
                ))
            }
        }
        Ok(RebaseResult::Failed(err)) => {
            state_machine.record_error(err.description());
            let _ = state_machine.transition_to(RebasePhase::RebaseAborted);
            Ok(RebaseResult::Failed(err))
        }
        Err(e) => {
            state_machine.record_error(format!("Rebase error: {e}"));
            Err(e.into())
        }
    }
}

/// Fallback rebase without state machine.
///
/// This function provides a fallback path when the state machine
/// cannot be initialized or loaded. It uses the old direct rebase
/// approach.
fn run_fallback_rebase(
    logger: &Logger,
    colors: Colors,
    config: &crate::config::Config,
    template_context: &TemplateContext,
) -> anyhow::Result<()> {
    logger.warn("Using fallback rebase mode (state machine unavailable)");

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

            match try_resolve_conflicts_with_fallback(
                &conflicted_files,
                config,
                template_context,
                logger,
                colors,
            ) {
                Ok(true) => {
                    logger.info("Continuing rebase after conflict resolution");
                    match continue_rebase() {
                        Ok(()) => {
                            logger.success("Rebase completed successfully after AI resolution");
                            Ok(())
                        }
                        Err(e) => {
                            logger.warn(&format!("Failed to continue rebase: {e}"));
                            let _ = abort_rebase();
                            Ok(())
                        }
                    }
                }
                Ok(false) => {
                    logger.warn("AI conflict resolution failed, aborting rebase");
                    let _ = abort_rebase();
                    Ok(())
                }
                Err(e) => {
                    logger.error(&format!("Conflict resolution error: {e}"));
                    let _ = abort_rebase();
                    Ok(())
                }
            }
        }
        Err(e) => {
            logger.warn(&format!("Rebase failed, continuing without rebase: {e}"));
            Ok(())
        }
    }
}

/// Attempt to resolve conflicts with state machine tracking.
///
/// This function performs AI-assisted conflict resolution with a mini dev cycle:
/// 1. AI attempts initial conflict resolution
/// 2. Resolution is validated (no markers remain, syntax is valid)
/// 3. If validation fails, fix with additional context
/// 4. Repeat until resolution succeeds or max attempts reached
///
/// # Arguments
///
/// * `state_machine` - Mutable reference to the rebase state machine
/// * `config` - Application configuration
/// * `template_context` - Template context for prompts
/// * `logger` - Logger for output
/// * `colors` - Color formatting
///
/// # Returns
///
/// Returns `Ok(true)` if conflicts were resolved, `Ok(false)` if resolution failed.
fn try_resolve_conflicts_with_state_machine(
    state_machine: &mut RebaseStateMachine,
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
) -> anyhow::Result<bool> {
    use crate::git_helpers::RebasePhase;

    // Get the actual conflicted files
    let conflicted_files = get_conflicted_files()?;
    if conflicted_files.is_empty() {
        logger.warn("No conflicted files found despite conflict state");
        return Ok(false);
    }

    // Transition to conflict resolution phase
    state_machine.transition_to(RebasePhase::ConflictResolutionInProgress)?;

    // Collect branch info for enhanced conflict resolution context
    let upstream_branch = state_machine.upstream_branch().to_string();
    let branch_info = match crate::prompts::collect_branch_info(&upstream_branch) {
        Ok(info) => {
            logger.info(&format!(
                "Branch context: {} diverging from {} by {} commit(s)",
                info.current_branch, info.upstream_branch, info.diverging_count
            ));
            Some(info)
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to collect branch info: {e}, continuing without it"
            ));
            None
        }
    };

    // Maximum iterations for the review/fix cycle
    let max_iterations = 3;

    // Track validation failures for better retry feedback
    let mut previous_validation_failures = Vec::new();

    for iteration in 1..=max_iterations {
        logger.info(&format!(
            "Conflict resolution cycle {iteration}/{max_iterations}",
            iteration = iteration,
            max_iterations = max_iterations
        ));

        // Collect conflict info and build prompt
        let conflicts = collect_conflict_info_or_error(&conflicted_files, logger)?;
        let resolution_prompt = if iteration == 1 {
            // First attempt: use enhanced prompt with branch info
            build_enhanced_resolution_prompt(&conflicts, branch_info.as_ref(), template_context)?
        } else {
            // Retry: add context about previous failures with specific feedback
            let failure_context = if previous_validation_failures.is_empty() {
                "Your previous resolution attempt was not successful.".to_string()
            } else {
                format!(
                    "Your previous resolution attempt failed validation with these issues:\n\
                     {}\n\nPlease address these specific issues in your next attempt.",
                    previous_validation_failures
                        .iter()
                        .map(|s| format!("- {s}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };

            format!(
                "{}\n\n## Previous Resolution Failed\n\n\
                 {}\n\n\
                 **Validation Requirements**:\n\
                 1. ALL conflict markers (<<<<<<<, =======, >>>>>>>) must be removed\n\
                 2. The code must be syntactically valid (balanced brackets, etc.)\n\
                 3. Files must be actually modified (not left unchanged)\n\
                 4. Git must report no conflicted files after resolution\n\
                 5. You must preserve the intent of BOTH sides where possible\n\n\
                 **Guidance for this attempt**:\n\
                 - Review each file carefully for remaining conflict markers\n\
                 - Check for syntax errors like unbalanced braces/brackets/parentheses\n\
                 - Ensure you're not accidentally leaving files unchanged\n\
                 - Consider using the JSON output format to confirm your resolutions\n\n{}",
                build_enhanced_resolution_prompt(
                    &conflicts,
                    branch_info.as_ref(),
                    template_context
                )?,
                failure_context,
                if iteration == max_iterations {
                    "**FINAL ATTEMPT**: If conflicts remain after this attempt, manual intervention will be required. \
                     Take extra care to ensure all validation criteria are met."
                } else {
                    "Please try again with careful attention to the validation feedback above."
                }
            )
        };

        // Run AI conflict resolution
        match run_ai_conflict_resolution(&resolution_prompt, config, logger, colors) {
            Ok(ConflictResolutionResult::WithJson(resolved_content)) => {
                // Agent provided JSON output - attempt to parse and write files
                // JSON is optional for verification - LibGit2 state is authoritative
                let resolved_files =
                    match parse_and_validate_resolved_files(&resolved_content, logger) {
                        Ok(files) => {
                            // Write files if JSON was successfully parsed
                            write_resolved_files(&files, logger)?;
                            Some(files)
                        }
                        Err(json_err) => {
                            // JSON parsing failed - this is NOT a verification failure
                            // We verify conflicts via LibGit2 state, not JSON parsing
                            logger.info(&format!(
                                "JSON output unavailable ({}), verifying via LibGit2 state...",
                                json_err
                            ));
                            None
                        }
                    };

                // Clear previous failures before new validation
                previous_validation_failures.clear();

                // Validate the resolution using LibGit2 state (authoritative source)
                match validate_conflict_resolution_detailed(logger, &conflicted_files) {
                    Ok(validation_result) if validation_result.is_valid() => {
                        // Mark files as resolved
                        if let Some(ref files) = resolved_files {
                            for path in files.keys() {
                                state_machine.record_resolution(path.clone());
                            }
                        }
                        logger.success(&format!(
                            "All conflicts resolved successfully after {} cycle(s)",
                            iteration
                        ));
                        return Ok(true);
                    }
                    Ok(validation_result) => {
                        // Validation failed - collect specific failures for retry
                        if !validation_result.files_with_markers.is_empty() {
                            previous_validation_failures.push(format!(
                                "Files still have conflict markers: {}",
                                validation_result.files_with_markers.join(", ")
                            ));
                        }
                        if !validation_result.files_with_syntax_errors.is_empty() {
                            previous_validation_failures.push(format!(
                                "Files have syntax errors: {}",
                                validation_result.files_with_syntax_errors.join(", ")
                            ));
                        }
                        if !validation_result.unmodified_files.is_empty() {
                            previous_validation_failures.push(format!(
                                "Files were not modified: {}",
                                validation_result.unmodified_files.join(", ")
                            ));
                        }
                        // Also check for remaining conflicts from git
                        let remaining = get_conflicted_files().unwrap_or_default();
                        if !remaining.is_empty() {
                            previous_validation_failures.push(format!(
                                "Git still reports conflicts: {}",
                                remaining.join(", ")
                            ));
                        }

                        state_machine.record_error(format!(
                            "Conflict resolution validation failed: {}",
                            validation_result.failure_summary()
                        ));
                        logger.warn(&format!(
                            "Resolution validation failed: {}, retrying...",
                            validation_result.failure_summary()
                        ));
                    }
                    Err(e) => {
                        previous_validation_failures.push(format!("Validation error: {e}"));
                        state_machine.record_error(format!("Validation error: {e}"));
                        logger.warn(&format!("Resolution validation error: {e}, retrying..."));
                    }
                }
            }
            Ok(ConflictResolutionResult::FileEditsOnly) => {
                // Agent resolved conflicts by editing files directly
                logger.info("Agent resolved conflicts via file edits (no JSON output)");

                // Clear previous failures and validate
                previous_validation_failures.clear();

                match validate_conflict_resolution_detailed(logger, &conflicted_files) {
                    Ok(validation_result) if validation_result.is_valid() => {
                        // Mark all original conflicted files as resolved
                        for file in &conflicted_files {
                            state_machine.record_resolution(file.clone());
                        }
                        logger.success(&format!(
                            "All conflicts resolved successfully after {} cycle(s)",
                            iteration
                        ));
                        return Ok(true);
                    }
                    Ok(validation_result) => {
                        // Collect failures for retry
                        if !validation_result.files_with_markers.is_empty() {
                            previous_validation_failures.push(format!(
                                "Files still have conflict markers: {}",
                                validation_result.files_with_markers.join(", ")
                            ));
                        }
                        if !validation_result.files_with_syntax_errors.is_empty() {
                            previous_validation_failures.push(format!(
                                "Files have syntax errors: {}",
                                validation_result.files_with_syntax_errors.join(", ")
                            ));
                        }

                        state_machine.record_error(format!(
                            "Conflict resolution validation failed: {}",
                            validation_result.failure_summary()
                        ));
                        logger.warn(&format!(
                            "Resolution validation failed: {}, retrying...",
                            validation_result.failure_summary()
                        ));
                    }
                    Err(e) => {
                        previous_validation_failures.push(format!("Validation error: {e}"));
                        state_machine.record_error(format!("Validation error: {e}"));
                        logger.warn(&format!("Resolution validation error: {e}, retrying..."));
                    }
                }
            }
            Ok(ConflictResolutionResult::Failed) | Err(_) => {
                logger.warn("AI conflict resolution attempt failed");

                // If this is the last iteration, don't retry
                if iteration >= max_iterations {
                    break;
                }
            }
        }
    }

    // All iterations exhausted - try to continue rebase anyway
    // User may have manually resolved conflicts
    logger.info("Resolution cycles exhausted, checking for manual resolution...");
    match crate::git_helpers::continue_rebase() {
        Ok(()) => {
            logger.info("Successfully continued rebase (possibly with manual resolution)");
            // Mark all conflicts as resolved
            for file in &conflicted_files {
                state_machine.record_resolution(file.clone());
            }
            Ok(true)
        }
        Err(rebase_err) => {
            logger.warn(&format!("Failed to continue rebase: {rebase_err}"));
            Ok(false)
        }
    }
}

/// Result of validating conflict resolution.
///
/// Provides detailed feedback on what failed during validation.
#[derive(Debug, Clone, Default)]
struct ConflictValidationResult {
    /// Files that still have conflict markers
    pub files_with_markers: Vec<String>,
    /// Files that have syntax errors (if detectable)
    pub files_with_syntax_errors: Vec<String>,
    /// Files that weren't modified despite being conflicted
    pub unmodified_files: Vec<String>,
    /// Overall validation status
    pub is_valid: bool,
}

impl ConflictValidationResult {
    /// Returns true if all validations passed.
    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Returns a summary of validation failures.
    pub fn failure_summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.files_with_markers.is_empty() {
            parts.push(format!(
                "{} file(s) still have conflict markers",
                self.files_with_markers.len()
            ));
        }
        if !self.files_with_syntax_errors.is_empty() {
            parts.push(format!(
                "{} file(s) have syntax errors",
                self.files_with_syntax_errors.len()
            ));
        }
        if !self.unmodified_files.is_empty() {
            parts.push(format!(
                "{} file(s) were not modified",
                self.unmodified_files.len()
            ));
        }

        if parts.is_empty() {
            "No specific issues detected".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Perform basic syntax validation for common file types.
///
/// This is a lightweight validation that checks for obvious syntax errors
/// like unmatched brackets, incomplete statements, etc.
///
/// # Arguments
///
/// * `extension` - File extension (e.g., "rs", "py", "js")
/// * `content` - File content to validate
///
/// # Returns
///
/// Returns `Ok(())` if syntax appears valid, `Err` if issues detected.
fn validate_file_syntax(extension: &str, content: &str) -> anyhow::Result<()> {
    match extension {
        // Rust files - check for balanced braces and parentheses
        "rs" => {
            let open_braces = content.matches('{').count();
            let close_braces = content.matches('}').count();
            let open_parens = content.matches('(').count();
            let close_parens = content.matches(')').count();
            let open_brackets = content.matches('[').count();
            let close_brackets = content.matches(']').count();

            if open_braces != close_braces {
                anyhow::bail!("Unbalanced braces: {open_braces} open, {close_braces} close");
            }
            if open_parens != close_parens {
                anyhow::bail!("Unbalanced parentheses: {open_parens} open, {close_parens} close");
            }
            if open_brackets != close_brackets {
                anyhow::bail!("Unbalanced brackets: {open_brackets} open, {close_brackets} close");
            }
            Ok(())
        }
        // Python files - check for basic indentation consistency
        "py" => {
            // Python's syntax is complex; we do a basic check for obvious issues
            let lines: Vec<&str> = content.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                // Check for tabs mixed with spaces (common issue)
                if line.contains('\t') && line.matches(' ').count() > 0 {
                    anyhow::bail!("Line {}: mixed tabs and spaces", i + 1);
                }
            }
            Ok(())
        }
        // JavaScript/TypeScript - check for balanced braces
        "js" | "ts" | "jsx" | "tsx" => {
            let open_braces = content.matches('{').count();
            let close_braces = content.matches('}').count();
            let open_parens = content.matches('(').count();
            let close_parens = content.matches(')').count();
            let open_brackets = content.matches('[').count();
            let close_brackets = content.matches(']').count();

            if open_braces != close_braces {
                anyhow::bail!("Unbalanced braces: {open_braces} open, {close_braces} close");
            }
            if open_parens != close_parens {
                anyhow::bail!("Unbalanced parentheses: {open_parens} open, {close_parens} close");
            }
            if open_brackets != close_brackets {
                anyhow::bail!("Unbalanced brackets: {open_brackets} open, {close_brackets} close");
            }
            Ok(())
        }
        // JSON files - check for balanced braces and brackets
        "json" => {
            let open_braces = content.matches('{').count();
            let close_braces = content.matches('}').count();
            let open_brackets = content.matches('[').count();
            let close_brackets = content.matches(']').count();

            if open_braces != close_braces {
                anyhow::bail!("Unbalanced braces: {open_braces} open, {close_braces} close");
            }
            if open_brackets != close_brackets {
                anyhow::bail!("Unbalanced brackets: {open_brackets} open, {close_brackets} close");
            }
            Ok(())
        }
        // YAML files - basic structure check
        "yaml" | "yml" => {
            // Check for obvious syntax issues
            for line in content.lines() {
                // Check for tabs (YAML doesn't allow tabs for indentation)
                if line.starts_with('\t') {
                    anyhow::bail!("YAML files should not use tabs for indentation");
                }
            }
            Ok(())
        }
        // Unknown file type - skip validation
        _ => Ok(()),
    }
}

/// Validate that conflict resolution was successful, returning detailed results.
///
/// Performs comprehensive validation and returns detailed feedback about
/// what failed, which can be used to provide better context for retry attempts.
///
/// # Arguments
///
/// * `logger` - Logger for output
/// * `original_conflicts` - List of originally conflicted files
///
/// # Returns
///
/// Returns `Ok(ConflictValidationResult)` with detailed validation results.
fn validate_conflict_resolution_detailed(
    logger: &Logger,
    original_conflicts: &[String],
) -> anyhow::Result<ConflictValidationResult> {
    use std::fs;

    let mut validation_result = ConflictValidationResult::default();

    // Check each originally conflicted file
    for path in original_conflicts {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Check for conflict markers
                let has_markers = content.contains("<<<<<<<")
                    || content.contains("=======")
                    || content.contains(">>>>>>>");

                if has_markers {
                    validation_result.files_with_markers.push(path.clone());
                    logger.warn(&format!("File {} still contains conflict markers", path));
                }

                // Check for basic syntax validation on known file types
                if let Some(ext) = std::path::Path::new(path).extension() {
                    if let Some(ext_str) = ext.to_str() {
                        if validate_file_syntax(ext_str, &content).is_err() {
                            validation_result
                                .files_with_syntax_errors
                                .push(path.clone());
                            logger.warn(&format!("File {} may have syntax errors", path));
                        }
                    }
                }
            }
            Err(e) => {
                logger.warn(&format!("Failed to read file {}: {}", path, e));
                // If we can't read the file, consider it invalid
                validation_result.files_with_markers.push(path.clone());
            }
        }
    }

    // Verify with git that no conflicts remain
    let remaining_conflicts = get_conflicted_files()?;
    if !remaining_conflicts.is_empty() {
        logger.warn(&format!(
            "Git still reports {} conflicted file(s): {}",
            remaining_conflicts.len(),
            remaining_conflicts.join(", ")
        ));
    }

    // Detect partial resolution: files that still show as conflicted in git
    for path in original_conflicts {
        if remaining_conflicts.contains(path)
            && !validation_result.files_with_markers.contains(path)
        {
            // File is still conflicted according to git but has no markers
            // This might indicate a partial resolution or state issue
            logger.warn(&format!(
                "File {} is still marked as conflicted by Git",
                path
            ));
        }
    }

    validation_result.is_valid = validation_result.files_with_markers.is_empty()
        && validation_result.files_with_syntax_errors.is_empty()
        && remaining_conflicts.is_empty();

    Ok(validation_result)
}
