//! Development phase execution.
//!
//! This module handles the development phase of the Ralph pipeline, which consists
//! of iterative planning and execution cycles. Each iteration:
//! 1. Creates a PLAN.md from PROMPT.md
//! 2. Executes the plan
//! 3. Deletes PLAN.md
//! 4. Optionally runs fast checks

#![expect(clippy::too_many_lines)]
use crate::agents::AgentRole;
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::files::{delete_plan_file, update_status};
use crate::files::{extract_plan, extract_plan_from_logs_text, restore_prompt_if_needed};
use crate::git_helpers::{git_snapshot, CommitResultFallback};
use crate::logger::print_progress;
use crate::phases::commit::commit_with_generated_message;
use crate::phases::common::get_primary_commit_agent;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{prompt_for_agent, Action, ContextLevel, Role};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Periodically restore PROMPT.md if it was deleted by an agent.
///
/// This is a defense-in-depth measure to ensure PROMPT.md is always available
/// even if an agent accidentally deletes it during pipeline execution.
///
/// The enhanced logging helps identify which phase/agent likely caused
/// the deletion for debugging purposes.
fn ensure_prompt_integrity(logger: &crate::logger::Logger, phase: &str, iteration: u32) {
    match restore_prompt_if_needed() {
        Ok(true) => {
            // File exists with content, no action needed
        }
        Ok(false) => {
            logger.warn("[PROMPT_INTEGRITY] PROMPT.md was missing or empty and has been restored from backup");
            logger.warn(&format!(
                "[PROMPT_INTEGRITY] Deletion detected during {phase} phase (iteration {iteration})"
            ));
            logger.warn("[PROMPT_INTEGRITY] Possible cause: Agent used 'rm' or file write tools on PROMPT.md");
            logger.success("PROMPT.md restored from .agent/PROMPT.md.backup");
        }
        Err(e) => {
            logger.error(&format!(
                "[PROMPT_INTEGRITY] Failed to restore PROMPT.md: {e}"
            ));
            logger.error(&format!(
                "[PROMPT_INTEGRITY] Error occurred during {phase} phase (iteration {iteration})"
            ));
            logger.error("Pipeline may not function correctly without PROMPT.md");
        }
    }
}

use super::context::PhaseContext;

/// Result of the development phase.
pub struct DevelopmentResult {
    /// Whether any errors occurred during the phase.
    pub had_errors: bool,
}

