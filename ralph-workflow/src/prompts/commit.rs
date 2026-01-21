//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

#![cfg_attr(any(test, feature = "test-utils"), allow(dead_code))]

use crate::prompts::template_context::TemplateContext;
use crate::prompts::template_engine::Template;
use std::collections::HashMap;

#[cfg(any(test, feature = "test-utils"))]
use crate::files::result_extraction::extract_file_paths_from_issues;

/// The XSD schema for commit message validation - included at compile time
const COMMIT_MESSAGE_XSD_SCHEMA: &str =
    include_str!("../files/llm_output_extraction/commit_message.xsd");

/// Generate fix prompt (applies to either role).
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Uses a template-based approach for consistency with review prompts.
///
/// # Agent-Orchestrator Separation
///
/// The fix agent receives ISSUES content (embedded by the orchestrator after extracting
/// from the reviewer's JSON output) and modifies source code files to fix issues.
/// The agent returns structured output (completion status) that the orchestrator
/// captures via JSON logging.
///
/// ISSUES.md is an orchestrator-managed file - the agent should NOT modify it.
/// The orchestrator writes ISSUES.md before invoking the fix agent and may
/// delete it after fix cycles (e.g., in isolation mode).
///
/// # Constraints
///
/// The fix agent is constrained to ONLY work on files mentioned in the ISSUES content.
/// This prevents the agent from exploring the repository and keeps changes
/// focused on the issues identified during review.
///
/// # Arguments
///
/// * `prompt_content` - Content of PROMPT.md for context about the original request
/// * `plan_content` - Content of PLAN.md for context about the implementation plan
/// * `issues_content` - Content of ISSUES.md for context about issues to fix
#[cfg(test)]
pub fn prompt_fix(prompt_content: &str, plan_content: &str, issues_content: &str) -> String {
    let template_content = include_str!("templates/fix_mode_xml.txt");

    // Extract file paths from ISSUES content to provide explicit list
    let files_to_modify = extract_file_paths_from_issues(issues_content);
    let files_section = format_files_section(&files_to_modify);

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("ISSUES", issues_content.to_string()),
        ("FILES_TO_MODIFY", files_section),
    ]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            // Embedded fallback template (XML format)
            format!(
                "FIX MODE\n\nRead .agent/ISSUES.md and fix the issues found.\n\nContext:\nPROMPT:\n{prompt_content}\n\nPLAN:\n{plan_content}\n\nOutput format: <ralph-fix-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-fix-result>\n"
            )
        })
}

/// Generate fix prompt using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It's the recommended way to generate prompts going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - Content of PROMPT.md for context about the original request
/// * `plan_content` - Content of PLAN.md for context about the implementation plan
/// * `issues_content` - Content of ISSUES.md for context about issues to fix
#[cfg(any(test, feature = "test-utils"))]
pub fn prompt_fix_with_context(
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
    let files_section = format_files_section(&files_to_modify);

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("ISSUES", issues_content.to_string()),
        ("FILES_TO_MODIFY", files_section),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            // Embedded fallback template (XML format)
            format!(
                "FIX MODE\n\nRead .agent/ISSUES.md and fix the issues found.\n\nContext:\nPROMPT:\n{prompt_content}\n\nPLAN:\n{plan_content}\n\nOutput format: <ralph-fix-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-fix-result>\n"
            )
        })
}

/// Format the files section for the fix prompt.
///
/// If files are found, formats them as a bulleted list with a clear header.
/// If no files are found, provides a fallback message indicating that the
/// agent may work on any files in the repository to fix the issues.
#[cfg(any(test, feature = "test-utils"))]
fn format_files_section(files: &[String]) -> String {
    if files.is_empty() {
        "================================================================================
FILES YOU MAY MODIFY
================================================================================

(No specific files were extracted from ISSUES content)

PERMISSIONS: FULL AUTO MODE - You may work on ANY files in the repository

You are authorized to modify any files in the repository that are needed to fix
the issues described in the ISSUES content above. Use your judgment to determine
which files need modification - you are not limited to files mentioned in ISSUES.

The ISSUES content is already embedded in this prompt - review it carefully.

================================================================================
END OF FILES SECTION
================================================================================
"
        .to_string()
    } else {
        let mut result = String::from(
            "================================================================================
FILES YOU MAY MODIFY
================================================================================

",
        );
        for file in files {
            result.push_str("- ");
            result.push_str(file);
            result.push('\n');
        }
        // Add explicit clarification that agent doesn't need to read any ISSUES file
        result.push_str(
            "
IMPORTANT: Work ONLY with the files listed above. The issues
content is already embedded in this prompt - you do NOT need to
read or discover any files to know what to fix.

================================================================================
END OF FILES SECTION
================================================================================
",
        );
        result
    }
}

