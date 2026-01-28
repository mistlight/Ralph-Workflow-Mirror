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
use crate::checkpoint::{save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase};
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, extract_development_result_xml, extract_plan_xml,
    extract_xml_with_file_fallback_with_workspace, validate_development_result_xml,
    validate_plan_xml, xml_paths, PlanElements,
};
use crate::files::{delete_plan_file_with_workspace, update_status_with_workspace};
use crate::git_helpers::{git_snapshot, CommitResultFallback};
use crate::logger::print_progress;
use crate::phases::commit::commit_with_generated_message;
use crate::phases::get_primary_commit_agent;
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_xsd_retry_with_session, PipelineRuntime, XsdRetryConfig};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_developer_iteration_continuation_xml,
    prompt_developer_iteration_xml_with_context, prompt_developer_iteration_xsd_retry_with_context,
    prompt_planning_xml_with_context, prompt_planning_xsd_retry_with_context, ContextLevel,
};
use crate::reducer::state::{ContinuationState, DevelopmentStatus};
use std::path::Path;

use super::context::PhaseContext;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};

use std::time::Instant;

const CONTINUATION_CONTEXT_PATH: &str = ".agent/tmp/continuation_context.md";

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

        // Ensure continuation context from a previous iteration does not leak forward.
        if !resuming_into_development {
            let _ = cleanup_continuation_context_file(ctx);
        }

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
                .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
            }
        }

        // Record this iteration as completed
        ctx.record_developer_iteration();

        // Step 2: Execute the PLAN
        ctx.logger.info("Executing plan...");
        update_status_with_workspace(
            ctx.workspace,
            "Starting development iteration",
            ctx.config.isolation_mode,
        )?;

        // Run development iteration with XML extraction and XSD validation.
        // Config semantics: max_dev_continuations counts *continuation attempts* beyond the
        // initial attempt. Total valid attempts is `1 + max_dev_continuations`.
        let continuation_state = if resuming_into_development {
            load_continuation_state_from_context_file(ctx.workspace)
                .unwrap_or_else(ContinuationState::new)
        } else {
            ContinuationState::new()
        };
        let max_continuations = ctx.config.max_dev_continuations.unwrap_or(2) as usize;
        let max_total_attempts = 1 + max_continuations;
        let continuation_config = ContinuationConfig {
            state: &continuation_state,
            max_attempts: max_total_attempts,
        };

        let dev_start_time = Instant::now();
        let dev_result = run_development_iteration_with_xml_retry(
            ctx,
            i,
            developer_context,
            resuming_into_development,
            resume_context,
            None,
            continuation_config,
        )?;

        // This iteration reached status="completed"; cleanup the continuation context file.
        let _ = cleanup_continuation_context_file(ctx);

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
        update_status_with_workspace(
            ctx.workspace,
            "Completed progress step",
            ctx.config.isolation_mode,
        )?;

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
        ensure_prompt_integrity(ctx.workspace, ctx.logger, "development", i);

        // Step 3: Delete the PLAN
        ctx.logger.info("Deleting PLAN.md...");
        if let Err(err) = delete_plan_file_with_workspace(ctx.workspace) {
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
                .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
            }
        }
    }

    Ok(DevelopmentResult { had_errors })
}

/// Result of a single development iteration.
#[derive(Debug)]
pub struct DevIterationResult {
    /// Whether an error occurred during iteration.
    pub had_error: bool,
    /// Optional summary of what was done.
    pub summary: Option<String>,
    /// Optional list of files changed.
    pub files_changed: Option<Vec<String>>,
}

/// Configuration for continuation-aware development iterations.
///
/// Groups the continuation state and limit together to reduce function argument count.
#[derive(Debug, Clone)]
pub struct ContinuationConfig<'a> {
    /// Current continuation state for the iteration.
    pub state: &'a ContinuationState,
    /// Maximum number of total valid attempts allowed (initial attempt + continuations).
    pub max_attempts: usize,
}

/// Result of a single development attempt (one session), including XSD retries.
#[derive(Debug, Clone)]
pub struct DevAttemptResult {
    /// Whether any agent run returned a non-zero exit code.
    pub had_error: bool,
    /// Whether the output was successfully validated against the XSD.
    pub output_valid: bool,
    /// Development status (completed/partial/failed).
    pub status: DevelopmentStatus,
    /// Summary of what was done in this attempt.
    pub summary: String,
    /// Optional list of files changed in this attempt.
    pub files_changed: Option<Vec<String>>,
    /// Optional next steps recommended by the agent.
    pub next_steps: Option<String>,
}

