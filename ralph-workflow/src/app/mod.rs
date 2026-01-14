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
pub mod plumbing;
pub mod validation;
pub mod resume;
pub mod detection;
pub mod finalization;

use crate::agents::AgentRegistry;
use crate::banner::print_welcome_banner;
use crate::cli::{
    create_prompt_from_template, handle_diagnose, handle_dry_run, handle_list_agents,
    handle_list_available_agents, handle_list_providers, prompt_template_selection, Args,
};
use crate::colors::Colors;
use crate::config::Config;
use crate::files::monitoring::PromptMonitor;
use crate::files::{
    create_prompt_backup, ensure_files, make_prompt_read_only, reset_context_for_isolation,
    update_status, validate_prompt_md,
};
use crate::git_helpers::{
    cleanup_orphaned_marker, get_repo_root, require_git_repo, reset_start_commit,
    save_start_commit, start_agent_phase,
};
use crate::logger::Logger;
use crate::phases::{run_development_phase, run_review_phase, PhaseContext};
use crate::pipeline::{AgentPhaseGuard, Stats};
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::timer::Timer;
use std::env;
use std::process::Command;

use config_init::initialize_config;
use detection::detect_project_stack;
use finalization::finalize_pipeline;
use plumbing::{handle_apply_commit, handle_generate_commit_msg, handle_show_commit_msg};
use resume::{handle_resume, phase_rank, should_run_from};
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

    // Get display names for UI/logging
    let developer_display = registry.display_name(&developer_agent);
    let reviewer_display = registry.display_name(&reviewer_agent);

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

    // In interactive mode, prompt to create PROMPT.md from a template BEFORE ensure_files().
    // If the user declines (or we can't prompt), exit without creating a placeholder PROMPT.md.
    if args.interactive && !std::path::Path::new("PROMPT.md").exists() {
        match prompt_template_selection(&colors) {
            Some(template_name) => {
                create_prompt_from_template(&template_name, &colors)?;
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
            None => {
                println!();
                logger.info("PROMPT.md is required to run the pipeline.");
                logger.info(
                    "Create one with 'ralph --init-prompt <template>' (see: 'ralph --list-templates'), then rerun.",
                );
                return Ok(());
            }
        }
    }

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
            &developer_display,
            &reviewer_display,
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
    PipelineContext {
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
    }
    .run()
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

/// Context for running the development/review/commit pipeline.
struct PipelineContext {
    args: Args,
    config: Config,
    registry: AgentRegistry,
    developer_agent: String,
    reviewer_agent: String,
    developer_display: String,
    reviewer_display: String,
    repo_root: std::path::PathBuf,
    logger: Logger,
    colors: Colors,
}

impl PipelineContext {
    /// Runs the full development/review/commit pipeline.
    fn run(self) -> anyhow::Result<()> {
        // Handle --resume
        let resume_checkpoint = handle_resume(
            &self.args,
            &self.logger,
            &self.developer_display,
            &self.reviewer_display,
        );

        // Set up git helpers
        let mut git_helpers = crate::git_helpers::GitHelpers::new();
        cleanup_orphaned_marker(&self.logger)?;
        start_agent_phase(&mut git_helpers)?;
        let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &self.logger);

        let mut timer = Timer::new();
        let mut stats = Stats::new();

        // Welcome banner
        print_welcome_banner(
            &self.colors,
            &self.developer_display,
            &self.reviewer_display,
        );
        self.logger.info(&format!(
            "Working directory: {}{}{}",
            self.colors.cyan(),
            self.repo_root.display(),
            self.colors.reset()
        ));
        self.logger.info(&format!(
            "Commit message: {}{}{}",
            self.colors.cyan(),
            self.config.commit_msg,
            self.colors.reset()
        ));

        // Validate PROMPT.md early so we don't run a "review" against an ill-formed prompt.
        // In non-strict mode this is warning-only for missing sections, but still surfaced
        // loudly because it impacts the review workflow.
        // Note: Interactive mode PROMPT.md creation is handled in run() before ensure_files()
        let prompt_validation =
            validate_prompt_md(self.config.strict_validation, self.args.interactive);
        for err in &prompt_validation.errors {
            self.logger.error(err);
        }
        for warn in &prompt_validation.warnings {
            self.logger.warn(warn);
        }
        if !prompt_validation.is_valid() {
            anyhow::bail!("PROMPT.md validation errors");
        }

        // Create a backup of PROMPT.md to protect against accidental deletion.
        // This must happen after validation and before any agent phases begin.
        // If PROMPT.md doesn't exist (e.g., non-interactive mode with missing file),
        // create_prompt_backup() returns Ok(None) and does nothing.
        match create_prompt_backup() {
            Ok(None) => {
                // Backup created successfully with read-only permissions
            }
            Ok(Some(warning)) => {
                self.logger.warn(&format!(
                    "PROMPT.md backup created but: {}. Continuing anyway.",
                    warning
                ));
            }
            Err(e) => {
                self.logger.warn(&format!(
                    "Failed to create PROMPT.md backup: {}. Continuing anyway.",
                    e
                ));
            }
        }

        // Make PROMPT.md read-only to protect against accidental deletion.
        // This is a best-effort protection - it may not work on all filesystems.
        // If PROMPT.md doesn't exist, make_prompt_read_only() returns Ok(None).
        match make_prompt_read_only() {
            Ok(None) => {
                // Read-only permissions set successfully
            }
            Ok(Some(warning)) => {
                self.logger
                    .warn(&format!("{}. Continuing anyway.", warning));
            }
            Err(e) => {
                self.logger.warn(&format!(
                    "Failed to make PROMPT.md read-only: {}. Continuing anyway.",
                    e
                ));
            }
        }

        // Start real-time monitoring of PROMPT.md for immediate deletion detection.
        // The monitor runs in a background thread and automatically restores PROMPT.md
        // if deletion is detected. We check for restoration events after each phase.
        let mut prompt_monitor = match PromptMonitor::new() {
            Ok(mut monitor) => {
                if let Err(e) = monitor.start() {
                    self.logger.warn(&format!(
                        "Failed to start PROMPT.md monitoring: {}. Continuing anyway.",
                        e
                    ));
                    None
                } else {
                    if self.config.verbosity.is_debug() {
                        self.logger.info("Started real-time PROMPT.md monitoring");
                    }
                    Some(monitor)
                }
            }
            Err(e) => {
                self.logger.warn(&format!(
                    "Failed to create PROMPT.md monitor: {}. Continuing anyway.",
                    e
                ));
                None
            }
        };

        // Detect project stack and generate review guidelines
        let (_project_stack, review_guidelines) =
            detect_project_stack(&self.config, &self.repo_root, &self.logger, &self.colors);

        if let Some(ref guidelines) = review_guidelines {
            self.logger.info(&format!(
                "Review guidelines: {}{}{}",
                self.colors.dim(),
                guidelines.summary(),
                self.colors.reset()
            ));
        }

        println!();

        // Create phase context
        let mut ctx = PhaseContext {
            config: &self.config,
            registry: &self.registry,
            logger: &self.logger,
            colors: &self.colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: &self.developer_agent,
            reviewer_agent: &self.reviewer_agent,
            review_guidelines: review_guidelines.as_ref(),
        };

        // Save the starting commit reference for incremental diff generation
        // This enables reviewers to see changes since pipeline start without git context
        //
        // If saving fails (e.g., due to filesystem issues), we log a warning but continue.
        // This may reduce incremental review quality (diffs may be empty after auto-commits).
        match save_start_commit() {
            Ok(()) => {
                if self.config.verbosity.is_debug() {
                    self.logger
                        .info("Saved starting commit for incremental diff generation");
                }
            }
            Err(e) => {
                self.logger.warn(&format!(
                    "Failed to save starting commit: {}. \
                 Incremental diffs may be unavailable as a result.",
                    e
                ));
                self.logger.info(
                "To fix this issue, ensure .agent directory is writable and you have a valid HEAD commit.",
            );
            }
        }

        // Run phases
        run_development(&mut ctx, &self.args, resume_checkpoint.as_ref())?;

        // Check for PROMPT.md restoration after development phase
        if let Some(ref mut monitor) = prompt_monitor {
            if monitor.check_and_restore() {
                self.logger
                    .warn("PROMPT.md was deleted and restored during development phase");
            }
        }
        update_status("In progress.", self.config.isolation_mode)?;

        run_review_and_fix(&mut ctx, &self.args, resume_checkpoint.as_ref())?;

        // Check for PROMPT.md restoration after review phase
        if let Some(ref mut monitor) = prompt_monitor {
            if monitor.check_and_restore() {
                self.logger
                    .warn("PROMPT.md was deleted and restored during review phase");
            }
        }
        update_status("In progress.", self.config.isolation_mode)?;

        run_final_validation(&ctx, resume_checkpoint.as_ref())?;

        // Commit phase
        finalize_pipeline(
            &mut agent_phase_guard,
            &self.logger,
            &self.colors,
            &self.config,
            &timer,
            &stats,
            prompt_monitor,
        )
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
