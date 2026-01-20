//! Review and fix result prompts with XML output format.
//!
//! Prompts for review and fix result generation using XML format with XSD validation.

#![cfg_attr(any(test, feature = "test-utils"), allow(dead_code))]

use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::prompts::template_context::TemplateContext;
use crate::prompts::template_engine::Template;
use std::collections::HashMap;

/// Generate XML-based review prompt using template registry.
///
/// This version uses XML output format with XSD validation for reliable parsing.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - Original user requirements
/// * `plan_content` - Implementation plan
/// * `changes_content` - Description of changes made
#[cfg(any(test, feature = "test-utils"))]
pub fn prompt_review_xml_with_context(
    context: &TemplateContext,
    prompt_content: &str,
    plan_content: &str,
    changes_content: &str,
) -> String {
    let template_content = context
        .registry()
        .get_template("review_xml")
        .unwrap_or_else(|_| include_str!("templates/review_xml.txt").to_string());
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("CHANGES", changes_content.to_string()),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "REVIEW MODE\n\nReview the implementation against:\n\nRequirements:\n{}\n\nPlan:\n{}\n\nChanges:\n{}\n\n\
                 Output format: <ralph-issues><ralph-issue>[Severity] file:line - Description. Fix.</ralph-issue></ralph-issues>\n",
                prompt_content, plan_content, changes_content
            )
        })
}

/// Generate XSD validation retry prompt for review with error feedback.
///
/// This prompt is used when an AI agent produces review XML that fails XSD validation.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - Original user requirements
/// * `plan_content` - Implementation plan
/// * `changes_content` - Description of changes made
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
#[cfg(any(test, feature = "test-utils"))]
pub fn prompt_review_xsd_retry_with_context(
    context: &TemplateContext,
    prompt_content: &str,
    plan_content: &str,
    changes_content: &str,
    xsd_error: &str,
    last_output: &str,
) -> String {
    let template_content = context
        .registry()
        .get_template("review_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/review_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("CHANGES", changes_content.to_string()),
        ("XSD_ERROR", xsd_error.to_string()),
        ("LAST_OUTPUT", last_output.to_string()),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "Your previous review failed XSD validation.\n\nError: {}\n\n\
                 Last output:\n{}\n\n\
                 Please resend your review in valid XML format:\n\
                 <ralph-issues><ralph-issue>[Severity] file:line - Description. Fix.</ralph-issue></ralph-issues>\n",
                xsd_error, last_output
            )
        })
}

/// Format the list of files to modify for the fix mode prompt.
///
/// This function takes a list of file paths and formats them into a string
/// suitable for display in the fix mode prompt templates.
///
/// # Arguments
///
/// * `files` - Slice of file paths that may be modified
///
/// # Returns
///
/// A formatted string listing the files, or a message indicating no specific files were found.
fn format_files_section_xml(files: &[String]) -> String {
    if files.is_empty() {
        "No specific files identified - you may modify any files needed to fix the issues."
            .to_string()
    } else {
        format!("Files identified in issues:\n{}\n\nNOTE: If the issue references a file that is not listed here, you may still modify it.",
            files.iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n"))
    }
}

/// Generate XML-based fix prompt using template registry.
///
/// This version uses XML output format with XSD validation for reliable parsing.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - Content of PROMPT.md for context about the original request
/// * `plan_content` - Content of PLAN.md for context about the implementation plan
/// * `issues_content` - Content of ISSUES.md for context about issues to fix
pub fn prompt_fix_xml_with_context(
    context: &TemplateContext,
    prompt_content: &str,
    plan_content: &str,
    issues_content: &str,
) -> String {
    let template_content = context
        .registry()
        .get_template("fix_mode_xml")
        .unwrap_or_else(|_| include_str!("templates/fix_mode_xml.txt").to_string());

    // Extract file paths from ISSUES content to provide explicit list
    let files_to_modify = extract_file_paths_from_issues(issues_content);
    let files_section = format_files_section_xml(&files_to_modify);

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("ISSUES", issues_content.to_string()),
        ("FILES_TO_MODIFY", files_section),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "FIX MODE\n\nFix the issues:\n\n{}\n\nContext:\nPROMPT:\n{}\n\nPLAN:\n{}\n\n\
                 Output format: <ralph-fix-result><ralph-status>all_issues_addressed</ralph-status></ralph-fix-result>\n",
                issues_content, prompt_content, plan_content
            )
        })
}

/// Generate XSD validation retry prompt for fix with error feedback.
///
/// This prompt is used when an AI agent produces fix result XML that fails XSD validation.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `issues_content` - Content of ISSUES.md for context about issues to fix
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
#[cfg(any(test, feature = "test-utils"))]
pub fn prompt_fix_xsd_retry_with_context(
    context: &TemplateContext,
    issues_content: &str,
    xsd_error: &str,
    last_output: &str,
) -> String {
    let template_content = context
        .registry()
        .get_template("fix_mode_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/fix_mode_xsd_retry.txt").to_string());

    // Extract file paths from ISSUES content to provide explicit list
    let files_to_modify = extract_file_paths_from_issues(issues_content);
    let files_section = format_files_section_xml(&files_to_modify);

    let variables = HashMap::from([
        ("ISSUES", issues_content.to_string()),
        ("FILES_TO_MODIFY", files_section),
        ("XSD_ERROR", xsd_error.to_string()),
        ("LAST_OUTPUT", last_output.to_string()),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "Your completion status failed XSD validation.\n\nError: {}\n\n\
                 Last output:\n{}\n\n\
                 Please resend your status in valid XML format:\n\
                 <ralph-fix-result><ralph-status>all_issues_addressed</ralph-status></ralph-fix-result>\n",
                xsd_error, last_output
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_review_xml_with_context() {
        let context = TemplateContext::default();
        let result =
            prompt_review_xml_with_context(&context, "test prompt", "test plan", "test changes");
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(result.contains("test changes"));
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("<ralph-issues>"));
    }

    #[test]
    fn test_prompt_review_xsd_retry_with_context() {
        let context = TemplateContext::default();
        let result = prompt_review_xsd_retry_with_context(
            &context,
            "test prompt",
            "test plan",
            "test changes",
            "XSD error",
            "last output",
        );
        assert!(result.contains("XSD error"));
        assert!(result.contains("last output"));
        assert!(result.contains("<ralph-issues>"));
    }

    #[test]
    fn test_prompt_fix_xml_with_context() {
        let context = TemplateContext::default();
        let result =
            prompt_fix_xml_with_context(&context, "test prompt", "test plan", "test issues");
        assert!(result.contains("test issues"));
        assert!(result.contains("FIX MODE"));
        assert!(result.contains("<ralph-fix-result>"));
    }

    #[test]
    fn test_prompt_fix_xsd_retry_with_context() {
        let context = TemplateContext::default();
        let result =
            prompt_fix_xsd_retry_with_context(&context, "test issues", "XSD error", "last output");
        assert!(result.contains("XSD error"));
        assert!(result.contains("last output"));
        assert!(result.contains("<ralph-fix-result>"));
    }
}