/// Run a single development attempt (one session) with XML extraction and XSD validation retry loop.
///
/// This does **not** perform continuation retries. If the agent returns status="partial" or
/// status="failed", callers should trigger a fresh continuation attempt (new session) at the
/// orchestration layer.
pub fn run_development_attempt_with_xml_retry(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    _developer_context: ContextLevel,
    _resuming_into_development: bool,
    _resume_context: Option<&ResumeContext>,
    _agent: Option<&str>,
    continuation_state: &ContinuationState,
) -> anyhow::Result<DevAttemptResult> {
    let prompt_md = ctx
        .workspace
        .read(Path::new("PROMPT.md"))
        .unwrap_or_default();
    let plan_md = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();
    let log_dir = format!(".agent/logs/developer_{iteration}");

    let max_xsd_retries = crate::reducer::state::MAX_DEV_VALIDATION_RETRY_ATTEMPTS as usize;
    let is_continuation = continuation_state.is_continuation();
    let mut had_error = false;

    let mut xsd_error: Option<String> = None;
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    // Inner loop: XSD validation retry with error feedback
    // Session continuation allows the AI to retain memory between XSD retries
    for retry_num in 0..max_xsd_retries {
        let is_retry = retry_num > 0;
        let total_attempts = retry_num + 1;

        // Before each retry, check if the XML file is writable and clean up if locked
        // This prevents "permission denied" errors from stale file handles
        if is_retry {
            use crate::files::io::check_and_cleanup_xml_before_retry_with_workspace;

            let xml_path =
                Path::new(crate::files::llm_output_extraction::xml_paths::DEVELOPMENT_RESULT_XML);
            let _ = check_and_cleanup_xml_before_retry_with_workspace(
                ctx.workspace,
                xml_path,
                ctx.logger,
            );
        }

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

            let last_output = read_last_development_output(Path::new(&log_dir), ctx.workspace);

            prompt_developer_iteration_xsd_retry_with_context(
                ctx.template_context,
                &prompt_md,
                &plan_md,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
                ctx.workspace,
            )
        } else if !is_retry {
            // Continuation only (first XSD attempt after continuation)
            ctx.logger.info(&format!(
                "  Continuation attempt {} (XSD validation attempt {}/{})",
                continuation_state.continuation_attempt, 1, max_xsd_retries
            ));

            let prompt_key = format!(
                "development_{}_continuation_{}",
                iteration, continuation_state.continuation_attempt
            );
            let (prompt, was_replayed) =
                get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                    prompt_developer_iteration_continuation_xml(
                        ctx.template_context,
                        continuation_state,
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

            let last_output = read_last_development_output(Path::new(&log_dir), ctx.workspace);

            prompt_developer_iteration_xsd_retry_with_context(
                ctx.template_context,
                &prompt_md,
                &plan_md,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
                ctx.workspace,
            )
        };

        // Run the agent with session continuation for XSD retries
        // This is completely fault-tolerant - if session continuation fails for any reason
        // (including agent crash, segfault, invalid session), it falls back to normal behavior
        let exit_code = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
                executor: ctx.executor,
                executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
                workspace: ctx.workspace,
            };
            let base_label = format!(
                "run #{}{}",
                iteration,
                if is_continuation {
                    format!(
                        " (continuation {})",
                        continuation_state.continuation_attempt
                    )
                } else {
                    String::new()
                }
            );
            let mut xsd_retry_config = XsdRetryConfig {
                role: AgentRole::Developer,
                base_label: &base_label,
                prompt: &dev_prompt,
                logfile_prefix: &log_dir,
                runtime: &mut runtime,
                registry: ctx.registry,
                primary_agent: _agent.unwrap_or(ctx.developer_agent),
                session_info: session_info.as_ref(),
                retry_num,
                output_validator: None,
                workspace: ctx.workspace,
            };
            run_xsd_retry_with_session(&mut xsd_retry_config)?
        };

        if exit_code != 0 {
            had_error = true;
        }

        // Extract and validate the development result XML
        let log_dir_path = Path::new(&log_dir);
        let dev_content = read_last_development_output(log_dir_path, ctx.workspace);

        // Extract session info for potential retry (only if we don't have it yet)
        // This is best-effort - if extraction fails, we just won't use session continuation
        if session_info.is_none() {
            if let Some(agent_config) = ctx.registry.resolve_config(ctx.developer_agent) {
                ctx.logger.info(&format!(
                    "  [dev] Extracting session from {:?} with parser {:?}",
                    log_dir_path, agent_config.json_parser
                ));
                session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                    log_dir_path,
                    agent_config.json_parser,
                    Some(ctx.developer_agent),
                    ctx.workspace,
                );
                if let Some(ref info) = session_info {
                    ctx.logger.info(&format!(
                        "  [dev] Extracted session: agent={}, session_id={}...",
                        info.agent_name,
                        &info.session_id[..8.min(info.session_id.len())]
                    ));
                } else {
                    ctx.logger
                        .warn("  [dev] Failed to extract session info from log");
                }
            }
        }

        // Try file-based extraction first - allows agents to write XML to .agent/tmp/development_result.xml
        let xml_to_validate = extract_xml_with_file_fallback_with_workspace(
            ctx.workspace,
            Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
            &dev_content,
            extract_development_result_xml,
        )
        .unwrap_or_else(|| {
            // No XML found anywhere - assume entire log content is XML for validation
            // This allows us to get specific XSD errors to send back to the agent
            dev_content.clone()
        });

        match validate_development_result_xml(&xml_to_validate) {
            Ok(result_elements) => {
                // XSD validation passed - format and log the result
                let formatted_xml = format_xml_for_display(&xml_to_validate);

                // Archive the XML file for debugging (moves to .xml.processed)
                archive_xml_file_with_workspace(
                    ctx.workspace,
                    Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
                );

                if is_retry {
                    ctx.logger
                        .success(&format!("Status validated after {} retries", retry_num));
                } else {
                    ctx.logger.success("Status extracted and validated (XML)");
                }

                ctx.logger.info(&format!("\n{}", formatted_xml));

                let files_changed = result_elements
                    .files_changed
                    .as_ref()
                    .map(|f| f.lines().map(|s| s.to_string()).collect());

                let status = if result_elements.is_completed() {
                    DevelopmentStatus::Completed
                } else if result_elements.is_partial() {
                    DevelopmentStatus::Partial
                } else {
                    DevelopmentStatus::Failed
                };

                return Ok(DevAttemptResult {
                    had_error,
                    output_valid: true,
                    status,
                    summary: result_elements.summary.clone(),
                    files_changed,
                    next_steps: result_elements.next_steps.clone(),
                });
            }
            Err(xsd_err) => {
                let error_msg = format_xsd_error(&xsd_err);
                ctx.logger
                    .warn(&format!("  XSD validation failed: {}", error_msg));

                if retry_num < max_xsd_retries - 1 {
                    xsd_error = Some(error_msg);
                    continue;
                }

                ctx.logger.warn(&format!(
                    "  XSD retries exhausted ({}/{}). Will attempt fresh continuation.",
                    retry_num + 1,
                    max_xsd_retries
                ));
                break;
            }
        }
    }

    Ok(DevAttemptResult {
        had_error,
        output_valid: false,
        status: DevelopmentStatus::Failed,
        summary: "XML output failed validation. Your previous (invalid) output is at .agent/tmp/last_output.xml for reference.".to_string(),
        files_changed: None,
        next_steps: Some(
            "Complete the task and provide valid XML output conforming to the XSD schema."
                .to_string(),
        ),
    })
}

