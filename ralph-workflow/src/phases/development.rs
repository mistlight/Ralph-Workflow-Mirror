//! Development phase execution.
//!
//! This module handles the development phase of the Ralph pipeline, which consists
//! of iterative planning and execution cycles. Each iteration:
//! 1. Creates a PLAN.md from PROMPT.md
//! 2. Executes the plan
//! 3. Deletes PLAN.md
//! 4. Optionally runs fast checks

use crate::agents::AgentRole;
use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::checkpoint::restore::ResumeContext;
use crate::checkpoint::{save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase};
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, extract_development_result_xml, extract_plan_xml,
    extract_xml_with_file_fallback_with_workspace, validate_development_result_xml,
    validate_plan_xml, xml_paths, PlanElements,
};
use crate::files::update_status_with_workspace;
use crate::pipeline::{run_xsd_retry_with_session, PipelineRuntime, XsdRetryConfig};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_developer_iteration_continuation_xml,
    prompt_developer_iteration_xml_with_context, prompt_developer_iteration_xsd_retry_with_context,
    prompt_planning_xml_with_context, prompt_planning_xsd_retry_with_context, ContextLevel,
};
use crate::reducer::state::{ContinuationState, DevelopmentStatus};
use std::path::Path;
use std::time::Instant;

use super::context::PhaseContext;

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
    /// Whether an authentication/credential error was detected.
    /// When true, the caller should trigger agent fallback instead of retrying.
    pub auth_failure: bool,
}

/// Authentication failure during development-related phases.
#[derive(Debug, thiserror::Error)]
pub enum AuthFailureError {
    #[error("Authentication error during planning - agent fallback required")]
    Planning,
    #[error("Authentication error during development - agent fallback required")]
    Development,
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
    let active_agent = _agent.unwrap_or(ctx.developer_agent);
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
        let xsd_result = {
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
                primary_agent: active_agent,
                session_info: session_info.as_ref(),
                retry_num,
                output_validator: None,
                workspace: ctx.workspace,
            };
            run_xsd_retry_with_session(&mut xsd_retry_config)?
        };

        // Check for auth error FIRST - if detected, signal for agent fallback
        // This breaks out of the XSD retry loop immediately
        if xsd_result.auth_error_detected {
            ctx.logger
                .warn("  Auth/credential error detected, signaling agent fallback");
            return Ok(DevAttemptResult {
                had_error: true,
                output_valid: false,
                status: DevelopmentStatus::Failed,
                summary: "Authentication error - agent fallback required".to_string(),
                files_changed: None,
                next_steps: None,
                auth_failure: true,
            });
        }

        if xsd_result.exit_code != 0 {
            had_error = true;
        }

        // Extract and validate the development result XML
        let log_dir_path = Path::new(&log_dir);
        let dev_content = read_last_development_output(log_dir_path, ctx.workspace);

        // Extract session info for potential retry (only if we don't have it yet)
        // This is best-effort - if extraction fails, we just won't use session continuation
        if session_info.is_none() {
            if let Some(agent_config) = ctx.registry.resolve_config(active_agent) {
                ctx.logger.info(&format!(
                    "  [dev] Extracting session from {:?} with parser {:?}",
                    log_dir_path, agent_config.json_parser
                ));
                session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                    log_dir_path,
                    agent_config.json_parser,
                    Some(active_agent),
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
                // XSD validation passed
                // Note: User-facing XML display is handled via UIEvent::XmlOutput
                // in the reducer effect handler, not here. This avoids duplicate output.

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
                    auth_failure: false,
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
        auth_failure: false,
    })
}

/// Run the planning step to create PLAN.md with an explicit agent.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent XML output.
/// Uses XSD validation with retry loop to ensure valid XML format.
fn run_planning_step_with_agent(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    agent: &str,
) -> anyhow::Result<()> {
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
                agent,
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
            primary_agent: agent,
            session_info: session_info.as_ref(),
            retry_num,
            output_validator: None,
            workspace: ctx.workspace,
        };

        let xsd_result = run_xsd_retry_with_session(&mut xsd_retry_config)?;

        // Check for auth error FIRST - if detected, bail with an error that signals agent fallback
        if xsd_result.auth_error_detected {
            ctx.logger
                .warn("  Auth/credential error detected during planning, signaling agent fallback");
            return Err(AuthFailureError::Planning.into());
        }

        // Extract and validate the plan XML
        let log_dir_path = Path::new(&log_dir);
        let plan_content = read_last_planning_output(log_dir_path, ctx.workspace);

        // Extract session info for potential retry (only if we don't have it yet)
        // This is best-effort - if extraction fails, we just won't use session continuation
        if session_info.is_none() {
            if let Some(agent_config) = ctx.registry.resolve_config(agent) {
                session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                    log_dir_path,
                    agent_config.json_parser,
                    Some(agent),
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
                    .with_agent(agent)
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
        .with_agent(agent)
        .with_duration(duration);
        ctx.execution_history.add_step(step);
    }

    anyhow::bail!("Planning failed after {} XSD retry attempts", max_retries)
}

/// Run the planning step to create PLAN.md.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent XML output.
/// Uses XSD validation with retry loop to ensure valid XML format.
pub fn run_planning_step(ctx: &mut PhaseContext<'_>, iteration: u32) -> anyhow::Result<()> {
    run_planning_step_with_agent(ctx, iteration, ctx.developer_agent)
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
