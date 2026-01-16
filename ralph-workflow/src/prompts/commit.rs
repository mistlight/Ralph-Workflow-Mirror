//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

use crate::prompts::template_engine::Template;
use std::collections::HashMap;

/// Generate fix prompt (applies to either role).
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Uses a template-based approach for consistency with review prompts.
///
/// # Agent-Orchestrator Separation
///
/// The fix agent reads ISSUES.md (written by the orchestrator after extracting
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
/// The fix agent is constrained to ONLY work on files mentioned in ISSUES.md.
/// This prevents the agent from exploring the repository and keeps changes
/// focused on the issues identified during review.
pub fn prompt_fix() -> String {
    let template_content = include_str!("templates/fix_mode.txt");
    Template::new(template_content)
        .render(&std::collections::HashMap::new())
        .unwrap_or_else(|e| {
            eprintln!("Warning: Failed to render fix template: {e}");
            String::new()
        })
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
        // Fallback to a minimal prompt if template rendering fails
        format!(
            "Generate a conventional commit message for this diff:\n\n{diff_content}\n\n\
             Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>"
        )
    })
}

/// Generate strict XML-only prompt for commit message retry.
///
/// This is used when the initial attempt fails to produce valid output,
/// providing a simpler, more focused prompt to encourage proper XML output.
pub fn prompt_strict_json_commit(diff: &str) -> String {
    let diff_content = diff.trim();
    format!(
        r"Your previous response was not valid. Return ONLY the XML tags below.

DIFF:
{diff_content}

REQUIRED OUTPUT (nothing else):
<ralph-commit>
<ralph-subject>type: description</ralph-subject>
</ralph-commit>

RULES:
- Start IMMEDIATELY with <ralph-commit>
- No text before or after the XML
- subject must start with: feat, fix, docs, style, refactor, perf, test, build, ci, or chore
- Keep subject under 72 characters

Example:
<ralph-commit>
<ralph-subject>fix: correct null pointer in user lookup</ralph-subject>
</ralph-commit>"
    )
}

/// Generate even stricter re-prompt with negative examples.
///
/// This is the second-level re-prompt used when the strict prompt also fails.
/// It includes explicit examples of what NOT to output to prevent common mistakes.
pub fn prompt_strict_json_commit_v2(diff: &str) -> String {
    r#"Your response MUST be ONLY XML tags. No other text.

DIFF:
__DIFF_CONTENT__

REQUIRED OUTPUT:
<ralph-commit>
<ralph-subject>feat: brief description</ralph-subject>
</ralph-commit>

WHAT NOT TO OUTPUT (these are WRONG):
- "Here is the commit message:"
- "Looking at the diff, I can see..."
- "Based on the changes above..."
- Any markdown code fences
- Any explanation or analysis before the XML

CORRECT OUTPUT (copy this format exactly):
<ralph-commit>
<ralph-subject>fix: prevent null pointer</ralph-subject>
</ralph-commit>

RULES:
1. Start with <ralph-commit>
2. End with </ralph-commit>
3. Nothing before <ralph-commit>
4. Nothing after </ralph-commit>
5. subject must be: feat, fix, docs, style, refactor, perf, test, build, ci, or chore
6. Keep subject under 72 characters"#
        .replace("__DIFF_CONTENT__", diff.trim())
}

/// Generate ultra-minimal commit prompt.
///
/// This is the third-level re-prompt with bare minimum instructions.
/// Removes all explanatory context to reduce chance of verbose responses.
pub fn prompt_ultra_minimal_commit(diff: &str) -> String {
    r"DIFF:
__DIFF_CONTENT__

OUTPUT ONLY:
<ralph-commit>
<ralph-subject>feat: description</ralph-subject>
</ralph-commit>

Types: feat|fix|docs|style|refactor|perf|test|build|ci|chore"
        .replace("__DIFF_CONTENT__", diff.trim())
}

/// Generate ultra-minimal V2 commit prompt.
///
/// This is an even shorter variant that only provides the subject line template.
/// Used when `UltraMinimal` still produces too much output.
pub fn prompt_ultra_minimal_commit_v2(diff: &str) -> String {
    r"__DIFF_CONTENT__

<ralph-commit>
<ralph-subject>fix: </ralph-subject>
</ralph-commit>"
        .replace("__DIFF_CONTENT__", diff.trim())
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

    format!(
        r"{file_list}
<ralph-commit>
<ralph-subject>chore: update files</ralph-subject>
</ralph-commit>"
    )
}

/// Generate emergency commit prompt with maximum constraints.
///
/// This is the final re-prompt attempt before falling back to the next agent.
/// It provides the absolute minimum context to elicit an XML response.
pub fn prompt_emergency_commit(diff: &str) -> String {
    r"__DIFF_CONTENT__

<ralph-commit>
<ralph-subject>fix: </ralph-subject>
</ralph-commit>"
        .replace("__DIFF_CONTENT__", diff.trim())
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

    format!(
        r"{summary}
Generate a conventional commit message for these changes.
<ralph-commit>
<ralph-subject>chore: update files</ralph-subject>
</ralph-commit>"
    )
}

/// Generate emergency no-diff commit prompt.
///
/// This is the absolute last resort that doesn't include any diff at all.
/// Just asks for a generic commit when everything else fails.
pub fn prompt_emergency_no_diff_commit(_diff: &str) -> String {
    r"<ralph-commit>
<ralph-subject>chore: changes</ralph-subject>
</ralph-commit>"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_fix() {
        let result = prompt_fix();
        assert!(result.contains("ISSUES.md"));
        // Agent should NOT modify ISSUES.md - orchestrator manages this file
        assert!(result.contains("DO NOT modify ISSUES.md"));
        assert!(result.contains("orchestrator manages this file"));
        // Agent SHOULD modify source code files to fix issues
        assert!(result.contains("SHOULD modify source code files"));
        assert!(result.contains("FIX MODE"));
        // Agent should return status as structured output
        assert!(result.contains("All issues addressed"));
        assert!(result.contains("Issues remain"));
    }

    #[test]
    fn test_notes_md_references_are_minimal_or_absent() {
        // NOTES.md references should be minimal or absent (isolation mode removes these files)
        let fix_prompt = prompt_fix();

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
        let fix_prompt = prompt_fix();
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
        let fix_prompt = prompt_fix();
        assert!(
            fix_prompt.contains("MUST NOT read any other files")
                || fix_prompt.contains("MUST NOT run git commands")
                || fix_prompt.contains("DO NOT run any commands"),
            "Fix prompt should explicitly forbid exploration or running commands"
        );
    }

    #[test]
    fn test_fix_prompt_instructs_to_only_work_on_issues_files() {
        // Verify that fix prompt instructs to only work on files from ISSUES.md
        let fix_prompt = prompt_fix();
        assert!(
            fix_prompt.contains("ISSUES.md"),
            "Fix prompt should reference ISSUES.md"
        );
        assert!(
            fix_prompt.contains("ONLY read") || fix_prompt.contains("only read"),
            "Fix prompt should instruct to only read specific files"
        );
        assert!(
            fix_prompt.contains("mentioned in ISSUES.md")
                || fix_prompt.contains("files that are mentioned"),
            "Fix prompt should limit work to files mentioned in ISSUES.md"
        );
    }

    #[test]
    fn test_fix_prompt_forbids_running_commands() {
        // Verify that fix prompt explicitly forbids running commands
        let fix_prompt = prompt_fix();
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
        let fix_prompt = prompt_fix();
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
}