/// Run a single development iteration with XML extraction and XSD validation retry loop.
///
/// This function implements a nested loop structure:
/// - **Outer loop (continuation)**: Continue while status != "completed" (max configurable)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback
///   (max `MAX_DEV_VALIDATION_RETRY_ATTEMPTS`, currently 10)
///
/// The continuation logic ignores non-XSD errors and only looks for valid XML.
/// If XML passes XSD validation with status="completed", we're done for this iteration.
/// If XML passes XSD validation with status="partial", we continue the outer loop.
/// If XML passes XSD validation with status="failed", we continue the outer loop.
///
/// The development iteration produces side effects (file changes) as its primary output.
/// The XML status is secondary - we use it for logging/tracking but don't fail the
/// entire iteration if XML is missing or invalid.
///
/// # Arguments
///
/// * `ctx` - Phase context with access to workspace, logger, and configuration
/// * `iteration` - Current iteration number
/// * `_developer_context` - Context level (deprecated, unused)
/// * `_resuming_into_development` - Whether resuming into development phase
/// * `_resume_context` - Optional resume context from checkpoint
/// * `_agent` - Optional agent override
/// * `continuation_config` - Configuration for continuation-aware prompting
pub fn run_development_iteration_with_xml_retry(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    _developer_context: ContextLevel,
    _resuming_into_development: bool,
    _resume_context: Option<&ResumeContext>,
    _agent: Option<&str>,
    continuation_config: ContinuationConfig<'_>,
) -> anyhow::Result<DevIterationResult> {
    let max_xsd_retries = crate::reducer::state::MAX_DEV_VALIDATION_RETRY_ATTEMPTS as usize;
    let max_total_attempts = continuation_config.max_attempts;
    let max_continuations = max_total_attempts.saturating_sub(1);

    // Track local continuation state (starts from the provided state)
    let mut local_continuation = continuation_config.state.clone();
    let mut had_any_error = false;
    let mut last_summary: Option<String> = None;

    // Outer loop: Continue until agent returns status="completed" or we hit the limit.
    // The loop count is total valid attempts (initial + continuations).
    for _ in 0..max_total_attempts {
        if local_continuation.is_continuation() {
            ctx.logger.info(&format!(
                "Continuation {} of {} (status was not 'completed')",
                local_continuation.continuation_attempt, max_continuations
            ));
        }

        let attempt = run_development_attempt_with_xml_retry(
            ctx,
            iteration,
            _developer_context,
            _resuming_into_development,
            _resume_context,
            _agent,
            &local_continuation,
        )?;

        had_any_error |= attempt.had_error;
        last_summary = Some(attempt.summary.clone());

        if attempt.output_valid && matches!(attempt.status, DevelopmentStatus::Completed) {
            return Ok(DevIterationResult {
                had_error: had_any_error,
                summary: Some(attempt.summary),
                files_changed: attempt.files_changed,
            });
        }

        // Trigger a fresh continuation attempt (outer loop will continue).
        // This treats "couldn't parse response" as equivalent to "failed" status.
        local_continuation = local_continuation.trigger_continuation(
            attempt.status,
            attempt.summary,
            attempt.files_changed,
            attempt.next_steps,
        );

        // Persist continuation context for resumability through checkpoints.
        // This file is referenced by the continuation prompt template.
        let _ = write_continuation_context_file(ctx, iteration, &local_continuation);
    }

    // If we get here, we exhausted the continuation limit without ever reaching
    // status="completed". This is an explicit failure signal: proceeding would
    // silently allow the pipeline to continue despite incomplete work.
    let summary = last_summary.unwrap_or_else(|| {
        format!(
            "Continuation stopped after {} attempts",
            max_total_attempts * max_xsd_retries
        )
    });
    anyhow::bail!(
        "Development iteration did not reach status='completed' after {} total valid attempts (max_continuations={}, max_xsd_retries={} per attempt). Last summary: {}",
        max_total_attempts,
        max_continuations,
        max_xsd_retries,
        summary
    );
}

