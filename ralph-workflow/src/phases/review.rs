//! Review phase execution.
//!
//! This module handles the review and fix phase of the Ralph pipeline. It runs
//! a configurable number of review-fix cycles, where each cycle:
//! 1. Reviews the code and creates ISSUES.md
//! 2. Fixes the issues found
//! 3. Cleans up ISSUES.md (in isolation mode)
//!
//! # Module Structure
//!
//! - `validation` - Pre-flight and post-flight validation checks

/// Maximum continuation attempts for fix passes to prevent infinite loops.
///
/// This is a safety limit for the outer loop that continues while
/// status != "all_issues_addressed". The fix agent should complete
/// well before reaching this limit under normal circumstances.
const MAX_CONTINUATION_ATTEMPTS: usize = 100;

use crate::agents::AgentRole;
use crate::checkpoint::restore::ResumeContext;
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, extract_fix_result_xml,
    extract_xml_with_file_fallback_with_workspace, try_extract_from_file_with_workspace,
    validate_fix_result_xml, validate_issues_xml, xml_paths, IssuesElements,
};
use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::files::{delete_issues_file_for_isolation_with_workspace, update_status_with_workspace};
use crate::pipeline::{run_xsd_retry_with_session, PipelineRuntime, XsdRetryConfig};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_fix_xml_with_context, prompt_fix_xsd_retry_with_context,
    prompt_review_xml_with_references, prompt_review_xsd_retry_with_context, ContextLevel,
    PromptContentBuilder,
};
use std::path::Path;

mod validation;
pub use validation::{
    post_flight_review_check, pre_flight_review_check, PostflightResult, PreflightResult,
};

use super::context::PhaseContext;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};

use std::time::Instant;

/// Result of running a review pass.
#[derive(Debug)]
pub struct ReviewPassResult {
    /// Whether the review found no issues and should exit early.
    pub early_exit: bool,
    /// Whether an authentication/credential error was detected.
    /// When true, the caller should trigger agent fallback instead of retrying.
    pub auth_failure: bool,
}

/// Result of parsing review output.
#[derive(Debug)]
enum ParseResult {
    /// Successfully parsed with issues found
    IssuesFound { issues: Vec<String> },
    /// Successfully parsed with explicit "no issues" declaration
    NoIssuesExplicit,
    /// Failed to parse - includes error description for re-prompting
    ParseFailed(String),
}

