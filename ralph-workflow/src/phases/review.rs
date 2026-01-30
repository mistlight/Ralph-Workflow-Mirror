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

use crate::checkpoint::restore::ResumeContext;
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, try_extract_from_file_with_workspace, validate_fix_result_xml,
    validate_issues_xml, xml_paths, IssuesElements,
};
use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::files::{delete_issues_file_for_isolation_with_workspace, update_status_with_workspace};
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_fix_xml_with_context, prompt_review_xml_with_references,
    ContextLevel, PromptContentBuilder,
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
    /// Whether the agent failed to run successfully.
    pub agent_failed: bool,
    /// Whether the review output was validated successfully.
    pub output_valid: bool,
    /// Whether issues were found in the validated output.
    pub issues_found: bool,
}

/// Result of running a fix pass.
#[derive(Debug)]
pub struct FixPassResult {
    /// Whether an authentication/credential error was detected.
    pub auth_failure: bool,
    /// Whether the agent failed to run successfully.
    pub agent_failed: bool,
    /// Whether the fix output was validated successfully.
    pub output_valid: bool,
    /// Whether changes were made according to the fix output.
    pub changes_made: bool,
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
/// This function runs a single review pass and validates the XML output.
pub fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    _review_prompt: &str, // Unused - we build XML prompt internally
    _agent: Option<&str>,
) -> anyhow::Result<ReviewPassResult> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let issues_path = Path::new(".agent/ISSUES.md");

    let plan_content = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();

    let (changes_content, baseline_oid_for_prompts) =
        match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
            Ok((diff, baseline_oid)) => (diff, baseline_oid),
            Err(e) => {
                ctx.logger
                    .warn(&format!("Failed to get baseline diff for review: {e}"));
                (String::new(), String::new())
            }
        };

    let prompt_key = format!("review_{}", j);
    let (review_prompt_xml, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            let refs = PromptContentBuilder::new(ctx.workspace)
                .with_plan(plan_content.clone())
                .with_diff(changes_content.clone(), &baseline_oid_for_prompts)
                .build();

            prompt_review_xml_with_references(ctx.template_context, &refs)
        });

    if !was_replayed {
        ctx.capture_prompt(&prompt_key, &review_prompt_xml);
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Review prompt length: {} characters",
            review_prompt_xml.len()
        ));
    }

    let log_dir = Path::new(".agent/logs");
    ctx.workspace.create_dir_all(log_dir)?;
    let logfile = format!(".agent/logs/reviewer_review_{j}.log");

    let agent_config = ctx
        .registry
        .resolve_config(active_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", active_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        executor: ctx.executor,
        executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
        workspace: ctx.workspace,
    };

    let prompt_cmd = PromptCommand {
        label: review_label,
        display_name: active_agent,
        cmd_str: &cmd_str,
        prompt: &review_prompt_xml,
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let attempt_start = Instant::now();
    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    if result.exit_code != 0 {
        let auth_failure = stderr_contains_auth_error(&result.stderr);
        return Ok(ReviewPassResult {
            early_exit: false,
            auth_failure,
            agent_failed: true,
            output_valid: false,
            issues_found: false,
        });
    }

    let log_prefix = format!(".agent/logs/reviewer_review_{j}");
    let parse_result = extract_and_validate_review_output_xml(ctx, &log_prefix, issues_path)?;

    match parse_result {
        ParseResult::IssuesFound { issues } => {
            handle_postflight_validation(ctx, j);

            ctx.logger
                .success(&format!("Issues extracted: {} total", issues.len()));

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
            .with_duration(attempt_start.elapsed().as_secs());
            ctx.execution_history.add_step(step);

            Ok(ReviewPassResult {
                early_exit: false,
                auth_failure: false,
                agent_failed: false,
                output_valid: true,
                issues_found: true,
            })
        }
        ParseResult::NoIssuesExplicit => {
            ctx.logger
                .success(&format!("No issues found after cycle {j} - stopping early"));

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
            .with_duration(attempt_start.elapsed().as_secs());
            ctx.execution_history.add_step(step);

            Ok(ReviewPassResult {
                early_exit: true,
                auth_failure: false,
                agent_failed: false,
                output_valid: true,
                issues_found: false,
            })
        }
        ParseResult::ParseFailed(reason) => {
            ctx.logger
                .warn(&format!("Review output validation failed: {reason}"));

            Ok(ReviewPassResult {
                early_exit: false,
                auth_failure: false,
                agent_failed: false,
                output_valid: false,
                issues_found: false,
            })
        }
    }
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

fn stderr_contains_auth_error(stderr: &str) -> bool {
    let combined = stderr.to_lowercase();
    combined.contains("authentication")
        || combined.contains("unauthorized")
        || combined.contains("credential")
        || combined.contains("api key")
        || combined.contains("not authorized")
}

/// Run the fix pass for a single cycle.
///
/// This function runs a single fix pass and validates the XML output.
pub fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    _reviewer_context: ContextLevel,
    _resume_context: Option<&ResumeContext>,
    _agent: Option<&str>,
) -> anyhow::Result<FixPassResult> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let fix_start_time = Instant::now();

    update_status_with_workspace(ctx.workspace, "Applying fixes", ctx.config.isolation_mode)?;

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

    let files_to_modify = extract_file_paths_from_issues(&issues_content);

    let prompt_key = format!("fix_{}", j);
    let (fix_prompt, was_replayed) =
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
        ctx.capture_prompt(&prompt_key, &fix_prompt);
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Fix prompt length: {} characters",
            fix_prompt.len()
        ));
    }

    let log_dir = Path::new(".agent/logs");
    ctx.workspace.create_dir_all(log_dir)?;
    let logfile = format!(".agent/logs/reviewer_fix_{j}.log");

    let agent_config = ctx
        .registry
        .resolve_config(active_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", active_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        executor: ctx.executor,
        executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
        workspace: ctx.workspace,
    };

    let prompt_cmd = PromptCommand {
        label: "fix",
        display_name: active_agent,
        cmd_str: &cmd_str,
        prompt: &fix_prompt,
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    if result.exit_code != 0 {
        let auth_failure = stderr_contains_auth_error(&result.stderr);
        return Ok(FixPassResult {
            auth_failure,
            agent_failed: true,
            output_valid: false,
            changes_made: false,
        });
    }

    let xml_content =
        try_extract_from_file_with_workspace(ctx.workspace, Path::new(xml_paths::FIX_RESULT_XML));

    let Some(xml_to_validate) = xml_content else {
        return Ok(FixPassResult {
            auth_failure: false,
            agent_failed: false,
            output_valid: false,
            changes_made: false,
        });
    };

    match validate_fix_result_xml(&xml_to_validate) {
        Ok(result_elements) => {
            archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::FIX_RESULT_XML));

            let changes_made = !result_elements.is_no_issues();

            let step = ExecutionStep::new(
                "Review",
                j,
                "fix",
                StepOutcome::success(result_elements.summary.clone(), vec![]),
            )
            .with_agent(active_agent)
            .with_duration(fix_start_time.elapsed().as_secs());
            ctx.execution_history.add_step(step);

            Ok(FixPassResult {
                auth_failure: false,
                agent_failed: false,
                output_valid: true,
                changes_made,
            })
        }
        Err(err) => {
            ctx.logger
                .warn(&format!("Fix XML validation failed: {err}"));
            Ok(FixPassResult {
                auth_failure: false,
                agent_failed: false,
                output_valid: false,
                changes_made: false,
            })
        }
    }
}
