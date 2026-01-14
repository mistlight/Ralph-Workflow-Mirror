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

/// Generate a simple/vague reviewer review prompt (without guidelines).
///
/// This prompt is intentionally vague to preserve "fresh eyes" perspective
/// and avoid context pollution. For detailed actionable output, use
/// `prompt_detailed_review_without_guidelines` instead.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
pub fn prompt_reviewer_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in REVIEW MODE with fresh eyes perspective.

YOUR TASK:
Evaluate the codebase changes. Focus on:
1) Code quality and correctness
2) Bugs, error handling, tests, security
3) Code style and idiomatic usage

OUTPUT:
Return your findings as structured output.
If no issues found, return "No issues found.""#
            .to_string(),
        ContextLevel::Normal => r#"You are in REVIEW MODE.

YOUR TASK:
Review the codebase for:
- Code quality and correctness
- Bugs, error handling, tests
- Security issues

OUTPUT:
Return your findings as structured output.
If no issues, return "No issues found.""#
            .to_string(),
    }
}

/// Generate detailed reviewer review prompt without language-specific guidelines.
///
/// Use this when the review needs to produce actionable issues output
/// even if stack detection did not produce `ReviewGuidelines`.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
pub fn prompt_detailed_review_without_guidelines(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in DETAILED REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DO NOT read .agent/STATUS.md or .agent/NOTES.md (you need unbiased perspective)

YOUR TASK:
Produce actionable issues:
1) Code quality and correctness
2) Bugs, security, tests
3) Code style and maintainability

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Description (blocks merge)
- [ ] High: [file:line] Description (should fix before merge)
- [ ] Medium: [file:line] Description (should address)
- [ ] Low: [file:line] Description (nice to have)

If no issues found, return "No issues found.""#
            .to_string(),
        ContextLevel::Normal => r#"You are in DETAILED REVIEW MODE.

YOUR TASK:
Review for quality and correctness:
- Bugs, security, tests
- Code style and maintainability

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Blocks merge
- [ ] High: [file:line] Should fix before merge
- [ ] Medium: [file:line] Should address
- [ ] Low: [file:line] Nice to have

If no issues found, return "No issues found.""#
            .to_string(),
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
{}
```

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found, return \"No issues found in changed files.\"",
            diff
        ),
        ContextLevel::Normal => format!(
            "You are in INCREMENTAL REVIEW MODE.

INPUTS TO READ:
- DIFF below - Changes since the start of this pipeline

DIFF TO REVIEW:
```diff
{}
```

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description",
            diff
        ),
    }
}

/// Generate a universal/simplified review prompt for maximum agent compatibility.
///
/// This prompt is designed to work with a wide range of AI agents, including
/// those with weaker instruction-following capabilities. It:
/// - Uses simpler, more direct language
/// - Provides explicit output templates
/// - Minimizes complex structured instructions
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
///
/// Use this for agents like GLM, ZhipuAI, and other models that may struggle
/// with more complex prompts.
pub fn prompt_universal_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"REVIEW TASK

Review the codebase for:
- Bugs, errors, security issues
- Missing tests
- Code quality and style

OUTPUT FORMAT

Return your findings using this format:

- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found, return exactly: "No issues found."

IMPORTANT: Use the format [file:line] for each issue."#
            .to_string(),
        ContextLevel::Normal => r#"REVIEW TASK

Review the codebase for quality and correctness.

OUTPUT FORMAT

Return your findings using this format:
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found, return exactly: "No issues found.""#
            .to_string(),
    }
}
