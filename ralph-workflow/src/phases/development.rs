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
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    extract_development_result_xml, extract_plan_xml, format_xml_for_display,
    validate_development_result_xml, validate_plan_xml, PlanElements,
};
use crate::files::{delete_plan_file, update_status};
use crate::git_helpers::{git_snapshot, CommitResultFallback};
use crate::logger::print_progress;
use crate::phases::commit::commit_with_generated_message;
use crate::phases::get_primary_commit_agent;
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_developer_iteration_xml_with_context,
    prompt_developer_iteration_xsd_retry_with_context, prompt_planning_xml_with_context,
    prompt_planning_xsd_retry_with_context, ContextLevel,
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

        // Run development iteration with XML extraction and XSD validation
        let dev_result = run_development_iteration_with_xml_retry(
            ctx,
            i,
            developer_context,
            resuming_into_development,
            resume_context,
        )?;

        if dev_result.had_error {
            ctx.logger.error(&format!(
                "Iteration {i} encountered an error but continuing"
            ));
            had_errors = true;
        }

        // Record stats
        ctx.stats.developer_runs_completed += 1;

        // Record execution history
        {
            let dev_start_time = Instant::now(); // Note: this is after the iteration runs
            let duration = dev_start_time.elapsed().as_secs();
            let outcome = if dev_result.had_error {
                StepOutcome::failure("Agent exited with non-zero code".to_string(), true)
            } else {
                StepOutcome::success(
                    dev_result.summary.clone(),
                    dev_result.files_changed.clone().unwrap_or_default(),
                )
            };
            let step = ExecutionStep::new("Development", i, "dev_run", outcome)
                .with_agent(ctx.developer_agent)
                .with_duration(duration);
            ctx.execution_history.add_step(step);
        }
        update_status("Completed progress step", ctx.config.isolation_mode)?;

        // Log the development result
        if let Some(ref summary) = dev_result.summary {
            ctx.logger
                .info(&format!("Development summary: {}", summary));
        }

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

/// Result of a single development iteration.
struct DevIterationResult {
    /// Whether an error occurred during the iteration.
    had_error: bool,
    /// Optional summary of what was done.
    summary: Option<String>,
    /// Optional list of files changed.
    files_changed: Option<Vec<String>>,
}

