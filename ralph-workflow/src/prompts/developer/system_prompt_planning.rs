// System prompt template and generation (planning).
//
// Contains functions for generating planning prompts and XSD retry prompts.

/// Generate prompt for planning phase.
///
/// The orchestrator provides requirements via the planning task context.
/// The plan content is returned as structured output (captured by JSON parser)
/// and the orchestrator writes it to .agent/PLAN.md.
///
/// This prompt is designed to be agent-agnostic and follows best practices
/// from Claude Code's plan mode implementation.
///
/// Reference: <https://github.com/Piebald-AI/claude-code-system-prompts>
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

/// Generate prompt for planning phase using template registry.
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
/// This variant assumes `.agent/tmp/last_output.xml` is already materialized.
pub fn prompt_planning_xsd_retry_with_context_files(
    context: &TemplateContext,
    xsd_error: &str,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    // Ensure schema file exists; last_output.xml is expected to already be present.
    write_planning_xsd_retry_schema_files(workspace);

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

/// Generate XSD validation retry prompt for planning with error feedback.
pub fn prompt_planning_xsd_retry_with_context(
    context: &TemplateContext,
    _prompt_content: &str,
    xsd_error: &str,
    last_output: &str,
    workspace: &dyn Workspace,
) -> String {
    // Write context files to .agent/tmp/ for the agent to read
    write_planning_xsd_retry_files(workspace, last_output);
    prompt_planning_xsd_retry_with_context_files(context, xsd_error, workspace)
}
