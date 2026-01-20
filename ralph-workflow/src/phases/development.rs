//! Development phase execution.
//!
//! This module handles the development phase of the Ralph pipeline, which consists
//! of iterative planning and execution cycles. Each iteration:
//! 1. Creates a PLAN.md from PROMPT.md
//! 2. Executes the plan
//! 3. Deletes PLAN.md
//! 4. Optionally runs fast checks

use crate::agents::AgentRole;
use crate::checkpoint::restore::ResumeContext;
use crate::checkpoint::{save_checkpoint, CheckpointBuilder, PipelinePhase};
use crate::files::{delete_plan_file, update_status};
use crate::files::{extract_plan, extract_plan_from_logs_text};
use crate::git_helpers::{git_snapshot, CommitResultFallback};
use crate::logger::print_progress;
use crate::phases::commit::commit_with_generated_message;
use crate::phases::get_primary_commit_agent;
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_for_agent, Action, ContextLevel, PromptConfig, Role,
};
use std::fs;
use std::path::Path;
use std::process::Command;

use super::context::PhaseContext;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};

use std::time::Instant;

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
/// * `resume_context` - Optional resume context for resumed sessions
///
/// # Returns
///
/// Returns `Ok(DevelopmentResult)` on success, or an error if a critical failure occurs.
pub fn run_development_phase(
    ctx: &mut PhaseContext<'_>,
    start_iter: u32,
    resume_context: Option<&ResumeContext>,
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

        let resuming_into_development = resume_context.is_some() && i == start_iter;

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
            let builder = CheckpointBuilder::new()
                .phase(PipelinePhase::Development, i, ctx.config.developer_iters)
                .reviewer_pass(0, ctx.config.reviewer_reviews)
                .capture_from_context(
                    ctx.config,
                    ctx.registry,
                    ctx.developer_agent,
                    ctx.reviewer_agent,
                    ctx.logger,
                    &ctx.run_context,
                )
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint(&checkpoint);
            }
        }

        // Record this iteration as completed
        ctx.record_developer_iteration();

        // Step 2: Execute the PLAN
        ctx.logger.info("Executing plan...");
        update_status("Starting development iteration", ctx.config.isolation_mode)?;

        // Read PROMPT.md and PLAN.md content directly to pass as context.
        // This prevents agents from discovering these files through exploration,
        // reducing the risk of accidental deletion.
        let prompt_md = fs::read_to_string("PROMPT.md").unwrap_or_default();
        let plan_md = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();

        let mut prompt_config = PromptConfig::new()
            .with_iterations(i, ctx.config.developer_iters)
            .with_prompt_and_plan(prompt_md, plan_md);

        // Set resume context if this is the first iteration of a resumed session
        if resuming_into_development {
            if let Some(resume_ctx) = resume_context {
                prompt_config = prompt_config.with_resume_context(resume_ctx.clone());
            }
        }

        // Use prompt replay if available, otherwise generate new prompt
        let prompt_key = format!("development_{}", i);
        let (prompt, was_replayed) =
            get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                prompt_for_agent(
                    Role::Developer,
                    Action::Iterate,
                    developer_context,
                    ctx.template_context,
                    prompt_config.clone(),
                )
            });

        // Capture the prompt for checkpoint/resume (only if newly generated)
        if !was_replayed {
            ctx.capture_prompt(&prompt_key, &prompt);
        } else {
            ctx.logger.info(&format!(
                "Using stored prompt from checkpoint for determinism: {}",
                prompt_key
            ));
        }

        let dev_start_time = Instant::now();

        let exit_code = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
                #[cfg(any(test, feature = "test-utils"))]
                agent_executor: None,
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

        {
            let duration = dev_start_time.elapsed().as_secs();
            let outcome = if exit_code != 0 {
                StepOutcome::Failure {
                    error: format!("Agent exited with code {exit_code}"),
                    recoverable: true,
                }
            } else {
                StepOutcome::Success {
                    output: None,
                    files_modified: vec![],
                }
            };
            let step = ExecutionStep::new("Development", i, "dev_run", outcome)
                .with_agent(ctx.developer_agent)
                .with_duration(duration);
            ctx.execution_history.add_step(step);
        }
        update_status("Completed progress step", ctx.config.isolation_mode)?;

        let snap = git_snapshot()?;
        if snap == prev_snap {
            if snap.is_empty() {
                ctx.logger
                    .warn("No git-status change detected (repository is clean)");
            } else {
                ctx.logger.warn(&format!(
                    "No git-status change detected (existing changes: {})",
                    snap.lines().count()
                ));
            }
        } else {
            ctx.logger.success(&format!(
                "Repository modified ({} file(s) changed)",
                snap.lines().count()
            ));
            ctx.stats.changes_detected += 1;
            handle_commit_after_development(ctx, i)?;
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

        // Save checkpoint after iteration completes (if enabled)
        // This checkpoint captures the completed iteration so resume won't re-run it
        if ctx.config.features.checkpoint_enabled {
            let next_iteration = i + 1;
            let builder = CheckpointBuilder::new()
                .phase(
                    PipelinePhase::Development,
                    next_iteration,
                    ctx.config.developer_iters,
                )
                .reviewer_pass(0, ctx.config.reviewer_reviews)
                .capture_from_context(
                    ctx.config,
                    ctx.registry,
                    ctx.developer_agent,
                    ctx.reviewer_agent,
                    ctx.logger,
                    &ctx.run_context,
                )
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint(&checkpoint);
            }
        }
    }

    Ok(DevelopmentResult { had_errors })
}