fn cleanup_continuation_context_file(ctx: &mut PhaseContext<'_>) -> anyhow::Result<()> {
    let path = Path::new(CONTINUATION_CONTEXT_PATH);
    if ctx.workspace.exists(path) {
        ctx.workspace.remove(path)?;
    }
    Ok(())
}

fn write_continuation_context_file(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    continuation_state: &ContinuationState,
) -> anyhow::Result<()> {
    let tmp_dir = Path::new(".agent/tmp");
    if !ctx.workspace.exists(tmp_dir) {
        ctx.workspace.create_dir_all(tmp_dir)?;
    }

    let mut content = String::new();
    content.push_str("# Development Continuation Context\n\n");
    content.push_str(&format!("- Iteration: {iteration}\n"));
    content.push_str(&format!(
        "- Continuation attempt: {}\n",
        continuation_state.continuation_attempt
    ));
    if let Some(ref status) = continuation_state.previous_status {
        content.push_str(&format!("- Previous status: {status}\n\n"));
    } else {
        content.push_str("- Previous status: unknown\n\n");
    }

    content.push_str("## Previous summary\n\n");
    if let Some(ref summary) = continuation_state.previous_summary {
        content.push_str(summary);
    }
    content.push('\n');

    if let Some(ref files) = continuation_state.previous_files_changed {
        content.push_str("\n## Files changed\n\n");
        for file in files {
            content.push_str("- ");
            content.push_str(file);
            content.push('\n');
        }
    }

    if let Some(ref next_steps) = continuation_state.previous_next_steps {
        content.push_str("\n## Recommended next steps\n\n");
        content.push_str(next_steps);
        content.push('\n');
    }

    content.push_str("\n## Reference files (do not modify)\n\n");
    content.push_str("- PROMPT.md\n");
    content.push_str("- .agent/PLAN.md\n");

    ctx.workspace
        .write(Path::new(CONTINUATION_CONTEXT_PATH), &content)?;

    Ok(())
}

fn load_continuation_state_from_context_file(
    workspace: &dyn crate::workspace::Workspace,
) -> Option<ContinuationState> {
    let path = Path::new(CONTINUATION_CONTEXT_PATH);
    if !workspace.exists(path) {
        return None;
    }
    let content = workspace.read(path).ok()?;
    parse_continuation_context_markdown(&content)
}

