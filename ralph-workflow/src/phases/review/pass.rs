use super::types::{FixPassResult, ParseResult, ReviewPassResult};
use super::validation::{post_flight_review_check, PostflightResult};
use super::xml_processing::extract_and_validate_review_output_xml;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::checkpoint::restore::ResumeContext;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, try_extract_from_file_with_workspace, validate_fix_result_xml,
    xml_paths,
};
use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::files::{delete_issues_file_for_isolation_with_workspace, update_status_with_workspace};
use crate::phases::context::PhaseContext;
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_fix_xml_with_context, prompt_review_xml_with_references,
    ContextLevel, PromptContentBuilder,
};
use anyhow::Context as _;

use std::path::Path;
use std::time::Instant;

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

            prompt_review_xml_with_references(ctx.template_context, &refs, ctx.workspace)
        });

    // Enforce that the rendered prompt does not contain unresolved template placeholders.
    // This must happen before any agent invocation.
    if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
        &review_prompt_xml,
        &[plan_content.as_str(), changes_content.as_str()],
    ) {
        return Err(crate::prompts::TemplateVariablesInvalidError {
            template_name: "review_xml".to_string(),
            missing_variables: Vec::new(),
            unresolved_placeholders: err.unresolved_placeholders,
        }
        .into());
    }

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

    // Use per-run log directory with simplified naming
    let base_log_path = ctx.run_log_context.agent_log("reviewer", j, None);
    let attempt = crate::pipeline::logfile::next_simplified_logfile_attempt_index(
        &base_log_path,
        ctx.workspace,
    );
    let logfile = if attempt == 0 {
        base_log_path.to_str().unwrap().to_string()
    } else {
        ctx.run_log_context
            .agent_log("reviewer", j, Some(attempt))
            .to_str()
            .unwrap()
            .to_string()
    };

    // Write log file header with agent metadata
    // Use append_bytes to avoid overwriting if file exists (defense-in-depth)
    let log_header = format!(
        "# Ralph Agent Invocation Log\n\
         # Role: Reviewer\n\
         # Agent: {}\n\
         # Model Index: 0\n\
         # Attempt: {}\n\
         # Phase: Review\n\
         # Timestamp: {}\n\n",
        active_agent,
        attempt,
        chrono::Utc::now().to_rfc3339()
    );
    ctx.workspace
        .append_bytes(std::path::Path::new(&logfile), log_header.as_bytes())
        .context("Failed to write agent log header - log would be incomplete without metadata")?;

    let log_prefix = format!("reviewer_{j}"); // For attribution only
    let model_index = 0usize; // Default model index for attribution

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
        log_prefix: &log_prefix,
        model_index: Some(model_index),
        attempt: Some(attempt),
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
            xml_content: None,
        });
    }

    let parse_result = extract_and_validate_review_output_xml(ctx, &log_prefix, issues_path)?;

    match parse_result {
        ParseResult::IssuesFound {
            issues,
            xml_content,
        } => {
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
                xml_content: Some(xml_content),
            })
        }
        ParseResult::NoIssuesExplicit { xml_content } => {
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
                xml_content: Some(xml_content),
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
                xml_content: None,
            })
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
                ctx.workspace,
            )
        });

    // Enforce that the rendered prompt does not contain unresolved template placeholders.
    // This must happen before any agent invocation.
    if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
        &fix_prompt,
        &[
            prompt_content.as_str(),
            plan_content.as_str(),
            issues_content.as_str(),
        ],
    ) {
        return Err(crate::prompts::TemplateVariablesInvalidError {
            template_name: "fix_mode_xml".to_string(),
            missing_variables: Vec::new(),
            unresolved_placeholders: err.unresolved_placeholders,
        }
        .into());
    }

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

    // Use per-run log directory with simplified naming
    let base_log_path = ctx.run_log_context.agent_log("reviewer_fix", j, None);
    let attempt = crate::pipeline::logfile::next_simplified_logfile_attempt_index(
        &base_log_path,
        ctx.workspace,
    );
    let logfile = if attempt == 0 {
        base_log_path.to_str().unwrap().to_string()
    } else {
        ctx.run_log_context
            .agent_log("reviewer_fix", j, Some(attempt))
            .to_str()
            .unwrap()
            .to_string()
    };

    // Write log file header with agent metadata
    // Use append_bytes to avoid overwriting if file exists (defense-in-depth)
    let log_header = format!(
        "# Ralph Agent Invocation Log\n\
         # Role: Reviewer (Fix Mode)\n\
         # Agent: {}\n\
         # Model Index: 0\n\
         # Attempt: {}\n\
         # Phase: Review Fix\n\
         # Timestamp: {}\n\n",
        active_agent,
        attempt,
        chrono::Utc::now().to_rfc3339()
    );
    if let Err(e) = ctx
        .workspace
        .append_bytes(std::path::Path::new(&logfile), log_header.as_bytes())
    {
        ctx.logger
            .warn(&format!("Failed to write agent log header: {}", e));
    }

    let log_prefix = format!("reviewer_fix_{j}"); // For attribution only
    let model_index = 0usize; // Default model index for attribution

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
        log_prefix: &log_prefix,
        model_index: Some(model_index),
        attempt: Some(attempt),
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
            status: None,
            summary: None,
            xml_content: None,
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
            status: None,
            summary: None,
            xml_content: None,
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
                status: Some(result_elements.status.clone()),
                summary: result_elements.summary.clone(),
                xml_content: Some(xml_to_validate),
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
                status: None,
                summary: None,
                xml_content: Some(xml_to_validate),
            })
        }
    }
}