/// Generate prompt for creating commit message from provided diff.
///
/// This is used by the orchestrator (not agents) to generate commit messages.
/// The diff is provided directly in the prompt, so the LLM doesn't need to
/// run git commands or access files.
///
/// Uses the XML-based template format for output, which is more reliable than JSON
/// because:
/// - No escape sequence issues (actual newlines work fine in XML)
/// - Distinctive tags (`<ralph-commit>`) unlikely to appear in LLM analysis
/// - Clear boundaries for parsing
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message for. If empty or
///   whitespace-only, the prompt will indicate no changes were detected.
///
/// # Note
///
/// This function includes a defensive check for empty diffs - if an empty diff
/// is passed, it returns an error prompt that will fail validation in the caller
/// and trigger fallback commit message generation. Callers should still check for
/// meaningful changes before calling this function for efficiency.
#[cfg(test)]
pub fn prompt_generate_commit_message_with_diff(diff: &str) -> String {
    // Check if diff is empty or whitespace-only
    let diff_content = diff.trim();
    let has_changes = !diff_content.is_empty();

    if !has_changes {
        // Return an error message instead of a placeholder
        // This will be caught by validation in commit_with_auto_message
        // and trigger fallback commit message generation
        return "ERROR: Empty diff provided. This indicates a bug in the caller - \
                meaningful changes should be checked before requesting a commit message."
            .to_string();
    }

    let template_content = include_str!("templates/commit_message_xml.txt");
    let template = Template::new(template_content);
    let variables = HashMap::from([("DIFF", diff_content.to_string())]);

    template.render(&variables).unwrap_or_else(|e| {
        eprintln!("Warning: Failed to render commit template: {e}");
        // Last resort: simple inline prompt (no fallback template needed)
        format!(
            "Generate a conventional commit message for this diff:\n\n{diff_content}\n\n\
             Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>"
        )
    })
}

/// Generate prompt for creating commit message from provided diff using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It's the recommended way to generate prompts going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `diff` - The git diff to generate a commit message for
pub fn prompt_generate_commit_message_with_diff_with_context(
    context: &TemplateContext,
    diff: &str,
) -> String {
    // Check if diff is empty or whitespace-only
    let diff_content = diff.trim();
    let has_changes = !diff_content.is_empty();

    if !has_changes {
        return "ERROR: Empty diff provided. This indicates a bug in the caller - \
                meaningful changes should be checked before requesting a commit message."
            .to_string();
    }

    let template_content = context
        .registry()
        .get_template("commit_message_xml")
        .unwrap_or_else(|_| include_str!("templates/commit_message_xml.txt").to_string());
    let template = Template::new(&template_content);
    let variables = HashMap::from([("DIFF", diff_content.to_string())]);

    template.render(&variables).unwrap_or_else(|e| {
        eprintln!("Warning: Failed to render commit template: {e}");
        // Last resort: simple inline prompt (no fallback template needed)
        format!(
            "Generate a conventional commit message for this diff:\n\n{diff_content}\n\n\
             Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>"
        )
    })
}

/// Generate simplified commit prompt using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It provides a more direct and concise version of the commit prompt.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `diff` - The git diff to generate a commit message for
pub fn prompt_simplified_commit_with_context(context: &TemplateContext, diff: &str) -> String {
    let template_content = context
        .registry()
        .get_template("commit_simplified")
        .unwrap_or_else(|_| include_str!("templates/commit_simplified.txt").to_string());
    let variables = HashMap::from([("DIFF", diff.trim().to_string())]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            // Fallback to simple prompt with diff if template rendering fails
            format!(
                "Generate a commit message for this diff:\n\n{}\n\n\
                 Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>",
                diff.trim()
            )
        })
}

