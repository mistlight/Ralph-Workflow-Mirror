// System prompt template and generation.
//
// Contains all functions for generating developer prompts including iteration,
// planning, and XSD retry prompts.

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

    let template_content = include_str!("../templates/developer_iteration_xml.txt");
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
    let template_content = include_str!("../templates/planning_xml.txt");
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
            include_str!("../templates/developer_iteration_xml.txt").to_string()
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
            include_str!("../templates/planning_xml.txt").to_string()
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
        .unwrap_or_else(|_| include_str!("../templates/planning_xml.txt").to_string());
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
        .unwrap_or_else(|_| include_str!("../templates/planning_xml.txt").to_string());
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