/// Run the development phase.
///
/// This phase runs `developer_iters` iterations, each consisting of:
/// 1. Planning: Create PLAN.md from PROMPT.md
/// 2. Execution: Execute the plan
/// 3. Cleanup: Delete PLAN.md
///
/// # Arguments
///
/// * `ctx` - The phase context containing shared state
/// * `start_iter` - The iteration to start from (for resume support)
/// * `resuming_from_development` - Whether we're resuming into the development step
///
/// # Returns
///
/// Returns `Ok(DevelopmentResult)` on success, or an error if a critical failure occurs.
pub fn run_development_phase(
    ctx: &mut PhaseContext<'_>,
    start_iter: u32,
    resuming_from_development: bool,
) -> anyhow::Result<DevelopmentResult> {
    let mut had_errors = false;
    let mut prev_snap = git_snapshot()?;
    let developer_context = ContextLevel::from(ctx.config.developer_context);

    for i in start_iter..=ctx.config.developer_iters {
        ctx.logger.subheader(&format!(
            "Iteration {} of {}",
            i, ctx.config.developer_iters
        ));
        print_progress(i, ctx.config.developer_iters, "Overall");

        let resuming_into_development = resuming_from_development && i == start_iter;

        // Step 1: Create PLAN from PROMPT (skip if resuming into development)
        if resuming_into_development {
            ctx.logger
                .info("Resuming at development step; skipping plan generation");
        } else {
            run_planning_step(ctx, i)?;
        }

        // Verify PLAN.md was created (required)
        let plan_ok = verify_plan_exists(ctx, i, resuming_into_development)?;
        if !plan_ok {
            anyhow::bail!("Planning phase did not create a non-empty .agent/PLAN.md");
        }
        ctx.logger.success("PLAN.md created");

        // Save checkpoint at start of development phase (if enabled)
        if ctx.config.checkpoint_enabled {
            let _ = save_checkpoint(&PipelineCheckpoint::new(
                PipelinePhase::Development,
                i,
                ctx.config.developer_iters,
                0,
                ctx.config.reviewer_reviews,
                ctx.developer_agent,
                ctx.reviewer_agent,
            ));
        }

        // Step 2: Execute the PLAN
        ctx.logger.info("Executing plan...");
        update_status("Starting development iteration", ctx.config.isolation_mode)?;

        let prompt = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            developer_context,
            Some(i),
            Some(ctx.config.developer_iters),
            None, // No guidelines needed for development iteration
            None, // No PROMPT.md content needed for iteration
        );

        let exit_code = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
            };
            run_with_fallback(
                AgentRole::Developer,
                &format!("run #{i}"),
                &prompt,
                &format!(".agent/logs/developer_{i}"),
                &mut runtime,
                ctx.registry,
                ctx.developer_agent,
            )?
        };

        if exit_code != 0 {
            ctx.logger.error(&format!(
                "Iteration {i} encountered an error but continuing"
            ));
            had_errors = true;
        }

        ctx.stats.developer_runs_completed += 1;
        update_status("Completed progress step", ctx.config.isolation_mode)?;

        let snap = git_snapshot()?;
        if snap == prev_snap {
            ctx.logger.warn("No git-status change detected");
        } else {
            ctx.logger.success("Repository modified");
            ctx.stats.changes_detected += 1;

            // Create a commit with auto-generated message
            // This is done by the orchestrator, not the agent
            // Note: We use fallback-aware commit generation which tries multiple agents
            // Get the primary commit agent from the registry
            let commit_agent = get_primary_commit_agent(ctx);

            if let Some(agent) = commit_agent {
                ctx.logger.info(&format!(
                    "Creating commit with auto-generated message (agent: {agent})..."
                ));

                // Get the diff for commit message generation
                let diff = match crate::git_helpers::git_diff() {
                    Ok(d) => d,
                    Err(e) => {
                        ctx.logger
                            .error(&format!("Failed to get diff for commit: {e}"));
                        return Err(anyhow::anyhow!(e));
                    }
                };

                // Get git identity from config
                let git_name = ctx.config.git_user_name.as_deref();
                let git_email = ctx.config.git_user_email.as_deref();

                match commit_with_generated_message(
                    &diff,
                    &agent,
                    git_name,
                    git_email,
                    ctx.registry,
                    ctx.logger,
                    ctx.colors,
                    ctx.config,
                    ctx.timer,
                ) {
                    CommitResultFallback::Success(oid) => {
                        ctx.logger
                            .success(&format!("Commit created successfully: {oid}"));
                        ctx.stats.commits_created += 1;
                    }
                    CommitResultFallback::NoChanges => {
                        // No meaningful changes to commit (already handled by has_meaningful_changes)
                        ctx.logger.info("No commit created (no meaningful changes)");
                    }
                    CommitResultFallback::Failed(err) => {
                        // Actual git operation failed - this is critical
                        ctx.logger.error(&format!(
                            "Failed to create commit (git operation failed): {err}"
                        ));
                        // Don't continue - this is a real error that needs attention
                        return Err(anyhow::anyhow!(err));
                    }
                }
            } else {
                ctx.logger
                    .warn("Unable to get primary commit agent for commit");
            }
        }
        prev_snap = snap;

        // Run fast check if configured
        if let Some(ref fast_cmd) = ctx.config.fast_check_cmd {
            run_fast_check(ctx, fast_cmd, i)?;
        }

        // Periodic restoration check - ensure PROMPT.md still exists
        // This catches agent deletions and restores from backup
        ensure_prompt_integrity(ctx.logger, "development", i);

        // Step 3: Delete the PLAN
        ctx.logger.info("Deleting PLAN.md...");
        if let Err(err) = delete_plan_file() {
            ctx.logger.warn(&format!("Failed to delete PLAN.md: {err}"));
        }
        ctx.logger.success("PLAN.md deleted");
    }

    Ok(DevelopmentResult { had_errors })
}

