//! Pipeline orchestration logic.
//!
//! This module contains the main pipeline orchestration that coordinates
//! the various phases of the workflow.

#![expect(clippy::needless_pass_by_value)]
#![expect(clippy::too_many_lines)]

use crate::banner::print_welcome_banner;
use crate::files::monitoring::PromptMonitor;
use crate::files::{
    create_prompt_backup, make_prompt_read_only, update_status, validate_prompt_md,
};
use crate::git_helpers::{cleanup_orphaned_marker, save_start_commit, start_agent_phase};
use crate::phases::PhaseContext;
use crate::pipeline::{AgentPhaseGuard, Stats};
use crate::timer::Timer;

use super::context::PipelineContext;
use super::detection::detect_project_stack;
use super::finalization::finalize_pipeline;
use super::phase_runners::{run_development, run_final_validation, run_review_and_fix};
use super::resume::handle_resume;

/// Runs the full development/review/commit pipeline.
pub fn run_pipeline(ctx: PipelineContext) -> anyhow::Result<()> {
    // Handle --resume
    let resume_checkpoint = handle_resume(
        &ctx.args,
        &ctx.logger,
        &ctx.developer_display,
        &ctx.reviewer_display,
    );

    // Set up git helpers
    let mut git_helpers = crate::git_helpers::GitHelpers::new();
    cleanup_orphaned_marker(&ctx.logger)?;
    start_agent_phase(&mut git_helpers)?;
    let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &ctx.logger);

    // Welcome banner
    print_welcome_banner(ctx.colors, &ctx.developer_display, &ctx.reviewer_display);
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

    // Validate PROMPT.md early so we don't run a "review" against an ill-formed prompt.
    // In non-strict mode this is warning-only for missing sections, but still surfaced
    // loudly because it impacts the review workflow.
    // Note: Interactive mode PROMPT.md creation is handled in run() before ensure_files()
    let prompt_validation = validate_prompt_md(ctx.config.strict_validation, ctx.args.interactive);
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
    // This must happen after validation and before any agent phases begin.
    // If PROMPT.md doesn't exist (e.g., non-interactive mode with missing file),
    // create_prompt_backup() returns Ok(None) and does nothing.
    match create_prompt_backup() {
        Ok(None) => {
            // Backup created successfully with read-only permissions
        }
        Ok(Some(warning)) => {
            ctx.logger.warn(&format!(
                "PROMPT.md backup created but: {warning}. Continuing anyway.",
            ));
        }
        Err(e) => {
            ctx.logger.warn(&format!(
                "Failed to create PROMPT.md backup: {e}. Continuing anyway.",
            ));
        }
    }

    // Make PROMPT.md read-only to protect against accidental deletion.
    // This is a best-effort protection - it may not work on all filesystems.
    // If PROMPT.md doesn't exist, make_prompt_read_only() returns None.
    match make_prompt_read_only() {
        None => {
            // Read-only permissions set successfully
        }
        Some(warning) => {
            ctx.logger.warn(&format!("{warning}. Continuing anyway."));
        }
    }

    // Start real-time monitoring of PROMPT.md for immediate deletion detection.
    // The monitor runs in a background thread and automatically restores PROMPT.md
    // if deletion is detected. We check for restoration events after each phase.
    let mut prompt_monitor = match PromptMonitor::new() {
        Ok(mut monitor) => {
            if let Err(e) = monitor.start() {
                ctx.logger.warn(&format!(
                    "Failed to start PROMPT.md monitoring: {e}. Continuing anyway.",
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
                "Failed to create PROMPT.md monitor: {e}. Continuing anyway.",
            ));
            None
        }
    };

    // Detect project stack and generate review guidelines
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

    // Create phase context
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

    // Save the starting commit reference for incremental diff generation
    // This enables reviewers to see changes since pipeline start without git context
    //
    // If saving fails (e.g., due to filesystem issues), we log a warning but continue.
    // This may reduce incremental review quality (diffs may be empty after auto-commits).
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
                 Incremental diffs may be unavailable as a result.",
            ));
            ctx.logger.info(
                "To fix this issue, ensure .agent directory is writable and you have a valid HEAD commit.",
            );
        }
    }

    // Run phases
    run_development(&mut phase_ctx, &ctx.args, resume_checkpoint.as_ref())?;

    // Check for PROMPT.md restoration after development phase
    if let Some(ref mut monitor) = prompt_monitor {
        if monitor.check_and_restore() {
            ctx.logger
                .warn("PROMPT.md was deleted and restored during development phase");
        }
    }
    update_status("In progress.", ctx.config.isolation_mode)?;

    run_review_and_fix(&mut phase_ctx, &ctx.args, resume_checkpoint.as_ref())?;

    // Check for PROMPT.md restoration after review phase
    if let Some(ref mut monitor) = prompt_monitor {
        if monitor.check_and_restore() {
            ctx.logger
                .warn("PROMPT.md was deleted and restored during review phase");
        }
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
