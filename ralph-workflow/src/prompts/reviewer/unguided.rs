//! Unguided reviewer prompts for general-purpose reviews.
//!
//! This module generates review prompts that do not include language-specific
//! guidelines. These are used when:
//!
//! - Stack detection did not identify a recognized language/framework
//! - A "fresh eyes" perspective is needed without framework-specific bias
//! - Reviewing general code quality, goal alignment, and acceptance criteria
//!
//! Available prompt types:
//! - **Simple review**: Minimal, vague prompt for unbiased perspective
//! - **Detailed review**: Actionable output with severity levels
//! - **Incremental review**: Focus only on recently changed files
//! - **Universal review**: Simplified prompt for agent compatibility

use super::super::types::ContextLevel;
use super::super::Template;
use std::collections::HashMap;

/// Load and render a template from a string with the given variables.
///
/// If template rendering fails, returns a minimal fallback prompt that still
/// includes the diff to ensure the review phase can proceed with content.
fn load_template_str(template_content: &str, diff: &str) -> String {
    let variables = HashMap::from([("DIFF", diff.to_string())]);
    let template = Template::new(template_content);
    match template.render(&variables) {
        Ok(rendered) => rendered,
        Err(e) => {
            // Fallback to a minimal prompt that still includes the diff
            // This ensures the review phase can proceed even if template rendering fails
            eprintln!("Warning: Failed to render template: {e}");
            format!(
                "Review the following code changes and identify any issues:\n\n```diff\n{diff}\n```"
            )
        }
    }
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
/// * `context` - The context level (minimal or normal)
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_detailed_review_without_guidelines_with_diff(
    context: ContextLevel,
    diff: &str,
) -> String {
    let template_content = match context {
        ContextLevel::Minimal => include_str!("templates/detailed_review_minimal.txt"),
        ContextLevel::Normal => include_str!("templates/detailed_review_normal.txt"),
    };
    load_template_str(template_content, diff)
}

/// Generate incremental review prompt with diff included directly.
///
/// This version receives the diff as a parameter instead of telling the agent
/// to run git commands. This keeps agents isolated from git operations.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
///
/// # Arguments
///
/// * `context` - The context level (minimal or normal)
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_incremental_review_with_diff(context: ContextLevel, diff: &str) -> String {
    let template_content = match context {
        ContextLevel::Minimal => include_str!("templates/incremental_review_minimal.txt"),
        ContextLevel::Normal => include_str!("templates/incremental_review_normal.txt"),
    };
    load_template_str(template_content, diff)
}

/// Generate a universal/simplified review prompt for maximum agent compatibility,
/// including the diff directly in the prompt.
///
/// This prompt is designed to work with a wide range of AI agents, including
/// those with weaker instruction-following capabilities. It:
/// - Uses simpler, more direct language
/// - Provides explicit output templates
/// - Minimizes complex structured instructions
/// - Includes the diff directly to keep agents isolated from git operations
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
///
/// # Arguments
///
/// * `context` - The context level (minimal or normal)
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_universal_review_with_diff(context: ContextLevel, diff: &str) -> String {
    let template_content = match context {
        ContextLevel::Minimal => include_str!("templates/universal_review_minimal.txt"),
        ContextLevel::Normal => include_str!("templates/universal_review_normal.txt"),
    };
    load_template_str(template_content, diff)
}
