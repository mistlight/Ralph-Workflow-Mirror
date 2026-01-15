//! Planning step for development phase.
//!
//! This module handles the creation of PLAN.md from PROMPT.md, including
//! orchestrator-controlled file I/O and various extraction strategies.

use crate::agents::AgentRole;
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::files::{extract_plan, extract_plan_from_logs_text, update_status};
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{prompt_for_agent, Action, ContextLevel, Role};
use std::fs;
use std::path::Path;

use crate::phases::context::PhaseContext;

/// Run the planning step to create PLAN.md.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent JSON output.
/// Agent file writes are ignored - the orchestrator is the sole writer.
pub fn run_planning_step(ctx: &mut PhaseContext<'_>, iteration: u32) -> anyhow::Result<()> {
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
pub fn verify_plan_exists(
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
