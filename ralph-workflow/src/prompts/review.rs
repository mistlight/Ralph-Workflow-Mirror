//! Review and fix result prompts with XML output format.
//!
//! Prompts for review and fix result generation using XML format with XSD validation.

use crate::files::llm_output_extraction::file_based_extraction::resolve_absolute_path;
use crate::prompts::partials::get_shared_partials;
use crate::prompts::template_context::TemplateContext;
use crate::prompts::template_engine::Template;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::Path;

/// The XSD schema for issues validation - included at compile time
const ISSUES_XSD_SCHEMA: &str = include_str!("../files/llm_output_extraction/issues.xsd");

/// The XSD schema for fix result validation - included at compile time
const FIX_RESULT_XSD_SCHEMA: &str = include_str!("../files/llm_output_extraction/fix_result.xsd");

/// Directory for XSD retry context files
const XSD_RETRY_TMP_DIR: &str = ".agent/tmp";

/// Write XSD retry context files for review to `.agent/tmp/` directory.
fn write_review_xsd_retry_files(workspace: &dyn Workspace, last_output: &str) {
    let tmp_dir = Path::new(XSD_RETRY_TMP_DIR);
    if workspace.create_dir_all(tmp_dir).is_err() {
        return;
    }
    let _ = workspace.write(&tmp_dir.join("issues.xsd"), ISSUES_XSD_SCHEMA);
    let _ = workspace.write(&tmp_dir.join("last_output.xml"), last_output);
}

/// Write XSD retry context files for fix result to `.agent/tmp/` directory.
fn write_fix_xsd_retry_files(workspace: &dyn Workspace, last_output: &str) {
    let tmp_dir = Path::new(XSD_RETRY_TMP_DIR);
    if workspace.create_dir_all(tmp_dir).is_err() {
        return;
    }
    let _ = workspace.write(&tmp_dir.join("fix_result.xsd"), FIX_RESULT_XSD_SCHEMA);
    let _ = workspace.write(&tmp_dir.join("last_output.xml"), last_output);
}

/// Generate XML-based review prompt using template registry.
///
/// This version uses XML output format with XSD validation for reliable parsing.
/// The reviewer is instructed to read `.agent/PROMPT.md.backup` directly for context
/// about the original requirements.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `_prompt_content` - Unused, kept for API compatibility. Reviewer reads PROMPT.md.backup directly.
/// * `plan_content` - Implementation plan
/// * `changes_content` - Description of changes made
pub fn prompt_review_xml_with_context(
    context: &TemplateContext,
    _prompt_content: &str,
    plan_content: &str,
    changes_content: &str,
) -> String {
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("review_xml")
        .unwrap_or_else(|_| include_str!("templates/review_xml.txt").to_string());
    let variables = HashMap::from([
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
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            format!(
                "REVIEW MODE\n\nReview the implementation against:\n\n\
                 Read `.agent/PROMPT.md.backup` for the original requirements (DO NOT modify it).\n\n\
                 Plan:\n{}\n\nChanges:\n{}\n\n\
                 Output format: <ralph-issues><ralph-issue>[Severity] file:line - Description. Fix.</ralph-issue></ralph-issues>\n",
                plan_content, changes_content
            )
        })
}

