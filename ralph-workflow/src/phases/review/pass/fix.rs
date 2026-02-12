// Legacy phase-based code - deprecated in favor of reducer/handler architecture
#[allow(deprecated)]
use super::super::types::FixPassResult;
use super::helpers::stderr_contains_auth_error;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::checkpoint::restore::ResumeContext;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, try_extract_from_file_with_workspace, validate_fix_result_xml,
    xml_paths,
};
use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::files::update_status_with_workspace;
use crate::phases::context::PhaseContext;
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::prompts::{get_stored_or_generate_prompt, prompt_fix_xml_with_context, ContextLevel};

use std::path::Path;
use std::time::Instant;

/// Run the fix pass for a single cycle.
///
/// This function orchestrates a single fix pass that applies fixes for issues
/// identified in ISSUES.md and validates the results. It handles:
///
/// - Prompt generation with context (PROMPT.md, PLAN.md, ISSUES.md)
/// - Agent invocation with appropriate configuration
/// - XML output extraction and validation (fix-result.xml)
/// - Execution history tracking
///
/// # Arguments
///
/// * `ctx` - Phase context with workspace, logger, and configuration
/// * `j` - The cycle number (used for logging and prompt keys)
/// * `_reviewer_context` - Context level for the fix prompt (currently unused)
/// * `_resume_context` - Optional resume context for checkpoint replay
/// * `_agent` - Optional agent override (defaults to ctx.reviewer_agent)
///
/// # Returns
///
/// A `FixPassResult` indicating whether fixes were applied and if the pass succeeded.
///
/// # Errors
///
/// Returns an error if:
/// - Agent configuration is missing
/// - Prompt template contains unresolved placeholders
/// - Status file cannot be updated
#[allow(deprecated)]
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

    // Legacy phase-based code - uses deprecated validation
    // TODO: Remove when migration to reducer/handler architecture is complete
    // The reducer/handler architecture uses log-based validation (SubstitutionLog::is_complete())
    // This legacy code still uses regex-based validation which can cause false positives.
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
        base_log_path
            .to_str()
            .expect("Path contains invalid UTF-8 - all paths in this codebase should be UTF-8")
            .to_string()
    } else {
        ctx.run_log_context
            .agent_log("reviewer_fix", j, Some(attempt))
            .to_str()
            .expect("Path contains invalid UTF-8 - all paths in this codebase should be UTF-8")
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
