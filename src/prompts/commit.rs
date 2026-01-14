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
- PROMPT.md - Original requirements for context

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
        r#"You are a commit message generation expert. Analyze the following git diff and generate a high-quality Conventional Commits message.

DIFF:
{}

---

## ANALYSIS INSTRUCTIONS

Before generating the commit message, analyze the diff to understand:

1. **What actually changed?** Look at the code changes, not just file counts
2. **Why was this change made?** What problem does it solve or what feature does it add?
3. **What is the scope?** Which module, component, or area is affected?
4. **Is this a breaking change?** Does it change public APIs or expected behavior?

## COMMIT MESSAGE FORMAT

<type>[optional scope][!]: <subject>

[optional body]

[optional footer]

## TYPE GUIDELINES

Choose the most appropriate type:
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

## SUBJECT LINE RULES

- Use **imperative mood** ("add" not "added", "fix" not "fixed")
- Use **lowercase** (except for proper nouns)
- **No period** at the end
- **Maximum 50 characters**
- Be specific and descriptive

## BODY RULES (when needed)

- Explain **what** and **why**, not **how**
- Wrap at **72 characters**
- Use for complex changes that need context
- Mention **breaking changes** explicitly

## FOOTER RULES (when needed)

- **Breaking changes**: Start with "BREAKING CHANGE: "
- **Issue references**: "Fixes #123" or "Refs #456"

---

## GOOD EXAMPLES

### Simple feature
```
feat(auth): add OAuth2 login flow
```

### Bug fix
```
fix: prevent null pointer in user lookup
```

### Code restructuring
```
refactor(api): extract validation into middleware
```

### Breaking change
```
feat!: drop Python 3.7 support

BREAKING CHANGE: Minimum Python version is now 3.8.
```

### Complex feature with body
```
feat: add CSV export for reports

Add ability to export analytics reports as CSV files.
Supports filtering by date range and custom column selection.

Fixes #42
```

### Documentation
```
docs: clarify API authentication flow
```

### Tests
```
test: add coverage for user registration edge cases
```

---

## BAD EXAMPLES (AVOID THESE)

```
chore: apply changes
```
❌ Too vague - what changes?

```
chore: update code
```
❌ Meaningless - what was updated?

```
chore: 3 files changed
```
❌ File count is not a description

```
feat: Add new feature.
```
❌ Capitalized, has period, vague

```
refactoring the code
```
❌ No type, not imperative mood

```
fix bug
```
❌ What bug? Where?

---

## YOUR TASK

1. **Analyze the actual code changes** in the diff above
2. **Identify the type** based on what actually changed
3. **Determine the scope** (if applicable) based on which files/components are affected
4. **Write a clear, descriptive subject line** using imperative mood
5. **Add a body** if the change needs explanation (why, what for)
6. **Include a footer** for breaking changes or issue references

**IMPORTANT**: Focus on the semantic meaning of the changes, not the number of files. A single-file change can be significant, and multi-file changes can be minor.

Respond with ONLY the commit message (no markdown fences, no explanations, no extra text)."#,
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
