//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

/// Generate fix prompt (applies to either role).
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
///
/// The fix agent reads ISSUES.md (written by the orchestrator after extracting
/// from the reviewer's output) and fixes the issues. The agent should NOT
/// modify ISSUES.md - the orchestrator handles file I/O.
pub fn prompt_fix() -> String {
    r#"You are in FIX MODE. Address issues found during review.

INPUTS TO READ:
- .agent/ISSUES.md - Issues to address (if it exists)

YOUR TASK:
1. Read .agent/ISSUES.md to understand any issues (if it exists)
2. Fix issues found, prioritizing by severity
3. Verify fixes work

AFTER FIXING:
Return your completion status as structured output:
- "All issues addressed." (if you believe everything is fixed)
- "Issues remain." (if you believe issues still exist)
- "No issues found." (if ISSUES.md didn't exist or was empty)

DO NOT modify ISSUES.md or any other files. The orchestrator handles file updates.

GUIDELINES:
- Fix issues properly, don't just suppress warnings
- Ensure fixes don't introduce new issues"#
        .to_string()
}

/// Generate prompt for creating commit message from provided diff.
///
/// This is used by the orchestrator (not agents) to generate commit messages.
/// The diff is provided directly in the prompt, so the LLM doesn't need to
/// run git commands or access files.
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
                meaningful changes should be checked before requesting a commit message.".to_string();
    }

    format!(
        r#"Generate a Conventional Commits message for the following git diff.

DIFF:
{}

FORMAT:
<type>[optional scope][!]: <subject>

[optional body]

[optional footer]

RULES:
- type: feat|fix|docs|refactor|test|chore|perf|build|ci (required)
- scope: area affected in parentheses, e.g., feat(parser): (optional)
- !: add before colon for breaking changes, e.g., feat!: or feat(api)!:
- subject: imperative mood ("add" not "added"), lowercase, no period, max 50 chars
- body: wrap at 72 chars, explain what/why not how (optional, for complex changes)
- footer: BREAKING CHANGE: description, or Fixes #123, Refs #456 (optional)

GOOD EXAMPLES:
feat(auth): add OAuth2 login flow
fix: prevent null pointer in user lookup
refactor(api): extract validation into middleware

feat!: drop Python 3.7 support

BREAKING CHANGE: Minimum Python version is now 3.8.

feat: add CSV export for reports

Add ability to export analytics reports as CSV files.
Supports filtering by date range and custom column selection.

Fixes #42

BAD EXAMPLES (avoid these patterns):
- "chore: apply changes" (too vague - what changes?)
- "chore: update code" (meaningless)
- "Updated the code" (no type, not imperative)
- "feat: Add new feature." (capitalized, has period, vague)

Respond with ONLY the commit message (no markdown fences, no extra text)."#,
        diff_content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_fix() {
        let result = prompt_fix();
        assert!(result.contains("ISSUES.md"));
        // Agent should NOT modify files - orchestrator handles I/O
        assert!(result.contains("DO NOT modify ISSUES.md"));
        assert!(result.contains("orchestrator handles file updates"));
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
}