/// Run the review pass for a single cycle.
///
/// This function implements a nested loop structure similar to fix:
/// - **Outer loop (continuation)**: Not used for review (single pass)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback (max 100)
pub fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    _review_prompt: &str, // Unused - we build XML prompt internally
    _agent: Option<&str>,
) -> anyhow::Result<ReviewPassResult> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let issues_path = Path::new(".agent/ISSUES.md");
    let max_xsd_retries = crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS as usize;

    // Read PROMPT.md, PLAN.md for context
    let prompt_content = ctx
        .workspace
        .read(Path::new("PROMPT.md"))
        .unwrap_or_default();
    let plan_content = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();

    // Get the diff for review context.
    // IMPORTANT: This must be the diff from the review baseline (or start_commit for the first
    // cycle) to the current state on disk. It may or may not correspond to the last commit.
    let (changes_content, baseline_oid_for_prompts) =
        match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
            Ok((diff, baseline_oid)) => (diff, baseline_oid),
            Err(e) => {
                ctx.logger
                    .warn(&format!("Failed to get baseline diff for review: {e}"));
                (String::new(), String::new())
            }
        };

    // Session info for potential session continuation on XSD retries
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    // Track previous log directory for reading errors and output on retries
    let mut prev_log_dir: Option<String> = None;

    // Inner loop: XSD validation retry with error feedback
    for retry_num in 0..max_xsd_retries {
        let is_retry = retry_num > 0;
        let log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_{retry_num}");

        // Before each retry, check if the XML file is writable and clean up if locked
        if is_retry {
            use crate::files::io::check_and_cleanup_xml_before_retry_with_workspace;
            use std::path::Path;
            let xml_path = Path::new(crate::files::llm_output_extraction::xml_paths::ISSUES_XML);
            let _ = check_and_cleanup_xml_before_retry_with_workspace(
                ctx.workspace,
                xml_path,
                ctx.logger,
            );
        }

        // For initial attempt, use XML prompt
        // For retries, use XSD retry prompt with error feedback
        let review_prompt_xml = if !is_retry {
            // First attempt - use initial XML prompt
            let prompt_key = format!("review_{}", j);
            let (prompt, was_replayed) =
                get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                    let refs = PromptContentBuilder::new(ctx.workspace)
                        .with_plan(plan_content.clone())
                        .with_diff(changes_content.clone(), &baseline_oid_for_prompts)
                        .build();

                    prompt_review_xml_with_references(ctx.template_context, &refs)
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
            // XSD retry - use retry prompt with error feedback
            ctx.logger.info(&format!(
                "  In-session retry {}/{} for XSD validation",
                retry_num,
                max_xsd_retries - 1
            ));

            // Read from PREVIOUS attempt's directory (the one that just failed)
            // prev_log_dir should be Some because is_retry means retry_num > 0
            let prev_dir = prev_log_dir.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Internal error: prev_log_dir missing on retry (iteration {})",
                    retry_num
                )
            })?;
            let last_output = read_last_review_output(Path::new(prev_dir), ctx.workspace);

            // Get XSD error from previous iteration
            let xsd_error = get_last_xsd_error(ctx, Path::new(prev_dir));

            if let Some(ref error) = xsd_error {
                ctx.logger.info(&format!("  XSD error: {}", error));
            }

            prompt_review_xsd_retry_with_context(
                ctx.template_context,
                &prompt_content,
                &plan_content,
                &changes_content,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
                ctx.workspace,
            )
        };

        // Log the review prompt details for debugging (when verbose)
        if ctx.config.verbosity.is_debug() && !is_retry {
            ctx.logger.info(&format!(
                "Review prompt length: {} characters",
                review_prompt_xml.len()
            ));
        }

        let attempt_start = Instant::now();

        // Run the agent with session continuation for XSD retries
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

            // Output validator: checks if reviewer produced valid output
            // Required: File-based XML at .agent/tmp/issues.xml
            let validate_output: crate::pipeline::OutputValidator =
                |ws: &dyn crate::workspace::Workspace,
                 log_dir_path: &Path,
                 _logger: &crate::logger::Logger|
                 -> std::io::Result<bool> {
                    use crate::files::llm_output_extraction::{has_valid_xml_output, xml_paths};

                    // First, check if XML file was written directly (file-based mode)
                    if has_valid_xml_output(ws, Path::new(xml_paths::ISSUES_XML)) {
                        return Ok(true); // Valid XML file exists
                    }

                    let _ = log_dir_path;
                    Ok(false)
                };

            let base_label = format!(
                "{review_label} #{j}{}",
                if is_retry {
                    format!(" (retry {retry_num})")
                } else {
                    String::new()
                }
            );

            let mut xsd_retry_config = XsdRetryConfig {
                role: AgentRole::Reviewer,
                base_label: &base_label,
                prompt: &review_prompt_xml,
                logfile_prefix: &log_dir,
                runtime: &mut runtime,
                registry: ctx.registry,
                primary_agent: active_agent,
                session_info: session_info.as_ref(),
                retry_num,
                output_validator: Some(validate_output),
                workspace: ctx.workspace,
            };
            run_xsd_retry_with_session(&mut xsd_retry_config)?
        };
        ctx.stats.reviewer_runs_completed += 1;

        // Check for auth error FIRST - if detected, signal for agent fallback
        // This breaks out of the XSD retry loop immediately
        if xsd_result.auth_error_detected {
            ctx.logger
                .warn("  Auth/credential error detected during review, signaling agent fallback");
            return Ok(ReviewPassResult {
                early_exit: false,
                auth_failure: true,
            });
        }

        // Extract session info for potential retry (only if we don't have it yet)
        // IMPORTANT: Always extract from attempt 0's log directory, as that's where the
        // initial session was created. Subsequent retries use continuation with the same session.
        if session_info.is_none() {
            let first_attempt_log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_0");
            let log_dir_path = Path::new(&first_attempt_log_dir);
            if let Some(agent_config) = ctx.registry.resolve_config(active_agent) {
                session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                    log_dir_path,
                    agent_config.json_parser,
                    Some(active_agent),
                    ctx.workspace,
                );
            }
        }

        let attempt_duration = attempt_start.elapsed().as_secs();

        // Extract and validate the review output using XML extraction
        let parse_result = extract_and_validate_review_output_xml(ctx, &log_dir, issues_path)?;

        match parse_result {
            ParseResult::IssuesFound { issues } => {
                // POST-FLIGHT VALIDATION: Check review output after agent completes
                handle_postflight_validation(ctx, j);

                ctx.logger
                    .success(&format!("Issues extracted: {} total", issues.len()));

                // Note: XML display is handled via UIEvent::XmlOutput in the effect handler

                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::success(
                        Some(format!("{} issues found", issues.len())),
                        vec![".agent/ISSUES.md".to_string()],
                    ),
                )
                .with_agent(active_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                return Ok(ReviewPassResult {
                    early_exit: false,
                    auth_failure: false,
                });
            }
            ParseResult::NoIssuesExplicit => {
                ctx.logger
                    .success(&format!("No issues found after cycle {j} - stopping early"));
                // Clean up ISSUES.md before early exit in isolation mode
                if ctx.config.isolation_mode {
                    delete_issues_file_for_isolation_with_workspace(ctx.workspace, ctx.logger)?;
                }

                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::success(Some("No issues found".to_string()), vec![]),
                )
                .with_agent(active_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                return Ok(ReviewPassResult {
                    early_exit: true,
                    auth_failure: false,
                });
            }
            ParseResult::ParseFailed(error_description) => {
                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::failure(
                        format!("XSD validation failed: {error_description}"),
                        true,
                    ),
                )
                .with_agent(active_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                // Store XSD error for next retry
                store_xsd_error_for_retry(ctx, Path::new(&log_dir), &error_description);

                // Last retry failed - write marker and continue
                if retry_num >= max_xsd_retries - 1 {
                    ctx.logger.error(&format!(
                        "Failed to get valid XML review output after {} XSD retries. Last error: {}",
                        max_xsd_retries, error_description
                    ));
                    // Write a marker file indicating the failure
                    let failure_marker = format!(
                        "# Review Output XSD Validation Failure\n\n\
                        The reviewer agent's output failed XSD validation after {} attempts.\n\n\
                        Last validation error: {}\n\n\
                        This does NOT mean there are no issues - it means the XML format was invalid.\n\n\
                        Please check the logs in .agent/logs/ for the raw reviewer output.\n",
                        max_xsd_retries, error_description
                    );
                    ctx.workspace.write(issues_path, &failure_marker)?;
                    // Continue with fix pass anyway - the fix agent will see the failure message
                    return Ok(ReviewPassResult {
                        early_exit: false,
                        auth_failure: false,
                    });
                }

                ctx.logger.warn(&format!(
                    "Review XSD validation failed (attempt {}/{}): {}",
                    retry_num + 1,
                    max_xsd_retries,
                    error_description
                ));
                // Continue to next retry with XSD error feedback
            }
        }

        // Update previous log directory for next iteration
        // This allows the next retry to read from this attempt's directory
        prev_log_dir = Some(log_dir);
    }

    // Should not reach here, but handle the case
    Ok(ReviewPassResult {
        early_exit: false,
        auth_failure: false,
    })
}

