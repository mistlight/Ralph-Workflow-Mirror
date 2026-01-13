//! Development phase execution.
//!
//! This module handles the development phase of the Ralph pipeline, which consists
//! of iterative planning and execution cycles. Each iteration:
//! 1. Creates a PLAN.md from PROMPT.md
//! 2. Executes the plan
//! 3. Deletes PLAN.md
//! 4. Optionally runs fast checks

use crate::agents::AgentRole;
use crate::git_helpers::{commit_with_auto_message_result, git_snapshot};
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{prompt_for_agent, Action, ContextLevel, Role};
use crate::utils::{
    delete_plan_file, print_progress, save_checkpoint, update_status, PipelineCheckpoint,
    PipelinePhase,
};
use serde_json::Value as JsonValue;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;

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
        if !resuming_into_development {
            run_planning_step(ctx, i)?;
        } else {
            ctx.logger
                .info("Resuming at development step; skipping plan generation");
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
                &format!("run #{}", i),
                &prompt,
                &format!(".agent/logs/developer_{}", i),
                &mut runtime,
                ctx.registry,
                ctx.developer_agent,
            )?
        };

        if exit_code != 0 {
            ctx.logger.error(&format!(
                "Iteration {} encountered an error but continuing",
                i
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
            // Note: commit_with_auto_message has fallback behavior if LLM fails
            // We use the developer agent for commit message generation (not reviewer)
            // because the commit should reflect the development work that was done
            let agent_cmd = ctx
                .config
                .developer_cmd
                .clone()
                .or_else(|| ctx.registry.developer_cmd(ctx.developer_agent));
            if let Some(agent_cmd) = agent_cmd {
                ctx.logger
                    .info("Creating commit with auto-generated message...");
                match commit_with_auto_message_result(&agent_cmd) {
                    crate::git_helpers::CommitResult::Success(oid) => {
                        ctx.logger.success(&format!("Commit created successfully: {}", oid));
                        ctx.stats.commits_created += 1;
                    }
                    crate::git_helpers::CommitResult::NoChanges => {
                        // No meaningful changes to commit (already handled by has_meaningful_changes)
                        ctx.logger.info("No commit created (no meaningful changes)");
                    }
                    crate::git_helpers::CommitResult::Failed(err) => {
                        // Actual git operation failed - this is critical
                        // The commit_with_auto_message function handles LLM failures internally
                        // So this error indicates a real git problem
                        ctx.logger
                            .error(&format!("Failed to create commit (git operation failed): {}", err));
                        // Don't continue - this is a real error that needs attention
                        return Err(anyhow::anyhow!(err));
                    }
                }
            } else {
                ctx.logger
                    .warn("Unable to get developer agent command for commit");
            }
        }
        prev_snap = snap;

        // Run fast check if configured
        if let Some(ref fast_cmd) = ctx.config.fast_check_cmd {
            run_fast_check(ctx, fast_cmd, i)?;
        }

        // Step 3: Delete the PLAN
        ctx.logger.info("Deleting PLAN.md...");
        if let Err(err) = delete_plan_file() {
            ctx.logger
                .warn(&format!("Failed to delete PLAN.md: {}", err));
        }
        ctx.logger.success("PLAN.md deleted");
    }

    Ok(DevelopmentResult { had_errors })
}

/// Extract the plan from the agent's JSON log file.
///
/// This is a fallback mechanism for when the agent couldn't create PLAN.md directly
/// (e.g., due to permission denials from GLM). It parses the JSON log to find
/// the final result text which contains the plan.
fn try_extract_plan_from_log(
    logger: &crate::utils::Logger,
    iteration: u32,
    _developer_agent: &str,
) -> anyhow::Result<Option<String>> {
    // Note: developer_agent is accepted for future use but currently unused
    // The log directory structure uses the iteration number, not the agent name
    let log_dir = Path::new(".agent/logs").join(format!("planning_{}", iteration));

    // Try to find any log file in the planning directory
    let log_entries = match fs::read_dir(&log_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(None),
    };

    for entry in log_entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Read the log file and parse JSON lines
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let reader = BufReader::new(file);
        let mut last_result: Option<String> = None;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            // Skip non-JSON lines
            if !line.trim().starts_with('{') {
                continue;
            }

            // Parse JSON and look for "result" events
            if let Ok(value) = serde_json::from_str::<JsonValue>(&line) {
                if let Some(typ) = value.get("type").and_then(|v| v.as_str()) {
                    if typ == "result" {
                        if let Some(result) = value.get("result").and_then(|v| v.as_str()) {
                            last_result = Some(result.to_string());
                        }
                    }
                }
            }
        }

        if let Some(result) = last_result {
            // Check if result looks like a plan using more robust validation:
            // 1. Contains markdown headers (lines starting with #)
            // 2. Has reasonable length (> 50 chars minimum for a meaningful plan)
            // 3. Contains plan-like structure indicators (steps, tasks, phases, etc.)
            let result_clean = result.trim().to_string();
            let has_header = result_clean.lines().any(|line| line.trim().starts_with('#'));
            let has_min_length = result_clean.len() > 50;
            let has_structure = result_clean.contains("step")
                || result_clean.contains("task")
                || result_clean.contains("phase")
                || result_clean.contains("implement")
                || result_clean.contains("create")
                || result_clean.contains("add");

            if has_header && has_min_length && has_structure {
                logger.info(&format!(
                    "Extracted plan from agent output log: {}",
                    path.display()
                ));
                return Ok(Some(result_clean));
            }
        }
    }

    Ok(None)
}

