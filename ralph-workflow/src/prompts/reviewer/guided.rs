#![allow(dead_code)]
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
use crate::git_helpers::{DiffReviewContent, DiffTruncationLevel};
use crate::guidelines::ReviewGuidelines;
use crate::prompts::template_context::TemplateContext;
use std::collections::HashMap;

/// Load and render a template from a string with the given variables.
fn load_template_str(template_content: &str, variables: &HashMap<&str, String>) -> String {
    let template = Template::new(template_content);
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
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
#[cfg(test)]
pub fn prompt_reviewer_review_with_guidelines_and_diff(
    _context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff: &str,
    prompt_content: &str,
    plan_content: &str,
) -> String {
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

    load_template_str(template_content, &variables)
}

/// Generate reviewer review prompt with language-specific guidelines,
/// including the diff directly in the prompt, using template registry.
///
/// This version uses the template registry which supports user template overrides.
///
/// # Arguments
///
/// * `template_context` - Template context containing the template registry
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `guidelines` - The language-specific review guidelines
/// * `diff_content` - The diff content with truncation metadata
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
pub fn prompt_reviewer_review_with_guidelines_and_diff_with_context(
    template_context: &TemplateContext,
    _context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff_content: &DiffReviewContent,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    let guidelines_section = guidelines.format_for_prompt();
    // NOTE: ContextLevel is now ignored - we use the consolidated standard_review template
    // The "minimal" context concept has been deprecated as it provided no real value
    let template_name = "standard_review";

    let tmpl_content = template_context
        .registry()
        .get_template(template_name)
        .unwrap_or_else(|_| include_str!("templates/standard_review.txt").to_string());

    // Build exploration instruction text
    let exploration_instruction = build_exploration_instruction(diff_content);

    // Build diff context header
    let diff_context = diff_content.format_context_header();

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff_content.content.clone()),
        ("DIFF_CONTEXT", diff_context),
        ("GUIDELINES", guidelines_section),
        ("EXPLORATION_REQUIRED", exploration_instruction),
        (
            "EXPLORATION_MODE",
            if diff_content.truncation_level != DiffTruncationLevel::Full {
                "true".to_string()
            } else {
                String::new()
            },
        ),
    ]);

    load_template_str(&tmpl_content, &variables)
}

/// Generate comprehensive review prompt with priority-based guidelines,
/// including the diff directly in the prompt, using template registry.
///
/// This version uses the template registry which supports user template overrides.
///
/// # Arguments
///
/// * `template_context` - Template context containing the template registry
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `guidelines` - The language-specific review guidelines
/// * `diff_content` - The diff content with truncation metadata
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
pub fn prompt_comprehensive_review_with_diff_with_context(
    template_context: &TemplateContext,
    _context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff_content: &DiffReviewContent,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    let priority_section = guidelines.format_for_prompt_with_priorities();
    // NOTE: ContextLevel is now ignored - we use the consolidated comprehensive_review template
    let template_name = "comprehensive_review";

    let tmpl_content = template_context
        .registry()
        .get_template(template_name)
        .unwrap_or_else(|_| include_str!("templates/comprehensive_review.txt").to_string());

    // Build exploration instruction text
    let exploration_instruction = build_exploration_instruction(diff_content);

    // Build diff context header
    let diff_context = diff_content.format_context_header();

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff_content.content.clone()),
        ("DIFF_CONTEXT", diff_context),
        ("GUIDELINES", priority_section),
        ("EXPLORATION_REQUIRED", exploration_instruction),
        (
            "EXPLORATION_MODE",
            if diff_content.truncation_level != DiffTruncationLevel::Full {
                "true".to_string()
            } else {
                String::new()
            },
        ),
    ]);

    load_template_str(&tmpl_content, &variables)
}

/// Generate security-focused review prompt with security-oriented guidelines,
/// including the diff directly in the prompt, using template registry.
///
/// This version uses the template registry which supports user template overrides.
///
/// # Arguments
///
/// * `template_context` - Template context containing the template registry
/// * `context` - The context level (minimal or normal) - NOTE: Now treated as normal
/// * `guidelines` - The language-specific review guidelines
/// * `diff_content` - The diff content with truncation metadata
/// * `prompt_content` - The original user request (PROMPT.md content)
/// * `plan_content` - The implementation plan (.agent/PLAN.md content)
pub fn prompt_security_focused_review_with_diff_with_context(
    template_context: &TemplateContext,
    _context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff_content: &DiffReviewContent,
    prompt_content: &str,
    plan_content: &str,
) -> String {
    let security_section = guidelines.format_for_prompt();
    // NOTE: ContextLevel is now ignored - we use the consolidated security_review template
    let template_name = "security_review";

    let tmpl_content = template_context
        .registry()
        .get_template(template_name)
        .unwrap_or_else(|_| include_str!("templates/security_review.txt").to_string());

    // Build exploration instruction text
    let exploration_instruction = build_exploration_instruction(diff_content);

    // Build diff context header
    let diff_context = diff_content.format_context_header();

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("DIFF", diff_content.content.clone()),
        ("DIFF_CONTEXT", diff_context),
        ("GUIDELINES", security_section),
        ("EXPLORATION_REQUIRED", exploration_instruction),
        (
            "EXPLORATION_MODE",
            if diff_content.truncation_level != DiffTruncationLevel::Full {
                "true".to_string()
            } else {
                String::new()
            },
        ),
    ]);

    load_template_str(&tmpl_content, &variables)
}

/// Build exploration instruction text based on truncation level.
pub fn build_exploration_instruction(diff_content: &DiffReviewContent) -> String {
    match diff_content.truncation_level {
        DiffTruncationLevel::Full => String::new(),
        DiffTruncationLevel::Abbreviated => format!(
            "[DIFF ABBREVIATED: {}/{} files shown. You MUST explore the full diff using 'git diff HEAD' to review properly.]",
            diff_content.shown_file_count.unwrap_or(0),
            diff_content.total_file_count
        ),
        DiffTruncationLevel::FileList => format!(
            "[FILE LIST ONLY: {} files changed. You MUST explore each file's diff using 'git diff HEAD -- <file>' to review properly.]",
            diff_content.total_file_count
        ),
        DiffTruncationLevel::FileListAbbreviated => format!(
            "[FILE LIST ABBREVIATED: {}/{} files shown. You MUST run 'git status' to find all files and explore their diffs.]",
            diff_content.shown_file_count.unwrap_or(0),
            diff_content.total_file_count
        ),
    }
}
