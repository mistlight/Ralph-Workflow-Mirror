//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::prompts::template_engine::Template;
use std::collections::HashMap;

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
pub fn prompt_fix(prompt_content: &str, plan_content: &str, issues_content: &str) -> String {
    let template_content = include_str!("templates/fix_mode.txt");

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
            // Use fallback template if main template fails
            let fallback_content = include_str!("templates/fix_mode_fallback.txt");
            let fallback_template = Template::new(fallback_content);
            fallback_template
                .render(&variables)
                .unwrap_or_else(|_| {
                    // Last resort emergency fallback
                    format!(
                        "FIX MODE\n\nRead .agent/ISSUES.md and fix the issues found.\n\nContext:\nPROMPT:\n{prompt_content}\n\nPLAN:\n{plan_content}\n"
                    )
                })
        })
}

/// Format the files section for the fix prompt.
///
/// If files are found, formats them as a bulleted list with a clear header.
/// If no files are found, provides a fallback message indicating that the
/// agent should fix issues wherever appropriate.
fn format_files_section(files: &[String]) -> String {
    if files.is_empty() {
        "No specific files listed in ISSUES - fix issues anywhere appropriate.".to_string()
    } else {
        let mut result = String::from("FILES YOU MAY MODIFY:\n\n");
        for file in files {
            result.push_str("- ");
            result.push_str(file);
            result.push('\n');
        }
        // Add explicit clarification that agent doesn't need to read any ISSUES file
        result.push_str(
            "\nIMPORTANT: Work ONLY with the files listed above. The issues\n\
            content is already embedded in this prompt - you do NOT need to\n\
            read or discover any files to know what to fix.\n",
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
        // Use fallback template if main template fails
        let fallback_content = include_str!("templates/commit_message_fallback.txt");
        let fallback_template = Template::new(fallback_content);
        fallback_template
            .render(&variables)
            .unwrap_or_else(|_| {
                // Last resort emergency fallback
                format!(
                    "Generate a conventional commit message for this diff:\n\n{diff_content}\n\n\
                     Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>"
                )
            })
    })
}

/// Generate strict XML-only prompt for commit message retry.
///
/// This is used when the initial attempt fails to produce valid output,
/// providing a simpler, more focused prompt to encourage proper XML output.
pub fn prompt_strict_json_commit(diff: &str) -> String {
    let template_content = include_str!("templates/commit_strict_json.txt");
    let variables = HashMap::from([("DIFF", diff.trim().to_string())]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_e| {
            // Fallback to minimal prompt with diff if template rendering fails
            format!(
                "Generate a conventional commit message. Output ONLY:\n\n\
                 <ralph-commit>\n\
                 <ralph-subject>type: description</ralph-subject>\n\
                 </ralph-commit>\n\n\
                 Diff:\n{}\n",
                diff.trim()
            )
        })
}

/// Generate even stricter re-prompt with negative examples.
///
/// This is the second-level re-prompt used when the strict prompt also fails.
/// It includes explicit examples of what NOT to output to prevent common mistakes.
pub fn prompt_strict_json_commit_v2(diff: &str) -> String {
    let template_content = include_str!("templates/commit_strict_json_v2.txt");
    let variables = HashMap::from([("DIFF", diff.trim().to_string())]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_e| {
            // Fallback to minimal prompt with diff if template rendering fails
            format!(
                "OUTPUT ONLY:\n\n<ralph-commit>\n<ralph-subject>type: description</ralph-subject>\n</ralph-commit>\n\nDiff:\n{}\n",
                diff.trim()
            )
        })
}

/// Generate ultra-minimal commit prompt.
///
/// This is the third-level re-prompt with bare minimum instructions.
/// Removes all explanatory context to reduce chance of verbose responses.
pub fn prompt_ultra_minimal_commit(diff: &str) -> String {
    let template_content = include_str!("templates/commit_ultra_minimal.txt");
    let variables = HashMap::from([("DIFF", diff.trim().to_string())]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_e| {
            // Fallback to minimal prompt with diff if template rendering fails
            format!(
                "OUTPUT ONLY:\n<ralph-commit>\n<ralph-subject>fix: </ralph-subject>\n</ralph-commit>\n\n{}\n",
                diff.trim()
            )
        })
}

