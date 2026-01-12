use super::super::types::ContextLevel;

/// Generate a simple/vague reviewer review prompt (without guidelines).
///
/// This prompt is intentionally vague to preserve "fresh eyes" perspective
/// and avoid context pollution. For detailed actionable output, use
/// `prompt_detailed_review_without_guidelines` instead.
pub fn prompt_reviewer_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)

YOUR TASK:
Evaluate the codebase against PROMPT.md. Focus on:
1) Goal alignment
2) Each acceptance check (explicit pass/fail)
3) Bugs, error handling, tests, security

OUTPUT:
Write findings to .agent/ISSUES.md.
If no issues found, write "No issues found." to .agent/ISSUES.md"#
            .to_string(),
        ContextLevel::Normal => r#"You are in REVIEW MODE.

INPUTS TO READ:
- PROMPT.md

YOUR TASK:
Review the repository against PROMPT.md requirements (goal, acceptance checks, quality).

OUTPUT:
Write findings to .agent/ISSUES.md.
If no issues, write "No issues found.""#
            .to_string(),
    }
}

/// Generate detailed reviewer review prompt without language-specific guidelines.
///
/// Use this when the review needs to produce actionable `.agent/ISSUES.md` output
/// even if stack detection did not produce `ReviewGuidelines`.
pub fn prompt_detailed_review_without_guidelines(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in DETAILED REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)
- DO NOT read .agent/STATUS.md or .agent/NOTES.md (you need unbiased perspective)

YOUR TASK:
Produce actionable issues against PROMPT.md:
1) Goal alignment
2) Each acceptance check (explicit pass/fail with evidence)
3) Code quality, bugs, security, tests

OUTPUT to .agent/ISSUES.md (prioritized checklist):
- [ ] Critical: [file:line] Description (blocks merge)
- [ ] High: [file:line] Description (should fix before merge)
- [ ] Medium: [file:line] Description (should address)
- [ ] Low: [file:line] Description (nice to have)

If no issues found, write "No issues found." to .agent/ISSUES.md"#
            .to_string(),
        ContextLevel::Normal => r#"You are in DETAILED REVIEW MODE.

INPUTS TO READ:
- PROMPT.md
- .agent/STATUS.md

YOUR TASK:
Review against PROMPT.md (goal, acceptance checks, quality).

OUTPUT to .agent/ISSUES.md:
- [ ] Critical: [file:line] Blocks merge
- [ ] High: [file:line] Should fix before merge
- [ ] Medium: [file:line] Should address
- [ ] Low: [file:line] Nice to have

If no issues found, write "No issues found.""#
            .to_string(),
    }
}

/// Generate incremental review prompt for reviewing only changed files.
pub fn prompt_incremental_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in INCREMENTAL REVIEW MODE with fresh eyes.

INPUTS TO READ:
- PROMPT.md
- Run `git diff HEAD~1` and `git status` to see changes
- DO NOT read .agent/STATUS.md or .agent/NOTES.md

YOUR TASK:
Review ONLY the changed files/lines. Focus on:
1) Alignment with PROMPT.md goal/acceptance checks
2) Bugs, error handling, tests
3) Security regressions (inputs validated, outputs escaped)

OUTPUT:
Write findings to .agent/ISSUES.md as a prioritized checklist.
If no issues found, write \"No issues found in changed files.\" to .agent/ISSUES.md"#
            .to_string(),
        ContextLevel::Normal => r#"You are in INCREMENTAL REVIEW MODE.

INPUTS TO READ:
- PROMPT.md
- Run `git diff HEAD~1` and `git status`
- .agent/STATUS.md

OUTPUT to .agent/ISSUES.md:
- [ ] Critical/High/Medium/Low: [file:line] Description"#
            .to_string(),
    }
}