/// Extract review output using XML extraction and validate with XSD.
///
/// Returns a `ParseResult` indicating whether the output was successfully parsed,
/// explicitly declared no issues, or failed to parse (with an error description).
///
/// # Extraction Priority
///
/// 1. File-based XML at `.agent/tmp/issues.xml` (required)
///
/// Legacy log extraction and ISSUES.md fallback have been removed. Agents must
/// produce XML output via the reducer/effect path.
fn extract_and_validate_review_output_xml(
    ctx: &mut PhaseContext<'_>,
    log_dir: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    // Priority 1: Check for file-based XML at .agent/tmp/issues.xml
    // This is the preferred path for agents that write XML directly (e.g., opencode parser)
    if let Some(xml_content) =
        try_extract_from_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML))
    {
        ctx.logger
            .info("Found XML in .agent/tmp/issues.xml (file-based mode)");
        return validate_and_process_issues_xml(ctx, &xml_content, issues_path);
    }

    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Review output missing at .agent/tmp/issues.xml; expected log prefix: {log_dir}"
        ));
    }

    // Legacy JSON log extraction removed - fail with clear error
    Ok(ParseResult::ParseFailed(
        "No review output captured. Agent did not write to .agent/tmp/issues.xml. \
         Ensure the agent produces valid XML output via the configured effects."
            .to_string(),
    ))
}

