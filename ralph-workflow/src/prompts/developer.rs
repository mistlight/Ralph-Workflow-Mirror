//! Developer prompts.
//!
//! Prompts for developer agent actions including iteration and planning.

use std::collections::HashMap;

use super::template_engine::Template;
use super::types::ContextLevel;

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

    let template_content = include_str!("templates/developer_iteration.txt");
    let template = Template::new(template_content);
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
    ]);

    template.render(&variables).unwrap_or_else(|_| {
        // Use fallback template if main template fails
        let fallback_content = include_str!("templates/developer_iteration_fallback.txt");
        let fallback_template = Template::new(fallback_content);
        fallback_template
            .render(&variables)
            .unwrap_or_else(|_| {
                // Last resort emergency fallback
                format!(
                    "IMPLEMENTATION MODE\n\nORIGINAL REQUEST:\n{prompt_content}\n\nIMPLEMENTATION PLAN:\n{plan_content}\n\nExecute the next steps from the plan above.\n"
                )
            })
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
pub fn prompt_plan(prompt_content: Option<&str>) -> String {
    let template_content = include_str!("templates/planning.txt");
    let template = Template::new(template_content);
    let prompt_md = prompt_content.unwrap_or("No requirements provided");
    let variables = HashMap::from([("PROMPT", prompt_md.to_string())]);

    template.render(&variables).unwrap_or_else(|_| {
        // Use fallback template if main template fails
        let fallback_content = include_str!("templates/planning_fallback.txt");
        let fallback_template = Template::new(fallback_content);
        fallback_template
            .render(&variables)
            .unwrap_or_else(|_| {
                // Last resort emergency fallback
                format!(
                    "PLANNING MODE\n\nCreate an implementation plan for:\n\n{prompt_md}\n\nIdentify critical files and implementation steps.\n"
                )
            })
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
        // Plan is now returned as structured output, not written to file
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("Implementation Steps"));
        assert!(result.contains("Critical Files"));
        assert!(result.contains("Verification Strategy"));

        // Ensure strict read-only constraints are present (Claude Code alignment)
        assert!(result.contains("READ-ONLY"));
        assert!(result.contains("STRICTLY PROHIBITED"));

        // Ensure 5-phase workflow structure (Claude Code alignment)
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        assert!(result.contains("PHASE 2: EXPLORATION"));
        assert!(result.contains("PHASE 3: DESIGN"));
        assert!(result.contains("PHASE 4: REVIEW"));
        assert!(result.contains("PHASE 5: WRITE PLAN"));
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
}