fn parse_continuation_context_markdown(content: &str) -> Option<ContinuationState> {
    let mut continuation_attempt: Option<u32> = None;
    let mut previous_status: Option<DevelopmentStatus> = None;
    let mut previous_summary_lines: Vec<String> = Vec::new();
    let mut previous_next_steps_lines: Vec<String> = Vec::new();
    let mut previous_files_changed: Vec<String> = Vec::new();

    enum Section {
        None,
        PreviousSummary,
        FilesChanged,
        NextSteps,
    }

    let mut section = Section::None;

    for line in content.lines() {
        let line = line.trim_end();

        if let Some(rest) = line.strip_prefix("- Continuation attempt:") {
            continuation_attempt = rest.trim().parse::<u32>().ok();
            continue;
        }
        if let Some(rest) = line.strip_prefix("- Previous status:") {
            let s = rest.trim().to_ascii_lowercase();
            previous_status = match s.as_str() {
                "completed" => Some(DevelopmentStatus::Completed),
                "partial" => Some(DevelopmentStatus::Partial),
                "failed" => Some(DevelopmentStatus::Failed),
                _ => None,
            };
            continue;
        }

        if line == "## Previous summary" {
            section = Section::PreviousSummary;
            continue;
        }
        if line == "## Files changed" {
            section = Section::FilesChanged;
            continue;
        }
        if line == "## Recommended next steps" {
            section = Section::NextSteps;
            continue;
        }
        if line.starts_with("## ") {
            section = Section::None;
            continue;
        }

        match section {
            Section::PreviousSummary => previous_summary_lines.push(line.to_string()),
            Section::FilesChanged => {
                if let Some(item) = line.strip_prefix("- ") {
                    if !item.trim().is_empty() {
                        previous_files_changed.push(item.trim().to_string());
                    }
                }
            }
            Section::NextSteps => previous_next_steps_lines.push(line.to_string()),
            Section::None => {}
        }
    }

    let continuation_attempt = continuation_attempt?;
    let previous_summary = previous_summary_lines.join("\n").trim().to_string();
    let previous_next_steps = previous_next_steps_lines.join("\n").trim().to_string();

    Some(ContinuationState {
        previous_status,
        previous_summary: if previous_summary.is_empty() {
            None
        } else {
            Some(previous_summary)
        },
        previous_files_changed: if previous_files_changed.is_empty() {
            None
        } else {
            Some(previous_files_changed)
        },
        previous_next_steps: if previous_next_steps.is_empty() {
            None
        } else {
            Some(previous_next_steps)
        },
        continuation_attempt,
    })
}

/// Run the planning step to create PLAN.md.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent XML output.
/// Uses XSD validation with retry loop to ensure valid XML format.
pub fn run_planning_step(ctx: &mut PhaseContext<'_>, iteration: u32) -> anyhow::Result<()> {
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
            .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
            .with_execution_history(ctx.execution_history.clone())
            .with_prompt_history(ctx.clone_prompt_history());

        if let Some(checkpoint) = builder.build() {
            let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
        }
    }

    ctx.logger.info("Creating plan from PROMPT.md...");
    update_status_with_workspace(
        ctx.workspace,
        "Starting planning phase",
        ctx.config.isolation_mode,
    )?;

    // Read PROMPT.md content to include directly in the planning prompt
    // This prevents agents from discovering PROMPT.md through file exploration,
    // which reduces the risk of accidental deletion.
    let prompt_md_content = ctx.workspace.read(Path::new("PROMPT.md")).ok();

    // Note: We don't set is_resume for planning since planning runs on each iteration.
    // The resume context is set during the development execution step.
    let prompt_key = format!("planning_{}", iteration);
    let prompt_md_str = prompt_md_content.as_deref().unwrap_or("");

    // Use prompt replay if available, otherwise generate new prompt
    let (plan_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            prompt_planning_xml_with_context(
                ctx.template_context,
                Some(prompt_md_str),
                ctx.workspace,
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
    let plan_path = Path::new(".agent/PLAN.md");

    // Ensure .agent directory exists
    if let Some(parent) = plan_path.parent() {
        ctx.workspace.create_dir_all(parent)?;
    }

    // In-session retry loop with XSD validation feedback
    // Session continuation allows the AI to retain memory between XSD retries
    let max_retries = crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS as usize;
    let mut xsd_error: Option<String> = None;
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    for retry_num in 0..max_retries {
        // Before each retry, check if the XML file is writable and clean up if locked
        if retry_num > 0 {
            use crate::files::io::check_and_cleanup_xml_before_retry_with_workspace;
            let xml_path = Path::new(crate::files::llm_output_extraction::xml_paths::PLAN_XML);
            let _ = check_and_cleanup_xml_before_retry_with_workspace(
                ctx.workspace,
                xml_path,
                ctx.logger,
            );
        }

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

            // Read the last output for retry context (used as fallback if session continuation fails)
            let last_output = read_last_planning_output(Path::new(&log_dir), ctx.workspace);

            prompt_planning_xsd_retry_with_context(
                ctx.template_context,
                prompt_md_str,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
                ctx.workspace,
            )
        };

        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            executor: ctx.executor,
            executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
            workspace: ctx.workspace,
        };

        // Use session continuation for XSD retries (retry_num > 0)
        // This is completely fault-tolerant - if session continuation fails for any reason
        // (including agent crash, segfault, invalid session), it falls back to normal behavior
        let mut xsd_retry_config = XsdRetryConfig {
            role: AgentRole::Developer,
            base_label: &format!("planning #{}", iteration),
            prompt: &plan_prompt,
            logfile_prefix: &log_dir,
            runtime: &mut runtime,
            registry: ctx.registry,
            primary_agent: ctx.developer_agent,
            session_info: session_info.as_ref(),
            retry_num,
            output_validator: None,
            workspace: ctx.workspace,
        };

        let _exit_code = run_xsd_retry_with_session(&mut xsd_retry_config)?;

        // Extract and validate the plan XML
        let log_dir_path = Path::new(&log_dir);
        let plan_content = read_last_planning_output(log_dir_path, ctx.workspace);

        // Extract session info for potential retry (only if we don't have it yet)
        // This is best-effort - if extraction fails, we just won't use session continuation
        if session_info.is_none() {
            if let Some(agent_config) = ctx.registry.resolve_config(ctx.developer_agent) {
                session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                    log_dir_path,
                    agent_config.json_parser,
                    Some(ctx.developer_agent),
                    ctx.workspace,
                );
            }
        }

        // Try file-based extraction first - allows agents to write XML to .agent/tmp/plan.xml
        let xml_to_validate = extract_xml_with_file_fallback_with_workspace(
            ctx.workspace,
            Path::new(xml_paths::PLAN_XML),
            &plan_content,
            extract_plan_xml,
        )
        .unwrap_or_else(|| {
            // No XML found anywhere - assume entire log content is XML for validation
            // This allows us to get specific XSD errors to send back to the agent
            plan_content.clone()
        });

        // Try to validate against XSD
        match validate_plan_xml(&xml_to_validate) {
            Ok(plan_elements) => {
                // XSD validation passed - convert XML to markdown format for PLAN.md
                // Note: XML display is handled via UIEvent::XmlOutput in the effect handler
                let markdown = format_plan_as_markdown(&plan_elements);
                ctx.workspace.write(plan_path, &markdown)?;

                // Archive the XML file for debugging (moves to .xml.processed)
                archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::PLAN_XML));

                if retry_num > 0 {
                    ctx.logger
                        .success(&format!("Plan validated after {} retries", retry_num));
                } else {
                    ctx.logger.success("Plan extracted and validated (XML)");
                }

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
                    ctx.workspace.write(plan_path, placeholder)?;
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
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/planning_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/planning_1_ccs-glm_0.log`
fn read_last_planning_output(
    log_prefix: &Path,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    read_last_output_from_prefix(log_prefix, workspace)
}

/// Read the last development output from logs.
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/development_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/development_1_ccs-glm_0.log`
fn read_last_development_output(
    log_prefix: &Path,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    read_last_output_from_prefix(log_prefix, workspace)
}

