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

use super::super::types::ContextLevel;
use super::super::Template;
use crate::guidelines::ReviewGuidelines;
use std::collections::HashMap;

/// Load and render a template from a string with the given variables.
fn load_template_str(template_content: &str, variables: &HashMap<&str, String>) -> String {
    let template = Template::new(template_content.to_string());
    match template.render(variables) {
        Ok(rendered) => rendered,
        Err(e) => {
            // Fallback to empty string if template rendering fails
            eprintln!("Warning: Failed to render template: {e}");
            String::new()
        }
    }
}

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
/// * `context` - The context level (minimal or normal)
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_reviewer_review_with_guidelines_and_diff(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff: &str,
) -> String {
    let guidelines_section = guidelines.format_for_prompt();
    let template_content = match context {
        ContextLevel::Minimal => include_str!("templates/standard_review_minimal.txt"),
        ContextLevel::Normal => include_str!("templates/standard_review_normal.txt"),
    };

    let variables = HashMap::from([
        ("DIFF", diff.to_string()),
        ("GUIDELINES", guidelines_section),
    ]);

    load_template_str(template_content, &variables)
}

/// Generate comprehensive review prompt with priority-based guidelines,
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
/// * `context` - The context level (minimal or normal)
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_comprehensive_review_with_diff(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff: &str,
) -> String {
    let priority_section = guidelines.format_for_prompt_with_priorities();
    let template_content = match context {
        ContextLevel::Minimal => include_str!("templates/comprehensive_review_minimal.txt"),
        ContextLevel::Normal => include_str!("templates/comprehensive_review_normal.txt"),
    };

    let variables = HashMap::from([("DIFF", diff.to_string()), ("GUIDELINES", priority_section)]);

    load_template_str(template_content, &variables)
}

/// Generate security-focused review prompt with security-oriented guidelines,
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
/// * `context` - The context level (minimal or normal)
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_security_focused_review_with_diff(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff: &str,
) -> String {
    let security_section = guidelines.format_for_prompt();
    let template_content = match context {
        ContextLevel::Minimal => include_str!("templates/security_review_minimal.txt"),
        ContextLevel::Normal => include_str!("templates/security_review_normal.txt"),
    };

    let variables = HashMap::from([("DIFF", diff.to_string()), ("GUIDELINES", security_section)]);

    load_template_str(template_content, &variables)
}
