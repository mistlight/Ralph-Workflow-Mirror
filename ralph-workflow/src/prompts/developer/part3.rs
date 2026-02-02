// Developer prompts - Part 3: XSD retry and continuation functions

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
        .unwrap_or_else(|_| include_str!("../templates/planning_xsd_retry.txt").to_string());
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
        .unwrap_or_else(|_| include_str!("../templates/developer_iteration_xml.txt").to_string());
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
        .unwrap_or_else(|_| include_str!("../templates/developer_iteration_xml.txt").to_string());
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
            include_str!("../templates/developer_iteration_xsd_retry.txt").to_string()
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
            include_str!("../templates/developer_iteration_continuation_xml.txt").to_string()
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