/// Generate ultra-minimal V2 commit prompt.
///
/// This is an even shorter variant that only provides the subject line template.
/// Used when `UltraMinimal` still produces too much output.
pub fn prompt_ultra_minimal_commit_v2(diff: &str) -> String {
    let template_content = include_str!("templates/commit_ultra_minimal_v2.txt");
    let variables = HashMap::from([("DIFF", diff.trim().to_string())]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_e| {
            // Fallback to minimal prompt with diff if template rendering fails
            format!(
                "OUTPUT:\n<ralph-subject>fix: </ralph-subject>\n\n{}\n",
                diff.trim()
            )
        })
}

/// Generate file-list-only commit prompt.
///
/// This variant asks for a commit based on just file paths (no diff content).
/// Used when all diff-including prompts fail due to token limits.
/// The diff is still provided but the prompt focuses on file paths.
pub fn prompt_file_list_only_commit(diff: &str) -> String {
    let diff_content = diff.trim();

    // Extract just the file paths from the diff
    let files: Vec<&str> = diff_content
        .lines()
        .filter(|line| line.starts_with("diff --git a/"))
        .collect();

    let file_list = if files.is_empty() {
        String::from("(no files detected)")
    } else {
        // Format file list concisely
        let mut result = String::from("Files changed:\n");
        for file in &files {
            if let Some(path) = file.split(" b/").nth(1) {
                result.push_str("- ");
                result.push_str(path);
                result.push('\n');
            }
        }
        result
    };

    let template_content = include_str!("templates/commit_file_list_only.txt");
    let variables = HashMap::from([("FILE_LIST", file_list.clone())]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            // Fallback to minimal prompt with file list if template rendering fails
            format!(
                "Generate commit message. Output ONLY:\n\n\
                 <ralph-commit>\n\
                 <ralph-subject>type: description</ralph-subject>\n\
                 </ralph-commit>\n\n\
                 Files changed:\n{file_list}\n"
            )
        })
}

/// Generate emergency commit prompt with maximum constraints.
///
/// This is the final re-prompt attempt before falling back to the next agent.
/// It provides the absolute minimum context to elicit an XML response.
pub fn prompt_emergency_commit(diff: &str) -> String {
    let template_content = include_str!("templates/commit_emergency.txt");
    let variables = HashMap::from([("DIFF", diff.trim().to_string())]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            // Fallback to minimal prompt with diff if template rendering fails
            format!(
                "<ralph-commit>\n<ralph-subject>fix: </ralph-subject>\n</ralph-commit>\n\n{}\n",
                diff.trim()
            )
        })
}

/// Generate file-list-summary-only commit prompt.
///
/// This variant provides only a summary of changed files with counts and categories,
/// without any diff content. Used when all diff-including prompts fail due to
/// extreme token limits.
pub fn prompt_file_list_summary_only_commit(diff: &str) -> String {
    use std::fmt::Write;

    let diff_content = diff.trim();

    // Extract file statistics from the diff
    let files: Vec<&str> = diff_content
        .lines()
        .filter(|line| line.starts_with("diff --git a/"))
        .collect();

    // Categorize files
    let mut src_files = 0;
    let mut test_files = 0;
    let mut doc_files = 0;
    let mut config_files = 0;
    let mut other_files = 0;

    for file in &files {
        let path_lower = file.to_lowercase();
        if path_lower.contains("/src/") || path_lower.contains("src/") {
            src_files += 1;
        } else if path_lower.contains("/test/") || path_lower.contains("test_") {
            test_files += 1;
        } else if path_lower.contains(".md") || path_lower.contains("/doc") {
            doc_files += 1;
        } else if path_lower.contains("cargo.toml") || path_lower.contains("package.json") {
            config_files += 1;
        } else {
            other_files += 1;
        }
    }

    let total_files = files.len();

    // Build the summary
    let mut summary = String::from("Changed files summary:\n");
    writeln!(summary, "Total files changed: {total_files}").unwrap();

    if src_files > 0 {
        writeln!(summary, "- Source files: {src_files}").unwrap();
    }
    if test_files > 0 {
        writeln!(summary, "- Test files: {test_files}").unwrap();
    }
    if doc_files > 0 {
        writeln!(summary, "- Documentation: {doc_files}").unwrap();
    }
    if config_files > 0 {
        writeln!(summary, "- Configuration: {config_files}").unwrap();
    }
    if other_files > 0 {
        writeln!(summary, "- Other files: {other_files}").unwrap();
    }

    let template_content = include_str!("templates/commit_file_list_summary.txt");
    let variables = HashMap::from([("FILE_SUMMARY", summary.clone())]);
    Template::new(template_content)
        .render(&variables)
        .unwrap_or_else(|_| {
            // Fallback to minimal prompt with summary if template rendering fails
            format!(
                "Generate commit message. Output ONLY:\n\n\
                 <ralph-commit>\n\
                 <ralph-subject>chore: changes</ralph-subject>\n\
                 </ralph-commit>\n\n\
                 {summary}\n"
            )
        })
}