/// Run a single development iteration with XML extraction and XSD validation retry loop.
///
/// This function implements a nested loop structure:
/// - **Outer loop (continuation)**: Continue while status != "completed" (max 100)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback (max 10)
///
/// The continuation logic ignores non-XSD errors and only looks for valid XML.
/// If XML passes XSD validation with status="completed", we're done for this iteration.
/// If XML passes XSD validation with status="partial", we continue the outer loop.
/// If XML passes XSD validation with status="failed", we continue the outer loop.
///
/// The development iteration produces side effects (file changes) as its primary output.
/// The XML status is secondary - we use it for logging/tracking but don't fail the
/// entire iteration if XML is missing or invalid.
fn run_development_iteration_with_xml_retry(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    _developer_context: ContextLevel,
    _resuming_into_development: bool,
    _resume_context: Option<&ResumeContext>,
) -> anyhow::Result<DevIterationResult> {
    let prompt_md = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_md = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();
    let log_dir = format!(".agent/logs/developer_{iteration}");

    let max_xsd_retries = 10;
    let max_continuations = 100; // Safety limit to prevent infinite loops
    let mut final_summary: Option<String> = None;
    let mut final_files_changed: Option<Vec<String>> = None;
    let mut had_any_error = false;

    // Outer loop: Continue until agent returns status="completed" or we hit the limit
    'continuation: for continuation_num in 0..max_continuations {
        let is_continuation = continuation_num > 0;
        if is_continuation {
            ctx.logger.info(&format!(
                "Continuation {} of {} (status was not 'completed')",
                continuation_num, max_continuations
            ));
        }

        let mut xsd_error: Option<String> = None;

        // Inner loop: XSD validation retry with error feedback
        for retry_num in 0..max_xsd_retries {
            let is_retry = retry_num > 0;
            let total_attempts = continuation_num * max_xsd_retries + retry_num + 1;

            // For initial attempt, use XML prompt
            // For retries, use XSD retry prompt with error feedback
            let dev_prompt = if !is_retry && !is_continuation {
                // First attempt ever - use initial XML prompt
                let prompt_key = format!("development_{}", iteration);
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_developer_iteration_xml_with_context(
                            ctx.template_context,
                            &prompt_md,
                            &plan_md,
                        )
                    });

                if !was_replayed {
                    ctx.capture_prompt(&prompt_key, &prompt);
                } else {
                    ctx.logger.info(&format!(
                        "Using stored prompt from checkpoint for determinism: {}",
                        prompt_key
                    ));
                }

                prompt
            } else if !is_continuation {
                // XSD retry only (no continuation yet)
                ctx.logger.info(&format!(
                    "  In-session retry {}/{} for XSD validation (total attempt: {})",
                    retry_num,
                    max_xsd_retries - 1,
                    total_attempts
                ));
                if let Some(ref error) = xsd_error {
                    ctx.logger.info(&format!("  XSD error: {}", error));
                }

                let last_output = read_last_development_output(Path::new(&log_dir));

                prompt_developer_iteration_xsd_retry_with_context(
                    ctx.template_context,
                    &prompt_md,
                    &plan_md,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                )
            } else if !is_retry {
                // Continuation only (first XSD attempt after continuation)
                ctx.logger.info(&format!(
                    "  Continuation attempt {} (XSD validation attempt {}/{})",
                    total_attempts, 1, max_xsd_retries
                ));

                prompt_developer_iteration_xml_with_context(
                    ctx.template_context,
                    &prompt_md,
                    &plan_md,
                )
            } else {
                // Both continuation and XSD retry
                ctx.logger.info(&format!(
                    "  Continuation retry {}/{} for XSD validation (total attempt: {})",
                    retry_num,
                    max_xsd_retries - 1,
                    total_attempts
                ));
                if let Some(ref error) = xsd_error {
                    ctx.logger.info(&format!("  XSD error: {}", error));
                }

                let last_output = read_last_development_output(Path::new(&log_dir));

                prompt_developer_iteration_xsd_retry_with_context(
                    ctx.template_context,
                    &prompt_md,
                    &plan_md,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                )
            };

            // Run the agent
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
                    &format!(
                        "run #{}{}",
                        iteration,
                        if is_continuation {
                            format!(" (continuation {})", continuation_num)
                        } else {
                            String::new()
                        }
                    ),
                    &dev_prompt,
                    &log_dir,
                    &mut runtime,
                    ctx.registry,
                    ctx.developer_agent,
                )?
            };

            // Track if any agent run had an error (for final result)
            if exit_code != 0 {
                had_any_error = true;
            }

            // Extract and validate the development result XML
            let log_dir_path = Path::new(&log_dir);
            let dev_content = read_last_development_output(log_dir_path);

            // Try to extract XML - if extraction fails, assume entire output is XML
            // and validate it to get specific XSD errors for retry
            let xml_to_validate =
                if let Some(xml_content) = extract_development_result_xml(&dev_content) {
                    xml_content
                } else {
                    // No XML tags found - assume the entire content is XML for validation
                    // This allows us to get specific XSD errors to send back to the agent
                    dev_content.clone()
                };

            // Try to validate against XSD
            match validate_development_result_xml(&xml_to_validate) {
                Ok(result_elements) => {
                    // XSD validation passed - format and log the result
                    let formatted_xml = format_xml_for_display(&xml_to_validate);

                    if is_retry {
                        ctx.logger
                            .success(&format!("Status validated after {} retries", retry_num));
                    } else {
                        ctx.logger.success("Status extracted and validated (XML)");
                    }

                    // Display the formatted status
                    ctx.logger.info(&format!("\n{}", formatted_xml));

                    // Store the results
                    final_summary = Some(result_elements.summary.clone());
                    final_files_changed = result_elements
                        .files_changed
                        .as_ref()
                        .map(|f| f.lines().map(|s| s.to_string()).collect());

                    // Check the status to determine if we should continue
                    if result_elements.is_completed() {
                        // Status is "completed" - we're done with this iteration
                        return Ok(DevIterationResult {
                            had_error: had_any_error,
                            summary: final_summary,
                            files_changed: final_files_changed,
                        });
                    } else if result_elements.is_partial() {
                        // Status is "partial" - continue the outer loop
                        ctx.logger
                            .info("Status is 'partial' - continuing with same iteration");
                        continue 'continuation;
                    } else if result_elements.is_failed() {
                        // Status is "failed" - continue the outer loop
                        ctx.logger
                            .warn("Status is 'failed' - continuing with same iteration");
                        continue 'continuation;
                    }
                }
                Err(xsd_err) => {
                    // XSD validation failed - check if we can retry
                    let error_msg = format_xsd_error(&xsd_err);
                    ctx.logger
                        .warn(&format!("  XSD validation failed: {}", error_msg));

                    if retry_num < max_xsd_retries - 1 {
                        // Store error for next retry attempt
                        xsd_error = Some(error_msg);
                        // Continue to next XSD retry iteration
                        continue;
                    } else {
                        ctx.logger
                            .warn("  No more in-session XSD retries remaining");
                        // Fall through to return what we have
                        break 'continuation;
                    }
                }
            }
        }

        // If we've exhausted XSD retries, break the continuation loop
        ctx.logger
            .warn("XSD retry loop exhausted - stopping continuation");
        break;
    }

    // If we get here, we exhausted the continuation limit or XSD retries
    Ok(DevIterationResult {
        had_error: had_any_error,
        summary: final_summary.or_else(|| {
            Some(format!(
                "Continuation stopped after {} attempts",
                max_continuations * max_xsd_retries
            ))
        }),
        files_changed: final_files_changed,
    })
}