/// Run the planning step to create PLAN.md.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent JSON output.
/// Agent file writes are ignored - the orchestrator is the sole writer.
fn run_planning_step(ctx: &mut PhaseContext<'_>, iteration: u32) -> anyhow::Result<()> {
    let start_time = Instant::now();
    // Save checkpoint at start of planning phase (if enabled)
    if ctx.config.features.checkpoint_enabled {
        let builder = CheckpointBuilder::new()
            .phase(
                PipelinePhase::Planning,
                iteration,
                ctx.config.developer_iters,
            )
            .reviewer_pass(0, ctx.config.reviewer_reviews)
            .capture_from_context(
                ctx.config,
                ctx.registry,
                ctx.developer_agent,
                ctx.reviewer_agent,
                ctx.logger,
                &ctx.run_context,
            )
            .with_execution_history(ctx.execution_history.clone())
            .with_prompt_history(ctx.clone_prompt_history());

        if let Some(checkpoint) = builder.build() {
            let _ = save_checkpoint(&checkpoint);
        }
    }

    ctx.logger.info("Creating plan from PROMPT.md...");
    update_status("Starting planning phase", ctx.config.isolation_mode)?;

    // Read PROMPT.md content to include directly in the planning prompt
    // This prevents agents from discovering PROMPT.md through file exploration,
    // which reduces the risk of accidental deletion.
    let prompt_md_content = std::fs::read_to_string("PROMPT.md").ok();

    // Note: We don't set is_resume for planning since planning runs on each iteration.
    // The resume context is set during the development execution step.
    let prompt_key = format!("planning_{}", iteration);
    let (plan_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            prompt_for_agent(
                Role::Developer,
                Action::Plan,
                ContextLevel::Normal,
                ctx.template_context,
                prompt_md_content
                    .as_ref()
                    .map(|content| PromptConfig::new().with_prompt_md(content.clone()))
                    .unwrap_or_default(),
            )
        });

    // Capture the planning prompt for checkpoint/resume (only if newly generated)
    if !was_replayed {
        ctx.capture_prompt(&prompt_key, &plan_prompt);
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    let log_dir = format!(".agent/logs/planning_{iteration}");
    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };
    let _exit_code = run_with_fallback(
        AgentRole::Developer,
        &format!("planning #{iteration}"),
        &plan_prompt,
        &log_dir,
        &mut runtime,
        ctx.registry,
        ctx.developer_agent,
    )?;

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

    {
        let duration = start_time.elapsed().as_secs();
        let step = ExecutionStep::new(
            "Planning",
            iteration,
            "plan_generation",
            StepOutcome::Success {
                output: None,
                files_modified: vec![".agent/PLAN.md".to_string()],
            },
        )
        .with_agent(ctx.developer_agent)
        .with_duration(duration);
        ctx.execution_history.add_step(step);
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
    let argv = crate::common::split_command(fast_cmd)
        .map_err(|e| anyhow::anyhow!("FAST_CHECK_CMD parse error (iteration {iteration}): {e}"))?;
    if argv.is_empty() {
        ctx.logger
            .warn("FAST_CHECK_CMD is empty; skipping fast check");
        return Ok(());
    }

    let display_cmd = crate::common::format_argv_for_log(&argv);
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

/// Handle commit creation after development changes are detected.
///
/// Creates a commit with an auto-generated message using the primary commit agent.
/// This is done by the orchestrator, not the agent, using fallback-aware commit
/// generation which tries multiple agents if needed.
fn handle_commit_after_development(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
) -> anyhow::Result<()> {
    let start_time = Instant::now();
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

        let result = commit_with_generated_message(&diff, &agent, git_name, git_email, ctx);

        match result {
            CommitResultFallback::Success(oid) => {
                ctx.logger
                    .success(&format!("Commit created successfully: {oid}"));
                ctx.stats.commits_created += 1;

                {
                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Development",
                        iteration,
                        "commit",
                        StepOutcome::Success {
                            output: Some(oid.to_string()),
                            files_modified: vec![],
                        },
                    )
                    .with_agent(&agent)
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }
            }
            CommitResultFallback::NoChanges => {
                // No meaningful changes to commit (already handled by has_meaningful_changes)
                ctx.logger.info("No commit created (no meaningful changes)");

                {
                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Development",
                        iteration,
                        "commit",
                        StepOutcome::Skipped {
                            reason: "No meaningful changes to commit".to_string(),
                        },
                    )
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }
            }
            CommitResultFallback::Failed(err) => {
                // Actual git operation failed - this is critical
                ctx.logger.error(&format!(
                    "Failed to create commit (git operation failed): {err}"
                ));

                {
                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Development",
                        iteration,
                        "commit",
                        StepOutcome::Failure {
                            error: err.to_string(),
                            recoverable: false,
                        },
                    )
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }

                // Don't continue - this is a real error that needs attention
                return Err(anyhow::anyhow!(err));
            }
        }
    } else {
        ctx.logger
            .warn("Unable to get primary commit agent for commit");

        {
            let duration = start_time.elapsed().as_secs();
            let step = ExecutionStep::new(
                "Development",
                iteration,
                "commit",
                StepOutcome::Failure {
                    error: "No commit agent available".to_string(),
                    recoverable: true,
                },
            )
            .with_duration(duration);
            ctx.execution_history.add_step(step);
        }
    }

    Ok(())
}
