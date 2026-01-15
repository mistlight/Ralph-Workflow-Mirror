//! Core development iteration execution logic.
//!
//! This module contains the main loop for the development phase, handling
//! each iteration of planning, execution, and cleanup.

use crate::agents::AgentRole;
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::files::{delete_plan_file, update_status};
use crate::git_helpers::{git_diff, git_snapshot, CommitResultFallback};
use crate::logger::print_progress;
use crate::phases::commit::commit_with_generated_message;
use crate::phases::context::PhaseContext;
use crate::phases::get_primary_commit_agent;
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{prompt_for_agent, Action, ContextLevel, Role};

use super::planning::{run_planning_step, verify_plan_exists};
use super::util::DevelopmentResult;

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
        if ctx.config.features.checkpoint_enabled {
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
                let diff = match git_diff() {
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

                match commit_with_generated_message(&diff, &agent, git_name, git_email, ctx) {
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
            super::util::run_fast_check(ctx, fast_cmd, i)?;
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
