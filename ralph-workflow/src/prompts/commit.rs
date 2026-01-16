//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

/// Template for commit message prompt with diff placeholder.
const COMMIT_MESSAGE_PROMPT_TEMPLATE: &str = r#"You are a commit message generation expert. Analyze the following git diff and generate a high-quality Conventional Commits message.

DIFF:
__DIFF_CONTENT__

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

**OUTPUT REQUIREMENT**: Return ONLY a JSON object with this exact schema:
{{"subject": "<type>[scope]: <description>", "body": "<optional body or null>"}}

CRITICAL JSON RULES:
- Return ONLY the JSON object, nothing else
- No text before or after the JSON
- No markdown fences around the JSON
- `subject` is required, must be valid conventional commit format (max 72 chars)
- `body` is optional (use null if no body needed)
- **JSON STRING ESCAPING**: Use \n for newlines, \t for tabs within JSON strings
  - ✅ CORRECT: {{"subject": "feat: add feature", "body": "First line\nSecond line"}}
  - ❌ WRONG: {{"subject": "feat: add feature", "body": "First line
Second line"}}
  - The body value must be a valid JSON string - use escape sequences, NOT literal newlines

WRONG (with preamble):
Here is the commit message:
{{"subject": "feat: add feature", "body": null}}

WRONG (with analysis):
```
Looking at this diff, I can see:
1. Updated parser

feat: add feature
```

WRONG (with prefix):
```
Here is the commit message:
feat: add feature
```

WRONG (literal newline in JSON - this is INVALID JSON):
{{"subject": "feat: add feature", "body": "First line
Second line"}}

CORRECT:
{{"subject": "feat: add feature", "body": null}}

CORRECT (with body using \n for newline):
{{"subject": "feat: add OAuth2 login", "body": "Implement Google and GitHub OAuth providers.\nAdd session management for OAuth tokens."}}"#;

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
    let diff_content = diff.trim();
    if diff_content.is_empty() {
        return "ERROR: Empty diff provided. This indicates a bug in the caller - \
                meaningful changes should be checked before requesting a commit message."
            .to_string();
    }
    COMMIT_MESSAGE_PROMPT_TEMPLATE.replace("__DIFF_CONTENT__", diff_content)
}

/// Generate strict JSON-only prompt for commit message retry.
///
/// This is used when the initial attempt fails to produce valid JSON,
/// providing a simpler, more focused prompt to encourage proper JSON output.
pub fn prompt_strict_json_commit(diff: &str) -> String {
    let diff_content = diff.trim();
    format!(
        r#"Your previous response was not valid JSON. Return ONLY a JSON object.

REQUIRED FORMAT (nothing else):
{{"subject": "<type>: <description>", "body": null}}

DIFF:
{diff_content}

RULES:
- Return ONLY the JSON object
- No text before or after
- subject must start with: feat, fix, docs, style, refactor, perf, test, build, ci, or chore
- Keep subject under 72 characters
- **JSON ESCAPING**: Use \\n for newlines in body, NOT literal newlines

Example:
{{"subject": "fix: correct null pointer in user lookup", "body": null}}"#
    )
}

/// Generate even stricter re-prompt with negative examples.
///
/// This is the second-level re-prompt used when the strict prompt also fails.
/// It includes explicit examples of what NOT to output to prevent common mistakes.
pub fn prompt_strict_json_commit_v2(diff: &str) -> String {
    r#"Your response MUST be ONLY a JSON object. No other text.

DIFF:
__DIFF_CONTENT__

REQUIRED OUTPUT:
{"subject": "feat: brief description", "body": null}

WHAT NOT TO OUTPUT (these are WRONG):
❌ "Here is the commit message:"
❌ "Looking at the diff, I can see..."
❌ "Based on the changes above..."
❌ ```json
   {"subject": "..."}
   ```
❌ Any explanation or analysis before the JSON
❌ Literal newlines in JSON strings (use \n instead)

CORRECT OUTPUT (copy this format):
{"subject": "fix: prevent null pointer", "body": null}

RULES:
1. Start with {"subject":
2. End with }
3. Nothing before the opening {
4. Nothing after the closing }
5. subject must be: feat, fix, docs, style, refactor, perf, test, build, ci, or chore
6. Keep subject under 72 characters
7. Keep all text on ONE LINE - no literal newlines in JSON strings"#
        .replace("__DIFF_CONTENT__", diff.trim())
}

/// Generate ultra-minimal commit prompt.
///
/// This is the third-level re-prompt with bare minimum instructions.
/// Removes all explanatory context to reduce chance of verbose responses.
pub fn prompt_ultra_minimal_commit(diff: &str) -> String {
    r#"DIFF:
__DIFF_CONTENT__

OUTPUT ONLY:
{"subject": "feat: description", "body": null}

Types: feat|fix|docs|style|refactor|perf|test|build|ci|chore"#
        .replace("__DIFF_CONTENT__", diff.trim())
}

/// Generate ultra-minimal V2 commit prompt.
///
/// This is an even shorter variant that only provides the subject line template.
/// Used when `UltraMinimal` still produces too much output.
pub fn prompt_ultra_minimal_commit_v2(diff: &str) -> String {
    r#"__DIFF_CONTENT__

{"subject": "fix: ", "body": null}"#
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
        r#"{file_list}
{{"subject": "chore: update files", "body": null}}"#
    )
}

/// Generate emergency commit prompt with maximum constraints.
///
/// This is the final re-prompt attempt before falling back to the next agent.
/// It provides the absolute minimum context to elicit a JSON response.
pub fn prompt_emergency_commit(diff: &str) -> String {
    r#"__DIFF_CONTENT__

{"subject": "fix: ", "body": null}"#
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
        r#"{summary}
Generate a conventional commit message for these changes.
{{"subject": "chore: update files", "body": null}}"#
    )
}

/// Generate emergency no-diff commit prompt.
///
/// This is the absolute last resort that doesn't include any diff at all.
/// Just asks for a generic commit when everything else fails.
pub fn prompt_emergency_no_diff_commit(_diff: &str) -> String {
    r#"{{"subject": "chore: changes", "body": null}}"#.to_string()
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
        assert!(result.contains(r#"{"subject": "fix: ", "body": null}"#));
    }

    #[test]
    fn test_strict_json_v2_has_negative_examples() {
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
}
