//! Unguided reviewer prompts for general-purpose reviews.
//!
//! This module generates review prompts that do not include language-specific
//! guidelines. These are used when:
//!
//! - Stack detection did not identify a recognized language/framework
//! - A "fresh eyes" perspective is needed without framework-specific bias
//! - Reviewing general code quality, goal alignment, and acceptance criteria
//! - Agent compatibility requires simplified prompts (e.g., GLM, ZhipuAI)
//!
//! Available prompt types:
//! - **Detailed review**: Uses standard_review template (when no guidelines available)
//! - **Incremental review**: DEPRECATED - now uses standard_review template
//! - **Universal review**: Simplified prompt for maximum agent compatibility

#[cfg(test)]
use super::super::types::ContextLevel;
#[cfg(test)]
use super::super::Template;

/// Generate detailed reviewer review prompt without language-specific guidelines,
/// including diff directly in prompt.
///
/// This version receives diff as a parameter instead of telling the agent
/// to run git commands. This keeps agents isolated from git operations and
/// ensures they only review changes made since the pipeline started.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
///
/// # Arguments
///
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `diff` - The git diff to review (changes since pipeline start)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
#[cfg(test)]
pub fn prompt_detailed_review_without_guidelines_with_diff(
    _context: ContextLevel,
    diff: &str,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    use crate::prompts::template_context::TemplateContext;
    use std::collections::HashMap;

    // NOTE: ContextLevel is now ignored - we use the consolidated standard_review template
    // The "detailed_review" template has been deprecated as it relied on non-existent partials
    let template_content = include_str!("templates/standard_review.txt");
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff.to_string()),
        ("DIFF_CONTEXT", String::new()),
        ("GUIDELINES", "".to_string()), // No guidelines for unguided review
        ("EXPLORATION_REQUIRED", String::new()),
        ("EXPLORATION_MODE", String::new()),
    ]);
    // Load template with provided variables
    let template = Template::new(template_content);
    let partials = super::super::partials::get_shared_partials();
    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_e| {
            // Fallback to minimal prompt that still includes the diff
            // This ensures that the review phase can proceed even if template rendering fails
            format!(
                "Review the following changes:\n\n{diff}\n\n\
             Provide feedback on any issues found."
            )
        })
}