/// Generate emergency no-diff commit prompt.
///
/// This is the absolute last resort that doesn't include any diff at all.
/// Just asks for a generic commit when everything else fails.
pub fn prompt_emergency_no_diff_commit(_diff: &str) -> String {
    let template_content = include_str!("templates/commit_emergency_no_diff.txt");
    Template::new(template_content)
        .render(&std::collections::HashMap::new())
        .unwrap_or_else(|_| {
            // Fallback to hardcoded commit message if template rendering fails
            "<ralph-commit>\n<ralph-subject>chore: automated commit</ralph-subject>\n</ralph-commit>".to_string()
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
        assert!(result.contains("DO NOT modify the ISSUES content"));
        assert!(result.contains("provided for reference only"));
        // Agent SHOULD modify source code files to fix issues
        assert!(result.contains("SHOULD modify source code files"));
        assert!(result.contains("FIX MODE"));
        // Agent should return status as structured output
        assert!(result.contains("All issues addressed"));
        assert!(result.contains("Issues remain"));
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
    fn test_strict_json_commit_v2_returns_valid_content() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn new_func() {}";
        let result = prompt_strict_json_commit_v2(diff);
        assert!(!result.is_empty());
        assert!(result.contains("DIFF:"));
        assert!(result.contains("WHAT NOT TO OUTPUT"));
        assert!(result.contains("CORRECT OUTPUT"));
    }

    #[test]
    fn test_ultra_minimal_commit_returns_valid_content() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn new_func() {}";
        let result = prompt_ultra_minimal_commit(diff);
        assert!(!result.is_empty());
        assert!(result.contains("DIFF:"));
        assert!(result.contains("OUTPUT ONLY:"));
        // Ultra-minimal should be shorter than standard prompt
        let standard = prompt_generate_commit_message_with_diff(diff);
        assert!(result.len() < standard.len());
    }

    #[test]
    fn test_emergency_commit_returns_valid_content() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn new_func() {}";
        let result = prompt_emergency_commit(diff);
        assert!(!result.is_empty());
        // Emergency prompt is extremely minimal - just diff + template
        assert!(result.len() < 200);
        // Now uses XML format
        assert!(result.contains("<ralph-commit>"));
        assert!(result.contains("<ralph-subject>fix: </ralph-subject>"));
    }

    #[test]
    fn test_strict_xml_v2_has_negative_examples() {
        let result = prompt_strict_json_commit_v2("dummy diff");
        // V2 should explicitly mention what NOT to output
        assert!(result.contains("WHAT NOT TO OUTPUT"));
        assert!(result.contains("Looking at the diff"));
        assert!(result.contains("Here is the commit message"));
    }

    #[test]
    fn test_emergency_is_minimal() {
        let diff = "dummy diff";
        let emergency = prompt_emergency_commit(diff);
        let ultra_minimal = prompt_ultra_minimal_commit(diff);
        // Emergency should be the shortest
        assert!(emergency.len() < ultra_minimal.len());
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
            fix_prompt.contains("MUST NOT read any other files")
                || fix_prompt.contains("MUST NOT run git commands")
                || fix_prompt.contains("DO NOT run any commands"),
            "Fix prompt should explicitly forbid exploration or running commands"
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
            fix_prompt.contains("No specific files listed"),
            "Fix prompt should indicate no specific files when extraction finds none"
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
        assert!(
            fix_prompt.contains("MUST NOT read any other files")
                || fix_prompt.contains("MUST NOT explore"),
            "Fix prompt should still prohibit reading unlisted files"
        );
        assert!(
            fix_prompt.contains("git grep")
                || fix_prompt.contains("ls")
                || fix_prompt.contains("find"),
            "Fix prompt should explicitly prohibit discovery commands"
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
            fix_prompt.contains("embedded above") || fix_prompt.contains("EMBEDDED"),
            "Fix prompt should explicitly state ISSUES content is embedded in the prompt"
        );
    }

    #[test]
    fn test_fix_prompt_tells_agent_not_to_read_issues_file() {
        let fix_prompt = prompt_fix("", "", "");
        assert!(
            fix_prompt.contains("do NOT need to read")
                || fix_prompt.contains("you do NOT need to")
                || fix_prompt.contains("DO NOT try to read"),
            "Fix prompt should explicitly tell agent it doesn't need to read any ISSUES file"
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
}