/// Generate review prompt with size-aware content references.
///
/// This version uses `PromptContentReferences` which automatically handles
/// oversized PLAN and DIFF content by referencing file paths or git commands.
///
/// Note: The reviewer is still instructed to read `.agent/PROMPT.md.backup` directly
/// for the original requirements.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `refs` - Content references for PLAN and CHANGES (DIFF)
pub fn prompt_review_xml_with_references(
    context: &TemplateContext,
    refs: &crate::prompts::content_builder::PromptContentReferences,
) -> String {
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("review_xml")
        .unwrap_or_else(|_| include_str!("templates/review_xml.txt").to_string());

    let variables = HashMap::from([
        ("PLAN", refs.plan_for_template()),
        ("CHANGES", refs.diff_for_template()),
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
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            let plan = refs.plan_for_template();
            let changes = refs.diff_for_template();
            format!("REVIEW MODE\n\nPLAN:\n{plan}\n\nCHANGES:\n{changes}\n")
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
/// * `workspace` - Workspace for writing XSD retry context files
pub fn prompt_review_xsd_retry_with_context(
    context: &TemplateContext,
    _prompt_content: &str,
    _plan_content: &str,
    _changes_content: &str,
    xsd_error: &str,
    last_output: &str,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    // Write context files to .agent/tmp/ for the agent to read
    write_review_xsd_retry_files(workspace, last_output);

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
        .render_with_partials(&variables, &partials)
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
    let partials = get_shared_partials();
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
        .render_with_partials(&variables, &partials)
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
/// * `workspace` - Workspace for writing XSD retry context files
pub fn prompt_fix_xsd_retry_with_context(
    context: &TemplateContext,
    _issues_content: &str,
    xsd_error: &str,
    last_output: &str,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    // Write context files to .agent/tmp/ for the agent to read
    write_fix_xsd_retry_files(workspace, last_output);

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
        .render_with_partials(&variables, &partials)
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
    use crate::workspace::MemoryWorkspace;

    #[test]
    fn test_prompt_review_xml_with_context() {
        let context = TemplateContext::default();
        let result =
            prompt_review_xml_with_context(&context, "test prompt", "test plan", "test changes");
        // prompt_content is no longer embedded - reviewer reads PROMPT.md.backup directly
        assert!(!result.contains("test prompt"));
        assert!(result.contains("PROMPT.md.backup"));
        assert!(result.contains("test plan"));
        assert!(result.contains("test changes"));
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("<ralph-issues>"));

        // Read-only modes: reviewer must still write exactly one XML file.
        assert!(
            result.contains("explicitly authorized") && result.contains("EXACTLY ONE file"),
            "review_xml should explicitly authorize writing exactly one XML file"
        );
        assert!(
            result.contains("MANDATORY"),
            "review_xml should mark XML file write mandatory"
        );
        assert!(
            result.contains("Not writing") && result.contains("FAILURE"),
            "review_xml should say not writing XML is a failure"
        );
        assert!(
            result.contains("does not conform")
                && result.contains("XSD")
                && result.contains("FAILURE"),
            "review_xml should say non-XSD XML is a failure"
        );
        assert!(
            result.contains("READ-ONLY")
                && (result.contains("EXCEPT FOR writing")
                    || result.contains("except for writing")
                    || result.contains("Except for writing"))
                && result.contains("issues.xml"),
            "review_xml should be read-only except for writing issues.xml"
        );

        assert!(
            !result.contains("DO NOT print")
                && !result.contains("Do NOT print")
                && !result.contains("ONLY acceptable output")
                && !result.contains("The ONLY acceptable output"),
            "review_xml should not include stdout suppression wording"
        );

        // Shared partials should be expanded (no raw partial directives left in output)
        assert!(
            result.contains("*** UNATTENDED MODE - NO USER INTERACTION ***"),
            "review_xml should render shared/_unattended_mode partial"
        );
        assert!(
            !result.contains("{{>"),
            "review_xml should not contain raw partial directives"
        );
    }

    #[test]
    fn test_prompt_review_xml_with_context_allows_empty_plan_and_changes() {
        let context = TemplateContext::default();
        let result = prompt_review_xml_with_context(&context, "prompt", "", "");

        assert!(
            !result.contains("{{PLAN}}"),
            "review prompt must not contain unresolved {{PLAN}} placeholder"
        );
        assert!(
            !result.contains("{{CHANGES}}"),
            "review prompt must not contain unresolved {{CHANGES}} placeholder"
        );
        assert!(
            result.contains("(no plan available)"),
            "review prompt should include a default when plan content is empty"
        );
        assert!(
            result.contains("(no diff available)"),
            "review prompt should include a default when changes/diff content is empty"
        );
    }

    #[test]
    fn test_prompt_review_xml_with_context_uses_inline_plan_and_changes_when_present() {
        let context = TemplateContext::default();
        let result = prompt_review_xml_with_context(&context, "prompt", "plan here", "diff here");

        assert!(result.contains("plan here"));
        assert!(result.contains("diff here"));

        assert!(
            !result.contains("(no plan available)"),
            "default plan text should not appear when plan is present"
        );
        assert!(
            !result.contains("(no diff available)"),
            "default diff text should not appear when diff is present"
        );
    }

    #[test]
    fn test_prompt_review_xsd_retry_with_context() {
        let context = TemplateContext::default();
        let workspace = MemoryWorkspace::new_test();
        let result = prompt_review_xsd_retry_with_context(
            &context,
            "test prompt",
            "test plan",
            "test changes",
            "XSD error",
            "last output",
            &workspace,
        );
        assert!(result.contains("XSD error"));
        assert!(result.contains(".agent/tmp/issues.xml"));
        assert!(result.contains(".agent/tmp/issues.xsd"));

        // Read-only modes: reviewer must still write exactly one XML file.
        assert!(
            result.contains("explicitly authorized") && result.contains("EXACTLY ONE file"),
            "review_xsd_retry should explicitly authorize writing exactly one XML file"
        );
        assert!(
            result.contains("MANDATORY"),
            "review_xsd_retry should mark XML file write mandatory"
        );
        assert!(
            result.contains("Not writing") && result.contains("FAILURE"),
            "review_xsd_retry should say not writing XML is a failure"
        );
        assert!(
            result.contains("does not conform")
                && result.contains("XSD")
                && result.contains("FAILURE"),
            "review_xsd_retry should say non-XSD XML is a failure"
        );
        assert!(
            result.contains("READ-ONLY")
                && (result.contains("EXCEPT FOR writing")
                    || result.contains("except for writing")
                    || result.contains("Except for writing"))
                && result.contains("issues.xml"),
            "review_xsd_retry should be read-only except for writing issues.xml"
        );

        assert!(
            !result.contains("DO NOT print")
                && !result.contains("Do NOT print")
                && !result.contains("ONLY acceptable output")
                && !result.contains("The ONLY acceptable output"),
            "review_xsd_retry should not include stdout suppression wording"
        );

        // Shared partials should be expanded
        assert!(
            result.contains("*** UNATTENDED MODE - NO USER INTERACTION ***"),
            "review_xsd_retry should render shared/_unattended_mode partial"
        );
        assert!(
            !result.contains("{{>"),
            "review_xsd_retry should not contain raw partial directives"
        );

        // Verify files were written to workspace
        assert!(workspace.was_written(".agent/tmp/issues.xsd"));
        assert!(workspace.was_written(".agent/tmp/last_output.xml"));
    }

    #[test]
    fn test_prompt_fix_xml_with_context() {
        let context = TemplateContext::default();
        let result =
            prompt_fix_xml_with_context(&context, "test prompt", "test plan", "test issues", &[]);
        assert!(result.contains("test issues"));
        assert!(result.contains("FIX MODE"));
        assert!(result.contains("<ralph-fix-result>"));

        // Shared partials should be expanded
        assert!(
            result.contains("*** UNATTENDED MODE - NO USER INTERACTION ***"),
            "fix_mode_xml should render shared/_unattended_mode partial"
        );
        assert!(
            !result.contains("{{>"),
            "fix_mode_xml should not contain raw partial directives"
        );
    }

    #[test]
    fn test_prompt_fix_xsd_retry_with_context() {
        let context = TemplateContext::default();
        let workspace = MemoryWorkspace::new_test();
        let result = prompt_fix_xsd_retry_with_context(
            &context,
            "test issues",
            "XSD error",
            "last output",
            &workspace,
        );
        assert!(result.contains("XSD error"));
        assert!(result.contains(".agent/tmp/fix_result.xml"));
        assert!(result.contains(".agent/tmp/fix_result.xsd"));
        // Verify files were written to workspace
        assert!(workspace.was_written(".agent/tmp/fix_result.xsd"));
        assert!(workspace.was_written(".agent/tmp/last_output.xml"));
    }

    // =========================================================================
    // Tests for _with_references variants
    // =========================================================================

    #[test]
    fn test_prompt_review_xml_with_references_small_content() {
        use crate::prompts::content_builder::PromptContentBuilder;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();

        let refs = PromptContentBuilder::new(&workspace)
            .with_plan("Small plan content".to_string())
            .with_diff("Small diff content".to_string(), "abc123")
            .build();

        let result = prompt_review_xml_with_references(&context, &refs);

        // Should embed content inline
        assert!(result.contains("Small plan content"));
        assert!(result.contains("Small diff content"));
        assert!(result.contains("REVIEW MODE"));
    }

    #[test]
    fn test_prompt_review_xml_with_references_large_plan() {
        use crate::prompts::content_builder::PromptContentBuilder;
        use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();
        let large_plan = "p".repeat(MAX_INLINE_CONTENT_SIZE + 1);

        let refs = PromptContentBuilder::new(&workspace)
            .with_plan(large_plan)
            .with_diff("Small diff".to_string(), "abc123")
            .build();

        let result = prompt_review_xml_with_references(&context, &refs);

        // Should reference PLAN.md file, not embed content
        assert!(result.contains(".agent/PLAN.md"));
        assert!(result.contains("plan.xml"));
        assert!(result.contains("Small diff"));
    }

    #[test]
    fn test_prompt_review_xml_with_references_large_diff() {
        use crate::prompts::content_builder::PromptContentBuilder;
        use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();
        let large_diff = "d".repeat(MAX_INLINE_CONTENT_SIZE + 1);

        let refs = PromptContentBuilder::new(&workspace)
            .with_plan("Small plan".to_string())
            .with_diff(large_diff, "abc123def")
            .build();

        let result = prompt_review_xml_with_references(&context, &refs);

        // Should instruct to use git diff fallback commands, not embed content
        assert!(result.contains("git diff abc123def"));
        assert!(result.contains("git diff --cached abc123def"));
        assert!(result.contains("Small plan"));
    }

    #[test]
    fn test_prompt_review_xml_with_references_both_large() {
        use crate::prompts::content_builder::PromptContentBuilder;
        use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;

        let workspace = MemoryWorkspace::new_test();
        let context = TemplateContext::default();
        let large_plan = "p".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let large_diff = "d".repeat(MAX_INLINE_CONTENT_SIZE + 1);

        let refs = PromptContentBuilder::new(&workspace)
            .with_plan(large_plan)
            .with_diff(large_diff, "start123")
            .build();

        let result = prompt_review_xml_with_references(&context, &refs);

        // Both should be referenced by file/git command
        assert!(result.contains(".agent/PLAN.md"));
        assert!(result.contains("git diff start123"));
        assert!(result.contains("git diff --cached start123"));
        // Should not contain the large content
        let pppp = "p".repeat(100);
        assert!(!result.contains(&pppp));
    }
}
