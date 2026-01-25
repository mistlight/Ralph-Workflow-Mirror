//! Guided reviewer prompts with language-specific guidelines.
//!
//! This module generates review prompts that incorporate [`ReviewGuidelines`]
//! tailored to the detected project stack. The guidelines provide language and
//! framework-specific checks (e.g., Rust unsafe usage, React hooks rules, Django
//! CSRF protection) that help the reviewer focus on relevant concerns.
//!
//! Available prompt types:
//! - **Standard review**: Basic guideline integration
//! - **Comprehensive review**: Priority-ordered checks with severity levels
//! - **Security-focused review**: OWASP Top 10 combined with language-specific
//!   security checks

/// Generate reviewer review prompt with language-specific guidelines,
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
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
#[cfg(test)]
pub fn prompt_reviewer_review_with_guidelines_and_diff(
    _context: super::super::types::ContextLevel,
    guidelines: &crate::guidelines::ReviewGuidelines,
    diff: &str,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    use super::super::Template;
    use std::collections::HashMap;

    let guidelines_section = guidelines.format_for_prompt();
    // NOTE: ContextLevel is now ignored - we use the consolidated standard_review.txt
    // The "minimal" context concept has been deprecated as it provided no real value
    let template_content = include_str!("templates/standard_review.txt");

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff.to_string()),
        ("DIFF_CONTEXT", String::new()),
        ("GUIDELINES", guidelines_section),
        ("EXPLORATION_REQUIRED", String::new()),
        ("EXPLORATION_MODE", String::new()),
    ]);

    /// Load and render a template from a string with the given variables.
    fn load_template_str(template_content: &str, variables: &HashMap<&str, String>) -> String {
        let template = Template::new(template_content);
        match template.render(variables) {
            Ok(rendered) => rendered,
            Err(e) => {
                eprintln!("Warning: Failed to render template: {e}");
                String::new()
            }
        }
    }

    load_template_str(template_content, &variables)
}