/// Read the most recent log file matching a prefix pattern.
///
/// This is a shared helper for reading log output. Truncation of large prompts
/// is handled centrally in `build_agent_command` to prevent E2BIG errors.
fn read_last_output_from_prefix(
    log_prefix: &Path,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    crate::pipeline::logfile::read_most_recent_logfile(log_prefix, workspace)
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

    // Summary section
    result.push_str("## Summary\n\n");
    result.push_str(&elements.summary.context);
    result.push_str("\n\n");

    // Scope items
    result.push_str("### Scope\n\n");
    for item in &elements.summary.scope_items {
        if let Some(ref count) = item.count {
            result.push_str(&format!("- **{}** {}", count, item.description));
        } else {
            result.push_str(&format!("- {}", item.description));
        }
        if let Some(ref category) = item.category {
            result.push_str(&format!(" ({})", category));
        }
        result.push('\n');
    }
    result.push('\n');

    // Implementation steps
    result.push_str("## Implementation Steps\n\n");
    for step in &elements.steps {
        // Step header
        let step_type_str = match step.step_type {
            crate::files::llm_output_extraction::xsd_validation_plan::StepType::FileChange => {
                "file-change"
            }
            crate::files::llm_output_extraction::xsd_validation_plan::StepType::Action => "action",
            crate::files::llm_output_extraction::xsd_validation_plan::StepType::Research => {
                "research"
            }
        };
        let priority_str = step.priority.map_or(String::new(), |p| {
            format!(
                " [{}]",
                match p {
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::Critical =>
                        "critical",
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::High =>
                        "high",
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::Medium =>
                        "medium",
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::Low =>
                        "low",
                }
            )
        });

        result.push_str(&format!(
            "### Step {} ({}){}:  {}\n\n",
            step.number, step_type_str, priority_str, step.title
        ));

        // Target files
        if !step.target_files.is_empty() {
            result.push_str("**Target Files:**\n");
            for tf in &step.target_files {
                let action_str = match tf.action {
                    crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Create => {
                        "create"
                    }
                    crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Modify => {
                        "modify"
                    }
                    crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Delete => {
                        "delete"
                    }
                };
                result.push_str(&format!("- `{}` ({})\n", tf.path, action_str));
            }
            result.push('\n');
        }

        // Location
        if let Some(ref location) = step.location {
            result.push_str(&format!("**Location:** {}\n\n", location));
        }

        // Rationale
        if let Some(ref rationale) = step.rationale {
            result.push_str(&format!("**Rationale:** {}\n\n", rationale));
        }

        // Content
        result.push_str(&format_rich_content(&step.content));
        result.push('\n');

        // Dependencies
        if !step.depends_on.is_empty() {
            result.push_str("**Depends on:** ");
            let deps: Vec<String> = step
                .depends_on
                .iter()
                .map(|d| format!("Step {}", d))
                .collect();
            result.push_str(&deps.join(", "));
            result.push_str("\n\n");
        }
    }

    // Critical files
    result.push_str("## Critical Files\n\n");
    result.push_str("### Primary Files\n\n");
    for pf in &elements.critical_files.primary_files {
        let action_str = match pf.action {
            crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Create => {
                "create"
            }
            crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Modify => {
                "modify"
            }
            crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Delete => {
                "delete"
            }
        };
        if let Some(ref est) = pf.estimated_changes {
            result.push_str(&format!("- `{}` ({}) - {}\n", pf.path, action_str, est));
        } else {
            result.push_str(&format!("- `{}` ({})\n", pf.path, action_str));
        }
    }
    result.push('\n');

    if !elements.critical_files.reference_files.is_empty() {
        result.push_str("### Reference Files\n\n");
        for rf in &elements.critical_files.reference_files {
            result.push_str(&format!("- `{}` - {}\n", rf.path, rf.purpose));
        }
        result.push('\n');
    }

    // Risks and mitigations
    result.push_str("## Risks & Mitigations\n\n");
    for rp in &elements.risks_mitigations {
        let severity_str = rp.severity.map_or(String::new(), |s| {
            format!(
                " [{}]",
                match s {
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::Low =>
                        "low",
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::Medium =>
                        "medium",
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::High =>
                        "high",
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::Critical =>
                        "critical",
                }
            )
        });
        result.push_str(&format!("**Risk{}:** {}\n", severity_str, rp.risk));
        result.push_str(&format!("**Mitigation:** {}\n\n", rp.mitigation));
    }

    // Verification strategy
    result.push_str("## Verification Strategy\n\n");
    for (i, v) in elements.verification_strategy.iter().enumerate() {
        result.push_str(&format!("{}. **{}**\n", i + 1, v.method));
        result.push_str(&format!("   Expected: {}\n\n", v.expected_outcome));
    }

    result
}

