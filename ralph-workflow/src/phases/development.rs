//! Development phase execution.
//!
//! This module handles the development phase of the Ralph pipeline, which consists
//! of iterative planning and execution cycles. Each iteration:
//! 1. Creates a PLAN.md from PROMPT.md
//! 2. Executes the plan
//! 3. Deletes PLAN.md
//! 4. Optionally runs fast checks

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::checkpoint::restore::ResumeContext;
use crate::checkpoint::{save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase};
use crate::files::llm_output_extraction::{
    validate_development_result_xml, validate_plan_xml, xml_paths, PlanElements,
};
use crate::files::update_status_with_workspace;
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_developer_iteration_continuation_xml,
    prompt_developer_iteration_xml_with_context, prompt_developer_iteration_xsd_retry_with_context,
    prompt_planning_xml_with_context, ContextLevel,
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
    /// Exit code from the agent invocation.
    pub exit_code: i32,
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

/// Run a single development attempt (one session) with XML extraction and validation.
///
/// This does **not** perform in-session XSD retries. If validation fails, the
/// caller should emit an OutputValidationFailed event and let the reducer decide
/// retry/fallback behavior.
pub fn run_development_attempt(
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

    let dev_prompt = if continuation_state.is_continuation() {
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
        }

        prompt
    } else if continuation_state.invalid_output_attempts > 0 {
        prompt_developer_iteration_xsd_retry_with_context(
            ctx.template_context,
            &prompt_md,
            &plan_md,
            "XML output failed validation. Provide valid XML output.",
            "",
            ctx.workspace,
        )
    } else {
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
        }

        prompt
    };

    let log_dir = Path::new(".agent/logs");
    ctx.workspace.create_dir_all(log_dir)?;
    let logfile = format!(".agent/logs/developer_{iteration}.log");

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
        label: active_agent,
        display_name: active_agent,
        cmd_str: &cmd_str,
        prompt: &dev_prompt,
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    let had_error = result.exit_code != 0;
    let auth_failure = had_error && stderr_contains_auth_error(&result.stderr);

    if auth_failure {
        return Ok(DevAttemptResult {
            had_error,
            exit_code: result.exit_code,
            output_valid: false,
            status: DevelopmentStatus::Failed,
            summary: "Authentication error detected".to_string(),
            files_changed: None,
            next_steps: None,
            auth_failure: true,
        });
    }

    let xml_content = crate::files::llm_output_extraction::file_based_extraction::try_extract_from_file_with_workspace(
        ctx.workspace,
        Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
    );

    let Some(xml_to_validate) = xml_content else {
        return Ok(DevAttemptResult {
            had_error,
            exit_code: result.exit_code,
            output_valid: false,
            status: DevelopmentStatus::Failed,
            summary:
                "XML output missing or invalid; agent must write .agent/tmp/development_result.xml"
                    .to_string(),
            files_changed: None,
            next_steps: Some("Provide valid XML output conforming to the XSD schema.".to_string()),
            auth_failure: false,
        });
    };

    match validate_development_result_xml(&xml_to_validate) {
        Ok(result_elements) => {
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

            Ok(DevAttemptResult {
                had_error,
                exit_code: result.exit_code,
                output_valid: true,
                status,
                summary: result_elements.summary.clone(),
                files_changed,
                next_steps: result_elements.next_steps.clone(),
                auth_failure: false,
            })
        }
        Err(_) => Ok(DevAttemptResult {
            had_error,
            exit_code: result.exit_code,
            output_valid: false,
            status: DevelopmentStatus::Failed,
            summary: "XML output failed validation. Provide valid XML output.".to_string(),
            files_changed: None,
            next_steps: Some("Provide valid XML output conforming to the XSD schema.".to_string()),
            auth_failure: false,
        }),
    }
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

        if let Some(checkpoint) = builder.build_with_workspace(ctx.workspace) {
            let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
        }
    }

    ctx.logger.info("Creating plan from PROMPT.md...");
    update_status_with_workspace(
        ctx.workspace,
        "Starting planning phase",
        ctx.config.isolation_mode,
    )?;

    let prompt_md_content = ctx.workspace.read(Path::new("PROMPT.md")).ok();

    let prompt_key = format!("planning_{}", iteration);
    let prompt_md_str = prompt_md_content.as_deref().unwrap_or("");

    let (plan_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            prompt_planning_xml_with_context(
                ctx.template_context,
                Some(prompt_md_str),
                ctx.workspace,
            )
        });

    if !was_replayed {
        ctx.capture_prompt(&prompt_key, &plan_prompt);
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    let plan_path = Path::new(".agent/PLAN.md");
    if let Some(parent) = plan_path.parent() {
        ctx.workspace.create_dir_all(parent)?;
    }

    let log_dir = Path::new(".agent/logs");
    ctx.workspace.create_dir_all(log_dir)?;
    let logfile = format!(".agent/logs/planning_{iteration}.log");

    let agent_config = ctx
        .registry
        .resolve_config(agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent))?;
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
        label: agent,
        display_name: agent,
        cmd_str: &cmd_str,
        prompt: &plan_prompt,
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    if result.exit_code != 0 {
        if stderr_contains_auth_error(&result.stderr) {
            return Err(AuthFailureError::Planning.into());
        }
        return Err(anyhow::anyhow!(
            "Planning agent failed with exit code {}",
            result.exit_code
        ));
    }

    let xml_content = crate::files::llm_output_extraction::file_based_extraction::try_extract_from_file_with_workspace(
        ctx.workspace,
        Path::new(xml_paths::PLAN_XML),
    )
    .ok_or_else(|| anyhow::anyhow!("Plan XML missing at .agent/tmp/plan.xml"))?;

    match validate_plan_xml(&xml_content) {
        Ok(plan_elements) => {
            let plan_md = format_plan_as_markdown(&plan_elements);
            ctx.workspace.write(plan_path, &plan_md)?;

            let step = ExecutionStep::new(
                "Planning",
                iteration,
                "planning",
                StepOutcome::success(None, vec![".agent/PLAN.md".to_string()]),
            )
            .with_agent(agent)
            .with_duration(start_time.elapsed().as_secs());
            ctx.execution_history.add_step(step);

            Ok(())
        }
        Err(err) => Err(anyhow::anyhow!("Plan XML validation failed: {err}")),
    }
}

/// Run the planning step to create PLAN.md.
///
/// The orchestrator ALWAYS extracts and writes PLAN.md from agent XML output.
pub fn run_planning_step(ctx: &mut PhaseContext<'_>, iteration: u32) -> anyhow::Result<()> {
    run_planning_step_with_agent(ctx, iteration, ctx.developer_agent)
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

fn stderr_contains_auth_error(stderr: &str) -> bool {
    let combined = stderr.to_lowercase();
    combined.contains("authentication")
        || combined.contains("unauthorized")
        || combined.contains("credential")
        || combined.contains("api key")
        || combined.contains("not authorized")
}