/// Generate XSD validation retry prompt with error feedback.
///
/// This prompt is used when an AI agent produces XML that fails XSD validation.
/// It provides clear, actionable error feedback to help the agent fix the issue.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `diff` - The git diff to generate a commit message for
/// * `xsd_error` - The XSD validation error message to include in the prompt
pub fn prompt_xsd_retry_with_context(
    context: &TemplateContext,
    diff: &str,
    xsd_error: &str,
) -> String {
    let template_content = context
        .registry()
        .get_template("commit_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/commit_xsd_retry.txt").to_string());
    let variables = std::collections::HashMap::from([
        ("DIFF", diff.to_string()),
        ("XSD_ERROR", xsd_error.to_string()),
        ("XSD_SCHEMA", COMMIT_MESSAGE_XSD_SCHEMA.to_string()),
    ]);
    Template::new(&template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            // Fallback to simple retry prompt if template rendering fails
            format!(
                "Your previous commit message failed validation.\n\nError: {}\n\n\
                 Please fix it and output a valid commit message in XML format:\n\
                 <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>\n\n\
                 Diff:\n{}",
                xsd_error, diff
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_fix() {
        let result = prompt_fix(
            "test prompt content",
            "test plan content",
            "test issues content",
        );
        assert!(result.contains("test issues content"));
        // Agent should NOT modify the ISSUES content - it is provided for reference only
        assert!(result.contains("MUST NOT modify the ISSUES content"));
        assert!(result.contains("provided for reference only"));
        // Agent SHOULD modify source code files to fix issues
        assert!(result.contains("MAY modify"));
        assert!(result.contains("FIX MODE"));
        // Agent should return status as XML output
        assert!(result.contains("<ralph-fix-result>"));
        assert!(result.contains("<ralph-status>"));
        assert!(result.contains("all_issues_addressed"));
        assert!(result.contains("issues_remain"));
        // Should include PROMPT and PLAN context
        assert!(result.contains("test prompt content"));
        assert!(result.contains("test plan content"));
    }

    #[test]
    fn test_prompt_fix_with_empty_context() {
        let result = prompt_fix("", "", "");
        assert!(result.contains("FIX MODE"));
        // Should still render successfully with empty context
        assert!(!result.is_empty());
    }

    #[test]
    fn test_notes_md_references_are_minimal_or_absent() {
        // NOTES.md references should be minimal or absent (isolation mode removes these files)
        let fix_prompt = prompt_fix("", "", "");

        // Fix prompt may have optional language or no reference
        // It uses "(if it exists)" when referencing NOTES.md
        if fix_prompt.contains("NOTES.md") {
            assert!(
                fix_prompt.contains("if it exists") || fix_prompt.contains("Optionally"),
                "Fix prompt NOTES.md reference should be optional"
            );
        }
    }

    #[test]
    fn test_fix_prompt_contains_constraint_language() {
        // Verify that fix prompt contains explicit constraint language
        let fix_prompt = prompt_fix("", "", "");
        assert!(
            fix_prompt.contains("MUST NOT") || fix_prompt.contains("DO NOT"),
            "Fix prompt should contain explicit constraint language (MUST NOT or DO NOT)"
        );
        assert!(
            fix_prompt.contains("CRITICAL CONSTRAINTS"),
            "Fix prompt should contain a CRITICAL CONSTRAINTS section"
        );
    }

    #[test]
    fn test_fix_prompt_forbids_exploration() {
        // Verify that fix prompt explicitly forbids repository exploration
        let fix_prompt = prompt_fix("", "", "");
        assert!(
            fix_prompt.contains("MUST NOT modify the ISSUES content")
                || fix_prompt.contains("LIMITEDLY")
                || fix_prompt.contains("stop exploring"),
            "Fix prompt should explicitly forbid unbounded exploration or limit it"
        );
    }

    #[test]
    fn test_fix_prompt_instructs_to_only_work_on_issues_files() {
        // Verify that fix prompt instructs to only work on files from ISSUES
        let fix_prompt = prompt_fix("", "", "test issues");
        assert!(
            fix_prompt.contains("test issues"),
            "Fix prompt should contain the embedded issues content"
        );
        assert!(
            fix_prompt.contains("ONLY") || fix_prompt.contains("only"),
            "Fix prompt should instruct to only work on specific files"
        );
        // Updated to match new constraint language that references FILES YOU MAY MODIFY
        assert!(
            fix_prompt.contains("FILES YOU MAY MODIFY")
                || fix_prompt.contains("embedded ISSUES content"),
            "Fix prompt should limit work to specific files from ISSUES"
        );
    }

    #[test]
    fn test_fix_prompt_forbids_running_commands() {
        // Verify that fix prompt explicitly forbids running commands
        let fix_prompt = prompt_fix("", "", "");
        let command_patterns = ["git", "ls", "find", "cat", "DO NOT run any commands"];
        let has_command_constraint = command_patterns
            .iter()
            .any(|pattern| fix_prompt.contains(pattern));
        assert!(
            has_command_constraint,
            "Fix prompt should explicitly forbid running commands"
        );
    }

    #[test]
    fn test_fix_prompt_is_template_based() {
        // Verify that fix prompt uses template-based approach (not hardcoded string)
        let fix_prompt = prompt_fix("", "", "");
        // If template loading failed, we'd get an empty string
        assert!(
            !fix_prompt.is_empty(),
            "Fix prompt should not be empty (template loading should succeed)"
        );
        assert!(
            fix_prompt.contains("FIX MODE"),
            "Fix prompt should contain FIX MODE indicator"
        );
    }

    #[test]
    fn test_fix_prompt_includes_file_list_from_issues() {
        // Verify that fix prompt includes extracted file list
        let issues = r"
# Issues
- [ ] [src/main.rs:42] Bug in main function
- [ ] [src/lib.rs:10] Style issue
";
        let fix_prompt = prompt_fix("", "", issues);
        assert!(
            fix_prompt.contains("FILES YOU MAY MODIFY"),
            "Fix prompt should include file list header"
        );
        assert!(
            fix_prompt.contains("src/main.rs"),
            "Fix prompt should list extracted files"
        );
        assert!(
            fix_prompt.contains("src/lib.rs"),
            "Fix prompt should list all extracted files"
        );
    }

    #[test]
    fn test_fix_prompt_handles_empty_file_list() {
        // Verify that fix prompt handles empty file list gracefully
        let issues = "# Issues\n- [ ] Fix the build system";
        let fix_prompt = prompt_fix("", "", issues);
        assert!(
            fix_prompt.contains("No specific files were extracted"),
            "Fix prompt should indicate no specific files when extraction finds none"
        );
        assert!(
            fix_prompt.contains("You may work on ANY files in the repository"),
            "Fix prompt should allow working on any files in the repository when extraction finds none"
        );
    }

    #[test]
    fn test_fix_prompt_allows_reading_listed_files() {
        // Verify that fix prompt explicitly allows reading listed files
        let issues = r"
# Issues
- [ ] [src/main.rs:42] Bug in main function
";
        let fix_prompt = prompt_fix("", "", issues);
        // Updated to match new constraint language that references FILES YOU MAY MODIFY
        assert!(
            fix_prompt.contains("MAY read the files listed")
                || fix_prompt.contains("FILES YOU MAY MODIFY"),
            "Fix prompt should explicitly allow reading listed files"
        );
    }

    #[test]
    fn test_fix_prompt_still_prohibits_exploration() {
        // Verify that fix prompt still prohibits exploration commands
        let fix_prompt = prompt_fix("", "", "");
        // The XML template allows LIMITED exploration for vague issue descriptions
        // but emphasizes stopping once relevant code is found
        assert!(
            fix_prompt.contains("stop exploring")
                || fix_prompt.contains("LIMITEDLY")
                || fix_prompt.contains("MUST stop exploring"),
            "Fix prompt should emphasize limited exploration"
        );
        assert!(
            fix_prompt.contains("git grep")
                || fix_prompt.contains("ripgrep")
                || fix_prompt.contains("locate"),
            "Fix prompt should explicitly allow discovery tools for finding relevant code"
        );
    }

    #[test]
    fn test_fix_prompt_file_list_is_sorted() {
        // Verify that file list is sorted alphabetically
        let issues = r"
# Issues
- [ ] [src/zebra.rs:1] Z file
- [ ] [src/alpha.rs:1] A file
- [ ] [src/beta.rs:1] B file
";
        let fix_prompt = prompt_fix("", "", issues);
        // Find the file list section
        let files_start = fix_prompt.find("FILES YOU MAY MODIFY").unwrap();
        let files_section = &fix_prompt[files_start..];

        // Check that alpha appears before beta before zebra
        let alpha_pos = files_section.find("src/alpha.rs").unwrap();
        let beta_pos = files_section.find("src/beta.rs").unwrap();
        let zebra_pos = files_section.find("src/zebra.rs").unwrap();

        assert!(
            alpha_pos < beta_pos && beta_pos < zebra_pos,
            "File list should be sorted alphabetically"
        );
    }

    #[test]
    fn test_fix_prompt_deduplicates_files() {
        // Verify that duplicate file references are deduplicated
        let issues = r"
# Issues
- [ ] [src/main.rs:42] First issue
- [ ] [src/main.rs:100] Second issue (same file)
- [ ] [src/lib.rs:10] Third issue
";
        let fix_prompt = prompt_fix("", "", issues);
        // Count occurrences of src/main.rs in the file list section
        let files_start = fix_prompt.find("FILES YOU MAY MODIFY").unwrap();
        let files_section = &fix_prompt[files_start..];

        let main_count = files_section.matches("src/main.rs").count();
        assert_eq!(
            main_count, 1,
            "File should appear only once in the list (deduplicated)"
        );
    }

    #[test]
    fn test_fix_prompt_explicitly_states_content_is_embedded() {
        let fix_prompt = prompt_fix("", "", "");
        assert!(
            fix_prompt.contains("ISSUES FROM REVIEW")
                || fix_prompt.contains("provided for reference only"),
            "Fix prompt should explicitly state ISSUES content is embedded in the prompt"
        );
    }

    #[test]
    fn test_fix_prompt_tells_agent_not_to_modify_issues_file() {
        let fix_prompt = prompt_fix("", "", "");
        assert!(
            fix_prompt.contains("MUST NOT modify ISSUES")
                || fix_prompt.contains("DO NOT modify")
                || fix_prompt.contains("provided for reference"),
            "Fix prompt should explicitly tell agent not to modify the ISSUES file"
        );
    }

    #[test]
    fn test_fix_prompt_references_file_list_section_explicitly() {
        let fix_prompt = prompt_fix("prompt", "plan", "issues");
        assert!(
            fix_prompt.contains("FILES YOU MAY MODIFY"),
            "Fix prompt should explicitly reference the FILES YOU MAY MODIFY section"
        );
    }

    #[test]
    fn test_prompt_fix_with_context() {
        let context = TemplateContext::default();
        let result = prompt_fix_with_context(
            &context,
            "test prompt content",
            "test plan content",
            "test issues content",
        );
        assert!(result.contains("test issues content"));
        assert!(result.contains("MUST NOT modify the ISSUES content"));
        assert!(result.contains("provided for reference only"));
        assert!(result.contains("MAY modify"));
        assert!(result.contains("FIX MODE"));
        assert!(result.contains("<ralph-fix-result>"));
        assert!(result.contains("<ralph-status>"));
        assert!(result.contains("all_issues_addressed"));
        assert!(result.contains("issues_remain"));
        assert!(result.contains("test prompt content"));
        assert!(result.contains("test plan content"));
    }

    #[test]
    fn test_prompt_fix_with_context_empty() {
        let context = TemplateContext::default();
        let result = prompt_fix_with_context(&context, "", "", "");
        assert!(result.contains("FIX MODE"));
        assert!(!result.is_empty());
    }

    #[test]
    fn test_context_based_fix_matches_regular() {
        let context = TemplateContext::default();
        let regular = prompt_fix("prompt", "plan", "issues");
        let with_context = prompt_fix_with_context(&context, "prompt", "plan", "issues");
        // Both should produce equivalent output
        assert_eq!(regular, with_context);
    }

    #[test]
    fn test_prompt_generate_commit_message_with_diff_with_context() {
        let context = TemplateContext::default();
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn new_func() {}";
        let result = prompt_generate_commit_message_with_diff_with_context(&context, diff);
        assert!(!result.is_empty());
        assert!(result.contains("DIFF:") || result.contains("diff"));
        assert!(!result.contains("ERROR: Empty diff"));
    }

    #[test]
    fn test_prompt_generate_commit_message_with_diff_with_context_empty() {
        let context = TemplateContext::default();
        let result = prompt_generate_commit_message_with_diff_with_context(&context, "");
        assert!(result.contains("ERROR: Empty diff"));
    }

    #[test]
    fn test_context_based_commit_matches_regular() {
        let context = TemplateContext::default();
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn new_func() {}";
        let regular = prompt_generate_commit_message_with_diff(diff);
        let with_context = prompt_generate_commit_message_with_diff_with_context(&context, diff);
        // Both should produce equivalent output
        assert_eq!(regular, with_context);
    }
}