/// Format rich content elements to markdown.
fn format_rich_content(
    content: &crate::files::llm_output_extraction::xsd_validation_plan::RichContent,
) -> String {
    use crate::files::llm_output_extraction::xsd_validation_plan::ContentElement;

    let mut result = String::new();

    for element in &content.elements {
        match element {
            ContentElement::Paragraph(p) => {
                result.push_str(&format_inline_content(&p.content));
                result.push_str("\n\n");
            }
            ContentElement::CodeBlock(cb) => {
                let lang = cb.language.as_deref().unwrap_or("");
                result.push_str(&format!("```{}\n", lang));
                result.push_str(&cb.content);
                if !cb.content.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("```\n\n");
            }
            ContentElement::Table(t) => {
                if let Some(ref caption) = t.caption {
                    result.push_str(&format!("**{}**\n\n", caption));
                }
                // Header row
                if !t.columns.is_empty() {
                    result.push_str("| ");
                    result.push_str(&t.columns.join(" | "));
                    result.push_str(" |\n");
                    result.push('|');
                    for _ in &t.columns {
                        result.push_str(" --- |");
                    }
                    result.push('\n');
                } else if let Some(first_row) = t.rows.first() {
                    // Infer column count from first row
                    result.push('|');
                    for _ in &first_row.cells {
                        result.push_str(" --- |");
                    }
                    result.push('\n');
                }
                // Data rows
                for row in &t.rows {
                    result.push_str("| ");
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| format_inline_content(&c.content))
                        .collect();
                    result.push_str(&cells.join(" | "));
                    result.push_str(" |\n");
                }
                result.push('\n');
            }
            ContentElement::List(l) => {
                result.push_str(&format_list(l, 0));
                result.push('\n');
            }
            ContentElement::Heading(h) => {
                let prefix = "#".repeat(h.level as usize);
                result.push_str(&format!("{} {}\n\n", prefix, h.text));
            }
        }
    }

    result
}

