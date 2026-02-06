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
    use crate::workspace::{Workspace, WorkspaceFs};
    use std::env;

    let workspace = WorkspaceFs::new(env::current_dir().unwrap());
    let partials = get_shared_partials();
    let template_content = include_str!("../templates/planning_xml.txt");
    let template = Template::new(template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([
        ("PROMPT", prompt_md.to_string()),
        (
            "PLAN_XML_PATH",
            workspace.absolute_str(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            workspace.absolute_str(".agent/tmp/plan.xsd"),
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
pub fn prompt_plan_with_context(
    context: &TemplateContext,
    prompt_content: Option<&str>,
    workspace: &dyn Workspace,
) -> String {
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
            workspace.absolute_str(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            workspace.absolute_str(".agent/tmp/plan.xsd"),
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
            workspace.absolute_str(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            workspace.absolute_str(".agent/tmp/plan.xsd"),
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
            workspace.absolute_str(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            workspace.absolute_str(".agent/tmp/plan.xsd"),
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
///
/// Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
/// If required files are missing, a deterministic fallback prompt is produced that includes
/// diagnostic information but still provides valid instructions to the agent.
pub fn prompt_planning_xsd_retry_with_context_files(
    context: &TemplateContext,
    xsd_error: &str,
    workspace: &dyn Workspace,
) -> String {
    use std::path::Path;

    let partials = get_shared_partials();
    // Ensure schema file exists; last_output.xml is expected to already be present.
    write_planning_xsd_retry_schema_files(workspace);

    // Check that required files exist
    let schema_path = Path::new(".agent/tmp/plan.xsd");
    let last_output_path = Path::new(".agent/tmp/last_output.xml");

    let schema_exists = workspace.exists(schema_path);
    let last_output_exists = workspace.exists(last_output_path);

    // Build diagnostic prefix for missing files (per acceptance criteria #3)
    let mut diagnostic_prefix = String::new();
    if !schema_exists || !last_output_exists {
        diagnostic_prefix.push_str("⚠️  WARNING: Required XSD retry files are missing:\n");
        if !schema_exists {
            diagnostic_prefix.push_str(&format!(
                "  - Schema file: {} (workspace.root() = {})\n",
                workspace.absolute_str(".agent/tmp/plan.xsd"),
                workspace.root().display()
            ));
        }
        if !last_output_exists {
            diagnostic_prefix.push_str(&format!(
                "  - Last output: {} (workspace.root() = {})\n",
                workspace.absolute_str(".agent/tmp/last_output.xml"),
                workspace.root().display()
            ));
        }
        diagnostic_prefix.push_str(
            "This likely indicates CWD != workspace.root() path mismatch.\n\n",
        );
    }

    // If both files are missing, return fallback prompt with diagnostics (per AC #5)
    if !schema_exists && !last_output_exists {
        return format!(
            "{}XSD VALIDATION FAILED - CREATE IMPLEMENTATION PLAN\n\n\
             Error: {}\n\n\
             The schema and previous output files could not be found. \
             Please create an implementation plan for the requirements in PROMPT.md.\n\n\
             Output format: <ralph-plan><ralph-summary>Summary</ralph-summary><ralph-implementation-steps>Steps</ralph-implementation-steps></ralph-plan>\n",
            diagnostic_prefix, xsd_error
        );
    }

    // Proceed with normal XSD retry prompt generation if at least schema exists
    let template_content = context
        .registry()
        .get_template("planning_xsd_retry")
        .unwrap_or_else(|_| include_str!("../templates/planning_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        (
            "PLAN_XML_PATH",
            workspace.absolute_str(".agent/tmp/plan.xml"),
        ),
        (
            "PLAN_XSD_PATH",
            workspace.absolute_str(".agent/tmp/plan.xsd"),
        ),
        (
            "LAST_OUTPUT_XML_PATH",
            workspace.absolute_str(".agent/tmp/last_output.xml"),
        ),
    ]);

    let rendered_prompt = Template::new(&template_content)
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            format!(
                "Your previous plan failed XSD validation.\n\nError: {}\n\n\
                 Read .agent/tmp/plan.xsd for the schema and .agent/tmp/last_output.xml for your previous output.\n\
                 Please resend your plan in valid XML format conforming to the XSD schema.\n",
                xsd_error
            )
        });

    // Prepend diagnostic prefix if files were missing but we continued anyway
    if !diagnostic_prefix.is_empty() {
        format!("{}\n{}", diagnostic_prefix, rendered_prompt)
    } else {
        rendered_prompt
    }
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
