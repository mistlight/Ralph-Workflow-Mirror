// System prompt template and generation (developer iteration).
//
// Contains functions for generating developer iteration prompts and XSD retry prompts.

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
/// * `workspace` - Workspace for resolving absolute paths
pub fn prompt_developer_iteration_xml_with_context(
    context: &TemplateContext,
    prompt_content: &str,
    plan_content: &str,
    workspace: &dyn Workspace,
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
            workspace.absolute_str(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/development_result.xsd"),
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

/// Generate developer iteration prompt with size-aware content references and substitution log.
///
/// This is the new log-based version that returns both content and substitution tracking.
/// Use this version in handlers to enable log-based validation.
pub fn prompt_developer_iteration_xml_with_references_and_log(
    context: &TemplateContext,
    refs: &super::content_builder::PromptContentReferences,
    workspace: &dyn Workspace,
    template_name: &str,
) -> crate::prompts::RenderedTemplate {
    use crate::prompts::{
        RenderedTemplate, SubstitutionEntry, SubstitutionLog, SubstitutionSource,
    };

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
            workspace.absolute_str(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/development_result.xsd"),
        ),
    ]);

    match template.render_with_log(template_name, &variables, &partials) {
        Ok(rendered) => rendered,
        Err(err) => {
            // Extract missing variable from error
            let unsubstituted = match &err {
                crate::prompts::template_engine::TemplateError::MissingVariable(name) => {
                    vec![name.clone()]
                }
                _ => vec![],
            };

            let prompt = refs.prompt_for_template();
            let plan = refs.plan_for_template();
            let content = format!(
                "IMPLEMENTATION MODE\n\nORIGINAL REQUEST:\n{prompt}\n\n\
             IMPLEMENTATION PLAN:\n{plan}\n\n\
             Output format: <ralph-development-result>...</ralph-development-result>\n"
            );
            RenderedTemplate {
                content,
                log: SubstitutionLog {
                    template_name: template_name.to_string(),
                    substituted: vec![
                        SubstitutionEntry {
                            name: "PROMPT".to_string(),
                            source: SubstitutionSource::Value,
                        },
                        SubstitutionEntry {
                            name: "PLAN".to_string(),
                            source: SubstitutionSource::Value,
                        },
                    ],
                    unsubstituted,
                },
            }
        }
    }
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
/// * `workspace` - Workspace for resolving absolute paths
pub fn prompt_developer_iteration_xml_with_references(
    context: &TemplateContext,
    refs: &super::content_builder::PromptContentReferences,
    workspace: &dyn Workspace,
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
            workspace.absolute_str(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/development_result.xsd"),
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
    // Write context files to .agent/tmp/ for the agent to read
    write_dev_iteration_xsd_retry_files(workspace, last_output);
    prompt_developer_iteration_xsd_retry_with_context_files(context, xsd_error, workspace)
}

/// Generate XSD validation retry prompt for developer iteration with error feedback.
///
/// This variant assumes `.agent/tmp/last_output.xml` is already materialized.
///
/// Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
/// If required files are missing, a deterministic fallback prompt is produced that includes
/// diagnostic information but still provides valid instructions to the agent.
pub fn prompt_developer_iteration_xsd_retry_with_context_files(
    context: &TemplateContext,
    xsd_error: &str,
    workspace: &dyn Workspace,
) -> String {
    use std::path::Path;

    let partials = get_shared_partials();
    // Ensure schema file exists; last_output.xml is expected to already be present.
    write_dev_iteration_xsd_retry_schema_files(workspace);

    // Check that required files exist
    let schema_path = Path::new(".agent/tmp/development_result.xsd");
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
                workspace.absolute_str(".agent/tmp/development_result.xsd"),
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
        diagnostic_prefix
            .push_str("This likely indicates CWD != workspace.root() path mismatch.\n\n");
    }

    // If both files are missing, return fallback prompt with diagnostics (per AC #5)
    if !schema_exists && !last_output_exists {
        return format!(
            "{}XSD VALIDATION FAILED - CONTINUE IMPLEMENTATION\n\n\
             Error: {}\n\n\
             The schema and previous output files could not be found. \
             Please continue the implementation based on PROMPT.md and PLAN.md.\n\n\
             Output format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n",
            diagnostic_prefix, xsd_error
        );
    }

    // Proceed with normal XSD retry prompt generation if at least schema exists
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
            workspace.absolute_str(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/development_result.xsd"),
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
                "Your previous development status failed XSD validation.\n\nError: {}\n\n\
                 Read .agent/tmp/development_result.xsd for the schema and .agent/tmp/last_output.xml for your previous output.\n\
                 Please resend your status in valid XML format conforming to the XSD schema.\n",
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

/// Generate XSD validation retry prompt for developer iteration with substitution log.
///
/// This variant assumes `.agent/tmp/last_output.xml` is already materialized.
pub fn prompt_developer_iteration_xsd_retry_with_context_files_and_log(
    context: &TemplateContext,
    xsd_error: &str,
    workspace: &dyn Workspace,
    template_name: &str,
) -> crate::prompts::RenderedTemplate {
    use crate::prompts::{
        RenderedTemplate, SubstitutionEntry, SubstitutionLog, SubstitutionSource,
    };
    use std::path::Path;

    let partials = get_shared_partials();
    // Ensure schema file exists; last_output.xml is expected to already be present.
    write_dev_iteration_xsd_retry_schema_files(workspace);

    // Check that required files exist
    let schema_path = Path::new(".agent/tmp/development_result.xsd");
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
                workspace.absolute_str(".agent/tmp/development_result.xsd"),
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
        diagnostic_prefix
            .push_str("This likely indicates CWD != workspace.root() path mismatch.\n\n");
    }

    // If both files are missing, return fallback prompt with diagnostics (per AC #5)
    if !schema_exists && !last_output_exists {
        let content = format!(
            "{}XSD VALIDATION FAILED - CONTINUE IMPLEMENTATION\n\n\
             Error: {}\n\n\
             The schema and previous output files could not be found. \
             Please continue the implementation based on PROMPT.md and PLAN.md.\n\n\
             Output format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n",
            diagnostic_prefix, xsd_error
        );
        return RenderedTemplate {
            content,
            log: SubstitutionLog {
                template_name: template_name.to_string(),
                substituted: vec![SubstitutionEntry {
                    name: "XSD_ERROR".to_string(),
                    source: SubstitutionSource::Value,
                }],
                unsubstituted: vec![],
            },
        };
    }

    // Proceed with normal XSD retry prompt generation if at least schema exists
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
            workspace.absolute_str(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/development_result.xsd"),
        ),
        (
            "LAST_OUTPUT_XML_PATH",
            workspace.absolute_str(".agent/tmp/last_output.xml"),
        ),
    ]);

    let template = Template::new(&template_content);
    match template.render_with_log(template_name, &variables, &partials) {
        Ok(mut rendered) => {
            if !diagnostic_prefix.is_empty() {
                rendered.content = format!("{}\n{}", diagnostic_prefix, rendered.content);
            }
            rendered
        }
        Err(_) => {
            let content = format!(
                "Your previous development status failed XSD validation.\n\nError: {}\n\n\
                 Read .agent/tmp/development_result.xsd for the schema and .agent/tmp/last_output.xml for your previous output.\n\
                 Please resend your status in valid XML format conforming to the XSD schema.\n",
                xsd_error
            );
            RenderedTemplate {
                content,
                log: SubstitutionLog {
                    template_name: template_name.to_string(),
                    substituted: vec![SubstitutionEntry {
                        name: "XSD_ERROR".to_string(),
                        source: SubstitutionSource::Value,
                    }],
                    unsubstituted: vec![],
                },
            }
        }
    }
}

