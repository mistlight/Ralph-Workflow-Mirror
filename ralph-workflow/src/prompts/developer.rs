//! Developer prompts.
//!
//! Prompts for developer agent actions including iteration and planning.

use std::collections::HashMap;
use std::path::Path;

use super::partials::get_shared_partials;
use super::template_context::TemplateContext;
use super::template_engine::Template;
#[cfg(any(test, feature = "test-utils"))]
use super::types::ContextLevel;
use crate::files::llm_output_extraction::file_based_extraction::resolve_absolute_path;
use crate::workspace::Workspace;

/// The XSD schema for development result validation - included at compile time
const DEVELOPMENT_RESULT_XSD_SCHEMA: &str =
    include_str!("../files/llm_output_extraction/development_result.xsd");

/// Generate developer iteration prompt.
///
/// Note: We do NOT tell the agent how many total iterations exist.
/// This prevents "context pollution" - the agent should complete their task fully
/// without knowing when the loop ends.
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
///
/// # Arguments
///
/// * `iteration` - The current iteration number (accepted for API compatibility, not exposed to agent)
/// * `total` - The total number of iterations (accepted for API compatibility, not exposed to agent)
/// * `context` - The context level (minimal or normal) (accepted for API compatibility, not used in template)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
#[cfg(test)]
pub fn prompt_developer_iteration(
    iteration: u32,
    total: u32,
    context: ContextLevel,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    let partials = get_shared_partials();
    // Note: iteration, total, and context are accepted for API compatibility
    // but are intentionally not exposed to the agent to prevent context pollution.
    let _ = (iteration, total, context);

    let template_content = include_str!("templates/developer_iteration_xml.txt");
    let template = Template::new(template_content);
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
        // Embedded fallback template (XML format)
        format!(
            "IMPLEMENTATION MODE\n\nORIGINAL REQUEST:\n{prompt_content}\n\nIMPLEMENTATION PLAN:\n{plan_content}\n\nExecute the next steps from the plan above.\n\nOutput format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n"
        )
    })
}