/// Run the planning step to create PLAN.md.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent XML output.
/// Uses XSD validation with retry loop to ensure valid XML format.
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
    let prompt_md_str = prompt_md_content.as_deref().unwrap_or("");

    // Use prompt replay if available, otherwise generate new prompt
    let (plan_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            prompt_planning_xml_with_context(ctx.template_context, Some(prompt_md_str))
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
    let plan_path = Path::new(".agent/PLAN.md");

    // Ensure .agent directory exists
    if let Some(parent) = plan_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // In-session retry loop with XSD validation feedback
    let max_retries = 10;
    let mut xsd_error: Option<String> = None;

    for retry_num in 0..max_retries {
        // For initial attempt, use XML prompt
        // For retries, use XSD retry prompt with error feedback
        let plan_prompt = if retry_num == 0 {
            plan_prompt.clone()
        } else {
            ctx.logger.info(&format!(
                "  In-session retry {}/{} for XSD validation",
                retry_num,
                max_retries - 1
            ));
            if let Some(ref error) = xsd_error {
                ctx.logger.info(&format!("  XSD error: {}", error));
            }

            // Read the last output for retry context
            let last_output = read_last_planning_output(Path::new(&log_dir));

            prompt_planning_xsd_retry_with_context(
                ctx.template_context,
                prompt_md_str,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
            )
        };

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
            &format!("planning #{}", iteration),
            &plan_prompt,
            &log_dir,
            &mut runtime,
            ctx.registry,
            ctx.developer_agent,
        )?;

        // Extract and validate the plan XML
        let log_dir_path = Path::new(&log_dir);
        let plan_content = read_last_planning_output(log_dir_path);

        // Try to extract XML - if extraction fails, assume entire output is XML
        // and validate it to get specific XSD errors for retry
        let xml_to_validate = if let Some(xml_content) = extract_plan_xml(&plan_content) {
            xml_content
        } else {
            // No XML tags found - assume the entire content is XML for validation
            // This allows us to get specific XSD errors to send back to the agent
            plan_content.clone()
        };

        // Try to validate against XSD
        match validate_plan_xml(&xml_to_validate) {
            Ok(plan_elements) => {
                // XSD validation passed - format and write the plan
                let formatted_xml = format_xml_for_display(&xml_to_validate);

                // Convert XML to markdown format for PLAN.md
                let markdown = format_plan_as_markdown(&plan_elements);
                fs::write(plan_path, &markdown)?;

                if retry_num > 0 {
                    ctx.logger
                        .success(&format!("Plan validated after {} retries", retry_num));
                } else {
                    ctx.logger.success("Plan extracted and validated (XML)");
                }

                // Display the formatted plan
                ctx.logger.info(&format!("\n{}", formatted_xml));

                // Record execution history before returning
                {
                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Planning",
                        iteration,
                        "plan_generation",
                        StepOutcome::success(None, vec![".agent/PLAN.md".to_string()]),
                    )
                    .with_agent(ctx.developer_agent)
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }

                return Ok(());
            }
            Err(xsd_err) => {
                // XSD validation failed - check if we can retry
                let error_msg = format_xsd_error(&xsd_err);
                ctx.logger
                    .warn(&format!("  XSD validation failed: {}", error_msg));

                if retry_num < max_retries - 1 {
                    // Store error for next retry attempt
                    xsd_error = Some(error_msg);
                    // Continue to next retry iteration
                    continue;
                } else {
                    ctx.logger
                        .error("  No more in-session XSD retries remaining");
                    // Write placeholder and fail
                    let placeholder = "# Plan\n\nAgent produced no valid XML output. Only XML format is accepted.\n";
                    fs::write(plan_path, placeholder)?;
                    anyhow::bail!(
                        "Planning agent did not produce valid XML output after {} attempts",
                        max_retries
                    );
                }
            }
        }
    }

    // Record execution history for failed planning (should never be reached since we always return above)
    {
        let duration = start_time.elapsed().as_secs();
        let step = ExecutionStep::new(
            "Planning",
            iteration,
            "plan_generation",
            StepOutcome::failure("No valid XML output produced".to_string(), false),
        )
        .with_agent(ctx.developer_agent)
        .with_duration(duration);
        ctx.execution_history.add_step(step);
    }

    anyhow::bail!("Planning failed after {} XSD retry attempts", max_retries)
}

