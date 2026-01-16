//! Guided reviewer prompts with language-specific guidelines.
//!
//! This module generates review prompts that incorporate [`ReviewGuidelines`]
//! tailored to the detected project stack. The guidelines provide language and
//! framework-specific checks (e.g., Rust unsafe usage, React hooks rules, Django
//! CSRF protection) that help the reviewer focus on relevant concerns.
//!
//! Available prompt types:
//! - **Standard review**: Basic guideline integration
//! - **Comprehensive review**: Priority-ordered checks with severity levels
//! - **Security-focused review**: OWASP Top 10 combined with language-specific
//!   security checks

#![allow(clippy::uninlined_format_args)]

use super::super::types::ContextLevel;
use crate::guidelines::ReviewGuidelines;

/// Generate reviewer review prompt with language-specific guidelines,
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
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_reviewer_review_with_guidelines_and_diff(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff: &str,
) -> String {
    let guidelines_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DIFF below - Changes since the start of this pipeline
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Review ONLY the changes in the DIFF below, then apply language-specific checks:

Language-Specific checks:
{}

DIFF TO REVIEW:
```diff
{}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found in the changed files, return "No issues found.""#,
            guidelines_section, diff
        ),
        ContextLevel::Normal => format!(
            r#"You are in REVIEW MODE.

YOUR TASK:
Review ONLY the changes in the DIFF below, then apply language-specific checks:

Language-Specific checks:
{}

DIFF TO REVIEW:
```diff
{}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found in the changed files, return "No issues found.""#,
            guidelines_section, diff
        ),
    }
}

/// Generate comprehensive review prompt with priority-based guidelines,
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
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_comprehensive_review_with_diff(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff: &str,
) -> String {
    let priority_section = guidelines.format_for_prompt_with_priorities();

    match context {
        ContextLevel::Minimal => format!(
            r"You are in COMPREHENSIVE REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DIFF below - Changes since the start of this pipeline
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Review ONLY the changes in the DIFF below:
1) Code quality and correctness
2) Security (injection, auth, secrets)
3) Performance/resources (bottlenecks, leaks)
4) Maintainability (error handling, tests)

LANGUAGE-SPECIFIC CHECKS (Priority-Ordered):
{}

DIFF TO REVIEW:
```diff
{}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description",
            priority_section, diff
        ),
        ContextLevel::Normal => format!(
            r"You are in COMPREHENSIVE REVIEW MODE.

YOUR TASK:
Review ONLY the changes in the DIFF below.

LANGUAGE-SPECIFIC CHECKS (Priority-Ordered):
{}

DIFF TO REVIEW:
```diff
{}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description",
            priority_section, diff
        ),
    }
}

/// Generate security-focused review prompt with security-oriented guidelines,
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
/// * `guidelines` - The language-specific review guidelines
/// * `diff` - The git diff to review (changes since pipeline start)
pub fn prompt_security_focused_review_with_diff(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
    diff: &str,
) -> String {
    let security_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in SECURITY REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DIFF below - Changes since the start of this pipeline
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Review ONLY the changes in the DIFF below for security issues:

SECURITY FOCUS (OWASP TOP 10):
- Broken Access Control
- Injection
- Cryptographic Failures
- Security Misconfiguration

LANGUAGE-SPECIFIC SECURITY:
{}

DIFF TO REVIEW:
```diff
{}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] SECURITY - Immediate fix required
- [ ] High: [file:line] SECURITY - Fix before merge
- [ ] Medium: [file:line] SECURITY - Address as needed
- [ ] Low: [file:line] SECURITY - Nice to have

If no security issues found in the changed files, return "No security issues found.""#,
            security_section, diff
        ),
        ContextLevel::Normal => format!(
            r#"You are in SECURITY REVIEW MODE.

YOUR TASK:
Review ONLY the changes in the DIFF below for security issues.

LANGUAGE-SPECIFIC SECURITY:
{}

DIFF TO REVIEW:
```diff
{}
```

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] SECURITY - Description
- [ ] High: [file:line] SECURITY - Description
- [ ] Medium: [file:line] SECURITY - Description
- [ ] Low: [file:line] SECURITY - Description

If no security issues found in the changed files, return "No security issues found.""#,
            security_section, diff
        ),
    }
}
