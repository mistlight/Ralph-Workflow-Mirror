use super::super::types::ContextLevel;
use crate::guidelines::ReviewGuidelines;

/// Generate reviewer review prompt with language-specific guidelines.
pub fn prompt_reviewer_review_with_guidelines(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
) -> String {
    let guidelines_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Evaluate the codebase against PROMPT.md (goal + acceptance checks), then apply:

Language-Specific checks:
{guidelines}

OUTPUT to .agent/ISSUES.md (prioritized checklist):
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

If no issues found, write "No issues found." to .agent/ISSUES.md"#,
            guidelines = guidelines_section
        ),
        ContextLevel::Normal => format!(
            r#"You are in REVIEW MODE.

INPUTS TO READ:
- PROMPT.md
- .agent/STATUS.md

Language-Specific checks:
{guidelines}

OUTPUT:
Write findings to .agent/ISSUES.md. If no issues, write "No issues found.""#,
            guidelines = guidelines_section
        ),
    }
}

/// Generate comprehensive review prompt with priority-based guidelines.
pub fn prompt_comprehensive_review(context: ContextLevel, guidelines: &ReviewGuidelines) -> String {
    let priority_section = guidelines.format_for_prompt_with_priorities();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in COMPREHENSIVE REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Perform a thorough review:
1) Goal alignment + each acceptance check (explicit pass/fail)
2) Security (injection, auth, secrets)
3) Performance/resources (bottlenecks, leaks)
4) Maintainability (error handling, tests)

LANGUAGE-SPECIFIC CHECKS (Priority-Ordered):
{priorities}

OUTPUT to .agent/ISSUES.md (prioritized checklist, with [file:line]):\n- [ ] Critical/High/Medium/Low ..."#,
            priorities = priority_section
        ),
        ContextLevel::Normal => format!(
            r#"You are in COMPREHENSIVE REVIEW MODE.

INPUTS TO READ:
- PROMPT.md
- .agent/STATUS.md

LANGUAGE-SPECIFIC CHECKS (Priority-Ordered):
{priorities}

OUTPUT to .agent/ISSUES.md: prioritized checklist."#,
            priorities = priority_section
        ),
    }
}

/// Generate security-focused review prompt with security-oriented guidelines.
pub fn prompt_security_focused_review(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
) -> String {
    let security_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in SECURITY REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

SECURITY FOCUS (OWASP TOP 10):
- Broken Access Control
- Injection
- Cryptographic Failures
- Security Misconfiguration

LANGUAGE-SPECIFIC SECURITY:
{security_section}

OUTPUT to .agent/ISSUES.md:
- [ ] Critical: [file:line] SECURITY - Immediate fix required
- [ ] High: [file:line] SECURITY - Fix before merge
- [ ] Medium/Low: [file:line] SECURITY - Address as needed

If no issues found, write \"No security issues found.\" to .agent/ISSUES.md"#,
            security_section = security_section
        ),
        ContextLevel::Normal => format!(
            r#"You are in SECURITY REVIEW MODE.

INPUTS TO READ:
- PROMPT.md
- .agent/STATUS.md

LANGUAGE-SPECIFIC:
{security_section}

OUTPUT to .agent/ISSUES.md:
- [ ] Critical/High/Medium/Low: [file:line] SECURITY ..."#,
            security_section = security_section
        ),
    }
}
