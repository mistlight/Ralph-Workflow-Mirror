//! Developer prompts.
//!
//! Prompts for developer agent actions including iteration and planning.

use std::collections::HashMap;

use super::template_context::TemplateContext;
use super::template_engine::Template;
#[cfg(any(test, feature = "test-utils"))]
use super::types::ContextLevel;

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
    // Note: iteration, total, and context are accepted for API compatibility
    // but are intentionally not exposed to the agent to prevent context pollution.
    let _ = (iteration, total, context);

    let template_content = include_str!("templates/developer_iteration_xml.txt");
    let template = Template::new(template_content);
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
    ]);

    template.render(&variables).unwrap_or_else(|_| {
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
    let template_content = include_str!("templates/planning_xml.txt");
    let template = Template::new(template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([("PROMPT", prompt_md.to_string())]);

    template.render(&variables).unwrap_or_else(|_| {
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

    template.render(&variables).unwrap_or_else(|_| {
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
    let template_content = context
        .registry()
        .get_template("planning_xml")
        .unwrap_or_else(|_| {
            // Fallback to embedded template if registry fails
            include_str!("templates/planning_xml.txt").to_string()
        });
    let template = Template::new(&template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([("PROMPT", prompt_md.to_string())]);

    template.render(&variables).unwrap_or_else(|_| {
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
pub fn prompt_planning_xml_with_context(
    context: &TemplateContext,
    prompt_content: Option<&str>,
) -> String {
    let template_content = context
        .registry()
        .get_template("planning_xml")
        .unwrap_or_else(|_| include_str!("templates/planning_xml.txt").to_string());
    let template = Template::new(&template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([("PROMPT", prompt_md.to_string())]);

    template.render(&variables).unwrap_or_else(|_| {
        format!(
            "PLANNING MODE\n\nCreate an implementation plan for:\n\n{prompt_md}\n\n\
             Output format: <ralph-plan><ralph-summary>Summary</ralph-summary><ralph-implementation-steps>Steps</ralph-implementation-steps></ralph-plan>\n"
        )
    })
}

/// The XSD schema for plan validation - included at compile time
const PLAN_XSD_SCHEMA: &str = include_str!("../files/llm_output_extraction/plan.xsd");

/// Generate XSD validation retry prompt for planning with error feedback.
///
/// This prompt is used when an AI agent produces plan XML that fails XSD validation.
/// The prompt includes the error, the last output (for context), and the full XSD schema.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `_prompt_content` - Original user requirements (unused - kept for API compatibility)
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
pub fn prompt_planning_xsd_retry_with_context(
    context: &TemplateContext,
    _prompt_content: &str,
    xsd_error: &str,
    last_output: &str,
) -> String {
    let template_content = context
        .registry()
        .get_template("planning_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/planning_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        ("LAST_OUTPUT", last_output.to_string()),
        ("XSD_SCHEMA", PLAN_XSD_SCHEMA.to_string()),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "Your previous plan failed XSD validation.\n\nError: {}\n\n\
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
    let template_content = context
        .registry()
        .get_template("developer_iteration_xml")
        .unwrap_or_else(|_| include_str!("templates/developer_iteration_xml.txt").to_string());
    let template = Template::new(&template_content);
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
    ]);

    template.render(&variables).unwrap_or_else(|_| {
        format!(
            "IMPLEMENTATION MODE\n\nORIGINAL REQUEST:\n{prompt_content}\n\n\
             IMPLEMENTATION PLAN:\n{plan_content}\n\n\
             Output format: <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n"
        )
    })
}

/// Generate XSD validation retry prompt for developer iteration with error feedback.
///
/// This prompt is used when an AI agent produces development result XML that fails XSD validation.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
pub fn prompt_developer_iteration_xsd_retry_with_context(
    context: &TemplateContext,
    prompt_content: &str,
    plan_content: &str,
    xsd_error: &str,
    last_output: &str,
) -> String {
    let template_content = context
        .registry()
        .get_template("developer_iteration_xsd_retry")
        .unwrap_or_else(|_| {
            include_str!("templates/developer_iteration_xsd_retry.txt").to_string()
        });
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("XSD_ERROR", xsd_error.to_string()),
        ("LAST_OUTPUT", last_output.to_string()),
        ("XSD_SCHEMA", DEVELOPMENT_RESULT_XSD_SCHEMA.to_string()),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "Your previous development status failed XSD validation.\n\nError: {}\n\n\
                 Last output:\n{}\n\n\
                 Please resend your status in valid XML format:\n\
                 <ralph-development-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-development-result>\n",
                xsd_error, last_output
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
}