/// Run the planning step to create PLAN.md.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent JSON output.
/// Agent file writes are ignored - the orchestrator is the sole writer.
fn run_planning_step(ctx: &mut PhaseContext<'_>, iteration: u32) -> anyhow::Result<()> {
    // Save checkpoint at start of planning phase (if enabled)
    if ctx.config.checkpoint_enabled {
        let _ = save_checkpoint(&PipelineCheckpoint::new(
            PipelinePhase::Planning,
            iteration,
            ctx.config.developer_iters,
            0,
            ctx.config.reviewer_reviews,
            ctx.developer_agent,
            ctx.reviewer_agent,
        ));
    }

    ctx.logger.info("Creating plan from PROMPT.md...");
    update_status("Starting planning phase", ctx.config.isolation_mode)?;

    // Read PROMPT.md content to include directly in the planning prompt
    // This prevents agents from discovering PROMPT.md through file exploration,
    // which reduces the risk of accidental deletion.
    let prompt_md_content = std::fs::read_to_string("PROMPT.md").ok();

    let plan_prompt = prompt_for_agent(
        Role::Developer,
        Action::Plan,
        ContextLevel::Normal,
        None,
        None,
        None, // No guidelines needed for planning
        prompt_md_content.as_deref(),
    );

    let log_dir = format!(".agent/logs/planning_{iteration}");
    let _exit_code = {
        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
        };
        run_with_fallback(
            AgentRole::Developer,
            &format!("planning #{iteration}"),
            &plan_prompt,
            &log_dir,
            &mut runtime,
            ctx.registry,
            ctx.developer_agent,
        )
    }?;

    // ORCHESTRATOR-CONTROLLED FILE I/O:
    // Prefer extraction from JSON log (orchestrator write), but fall back to
    // agent-written file if extraction fails (legacy/test compatibility).
    let plan_path = Path::new(".agent/PLAN.md");
    let log_dir_path = Path::new(&log_dir);

    // Ensure .agent directory exists
    if let Some(parent) = plan_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let extraction = extract_plan(log_dir_path)?;

    if let Some(content) = extraction.raw_content {
        // Extraction succeeded - orchestrator writes the file
        fs::write(plan_path, &content)?;

        if extraction.is_valid {
            ctx.logger
                .success("Plan extracted from agent output (JSON)");
        } else {
            ctx.logger.warn(&format!(
                "Plan written but validation failed: {}",
                extraction.validation_warning.unwrap_or_default()
            ));
        }
    } else {
        // JSON extraction failed - try text-based fallback
        ctx.logger
            .info("No JSON result event found, trying text-based extraction...");

        if let Some(text_plan) = extract_plan_from_logs_text(log_dir_path)? {
            fs::write(plan_path, &text_plan)?;
            ctx.logger
                .success("Plan extracted from agent output (text fallback)");
        } else {
            // Text extraction also failed - check if agent wrote the file directly (legacy fallback)
            let agent_wrote_file = plan_path
                .exists()
                .then(|| fs::read_to_string(plan_path).ok())
                .flatten()
                .is_some_and(|s| !s.trim().is_empty());

            if agent_wrote_file {
                ctx.logger.info("Using agent-written PLAN.md (legacy mode)");
            } else {
                // No content from any source - write placeholder and fail
                // The placeholder serves as a recovery mechanism (file exists for debugging)
                // but the pipeline should still fail because we can't proceed without a plan
                let placeholder = "# Plan\n\nAgent produced no extractable plan content.\n";
                fs::write(plan_path, placeholder)?;
                ctx.logger
                    .error("No plan content found in agent output - wrote placeholder");
                anyhow::bail!(
                    "Planning agent completed successfully but no plan was found in output"
                );
            }
        }
    }

    Ok(())
}

/// Verify that PLAN.md exists and is non-empty.
///
/// With orchestrator-controlled file I/O, `run_planning_step` always writes
/// PLAN.md (even if just a placeholder). This function checks if the file
/// exists and has meaningful content. If resuming and plan is missing,
/// re-run planning.
fn verify_plan_exists(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    resuming_into_development: bool,
) -> anyhow::Result<bool> {
    let plan_path = Path::new(".agent/PLAN.md");

    let plan_ok = plan_path
        .exists()
        .then(|| fs::read_to_string(plan_path).ok())
        .flatten()
        .is_some_and(|s| !s.trim().is_empty());

    // If resuming and plan is missing, re-run planning to recover
    if !plan_ok && resuming_into_development {
        ctx.logger
            .warn("Missing .agent/PLAN.md; rerunning plan generation to recover");
        run_planning_step(ctx, iteration)?;

        // Check again after rerunning - orchestrator guarantees file exists
        let plan_ok = plan_path
            .exists()
            .then(|| fs::read_to_string(plan_path).ok())
            .flatten()
            .is_some_and(|s| !s.trim().is_empty());

        return Ok(plan_ok);
    }

    Ok(plan_ok)
}

/// Run fast check command.
fn run_fast_check(ctx: &PhaseContext<'_>, fast_cmd: &str, iteration: u32) -> anyhow::Result<()> {
    let argv = crate::cli::split_command(fast_cmd)
        .map_err(|e| anyhow::anyhow!("FAST_CHECK_CMD parse error (iteration {iteration}): {e}"))?;
    if argv.is_empty() {
        ctx.logger
            .warn("FAST_CHECK_CMD is empty; skipping fast check");
        return Ok(());
    }

    let display_cmd = crate::cli::format_argv_for_log(&argv);
    ctx.logger.info(&format!(
        "Running fast check: {}{}{}",
        ctx.colors.dim(),
        display_cmd,
        ctx.colors.reset()
    ));

    let Some((program, cmd_args)) = argv.split_first() else {
        ctx.logger
            .warn("FAST_CHECK_CMD is empty after parsing; skipping fast check");
        return Ok(());
    };
    let status = Command::new(program).args(cmd_args).status()?;

    if status.success() {
        ctx.logger.success("Fast check passed");
    } else {
        ctx.logger.warn("Fast check had issues (non-blocking)");
    }

    Ok(())
}