/// Read the last planning output from logs.
fn read_last_planning_output(log_dir: &Path) -> String {
    // Try to read from the latest log file
    let log_path = log_dir.join("latest.log");
    if let Ok(content) = fs::read_to_string(&log_path) {
        return content;
    }

    // Fallback to reading all .log files in the directory
    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                if let Ok(content) = fs::read_to_string(&path) {
                    return content;
                }
            }
        }
    }

    String::new()
}

/// Read the last development output from logs.
fn read_last_development_output(log_dir: &Path) -> String {
    // Try to read from the latest log file
    let log_path = log_dir.join("latest.log");
    if let Ok(content) = fs::read_to_string(&log_path) {
        return content;
    }

    // Fallback to reading all .log files in the directory
    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                if let Ok(content) = fs::read_to_string(&path) {
                    return content;
                }
            }
        }
    }

    String::new()
}

/// Format XSD error for display.
fn format_xsd_error(error: &XsdValidationError) -> String {
    format!(
        "{} - expected: {}, found: {}",
        error.element_path, error.expected, error.found
    )
}

/// Format plan elements as markdown for PLAN.md.
fn format_plan_as_markdown(elements: &PlanElements) -> String {
    let mut result = String::new();

    result.push_str("## Summary\n\n");
    result.push_str(&elements.summary);
    result.push_str("\n\n");

    result.push_str("## Implementation Steps\n\n");
    result.push_str(&elements.implementation_steps);
    result.push_str("\n\n");

    if let Some(ref critical_files) = elements.critical_files {
        result.push_str("## Critical Files for Implementation\n\n");
        result.push_str(critical_files);
        result.push_str("\n\n");
    }

    if let Some(ref risks) = elements.risks_mitigations {
        result.push_str("## Risks & Mitigations\n\n");
        result.push_str(risks);
        result.push_str("\n\n");
    }

    if let Some(ref verification) = elements.verification_strategy {
        result.push_str("## Verification Strategy\n\n");
        result.push_str(verification);
        result.push_str("\n\n");
    }

    result
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
                        StepOutcome::success(Some(oid.to_string()), vec![]),
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
                        StepOutcome::skipped("No meaningful changes to commit".to_string()),
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
                        StepOutcome::failure(err.to_string(), false),
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
                StepOutcome::failure("No commit agent available".to_string(), true),
            )
            .with_duration(duration);
            ctx.execution_history.add_step(step);
        }
    }

    Ok(())
}