/// Generate continuation prompt for development iteration.
///
/// Used when the previous attempt returned status="partial" or "failed".
/// Includes context about what was previously done and guidance to continue.
pub fn prompt_developer_iteration_continuation_xml(
    context: &TemplateContext,
    continuation_state: &crate::reducer::state::ContinuationState,
    workspace: &dyn Workspace,
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
    let prompt_content = workspace
        .read(std::path::Path::new("PROMPT.md"))
        .unwrap_or_else(|_| "(no prompt available)".to_string());
    let plan_content = workspace
        .read(std::path::Path::new(".agent/PLAN.md"))
        .unwrap_or_else(|_| "(no plan available)".to_string());

    let mut variables: HashMap<&str, String> = HashMap::new();
    variables.insert("PROMPT_PATH", "PROMPT.md".to_string());
    variables.insert("PLAN_PATH", ".agent/PLAN.md".to_string());
    variables.insert("PREVIOUS_STATUS", previous_status);
    variables.insert("PREVIOUS_SUMMARY", previous_summary);
    variables.insert("PROMPT", prompt_content);
    variables.insert("PLAN", plan_content);
    variables.insert(
        "CONTINUATION_ATTEMPT",
        continuation_state.continuation_attempt.to_string(),
    );
    variables.insert(
        "DEVELOPMENT_RESULT_XML_PATH",
        workspace.absolute_str(".agent/tmp/development_result.xml"),
    );
    variables.insert(
        "DEVELOPMENT_RESULT_XSD_PATH",
        workspace.absolute_str(".agent/tmp/development_result.xsd"),
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

/// Generate continuation prompt for development iteration with substitution log.
pub fn prompt_developer_iteration_continuation_xml_with_log(
    context: &TemplateContext,
    continuation_state: &crate::reducer::state::ContinuationState,
    workspace: &dyn Workspace,
    template_name: &str,
) -> crate::prompts::RenderedTemplate {
    use crate::prompts::{
        RenderedTemplate, SubstitutionEntry, SubstitutionLog, SubstitutionSource,
    };

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
    let prompt_content = workspace
        .read(std::path::Path::new("PROMPT.md"))
        .unwrap_or_else(|_| "(no prompt available)".to_string());
    let plan_content = workspace
        .read(std::path::Path::new(".agent/PLAN.md"))
        .unwrap_or_else(|_| "(no plan available)".to_string());

    let mut variables: HashMap<&str, String> = HashMap::new();
    variables.insert("PROMPT_PATH", "PROMPT.md".to_string());
    variables.insert("PLAN_PATH", ".agent/PLAN.md".to_string());
    variables.insert("PREVIOUS_STATUS", previous_status);
    variables.insert("PREVIOUS_SUMMARY", previous_summary);
    variables.insert("PROMPT", prompt_content);
    variables.insert("PLAN", plan_content);
    variables.insert(
        "CONTINUATION_ATTEMPT",
        continuation_state.continuation_attempt.to_string(),
    );
    variables.insert(
        "DEVELOPMENT_RESULT_XML_PATH",
        workspace.absolute_str(".agent/tmp/development_result.xml"),
    );
    variables.insert(
        "DEVELOPMENT_RESULT_XSD_PATH",
        workspace.absolute_str(".agent/tmp/development_result.xsd"),
    );

    // Optional fields - add if present
    if let Some(files) = previous_files_changed {
        variables.insert("PREVIOUS_FILES_CHANGED", files);
    }
    if let Some(next_steps) = previous_next_steps {
        variables.insert("PREVIOUS_NEXT_STEPS", next_steps);
    }

    match template.render_with_log(template_name, &variables, &partials) {
        Ok(rendered) => rendered,
        Err(_) => {
            let status =
                continuation_state
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
            let content = format!(
                "CONTINUATION MODE\n\n\
                 This is continuation attempt #{}. Previous status: {}\n\n\
                 Previous summary: {}\n\n\
                 Continue the implementation from where you left off.\n\
                 Read PROMPT.md and .agent/PLAN.md for the full context.\n\n\
                 Output format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n",
                continuation_state.continuation_attempt,
                status,
                summary
            );
            RenderedTemplate {
                content,
                log: SubstitutionLog {
                    template_name: template_name.to_string(),
                    substituted: vec![
                        SubstitutionEntry {
                            name: "PREVIOUS_STATUS".to_string(),
                            source: SubstitutionSource::Value,
                        },
                        SubstitutionEntry {
                            name: "PREVIOUS_SUMMARY".to_string(),
                            source: SubstitutionSource::Value,
                        },
                    ],
                    unsubstituted: vec![],
                },
            }
        }
    }
}
