//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

/// Generate fix prompt (applies to either role).
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
pub fn prompt_fix() -> String {
    r#"You are in FIX MODE. Address issues found during review.

INPUTS TO READ:
- .agent/ISSUES.md - Issues to address (if it exists)
- PROMPT.md - Original requirements for context

YOUR TASK:
1. Read .agent/ISSUES.md to understand any issues (if it exists)
2. Fix issues found, prioritizing by severity
3. Verify fixes work

AFTER FIXING:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "Issues addressed." (if you believe everything is fixed), OR
- "Issues remain." (if you believe issues still exist)
If .agent/NOTES.md exists, OVERWRITE it with exactly ONE vague sentence (no details).

GUIDELINES:
- Fix issues properly, don't just suppress warnings
- Ensure fixes don't introduce new issues"#
        .to_string()
}

/// Generate prompt for agent to generate a commit message.
///
/// Agent writes the commit message to .agent/commit-message.txt.
/// NOTES.md reference is explicitly optional since it may not exist in isolation mode.
pub fn prompt_generate_commit_message() -> String {
    r#"Generate a commit message for all changes made.

FIRST, gather context:
1. Run `git diff HEAD` to see exactly what changed
2. Read PROMPT.md to understand the original goal
3. Optionally read .agent/NOTES.md for additional context (if it exists)

THEN: Write a Conventional Commits message to .agent/commit-message.txt

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

Write ONLY the commit message to .agent/commit-message.txt (no markdown fences, no extra text)."#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_fix() {
        let result = prompt_fix();
        assert!(result.contains("ISSUES.md"));
        // NOTES.md/ISSUES.md should be constrained to vague overwrite semantics
        assert!(result.contains("OVERWRITE"));
        assert!(result.contains("exactly ONE vague sentence"));
        assert!(result.contains("FIX MODE"));
    }

    #[test]
    fn test_prompt_generate_commit_message() {
        let result = prompt_generate_commit_message();
        // Basic structure
        assert!(result.contains("commit-message.txt"));
        assert!(result.contains("Conventional Commits"));

        // Context gathering instructions
        assert!(result.contains("git diff HEAD"));
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("NOTES.md"));

        // Type prefixes
        assert!(result.contains("feat"));
        assert!(result.contains("fix"));
        assert!(result.contains("docs"));
        assert!(result.contains("refactor"));
        assert!(result.contains("test"));
        assert!(result.contains("chore"));
        assert!(result.contains("perf"));

        // Scope support
        assert!(result.contains("scope"));
        assert!(result.contains("feat(parser):"));

        // Breaking change notation
        assert!(result.contains("!:"));
        assert!(result.contains("BREAKING CHANGE"));

        // Imperative mood guidance
        assert!(result.contains("imperative"));
        assert!(result.contains("\"add\" not \"added\""));

        // Character limits
        assert!(result.contains("max 50 chars"));
        assert!(result.contains("72 chars"));

        // Issue references
        assert!(result.contains("Fixes #"));

        // Good examples
        assert!(result.contains("feat(auth): add OAuth2 login flow"));
        assert!(result.contains("fix: prevent null pointer"));

        // Bad examples (anti-patterns to avoid)
        assert!(result.contains("BAD EXAMPLES"));
        assert!(result.contains("chore: apply changes"));
        assert!(result.contains("too vague"));
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
