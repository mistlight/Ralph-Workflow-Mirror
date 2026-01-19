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

use super::super::partials::get_shared_partials;
use super::super::types::ContextLevel;
use super::super::Template;
use crate::prompts::template_context::TemplateContext;
use std::collections::HashMap;

/// Load and render a template from a string with the given variables.
///
/// Templates are embedded at compile time via `include_str!`, so any failure
/// indicates a programming error (missing template file or malformed template).
/// Returns a minimal fallback prompt on failure to ensure the review phase can proceed.
///
/// This version supports partials via the `{{> partial_name}}` syntax.
fn load_template_str(template_content: &str, variables: &HashMap<&str, String>) -> String {
    let template = Template::new(template_content);
    let partials = get_shared_partials();
    template
        .render_with_partials(variables, &partials)
        .unwrap_or_else(|_e| {
            // Fallback to minimal prompt that still includes the diff
            // This ensures the review phase can proceed even if template rendering fails
            let diff = variables.get("DIFF").map_or("", String::as_str);
            format!(
                "Review the following changes:\n\n{diff}\n\n\
             Provide feedback on any issues found."
            )
        })
}

/// Generate detailed reviewer review prompt without language-specific guidelines,
/// including the diff directly in the prompt.
///
/// This version receives the diff as a parameter instead of telling the agent
/// to run git commands. This keeps agents isolated from git operations and
/// ensures they only review the changes made since the pipeline started.
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
    // NOTE: ContextLevel is now ignored - we use the consolidated standard_review template
    // The "detailed_review" template has been deprecated as it relied on non-existent partials
    let template_content = include_str!("templates/standard_review.txt");
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff.to_string()),
        ("GUIDELINES", "".to_string()), // No guidelines for unguided review
    ]);
    load_template_str(template_content, &variables)
}

/// Generate detailed reviewer review prompt without language-specific guidelines,
/// including the diff directly in the prompt, using template registry.
///
/// This version uses the template registry which supports user template overrides.
///
/// # Arguments
///
/// * `template_context` - Template context containing the template registry
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `diff` - The git diff to review (changes since pipeline start)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
pub fn prompt_detailed_review_without_guidelines_with_diff_with_context(
    template_context: &TemplateContext,
    _context: ContextLevel,
    diff: &str,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    // NOTE: ContextLevel is now ignored - we use the consolidated standard_review template
    // The "detailed_review" template has been deprecated as it relied on non-existent partials
    let template_name = "standard_review";

    let tmpl_content = template_context
        .registry()
        .get_template(template_name)
        .unwrap_or_else(|_| include_str!("templates/standard_review.txt").to_string());

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff.to_string()),
        ("GUIDELINES", "".to_string()), // No guidelines for unguided review
    ]);
    load_template_str(&tmpl_content, &variables)
}

/// Generate incremental review prompt with diff included directly, using template registry.
///
/// DEPRECATED: Incremental review now uses the standard_review template.
/// The incremental concept was redundant with the existing baseline tracking.
///
/// # Arguments
///
/// * `template_context` - Template context containing the template registry
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `diff` - The git diff to review (changes since pipeline start)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
pub fn prompt_incremental_review_with_diff_with_context(
    template_context: &TemplateContext,
    _context: ContextLevel,
    diff: &str,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    // NOTE: ContextLevel is now ignored - we use the consolidated standard_review template
    // The "incremental_review" template has been deprecated - baseline tracking provides this functionality
    let template_name = "standard_review";

    let tmpl_content = template_context
        .registry()
        .get_template(template_name)
        .unwrap_or_else(|_| include_str!("templates/standard_review.txt").to_string());

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff.to_string()),
        ("GUIDELINES", "".to_string()), // No guidelines for incremental review
    ]);
    load_template_str(&tmpl_content, &variables)
}

/// Generate a universal/simplified review prompt for maximum agent compatibility,
/// including the diff directly in the prompt, using template registry.
///
/// This version uses the template registry which supports user template overrides.
///
/// # Arguments
///
/// * `template_context` - Template context containing the template registry
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `diff` - The git diff to review (changes since pipeline start)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
pub fn prompt_universal_review_with_diff_with_context(
    template_context: &TemplateContext,
    _context: ContextLevel,
    diff: &str,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    // NOTE: ContextLevel is now ignored - we use the consolidated universal_review template
    let template_name = "universal_review";

    let tmpl_content = template_context
        .registry()
        .get_template(template_name)
        .unwrap_or_else(|_| include_str!("templates/universal_review.txt").to_string());

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff.to_string()),
    ]);
    load_template_str(&tmpl_content, &variables)
}