/// Helper to validate XML and process the result for issues extraction.
fn validate_and_process_issues_xml(
    ctx: &mut PhaseContext<'_>,
    xml_content: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    // Validate the extracted XML against XSD
    let validated: Result<IssuesElements, XsdValidationError> = validate_issues_xml(xml_content);

    match validated {
        Ok(elements) => {
            // Write the validated XML to ISSUES.md
            ctx.workspace.write(issues_path, xml_content)?;

            // Archive the XML file for debugging (moves to .xml.processed)
            archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML));

            if elements.no_issues_found.is_some() {
                return Ok(ParseResult::NoIssuesExplicit);
            }

            if !elements.issues.is_empty() {
                return Ok(ParseResult::IssuesFound {
                    issues: elements.issues,
                });
            }

            Ok(ParseResult::ParseFailed(
                "XML validated but contains no issues or no-issues-found element.".to_string(),
            ))
        }
        Err(xsd_error) => {
            // Return the specific XSD error for retry
            Ok(ParseResult::ParseFailed(xsd_error.format_for_ai_retry()))
        }
    }
}

/// Read the last review output from logs.
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/reviewer_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/reviewer_1_ccs-glm_0.log`
fn read_last_review_output(
    log_prefix: &Path,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    read_last_output_from_prefix(log_prefix, workspace)
}

/// Get the last XSD error from the log directory for retry feedback.
fn get_last_xsd_error(ctx: &PhaseContext<'_>, log_dir: &Path) -> Option<String> {
    let error_file = log_dir.join("xsd_error.txt");
    if let Ok(content) = ctx.workspace.read(&error_file) {
        if !content.trim().is_empty() {
            return Some(content);
        }
    }
    None
}

/// Store XSD error for the next retry attempt.
fn store_xsd_error_for_retry(ctx: &PhaseContext<'_>, log_dir: &Path, error: &str) {
    let error_file = log_dir.join("xsd_error.txt");
    let _ = ctx.workspace.write(&error_file, error);
}

/// Handle post-flight validation after a review pass.
fn handle_postflight_validation(ctx: &PhaseContext<'_>, j: u32) {
    let postflight_result = post_flight_review_check(ctx.workspace, ctx.logger, j);
    match postflight_result {
        PostflightResult::Valid => {
            // ISSUES.md found and valid, continue
        }
        PostflightResult::Missing(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. Proceeding with fix pass anyway."
            ));
        }
        PostflightResult::Malformed(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. The fix pass may not work correctly."
            ));
            ctx.logger.info(&format!(
                "{}Tip:{} Try with generic parser: {}RALPH_REVIEWER_JSON_PARSER=generic ralph{}",
                ctx.colors.bold(),
                ctx.colors.reset(),
                ctx.colors.bold(),
                ctx.colors.reset()
            ));
        }
    }
}

/// Read the last fix output from logs.
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/fix_1_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/fix_1_1_ccs-glm_0.log`
fn read_last_fix_output(log_prefix: &Path, workspace: &dyn crate::workspace::Workspace) -> String {
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

/// Format XSD error for display (for fix result).
fn format_xsd_error_for_fix(error: &XsdValidationError) -> String {
    format!(
        "{} - expected: {}, found: {}",
        error.element_path, error.expected, error.found
    )
}

/// Run the fix pass for a single cycle.
///
/// This function implements a nested loop structure similar to development:
/// - **Outer loop (continuation)**: Continue while status != "all_issues_addressed" (max 100)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback (max 100)
pub fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    _reviewer_context: ContextLevel,
    _resume_context: Option<&ResumeContext>,
    _agent: Option<&str>,
) -> anyhow::Result<()> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let fix_start_time = Instant::now();

    update_status_with_workspace(ctx.workspace, "Applying fixes", ctx.config.isolation_mode)?;

    // Read PROMPT.md, PLAN.md, and ISSUES.md for context
    let prompt_content = ctx
        .workspace
        .read(Path::new("PROMPT.md"))
        .unwrap_or_default();
    let plan_content = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();
    let issues_content = ctx
        .workspace
        .read(Path::new(".agent/ISSUES.md"))
        .unwrap_or_default();

    // Extract file paths from issues for the fix prompt
    let files_to_modify = extract_file_paths_from_issues(&issues_content);

    let log_dir = format!(".agent/logs/reviewer_fix_{j}");

    let max_xsd_retries = crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS as usize;
    let max_continuations = MAX_CONTINUATION_ATTEMPTS;
    let mut _had_any_error = false; // Tracked for potential future use

    // Session info for potential session continuation on XSD retries
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    // Outer loop: Continue until agent returns status="all_issues_addressed" or "no_issues_found"
    'continuation: for continuation_num in 0..max_continuations {
        let is_continuation = continuation_num > 0;
        if is_continuation {
            ctx.logger.info(&format!(
                "Fix continuation {} of {} (status was not complete)",
                continuation_num, max_continuations
            ));
        }

        let mut xsd_error: Option<String> = None;

        // Inner loop: XSD validation retry with error feedback
        for retry_num in 0..max_xsd_retries {
            let is_retry = retry_num > 0;
            let total_attempts = continuation_num * max_xsd_retries + retry_num + 1;

            // Before each retry, check if the XML file is writable and clean up if locked
            if is_retry {
                use crate::files::io::check_and_cleanup_xml_before_retry_with_workspace;
                use std::path::Path;
                let xml_path =
                    Path::new(crate::files::llm_output_extraction::xml_paths::FIX_RESULT_XML);
                let _ = check_and_cleanup_xml_before_retry_with_workspace(
                    ctx.workspace,
                    xml_path,
                    ctx.logger,
                );
            }

            // For initial attempt, use XML prompt
            // For retries, use XSD retry prompt with error feedback
            let fix_prompt = if !is_retry && !is_continuation {
                // First attempt ever - use initial XML prompt
                let prompt_key = format!("fix_{}", j);
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_fix_xml_with_context(
                            ctx.template_context,
                            &prompt_content,
                            &plan_content,
                            &issues_content,
                            &files_to_modify,
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

                let last_output = read_last_fix_output(Path::new(&log_dir), ctx.workspace);

                prompt_fix_xsd_retry_with_context(
                    ctx.template_context,
                    &issues_content,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                    ctx.workspace,
                )
            } else if !is_retry {
                // Continuation only (first XSD attempt after continuation)
                ctx.logger.info(&format!(
                    "  Continuation attempt {} (XSD validation attempt {}/{})",
                    total_attempts, 1, max_xsd_retries
                ));

                prompt_fix_xml_with_context(
                    ctx.template_context,
                    &prompt_content,
                    &plan_content,
                    &issues_content,
                    &files_to_modify,
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

                let last_output = read_last_fix_output(Path::new(&log_dir), ctx.workspace);

                prompt_fix_xsd_retry_with_context(
                    ctx.template_context,
                    &issues_content,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                    ctx.workspace,
                )
            };

            // Log the fix prompt details for debugging (when verbose)
            if ctx.config.verbosity.is_debug() && !is_continuation && !is_retry {
                ctx.logger.info(&format!(
                    "Fix prompt length: {} characters",
                    fix_prompt.len()
                ));
            }

            // Run the agent with session continuation for XSD retries
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

                // Output validator: checks if fixer produced valid XML output
                // Requires file-based XML at .agent/tmp/fix_result.xml
                // JSON log extraction fallback has been removed - agents must write XML files
                let validate_output: crate::pipeline::OutputValidator =
                    |ws: &dyn crate::workspace::Workspace,
                     _log_dir_path: &Path,
                     _logger: &crate::logger::Logger|
                     -> std::io::Result<bool> {
                        use crate::files::llm_output_extraction::{
                            has_valid_xml_output, xml_paths,
                        };

                        // Check if XML file was written directly (required)
                        Ok(has_valid_xml_output(
                            ws,
                            Path::new(xml_paths::FIX_RESULT_XML),
                        ))
                    };

                let base_label = format!(
                    "fix #{}{}",
                    j,
                    if is_continuation {
                        format!(" (continuation {})", continuation_num)
                    } else {
                        String::new()
                    }
                );

                let mut xsd_retry_config = XsdRetryConfig {
                    role: AgentRole::Reviewer,
                    base_label: &base_label,
                    prompt: &fix_prompt,
                    logfile_prefix: &log_dir,
                    runtime: &mut runtime,
                    registry: ctx.registry,
                    primary_agent: active_agent,
                    session_info: session_info.as_ref(),
                    retry_num,
                    output_validator: Some(validate_output),
                    workspace: ctx.workspace,
                };
                run_xsd_retry_with_session(&mut xsd_retry_config)?
            };

            ctx.stats.reviewer_runs_completed += 1;

            // Check for auth error FIRST - if detected, bail with an error that signals agent fallback
            if xsd_result.auth_error_detected {
                ctx.logger
                    .warn("  Auth/credential error detected during fix, signaling agent fallback");
                anyhow::bail!("Authentication error during fix - agent fallback required");
            }

            // Extract session info for potential retry (only if we don't have it yet)
            let log_dir_path = Path::new(&log_dir);
            if session_info.is_none() {
                if let Some(agent_config) = ctx.registry.resolve_config(active_agent) {
                    session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                        log_dir_path,
                        agent_config.json_parser,
                        Some(active_agent),
                        ctx.workspace,
                    );
                }
            }

            // Track if any agent run had an error
            if xsd_result.exit_code != 0 {
                _had_any_error = true;
            }
            let fix_content = read_last_fix_output(log_dir_path, ctx.workspace);

            // Try file-based extraction first - allows agents to write XML to .agent/tmp/fix_result.xml
            let xml_to_validate = extract_xml_with_file_fallback_with_workspace(
                ctx.workspace,
                Path::new(xml_paths::FIX_RESULT_XML),
                &fix_content,
                extract_fix_result_xml,
            )
            .unwrap_or_else(|| {
                // No XML found anywhere - assume entire log content is XML for validation
                // This allows us to get specific XSD errors to send back to the agent
                fix_content.clone()
            });

            // Try to validate against XSD
            match validate_fix_result_xml(&xml_to_validate) {
                Ok(result_elements) => {
                    // XSD validation passed - archive the file for debugging (moves to .xml.processed)
                    // Note: XML display is handled via UIEvent::XmlOutput in the effect handler
                    archive_xml_file_with_workspace(
                        ctx.workspace,
                        Path::new(xml_paths::FIX_RESULT_XML),
                    );

                    if is_retry {
                        ctx.logger
                            .success(&format!("Fix validated after {} retries", retry_num));
                    } else {
                        ctx.logger
                            .success("Fix status extracted and validated (XML)");
                    }

                    // Check the status to determine if we should continue
                    if result_elements.is_complete() || result_elements.is_no_issues() {
                        // Status is "all_issues_addressed" or "no_issues_found" - we're done
                        let duration = fix_start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            j,
                            "fix",
                            StepOutcome::success(result_elements.summary, vec![]),
                        )
                        .with_agent(active_agent)
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);

                        return Ok(());
                    } else if result_elements.has_remaining_issues() {
                        // Status is "issues_remain" - continue the outer loop
                        ctx.logger
                            .info("Status is 'issues_remain' - continuing with same fix pass");
                        continue 'continuation;
                    }
                }
                Err(xsd_err) => {
                    // XSD validation failed - check if we can retry
                    let error_msg = format_xsd_error_for_fix(&xsd_err);
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

    // If we get here, we exhausted the continuation limit
    let duration = fix_start_time.elapsed().as_secs();
    let step = ExecutionStep::new(
        "Review",
        j,
        "fix",
        StepOutcome::failure(
            format!(
                "Continuation stopped after {} attempts",
                max_continuations * max_xsd_retries
            ),
            true,
        ),
    )
    .with_agent(active_agent)
    .with_duration(duration);
    ctx.execution_history.add_step(step);

    Ok(())
}
