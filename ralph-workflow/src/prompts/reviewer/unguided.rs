//! Unguided reviewer prompts for general-purpose reviews.
//!
//! This module generates review prompts that do not include language-specific
//! guidelines. These are used when:
//!
//! - Stack detection did not identify a recognized language/framework
//! - A "fresh eyes" perspective is needed without framework-specific bias
//! - Reviewing general code quality, goal alignment, and acceptance criteria
//!
//! Available prompt types:
//! - **Simple review**: Minimal, vague prompt for unbiased perspective
//! - **Detailed review**: Actionable output with severity levels
//! - **Incremental review**: Focus only on recently changed files
//! - **Universal review**: Simplified prompt for agent compatibility

use super::super::types::ContextLevel;

/// Generate detailed reviewer review prompt without language-specific guidelines,
/// including the diff directly in the prompt.
///
/// This version receives the diff as a parameter instead of telling the agent
/// to run git commands. This keeps agents isolated from git operations and
/// ensures they only review the changes made since the pipeline started.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
///
/// # Arguments
///
/// * `context` - The context level (minimal or normal)
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_detailed_review_without_guidelines_with_diff(
    context: ContextLevel,
    diff: &str,
) -> String {
    match context {
        ContextLevel::Minimal => format!(
            r#"You are in DETAILED REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DIFF below - Changes since the start of this pipeline
- DO NOT read .agent/STATUS.md or .agent/NOTES.md (you need unbiased perspective)

YOUR TASK:
Review ONLY the changes in the DIFF below. Focus on:
1) Code quality and correctness
2) Bugs, security, tests
3) Code style and maintainability

DIFF TO REVIEW:
```diff
{diff}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Description (blocks merge)
- [ ] High: [file:line] Description (should fix before merge)
- [ ] Medium: [file:line] Description (should address)
- [ ] Low: [file:line] Description (nice to have)

If no issues found in the changed files, return "No issues found.""#
        ),
        ContextLevel::Normal => format!(
            r#"You are in DETAILED REVIEW MODE.

YOUR TASK:
Review ONLY the changes in the DIFF below. Focus on:
- Bugs, security, tests
- Code style and maintainability

DIFF TO REVIEW:
```diff
{diff}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Blocks merge
- [ ] High: [file:line] Should fix before merge
- [ ] Medium: [file:line] Should address
- [ ] Low: [file:line] Nice to have

If no issues found in the changed files, return "No issues found.""#
        ),
    }
}

/// Generate incremental review prompt with diff included directly.
///
/// This version receives the diff as a parameter instead of telling the agent
/// to run git commands. This keeps agents isolated from git operations.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
///
/// # Arguments
///
/// * `context` - The context level (minimal or normal)
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_incremental_review_with_diff(context: ContextLevel, diff: &str) -> String {
    match context {
        ContextLevel::Minimal => format!(
            "You are in INCREMENTAL REVIEW MODE with fresh eyes.

INPUTS TO READ:
- DIFF below - Changes since the start of this pipeline

YOUR TASK:
Review ONLY the changes in the DIFF below. Focus on:
1) Code quality and correctness
2) Bugs, error handling, tests
3) Security regressions (inputs validated, outputs escaped)

DIFF TO REVIEW:
```diff
{diff}
```

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found, return \"No issues found in changed files.\""
        ),
        ContextLevel::Normal => format!(
            "You are in INCREMENTAL REVIEW MODE.

INPUTS TO READ:
- DIFF below - Changes since the start of this pipeline

DIFF TO REVIEW:
```diff
{diff}
```

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description"
        ),
    }
}

/// Generate a universal/simplified review prompt for maximum agent compatibility,
/// including the diff directly in the prompt.
///
/// This prompt is designed to work with a wide range of AI agents, including
/// those with weaker instruction-following capabilities. It:
/// - Uses simpler, more direct language
/// - Provides explicit output templates
/// - Minimizes complex structured instructions
/// - Includes the diff directly to keep agents isolated from git operations
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
///
/// # Arguments
///
/// * `context` - The context level (minimal or normal)
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_universal_review_with_diff(context: ContextLevel, diff: &str) -> String {
    match context {
        ContextLevel::Minimal => format!(
            r#"REVIEW TASK

Review ONLY the changes in the DIFF below for:
- Bugs, errors, security issues
- Missing tests
- Code quality and style

DIFF TO REVIEW:
```diff
{diff}
```

OUTPUT FORMAT

Return your findings using this format:

- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found in the changed files, return exactly: "No issues found."

IMPORTANT: Use the format [file:line] for each issue."#
        ),
        ContextLevel::Normal => format!(
            r#"REVIEW TASK

Review ONLY the changes in the DIFF below for quality and correctness.

DIFF TO REVIEW:
```diff
{diff}
```

OUTPUT FORMAT

Return your findings using this format:
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found in the changed files, return exactly: "No issues found.""#
        ),
    }
}