/// Run the planning step to create PLAN.md.
///
/// The agent returns structured plan content (captured by JSON parser),
/// and the orchestrator writes it to .agent/PLAN.md.
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

    let plan_prompt = prompt_for_agent(
        Role::Developer,
        Action::Plan,
        ContextLevel::Normal,
        None,
        None,
        None, // No guidelines needed for planning
    );

    let log_dir = format!(".agent/logs/planning_{}", iteration);
    let exit_code = {
        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
        };
        run_with_fallback(
            AgentRole::Developer,
            &format!("planning #{}", iteration),
            &plan_prompt,
            &log_dir,
            &mut runtime,
            ctx.registry,
            ctx.developer_agent,
        )
    }?;

    // After agent completes, check if PLAN.md was created or extract from JSON log
    // The orchestrator (not the agent) handles file I/O for production agents
    // For tests/legacy agents that write PLAN.md directly, we accept that too
    let plan_path = Path::new(".agent/PLAN.md");
    let plan_exists = plan_path
        .exists()
        .then(|| fs::read_to_string(plan_path).ok())
        .flatten()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if !plan_exists {
        // Try to extract plan from JSON log (for production agents)
        if let Some(plan_content) = try_extract_plan_from_log(ctx.logger, iteration, ctx.developer_agent)?
        {
            if let Some(parent) = plan_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(plan_path, &plan_content)?;
            ctx.logger.success("Extracted plan from agent output and created PLAN.md");
        } else if exit_code == 0 {
            // Agent succeeded but we couldn't extract a plan and no PLAN.md exists
            anyhow::bail!(
                "Planning agent completed successfully but no plan was found in output"
            );
        }
    } else {
        ctx.logger.success("PLAN.md created by agent");
    }
    // If exit_code != 0, the error will be propagated by the caller

    Ok(())
}

/// Verify that PLAN.md exists and is non-empty.
///
/// With the new architecture, the orchestrator writes PLAN.md after extracting
/// the plan content from the agent's JSON output. If PLAN.md is missing,
/// we attempt to extract it from the log as a fallback.
/// If resuming and plan is still missing, re-run planning.
fn verify_plan_exists(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    resuming_into_development: bool,
) -> anyhow::Result<bool> {
    let plan_path = std::path::Path::new(".agent/PLAN.md");

    let mut plan_ok = plan_path
        .exists()
        .then(|| fs::read_to_string(plan_path).ok())
        .flatten()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    // Fallback: try to extract plan from agent's JSON log output
    // This handles edge cases where the orchestrator failed to write the file
    if !plan_ok {
        ctx.logger
            .info("PLAN.md not found; attempting to extract from agent output log...");
        if let Ok(Some(extracted_plan)) =
            try_extract_plan_from_log(ctx.logger, iteration, ctx.developer_agent)
        {
            // Ensure .agent directory exists
            if let Some(parent) = plan_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(plan_path, &extracted_plan)?;
            ctx.logger.success("Extracted plan and created PLAN.md");
            plan_ok = true;
        }
    }

    if !plan_ok && resuming_into_development {
        ctx.logger
            .warn("Missing .agent/PLAN.md; rerunning plan generation to recover");
        run_planning_step(ctx, iteration)?;

        // Check again after rerunning
        plan_ok = plan_path
            .exists()
            .then(|| fs::read_to_string(plan_path).ok())
            .flatten()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        // Try extraction one more time after rerun
        if !plan_ok {
            if let Ok(Some(extracted_plan)) =
                try_extract_plan_from_log(ctx.logger, iteration, ctx.developer_agent)
            {
                fs::write(plan_path, &extracted_plan)?;
                ctx.logger
                    .success("Extracted plan from second attempt and created PLAN.md");
                plan_ok = true;
            }
        }
    }

    Ok(plan_ok)
}

/// Run fast check command.
fn run_fast_check(ctx: &PhaseContext<'_>, fast_cmd: &str, iteration: u32) -> anyhow::Result<()> {
    let argv = crate::utils::split_command(fast_cmd).map_err(|e| {
        anyhow::anyhow!(
            "FAST_CHECK_CMD parse error (iteration {}): {}",
            iteration,
            e
        )
    })?;
    if argv.is_empty() {
        ctx.logger
            .warn("FAST_CHECK_CMD is empty; skipping fast check");
        return Ok(());
    }

    let display_cmd = crate::utils::format_argv_for_log(&argv);
    ctx.logger.info(&format!(
        "Running fast check: {}{}{}",
        ctx.colors.dim(),
        display_cmd,
        ctx.colors.reset()
    ));

    let (program, args) = argv
        .split_first()
        .expect("argv is non-empty after empty check");
    let status = Command::new(program).args(args).status()?;

    if status.success() {
        ctx.logger.success("Fast check passed");
    } else {
        ctx.logger.warn("Fast check had issues (non-blocking)");
    }

    Ok(())
}
