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

use super::super::types::ContextLevel;
use crate::guidelines::ReviewGuidelines;

/// Generate reviewer review prompt with language-specific guidelines.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
pub fn prompt_reviewer_review_with_guidelines(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
) -> String {
    let guidelines_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Evaluate the codebase, then apply language-specific checks:

Language-Specific checks:
{guidelines_section}

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found, return "No issues found.""#
        ),
        ContextLevel::Normal => format!(
            r#"You are in REVIEW MODE.

Language-Specific checks:
{guidelines_section}

OUTPUT (prioritized checklist):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues, return "No issues found.""#
        ),
    }
}

/// Generate comprehensive review prompt with priority-based guidelines.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
pub fn prompt_comprehensive_review(context: ContextLevel, guidelines: &ReviewGuidelines) -> String {
    let priority_section = guidelines.format_for_prompt_with_priorities();

    match context {
        ContextLevel::Minimal => format!(
            r"You are in COMPREHENSIVE REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Perform a thorough review:
1) Code quality and correctness
2) Security (injection, auth, secrets)
3) Performance/resources (bottlenecks, leaks)
4) Maintainability (error handling, tests)

LANGUAGE-SPECIFIC CHECKS (Priority-Ordered):
{priority_section}

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description"
        ),
        ContextLevel::Normal => format!(
            r"You are in COMPREHENSIVE REVIEW MODE.

LANGUAGE-SPECIFIC CHECKS (Priority-Ordered):
{priority_section}

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description"
        ),
    }
}

/// Generate security-focused review prompt with security-oriented guidelines.
///
/// The reviewer returns structured issues data (captured by JSON parser)
/// and the orchestrator writes it to .agent/ISSUES.md.
pub fn prompt_security_focused_review(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
) -> String {
    let security_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in SECURITY REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

SECURITY FOCUS (OWASP TOP 10):
- Broken Access Control
- Injection
- Cryptographic Failures
- Security Misconfiguration

LANGUAGE-SPECIFIC SECURITY:
{security_section}

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] SECURITY - Immediate fix required
- [ ] High: [file:line] SECURITY - Fix before merge
- [ ] Medium: [file:line] SECURITY - Address as needed
- [ ] Low: [file:line] SECURITY - Nice to have

If no issues found, return "No security issues found.""#
        ),
        ContextLevel::Normal => format!(
            r"You are in SECURITY REVIEW MODE.

LANGUAGE-SPECIFIC SECURITY:
{security_section}

OUTPUT (prioritized checklist with [file:line]):
- [ ] Critical: [file:line] SECURITY - Description
- [ ] High: [file:line] SECURITY - Description
- [ ] Medium: [file:line] SECURITY - Description
- [ ] Low: [file:line] SECURITY - Description"
        ),
    }
}
