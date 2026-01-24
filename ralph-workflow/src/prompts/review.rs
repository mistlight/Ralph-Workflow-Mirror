//! Review and fix result prompts with XML output format.
//!
//! Prompts for review and fix result generation using XML format with XSD validation.

use crate::files::llm_output_extraction::file_based_extraction::resolve_absolute_path;
use crate::prompts::template_context::TemplateContext;
use crate::prompts::template_engine::Template;
use std::collections::HashMap;

/// The XSD schema for issues validation - included at compile time
const ISSUES_XSD_SCHEMA: &str = include_str!("../files/llm_output_extraction/issues.xsd");

/// The XSD schema for fix result validation - included at compile time
const FIX_RESULT_XSD_SCHEMA: &str = include_str!("../files/llm_output_extraction/fix_result.xsd");

/// Directory for XSD retry context files
const XSD_RETRY_TMP_DIR: &str = ".agent/tmp";

/// Write XSD retry context files for review to `.agent/tmp/` directory.
fn write_review_xsd_retry_files(last_output: &str) {
    let tmp_dir = std::path::Path::new(XSD_RETRY_TMP_DIR);
    if std::fs::create_dir_all(tmp_dir).is_err() {
        return;
    }
    let _ = std::fs::write(tmp_dir.join("issues.xsd"), ISSUES_XSD_SCHEMA);
    let _ = std::fs::write(tmp_dir.join("last_output.xml"), last_output);
}

/// Write XSD retry context files for fix result to `.agent/tmp/` directory.
fn write_fix_xsd_retry_files(last_output: &str) {
    let tmp_dir = std::path::Path::new(XSD_RETRY_TMP_DIR);
    if std::fs::create_dir_all(tmp_dir).is_err() {
        return;
    }
    let _ = std::fs::write(tmp_dir.join("fix_result.xsd"), FIX_RESULT_XSD_SCHEMA);
    let _ = std::fs::write(tmp_dir.join("last_output.xml"), last_output);
}

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
        (
            "ISSUES_XML_PATH",
            resolve_absolute_path(".agent/tmp/issues.xml"),
        ),
        (
            "ISSUES_XSD_PATH",
            resolve_absolute_path(".agent/tmp/issues.xsd"),
        ),
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
/// The XSD schema and last output are written to files at `.agent/tmp/` to avoid
/// bloating the prompt. The agent should read these files.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `_prompt_content` - Original user requirements (unused - kept for API compatibility)
/// * `_plan_content` - Implementation plan (unused - kept for API compatibility)
/// * `_changes_content` - Description of changes made (unused - kept for API compatibility)
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
pub fn prompt_review_xsd_retry_with_context(
    context: &TemplateContext,
    _prompt_content: &str,
    _plan_content: &str,
    _changes_content: &str,
    xsd_error: &str,
    last_output: &str,
) -> String {
    // Write context files to .agent/tmp/ for the agent to read
    write_review_xsd_retry_files(last_output);

    let template_content = context
        .registry()
        .get_template("review_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/review_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        (
            "ISSUES_XML_PATH",
            resolve_absolute_path(".agent/tmp/issues.xml"),
        ),
        (
            "ISSUES_XSD_PATH",
            resolve_absolute_path(".agent/tmp/issues.xsd"),
        ),
        (
            "LAST_OUTPUT_XML_PATH",
            resolve_absolute_path(".agent/tmp/last_output.xml"),
        ),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "Your previous review failed XSD validation.\n\nError: {}\n\n\
                 Read .agent/tmp/issues.xsd for the schema and .agent/tmp/last_output.xml for your previous output.\n\
                 Please resend your review in valid XML format conforming to the XSD schema.\n",
                xsd_error
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
    files_to_modify: &[String],
) -> String {
    let template_content = context
        .registry()
        .get_template("fix_mode_xml")
        .unwrap_or_else(|_| include_str!("templates/fix_mode_xml.txt").to_string());
    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("ISSUES", issues_content.to_string()),
        ("FILES_TO_MODIFY", format_files_section_xml(files_to_modify)),
        (
            "FIX_RESULT_XML_PATH",
            resolve_absolute_path(".agent/tmp/fix_result.xml"),
        ),
        (
            "FIX_RESULT_XSD_PATH",
            resolve_absolute_path(".agent/tmp/fix_result.xsd"),
        ),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "FIX MODE\n\nFix the issues:\n\n{}\n\n\
                 Based on requirements:\n{}\n\nPlan:\n{}\n\n\
                 Output format: <ralph-fix-result><ralph-summary>Summary</ralph-summary><ralph-fixes-applied>Changes made</ralph-fixes-applied></ralph-fix-result>\n",
                issues_content, prompt_content, plan_content
            )
        })
}

/// Generate XSD validation retry prompt for fix with error feedback.
///
/// This prompt is used when an AI agent produces fix result XML that fails XSD validation.
/// The XSD schema and last output are written to files at `.agent/tmp/` to avoid
/// bloating the prompt. The agent should read these files.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `_issues_content` - Content of ISSUES.md (unused - kept for API compatibility)
/// * `xsd_error` - The XSD validation error message to include in the prompt
/// * `last_output` - The invalid XML output that failed validation
pub fn prompt_fix_xsd_retry_with_context(
    context: &TemplateContext,
    _issues_content: &str,
    xsd_error: &str,
    last_output: &str,
) -> String {
    // Write context files to .agent/tmp/ for the agent to read
    write_fix_xsd_retry_files(last_output);

    let template_content = context
        .registry()
        .get_template("fix_mode_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/fix_mode_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        (
            "FIX_RESULT_XML_PATH",
            resolve_absolute_path(".agent/tmp/fix_result.xml"),
        ),
        (
            "FIX_RESULT_XSD_PATH",
            resolve_absolute_path(".agent/tmp/fix_result.xsd"),
        ),
        (
            "LAST_OUTPUT_XML_PATH",
            resolve_absolute_path(".agent/tmp/last_output.xml"),
        ),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            format!(
                "Your previous fix failed XSD validation.\n\nError: {}\n\n\
                 Read .agent/tmp/fix_result.xsd for the schema and .agent/tmp/last_output.xml for your previous output.\n\
                 Please resend your fix in valid XML format conforming to the XSD schema.\n",
                xsd_error
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
        assert!(result.contains(".agent/tmp/issues.xml"));
        assert!(result.contains(".agent/tmp/issues.xsd"));
    }

    #[test]
    fn test_prompt_fix_xml_with_context() {
        let context = TemplateContext::default();
        let result =
            prompt_fix_xml_with_context(&context, "test prompt", "test plan", "test issues", &[]);
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
        assert!(result.contains(".agent/tmp/fix_result.xml"));
        assert!(result.contains(".agent/tmp/fix_result.xsd"));
    }
}