/// Format inline content elements.
fn format_inline_content(
    content: &[crate::files::llm_output_extraction::xsd_validation_plan::InlineElement],
) -> String {
    use crate::files::llm_output_extraction::xsd_validation_plan::InlineElement;

    content
        .iter()
        .map(|e| match e {
            InlineElement::Text(s) => s.clone(),
            InlineElement::Emphasis(s) => format!("**{}**", s),
            InlineElement::Code(s) => format!("`{}`", s),
            InlineElement::Link { href, text } => format!("[{}]({})", text, href),
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Format a list element with proper indentation.
fn format_list(
    list: &crate::files::llm_output_extraction::xsd_validation_plan::List,
    indent: usize,
) -> String {
    use crate::files::llm_output_extraction::xsd_validation_plan::ListType;

    let mut result = String::new();
    let indent_str = "  ".repeat(indent);

    for (i, item) in list.items.iter().enumerate() {
        let marker = match list.list_type {
            ListType::Ordered => format!("{}. ", i + 1),
            ListType::Unordered => "- ".to_string(),
        };

        result.push_str(&indent_str);
        result.push_str(&marker);
        result.push_str(&format_inline_content(&item.content));
        result.push('\n');

        if let Some(ref nested) = item.nested_list {
            result.push_str(&format_list(nested, indent + 1));
        }
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

    let plan_ok = ctx
        .workspace
        .exists(plan_path)
        .then(|| ctx.workspace.read(plan_path).ok())
        .flatten()
        .is_some_and(|s| !s.trim().is_empty());

    // If resuming and plan is missing, re-run planning to recover
    if !plan_ok && resuming_into_development {
        ctx.logger
            .warn("Missing .agent/PLAN.md; rerunning plan generation to recover");
        run_planning_step(ctx, iteration)?;

        // Check again after rerunning - orchestrator guarantees file exists
        let plan_ok = ctx
            .workspace
            .exists(plan_path)
            .then(|| ctx.workspace.read(plan_path).ok())
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
    let args_refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
    let output = ctx.executor.execute(program, &args_refs, &[], None)?;
    let status = output.status;

    if status.success() {
        ctx.logger.success("Fast check passed");
    } else {
        ctx.logger.warn("Fast check had issues (non-blocking)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRegistry;
    use crate::checkpoint::execution_history::ExecutionHistory;
    use crate::checkpoint::RunContext;
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::workspace::MemoryWorkspace;
    use crate::workspace::Workspace;
    use std::path::{Path, PathBuf};

    struct TestFixture {
        config: Config,
        registry: AgentRegistry,
        colors: Colors,
        logger: Logger,
        timer: Timer,
        stats: Stats,
        template_context: TemplateContext,
        executor_arc: std::sync::Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: PathBuf,
        workspace: MemoryWorkspace,
    }

    impl TestFixture {
        fn new() -> Self {
            let colors = Colors { enabled: false };
            let executor = MockProcessExecutor::new();
            let executor_arc = std::sync::Arc::new(executor)
                as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
            let repo_root = PathBuf::from("/test/repo");
            let workspace = MemoryWorkspace::new(repo_root.clone());
            let registry = AgentRegistry::new().unwrap();

            Self {
                config: Config::default(),
                registry,
                colors,
                logger: Logger::new(colors),
                timer: Timer::new(),
                stats: Stats::default(),
                template_context: TemplateContext::default(),
                executor_arc,
                repo_root,
                workspace,
            }
        }
    }

    #[test]
    fn test_run_development_iteration_with_xml_retry_errors_when_continuations_exhausted_without_completion(
    ) {
        let mut fixture = TestFixture::new();
        fixture.config.max_dev_continuations = Some(1);

        fixture
            .workspace
            .write(Path::new("PROMPT.md"), "do the thing")
            .unwrap();
        fixture
            .workspace
            .write(Path::new(".agent/PLAN.md"), "plan")
            .unwrap();
        fixture
            .workspace
            .create_dir_all(Path::new(".agent/tmp"))
            .unwrap();
        fixture
            .workspace
            .write(
                Path::new(".agent/tmp/development_result.xml"),
                r#"<ralph-development-result>
<ralph-status>partial</ralph-status>
<ralph-summary>partial work</ralph-summary>
</ralph-development-result>"#,
            )
            .unwrap();

        let mut ctx = PhaseContext {
            config: &fixture.config,
            registry: &fixture.registry,
            logger: &fixture.logger,
            colors: &fixture.colors,
            timer: &mut fixture.timer,
            stats: &mut fixture.stats,
            developer_agent: "codex",
            reviewer_agent: "codex",
            review_guidelines: None,
            template_context: &fixture.template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*fixture.executor_arc,
            executor_arc: std::sync::Arc::clone(&fixture.executor_arc),
            repo_root: &fixture.repo_root,
            workspace: &fixture.workspace,
        };

        let continuation_state = ContinuationState::new();
        let continuation_config = ContinuationConfig {
            state: &continuation_state,
            // Config semantics: max_dev_continuations counts continuation attempts beyond the
            // initial attempt.
            max_attempts: 1 + fixture.config.max_dev_continuations.unwrap_or(2) as usize,
        };

        let result = run_development_iteration_with_xml_retry(
            &mut ctx,
            1,
            ContextLevel::Minimal,
            false,
            None::<&crate::checkpoint::restore::ResumeContext>,
            Some("codex"),
            continuation_config,
        );

        assert!(
            result.is_err(),
            "Expected error when continuations exhausted without status='completed'"
        );
    }
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