/// Generate prompt for planning phase.
///
/// The orchestrator provides requirements via the planning task context.
/// The plan content is returned as structured output (captured by JSON parser)
/// and the orchestrator writes it to .agent/PLAN.md.
///
/// This prompt is designed to be agent-agnostic and follows best practices
/// from Claude Code's plan mode implementation:
/// - Multi-phase workflow (Understanding → Exploration → Design → Review → Final Plan)
/// - Strict read-only constraints during planning
/// - Critical files identification (3-5 files with justifications)
/// - Verification strategy
/// - Clear exit criteria
///
/// Reference: <https://github.com/Piebald-AI/claude-code-system-prompts>
///
/// # Arguments
///
/// * `prompt_content` - Optional PROMPT.md content to include directly in the prompt.
///   When provided, the agent doesn't need to discover PROMPT.md through file exploration,
///   which prevents accidental deletion.
#[cfg(test)]
pub fn prompt_plan(prompt_content: Option<&str>) -> String {
    let partials = get_shared_partials();
    let template_content = include_str!("templates/planning_xml.txt");
    let template = Template::new(template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([
        ("PROMPT", prompt_md.to_string()),
        (
            "PLAN_XML_PATH",
            resolve_absolute_path(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            resolve_absolute_path(".agent/tmp/plan.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
        // Embedded fallback template (XML format)
        format!(
            "PLANNING MODE\n\nCreate an implementation plan for:\n\n{prompt_md}\n\nIdentify critical files and implementation steps.\n\nOutput format: <ralph-plan><ralph-summary>Summary</ralph-summary><ralph-implementation-steps>Steps</ralph-implementation-steps></ralph-plan>\n"
        )
    })
}

/// Generate developer iteration prompt using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It's the recommended way to generate prompts going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `iteration` - The current iteration number (accepted for API compatibility, not exposed to agent)
/// * `total` - The total number of iterations (accepted for API compatibility, not exposed to agent)
/// * `ctx_level` - The context level (minimal or normal) (accepted for API compatibility, not used in template)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
#[cfg(any(test, feature = "test-utils"))]
pub fn prompt_developer_iteration_with_context(
    context: &TemplateContext,
    iteration: u32,
    total: u32,
    ctx_level: ContextLevel,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    let partials = get_shared_partials();
    // Note: iteration, total, and ctx_level are accepted for API compatibility
    // but are intentionally not exposed to the agent to prevent context pollution.
    let _ = (iteration, total, ctx_level);

    let template_content = context
        .registry()
        .get_template("developer_iteration_xml")
        .unwrap_or_else(|_| {
            // Fallback to embedded template if registry fails
            include_str!("templates/developer_iteration_xml.txt").to_string()
        });
    let template = Template::new(&template_content);
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
        // Embedded fallback template (XML format)
        format!(
            "IMPLEMENTATION MODE\n\nORIGINAL REQUEST:\n{prompt_content}\n\nIMPLEMENTATION PLAN:\n{plan_content}\n\nExecute the next steps from the plan above.\n\nOutput format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n"
        )
    })
}

/// Generate prompt for planning phase using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It's the recommended way to generate prompts going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - Optional PROMPT.md content to include directly in the prompt.
///   When provided, the agent doesn't need to discover PROMPT.md through file exploration,
///   which prevents accidental deletion.
#[cfg(any(test, feature = "test-utils"))]
pub fn prompt_plan_with_context(context: &TemplateContext, prompt_content: Option<&str>) -> String {
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("planning_xml")
        .unwrap_or_else(|_| {
            // Fallback to embedded template if registry fails
            include_str!("templates/planning_xml.txt").to_string()
        });
    let template = Template::new(&template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([
        ("PROMPT", prompt_md.to_string()),
        (
            "PLAN_XML_PATH",
            resolve_absolute_path(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            resolve_absolute_path(".agent/tmp/plan.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
        // Embedded fallback template (XML format)
        format!(
            "PLANNING MODE\n\nCreate an implementation plan for:\n\n{prompt_md}\n\nIdentify critical files and implementation steps.\n\nOutput format: <ralph-plan><ralph-summary>Summary</ralph-summary><ralph-implementation-steps>Steps</ralph-implementation-steps></ralph-plan>\n"
        )
    })
}

/// Generate XML-based planning prompt using template registry.
///
/// This version uses XML output format with XSD validation for reliable parsing.
/// It's the recommended format for planning going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - Optional PROMPT.md content to include directly in the prompt.
/// * `workspace` - Workspace for writing XSD schema files
pub fn prompt_planning_xml_with_context(
    context: &TemplateContext,
    prompt_content: Option<&str>,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    // Write the XSD schema file so it's available for the agent to reference
    write_planning_xsd_schema_file(workspace);

    let template_content = context
        .registry()
        .get_template("planning_xml")
        .unwrap_or_else(|_| include_str!("templates/planning_xml.txt").to_string());
    let template = Template::new(&template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([
        ("PROMPT", prompt_md.to_string()),
        (
            "PLAN_XML_PATH",
            resolve_absolute_path(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            resolve_absolute_path(".agent/tmp/plan.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
        format!(
            "PLANNING MODE\n\nCreate an implementation plan for:\n\n{prompt_md}\n\n\
             Output format: <ralph-plan><ralph-summary>Summary</ralph-summary><ralph-implementation-steps>Steps</ralph-implementation-steps></ralph-plan>\n"
        )
    })
}

/// Generate planning prompt with size-aware content references.
///
/// This version uses `PromptContentReference` which automatically handles
/// oversized PROMPT content by referencing the backup file path.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_ref` - Content reference for PROMPT.md content
/// * `workspace` - Workspace for writing XSD schema files
pub fn prompt_planning_xml_with_references(
    context: &TemplateContext,
    prompt_ref: &super::content_reference::PromptContentReference,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    // Write the XSD schema file so it's available for the agent to reference
    write_planning_xsd_schema_file(workspace);

    let template_content = context
        .registry()
        .get_template("planning_xml")
        .unwrap_or_else(|_| include_str!("templates/planning_xml.txt").to_string());
    let template = Template::new(&template_content);

    let variables = HashMap::from([
        ("PROMPT", prompt_ref.render_for_template()),
        (
            "PLAN_XML_PATH",
            resolve_absolute_path(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            resolve_absolute_path(".agent/tmp/plan.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            let prompt = prompt_ref.render_for_template();
            format!("PLANNING MODE\n\nCreate an implementation plan for:\n\n{prompt}\n")
        })
}

/// The XSD schema for plan validation - included at compile time
const PLAN_XSD_SCHEMA: &str = include_str!("../files/llm_output_extraction/plan.xsd");

/// Directory for XSD retry context files
const XSD_RETRY_TMP_DIR: &str = ".agent/tmp";

/// Write just the XSD schema file to `.agent/tmp/` directory.
///
/// This is called before the initial planning prompt so the agent can reference
/// the schema if needed. The schema provides the authoritative definition of
/// valid XML structure.
fn write_planning_xsd_schema_file(workspace: &dyn Workspace) {
    let tmp_dir = Path::new(XSD_RETRY_TMP_DIR);
    if workspace.create_dir_all(tmp_dir).is_err() {
        return;
    }

    let _ = workspace.write(&tmp_dir.join("plan.xsd"), PLAN_XSD_SCHEMA);
}

/// Write XSD retry context files to `.agent/tmp/` directory.
///
/// This writes the XSD schema and last output to files so they don't bloat the prompt.
/// The agent MUST read these files to understand what went wrong and fix it.
fn write_planning_xsd_retry_files(workspace: &dyn Workspace, last_output: &str) {
    let tmp_dir = Path::new(XSD_RETRY_TMP_DIR);
    if workspace.create_dir_all(tmp_dir).is_err() {
        return;
    }

    let _ = workspace.write(&tmp_dir.join("plan.xsd"), PLAN_XSD_SCHEMA);
    let _ = workspace.write(&tmp_dir.join("last_output.xml"), last_output);
}

/// Write XSD retry context files for development iteration to `.agent/tmp/` directory.
fn write_dev_iteration_xsd_retry_files(workspace: &dyn Workspace, last_output: &str) {
    let tmp_dir = Path::new(XSD_RETRY_TMP_DIR);
    if workspace.create_dir_all(tmp_dir).is_err() {
        return;
    }

    let _ = workspace.write(
        &tmp_dir.join("development_result.xsd"),
        DEVELOPMENT_RESULT_XSD_SCHEMA,
    );
    let _ = workspace.write(&tmp_dir.join("last_output.xml"), last_output);
}

/// Generate XSD validation retry prompt for planning with error feedback.
///
/// This prompt is used when an AI agent produces plan XML that fails XSD validation.
/// The XSD schema and last output are written to files at `.agent/tmp/` to avoid
/// bloating the prompt. The agent should read these files.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `_prompt_content` - Original user requirements (unused - kept for API compatibility)
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
/// * `workspace` - Workspace for writing XSD retry context files
pub fn prompt_planning_xsd_retry_with_context(
    context: &TemplateContext,
    _prompt_content: &str,
    xsd_error: &str,
    last_output: &str,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    // Write context files to .agent/tmp/ for the agent to read
    write_planning_xsd_retry_files(workspace, last_output);

    let template_content = context
        .registry()
        .get_template("planning_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/planning_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        (
            "PLAN_XML_PATH",
            resolve_absolute_path(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            resolve_absolute_path(".agent/tmp/plan.xsd"),
        ),
        (
            "LAST_OUTPUT_XML_PATH",
            resolve_absolute_path(".agent/tmp/last_output.xml"),
        ),
    ]);
    Template::new(&template_content)
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            format!(
                "Your previous plan failed XSD validation.\n\nError: {}\n\n\
                 Read .agent/tmp/plan.xsd for the schema and .agent/tmp/last_output.xml for your previous output.\n\
                 Please resend your plan in valid XML format conforming to the XSD schema.\n",
                xsd_error
            )
        })
}

/// Generate XML-based developer iteration prompt using template registry.
///
/// This version uses XML output format with XSD validation for reliable parsing.
/// It's the recommended format for development iteration going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
pub fn prompt_developer_iteration_xml_with_context(
    context: &TemplateContext,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("developer_iteration_xml")
        .unwrap_or_else(|_| include_str!("templates/developer_iteration_xml.txt").to_string());
    let template = Template::new(&template_content);
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        (
            "DEVELOPMENT_RESULT_XML_PATH",
            resolve_absolute_path(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            resolve_absolute_path(".agent/tmp/development_result.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
        format!(
            "IMPLEMENTATION MODE\n\nORIGINAL REQUEST:\n{prompt_content}\n\n\
             IMPLEMENTATION PLAN:\n{plan_content}\n\n\
             Output format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n"
        )
    })
}

/// Generate developer iteration prompt with size-aware content references.
///
/// This version uses `PromptContentReferences` which automatically handles
/// oversized content by referencing file paths instead of embedding inline.
/// Use this when content may exceed CLI argument limits.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `refs` - Content references for PROMPT and PLAN
pub fn prompt_developer_iteration_xml_with_references(
    context: &TemplateContext,
    refs: &super::content_builder::PromptContentReferences,
) -> String {
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("developer_iteration_xml")
        .unwrap_or_else(|_| include_str!("templates/developer_iteration_xml.txt").to_string());
    let template = Template::new(&template_content);
    let variables = HashMap::from([
        ("PROMPT", refs.prompt_for_template()),
        ("PLAN", refs.plan_for_template()),
        (
            "DEVELOPMENT_RESULT_XML_PATH",
            resolve_absolute_path(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            resolve_absolute_path(".agent/tmp/development_result.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            let prompt = refs.prompt_for_template();
            let plan = refs.plan_for_template();
            format!(
                "IMPLEMENTATION MODE\n\nORIGINAL REQUEST:\n{prompt}\n\n\
             IMPLEMENTATION PLAN:\n{plan}\n\n\
             Output format: <ralph-development-result>...</ralph-development-result>\n"
            )
        })
}

/// Generate XSD validation retry prompt for developer iteration with error feedback.
///
/// This prompt is used when an AI agent produces development result XML that fails XSD validation.
/// The XSD schema and last output are written to files at `.agent/tmp/` to avoid
/// bloating the prompt. The agent should read these files.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `_prompt_content` - The original user request (unused - kept for API compatibility)
/// * `_plan_content` - The implementation plan (unused - kept for API compatibility)
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
/// * `workspace` - Workspace for writing XSD retry context files
pub fn prompt_developer_iteration_xsd_retry_with_context(
    context: &TemplateContext,
    _prompt_content: &str,
    _plan_content: &str,
    xsd_error: &str,
    last_output: &str,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    // Write context files to .agent/tmp/ for the agent to read
    write_dev_iteration_xsd_retry_files(workspace, last_output);

    let template_content = context
        .registry()
        .get_template("developer_iteration_xsd_retry")
        .unwrap_or_else(|_| {
            include_str!("templates/developer_iteration_xsd_retry.txt").to_string()
        });
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        (
            "DEVELOPMENT_RESULT_XML_PATH",
            resolve_absolute_path(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            resolve_absolute_path(".agent/tmp/development_result.xsd"),
        ),
        (
            "LAST_OUTPUT_XML_PATH",
            resolve_absolute_path(".agent/tmp/last_output.xml"),
        ),
    ]);
    Template::new(&template_content)
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            format!(
                "Your previous development status failed XSD validation.\n\nError: {}\n\n\
                 Read .agent/tmp/development_result.xsd for the schema and .agent/tmp/last_output.xml for your previous output.\n\
                 Please resend your status in valid XML format conforming to the XSD schema.\n",
                xsd_error
            )
        })
}

/// Generate continuation prompt for development iteration.
///
/// Used when the previous attempt returned status="partial" or "failed".
/// Includes context about what was previously done and guidance to continue.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `continuation_state` - The continuation state with context from previous attempt
///
/// # Example
///
/// ```ignore
/// let continuation_state = ContinuationState::new()
///     .trigger_continuation(
///         DevelopmentStatus::Partial,
///         "Implemented half the feature".to_string(),
///         Some(vec!["src/lib.rs".to_string()]),
///         Some("Add tests".to_string()),
///     );
/// let prompt = prompt_developer_iteration_continuation_xml(
///     &template_context,
///     &continuation_state,
/// );
/// ```
pub fn prompt_developer_iteration_continuation_xml(
    context: &TemplateContext,
    continuation_state: &crate::reducer::state::ContinuationState,
) -> String {
    use crate::prompts::partials::get_shared_partials;

    let template_content = context
        .registry()
        .get_template("developer_iteration_continuation_xml")
        .unwrap_or_else(|_| {
            include_str!("templates/developer_iteration_continuation_xml.txt").to_string()
        });
    let template = Template::new(&template_content);
    let partials = get_shared_partials();

    let previous_status = continuation_state
        .previous_status
        .as_ref()
        .map_or("unknown".to_string(), |s| format!("{}", s));

    let previous_summary = continuation_state
        .previous_summary
        .clone()
        .unwrap_or_else(|| "No summary available".to_string());

    let previous_files_changed = continuation_state
        .previous_files_changed
        .as_ref()
        .map(|files| files.join("\n"));

    let previous_next_steps = continuation_state.previous_next_steps.clone();

    let mut variables: HashMap<&str, String> = HashMap::new();
    variables.insert("PROMPT_PATH", "PROMPT.md".to_string());
    variables.insert("PLAN_PATH", ".agent/PLAN.md".to_string());
    variables.insert("PREVIOUS_STATUS", previous_status);
    variables.insert("PREVIOUS_SUMMARY", previous_summary);
    variables.insert(
        "CONTINUATION_ATTEMPT",
        continuation_state.continuation_attempt.to_string(),
    );
    variables.insert(
        "DEVELOPMENT_RESULT_XML_PATH",
        resolve_absolute_path(".agent/tmp/development_result.xml"),
    );
    variables.insert(
        "DEVELOPMENT_RESULT_XSD_PATH",
        resolve_absolute_path(".agent/tmp/development_result.xsd"),
    );

    // Optional fields - add if present
    if let Some(files) = previous_files_changed {
        variables.insert("PREVIOUS_FILES_CHANGED", files);
    }
    if let Some(next_steps) = previous_next_steps {
        variables.insert("PREVIOUS_NEXT_STEPS", next_steps);
    }

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            // Fallback template if rendering fails
            let status = continuation_state
                .previous_status
                .as_ref()
                .map_or("unknown", |s| match s {
                    crate::reducer::state::DevelopmentStatus::Completed => "completed",
                    crate::reducer::state::DevelopmentStatus::Partial => "partial",
                    crate::reducer::state::DevelopmentStatus::Failed => "failed",
                });
            let summary = continuation_state
                .previous_summary
                .as_ref()
                .map_or("No summary", |s| s.as_str());
            format!(
                "CONTINUATION MODE\n\n\
                 This is continuation attempt #{}. Previous status: {}\n\n\
                 Previous summary: {}\n\n\
                 Continue the implementation from where you left off.\n\
                 Read PROMPT.md and .agent/PLAN.md for the full context.\n\n\
                 Output format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n",
                continuation_state.continuation_attempt,
                status,
                summary
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_developer_iteration() {
        let result =
            prompt_developer_iteration(2, 5, ContextLevel::Normal, "test prompt", "test plan");
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(result.contains("IMPLEMENTATION MODE"));
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_developer_iteration_minimal_context() {
        let result =
            prompt_developer_iteration(1, 5, ContextLevel::Minimal, "test prompt", "test plan");
        // Minimal context should include essential files (not STATUS.md in isolation mode)
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_prompt_plan() {
        let result = prompt_plan(None);
        // Prompt should NOT explicitly mention PROMPT.md file name
        // Agents receive content directly without knowing the source file
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("NEVER read, write, or delete this file"));
        // Plan is now returned as XML output format
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("<ralph-implementation-steps>"));
        assert!(result.contains("<ralph-critical-files>"));
        assert!(result.contains("<ralph-verification-strategy>"));

        // Ensure strict read-only constraints are present (Claude Code alignment)
        assert!(result.contains("READ-ONLY"));
        assert!(result.contains("STRICTLY PROHIBITED"));

        // Ensure 5-phase workflow structure (Claude Code alignment)
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        assert!(result.contains("PHASE 2: EXPLORATION"));
        assert!(result.contains("PHASE 3: DESIGN"));
        assert!(result.contains("PHASE 4: REVIEW"));
        assert!(result.contains("PHASE 5: WRITE STRUCTURED PLAN"));

        // Ensure XML output format is specified
        assert!(result.contains("<ralph-plan>"));
        assert!(result.contains("<ralph-summary>"));
    }

    #[test]
    fn test_prompt_plan_with_content() {
        let prompt_md = "# Test Prompt\n\nThis is the content.";
        let result = prompt_plan(Some(prompt_md));
        // Should include the content WITHOUT naming PROMPT.md
        assert!(result.contains("USER REQUIREMENTS:"));
        assert!(result.contains("This is the content."));
        // Should NOT mention PROMPT.md file name
        assert!(!result.contains("PROMPT.md"));
        // Should still have the planning structure
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        // Should have XML output format
        assert!(result.contains("<ralph-plan>"));
    }

    #[test]
    fn all_developer_prompts_isolate_agents_from_git() {
        // Verify developer prompts don't tell agents to run git commands
        let prompts = vec![
            prompt_developer_iteration(1, 3, ContextLevel::Minimal, "", ""),
            prompt_developer_iteration(2, 3, ContextLevel::Normal, "", ""),
            prompt_plan(None),
        ];

        for prompt in prompts {
            assert!(
                !prompt.contains("git diff"),
                "Developer prompt should not tell agent to run git diff"
            );
            assert!(
                !prompt.contains("git status"),
                "Developer prompt should not tell agent to run git status"
            );
            assert!(
                !prompt.contains("git commit"),
                "Developer prompt should not tell agent to run git commit"
            );
            assert!(
                !prompt.contains("git add"),
                "Developer prompt should not tell agent to run git add"
            );
        }
    }

    #[test]
    fn test_prompt_developer_iteration_with_context() {
        let context = TemplateContext::default();
        let result = prompt_developer_iteration_with_context(
            &context,
            2,
            5,
            ContextLevel::Normal,
            "test prompt",
            "test plan",
        );
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(result.contains("IMPLEMENTATION MODE"));
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
    }

    #[test]
    fn test_prompt_developer_iteration_with_context_minimal() {
        let context = TemplateContext::default();
        let result = prompt_developer_iteration_with_context(
            &context,
            1,
            5,
            ContextLevel::Minimal,
            "test prompt",
            "test plan",
        );
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
    }

    #[test]
    fn test_prompt_plan_with_context() {
        let context = TemplateContext::default();
        let result = prompt_plan_with_context(&context, None);
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("<ralph-implementation-steps>"));
        assert!(result.contains("<ralph-critical-files>"));
        assert!(result.contains("<ralph-verification-strategy>"));
        assert!(result.contains("READ-ONLY"));
        assert!(result.contains("STRICTLY PROHIBITED"));
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        assert!(result.contains("PHASE 2: EXPLORATION"));
        assert!(result.contains("PHASE 3: DESIGN"));
        assert!(result.contains("PHASE 4: REVIEW"));
        assert!(result.contains("PHASE 5: WRITE STRUCTURED PLAN"));
        assert!(result.contains("<ralph-plan>"));
    }

    #[test]
    fn test_prompt_plan_with_context_and_content() {
        let context = TemplateContext::default();
        let prompt_md = "# Test Prompt\n\nThis is the content.";
        let result = prompt_plan_with_context(&context, Some(prompt_md));
        assert!(result.contains("USER REQUIREMENTS:"));
        assert!(result.contains("This is the content."));
        assert!(!result.contains("PROMPT.md"));
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
    }

    #[test]
    fn test_context_based_prompts_isolate_from_git() {
        let context = TemplateContext::default();
        let prompts = vec![
            prompt_developer_iteration_with_context(&context, 1, 3, ContextLevel::Minimal, "", ""),
            prompt_developer_iteration_with_context(&context, 2, 3, ContextLevel::Normal, "", ""),
            prompt_plan_with_context(&context, None),
        ];

        for prompt in prompts {
            assert!(
                !prompt.contains("git diff"),
                "Developer prompt should not tell agent to run git diff"
            );
            assert!(
                !prompt.contains("git status"),
                "Developer prompt should not tell agent to run git status"
            );
            assert!(
                !prompt.contains("git commit"),
                "Developer prompt should not tell agent to run git commit"
            );
            assert!(
                !prompt.contains("git add"),
                "Developer prompt should not tell agent to run git add"
            );
        }
    }

    #[test]
    fn test_context_based_matches_regular_functions() {
        let context = TemplateContext::default();
        let regular = prompt_developer_iteration(1, 3, ContextLevel::Normal, "prompt", "plan");
        let with_context = prompt_developer_iteration_with_context(
            &context,
            1,
            3,
            ContextLevel::Normal,
            "prompt",
            "plan",
        );
        // Both should produce equivalent output
        assert_eq!(regular, with_context);

        let regular_plan = prompt_plan(None);
        let with_context_plan = prompt_plan_with_context(&context, None);
        assert_eq!(regular_plan, with_context_plan);
    }

    #[test]
    fn test_prompt_developer_iteration_xml_with_context_renders_shared_partials() {
        let context = TemplateContext::default();

        let result =
            prompt_developer_iteration_xml_with_context(&context, "test prompt", "test plan");

        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(result.contains("IMPLEMENTATION MODE"));

        // Shared partials should be expanded
        assert!(
            result.contains("*** UNATTENDED MODE - NO USER INTERACTION ***"),
            "developer_iteration_xml should render shared/_unattended_mode partial"
        );
        assert!(
            !result.contains("{{>"),
            "developer_iteration_xml should not contain raw partial directives"
        );
    }

    // =========================================================================
    // Tests for _with_references variants
    // =========================================================================

    #[test]
    fn test_prompt_developer_iteration_xml_with_references_small_content() {
        use crate::prompts::content_builder::PromptContentBuilder;
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();

        let refs = PromptContentBuilder::new(&workspace)
            .with_prompt("Small prompt content".to_string())
            .with_plan("Small plan content".to_string())
            .build();

        let result = prompt_developer_iteration_xml_with_references(&context, &refs);

        // Should embed content inline
        assert!(result.contains("Small prompt content"));
        assert!(result.contains("Small plan content"));
        assert!(result.contains("IMPLEMENTATION MODE"));

        // Shared partials should be expanded
        assert!(
            result.contains("*** UNATTENDED MODE - NO USER INTERACTION ***"),
            "developer_iteration_xml should render shared/_unattended_mode partial"
        );
        assert!(
            !result.contains("{{>"),
            "developer_iteration_xml should not contain raw partial directives"
        );
    }

    #[test]
    fn test_prompt_developer_iteration_xml_with_references_large_prompt() {
        use crate::prompts::content_builder::PromptContentBuilder;
        use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test().with_file(".agent/PROMPT.md.backup", "backup");

        let context = TemplateContext::default();
        let large_prompt = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);

        let refs = PromptContentBuilder::new(&workspace)
            .with_prompt(large_prompt)
            .with_plan("Small plan".to_string())
            .build();

        let result = prompt_developer_iteration_xml_with_references(&context, &refs);

        // Should reference backup file, not embed content
        assert!(result.contains("PROMPT.md.backup"));
        assert!(result.contains("Read from"));
        assert!(result.contains("Small plan"));
    }

    #[test]
    fn test_prompt_developer_iteration_xml_with_references_large_plan() {
        use crate::prompts::content_builder::PromptContentBuilder;
        use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();
        let large_plan = "p".repeat(MAX_INLINE_CONTENT_SIZE + 1);

        let refs = PromptContentBuilder::new(&workspace)
            .with_prompt("Small prompt".to_string())
            .with_plan(large_plan)
            .build();

        let result = prompt_developer_iteration_xml_with_references(&context, &refs);

        // Should reference PLAN.md file, not embed content
        assert!(result.contains(".agent/PLAN.md"));
        assert!(result.contains("plan.xml"));
        assert!(result.contains("Small prompt"));
    }

    #[test]
    fn test_prompt_planning_xml_with_references_small_content() {
        use crate::prompts::content_reference::PromptContentReference;
        use crate::workspace::MemoryWorkspace;
        use std::path::Path;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();

        let prompt_ref = PromptContentReference::from_content(
            "Small requirements".to_string(),
            Path::new(".agent/PROMPT.md.backup"),
            "User requirements",
        );

        let result = prompt_planning_xml_with_references(&context, &prompt_ref, &workspace);

        // Should embed content inline
        assert!(result.contains("Small requirements"));
        assert!(result.contains("PLANNING MODE"));

        // Read-only modes: planner must still write exactly one XML file.
        assert!(
            result.contains("explicitly authorized") && result.contains("EXACTLY ONE file"),
            "planning_xml should explicitly authorize writing exactly one XML file"
        );
        assert!(
            result.contains("MANDATORY"),
            "planning_xml should mark XML file write mandatory"
        );
        assert!(
            result.contains("Not writing") && result.contains("FAILURE"),
            "planning_xml should say not writing XML is a failure"
        );
        assert!(
            result.contains("does not conform")
                && result.contains("XSD")
                && result.contains("FAILURE"),
            "planning_xml should say non-XSD XML is a failure"
        );
        assert!(
            result.contains("DO NOT") && (result.contains("print") || result.contains("stdout")),
            "planning_xml should forbid stdout output"
        );
    }

    #[test]
    fn test_prompt_planning_xml_with_references_large_content() {
        use crate::prompts::content_reference::{PromptContentReference, MAX_INLINE_CONTENT_SIZE};
        use crate::workspace::MemoryWorkspace;
        use std::path::Path;

        let workspace = MemoryWorkspace::new_test().with_file(".agent/PROMPT.md.backup", "backup");
        let context = TemplateContext::default();
        let large_content = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);

        let prompt_ref = PromptContentReference::from_content(
            large_content,
            Path::new(".agent/PROMPT.md.backup"),
            "User requirements",
        );

        let result = prompt_planning_xml_with_references(&context, &prompt_ref, &workspace);

        // Should reference backup file, not embed content
        assert!(result.contains("PROMPT.md.backup"));
        assert!(result.contains("Read from"));
        assert!(result.contains("PLANNING MODE"));
    }

    #[test]
    fn test_prompt_planning_xml_with_references_writes_xsd() {
        use crate::prompts::content_reference::PromptContentReference;
        use crate::workspace::MemoryWorkspace;
        use std::path::Path;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();

        let prompt_ref = PromptContentReference::inline("Test requirements".to_string());

        let _result = prompt_planning_xml_with_references(&context, &prompt_ref, &workspace);

        // Should have written the XSD schema file
        assert!(workspace.exists(Path::new(".agent/tmp/plan.xsd")));
    }

    #[test]
    fn test_prompt_planning_xsd_retry_with_context_has_read_only_overrides() {
        use crate::workspace::MemoryWorkspace;

        let context = TemplateContext::default();
        let workspace = MemoryWorkspace::new_test();

        let result = prompt_planning_xsd_retry_with_context(
            &context,
            "prompt content",
            "XSD error",
            "last output",
            &workspace,
        );

        assert!(result.contains("XSD error"));
        assert!(result.contains(".agent/tmp/plan.xsd"));
        assert!(result.contains(".agent/tmp/last_output.xml"));

        assert!(
            result.contains("explicitly authorized") && result.contains("EXACTLY ONE file"),
            "planning_xsd_retry should explicitly authorize writing exactly one XML file"
        );
        assert!(
            result.contains("MANDATORY"),
            "planning_xsd_retry should mark XML file write mandatory"
        );
        assert!(
            result.contains("Not writing") && result.contains("FAILURE"),
            "planning_xsd_retry should say not writing XML is a failure"
        );
        assert!(
            result.contains("does not conform")
                && result.contains("XSD")
                && result.contains("FAILURE"),
            "planning_xsd_retry should say non-XSD XML is a failure"
        );
        assert!(
            result.contains("DO NOT") && (result.contains("print") || result.contains("stdout")),
            "planning_xsd_retry should forbid stdout output"
        );

        // Verify files were written to workspace
        assert!(workspace.was_written(".agent/tmp/plan.xsd"));
        assert!(workspace.was_written(".agent/tmp/last_output.xml"));
    }

    #[test]
    fn test_continuation_prompt_contains_expected_elements() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let context = TemplateContext::default();
        let continuation_state = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Implemented half the feature".to_string(),
            Some(vec!["src/lib.rs".to_string(), "src/main.rs".to_string()]),
            Some("Add tests for the new functionality".to_string()),
        );

        let prompt = prompt_developer_iteration_continuation_xml(&context, &continuation_state);

        // Debug: print the prompt to see what we're actually getting
        eprintln!("Generated prompt:\n{}", prompt);

        // Verify the prompt contains key elements
        assert!(
            prompt.contains("CONTINUATION MODE"),
            "Prompt should indicate continuation mode"
        );
        assert!(
            prompt.contains("partial"),
            "Prompt should include previous status"
        );
        assert!(
            prompt.contains("Implemented half the feature"),
            "Prompt should include previous summary"
        );
        assert!(
            prompt.contains("src/lib.rs"),
            "Prompt should include changed files"
        );
        assert!(
            prompt.contains("Add tests"),
            "Prompt should include next steps"
        );
        assert!(
            prompt.contains("#1"),
            "Prompt should include continuation attempt number"
        );
        assert!(
            prompt.contains("PROMPT.md"),
            "Prompt should reference PROMPT.md"
        );
        assert!(
            prompt.contains("PLAN.md"),
            "Prompt should reference PLAN.md"
        );
        assert!(
            prompt.contains("do NOT modify") || prompt.contains("DO NOT MODIFY"),
            "Prompt should warn against modifying original files. Actual prompt: {}",
            prompt
        );
    }
}
