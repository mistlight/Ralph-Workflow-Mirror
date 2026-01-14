//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

/// Generate fix prompt (applies to either role).
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
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

DO NOT modify ISSUES.md - the orchestrator manages this file.
You SHOULD modify source code files to fix the issues.

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
                meaningful changes should be checked before requesting a commit message."
            .to_string();
    }

    format!(
        r#"You are a commit message generation expert. Analyze the following git diff and generate a high-quality Conventional Commits message.

DIFF:
{diff_content}

---

## CRITICAL: DO NOT PRODUCE THESE BAD COMMIT MESSAGES

These are WRONG - they are vague, meaningless, and unhelpful:
❌ chore: apply changes
❌ chore: update code
❌ chore: 6 file(s) changed
❌ chore: update src/files/result_extraction.rs
❌ chore: update src/git_helpers/repo.rs, src/prompts/commit.rs, tests/commit_message_generation.rs
❌ fix: fixed bug
❌ feat: Add New Feature.

NEVER say "apply changes", "update code", "update [filename]", "N files changed", or just list filenames.
ALWAYS describe WHAT changed and WHY.

**When analyzing multi-file changes:**
- Look for the SEMANTIC RELATIONSHIP between files
- Are they all part of one feature? Use a single message with the feature's purpose
- Are they unrelated changes? Use the highest-priority type with a descriptive subject
- Examples:
  - ❌ "chore: update src/auth.rs, src/auth_test.rs, docs/auth.md"
  - ✅ "feat(auth): add OAuth2 login flow with tests and docs"
  - ❌ "chore: 3 file(s) changed"
  - ✅ "refactor: extract validation logic into shared module"

---

## COMMIT MESSAGE FORMAT

<type>[optional scope][!]: <subject>

[optional body]

[optional footer]

## TYPE GUIDELINES

- **feat**: A new feature (user-visible change)
- **fix**: A bug fix (correcting incorrect behavior)
- **docs**: Documentation changes only
- **style**: Code style changes (formatting, semicolons, etc.) - no logic change
- **refactor**: Code restructuring without changing behavior
- **perf**: Performance improvement
- **test**: Adding or updating tests
- **build**: Build system or dependency changes
- **ci**: CI/CD configuration changes
- **chore**: Other changes that don't modify src/test files

## SUBJECT LINE RULES (CRITICAL)

- Use **imperative mood** ("add" not "added", "fix" not "fixed")
- Use **lowercase** (except for proper nouns)
- **No period** at the end
- **Maximum 50 characters**
- **Be specific**: describe WHAT changed, not THAT something changed
- For multi-file changes: describe the OVERALL PURPOSE, not just "update files"

## GOOD EXAMPLES

feat(auth): add OAuth2 login flow
fix: prevent null pointer in user lookup
refactor(api): extract validation into middleware
docs: clarify API authentication flow
test: add coverage for user registration edge cases

feat!: drop Python 3.7 support

BREAKING CHANGE: Minimum Python version is now 3.8.

feat: add CSV export for reports

Add ability to export analytics reports as CSV files.
Supports filtering by date range and custom column selection.

Fixes #42

---

## YOUR TASK

1. **Analyze the actual code changes** in the diff above
2. **Identify the semantic type** (feat/fix/refactor/docs/etc.) based on what changed
3. **Determine the scope** (if applicable) based on which files/components are affected
4. **Write a clear, descriptive subject line** that says WHAT was done
5. **Add a body** only if the change needs context (why, what for)

**MULTI-FILE ANALYSIS**: When you see changes to multiple files, determine:
- Are they all part of one cohesive change? → Single message describing the purpose
- Are they semantically different? → Use the most significant type with a comprehensive subject
- What is the COMMON THREAD that connects these changes?

**OUTPUT REQUIREMENT**: Respond with ONLY the commit message.
- NO markdown fences (no ``` ``` )
- NO explanations
- NO "Here is the commit message:" prefix
- JUST the commit message itself

Example of WRONG output:
```
Here is the commit message:
feat: add feature
```

Example of CORRECT output:
feat: add feature"#
    )
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
}
