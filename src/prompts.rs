//! Prompt Templates Module
//!
//! Provides context-controlled prompts for agents.
//! Key design: reviewers get minimal context for "fresh eyes" perspective.
//!
//! Enhanced with language-specific review guidelines based on detected project stack.

use crate::review_guidelines::ReviewGuidelines;

/// Context level for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextLevel {
    /// Minimal context (fresh eyes) - only essential info
    Minimal = 0,
    /// Normal context - includes status information
    Normal = 1,
}

impl From<u8> for ContextLevel {
    fn from(v: u8) -> Self {
        if v == 0 {
            ContextLevel::Minimal
        } else {
            ContextLevel::Normal
        }
    }
}

/// Generate developer iteration prompt
/// Note: We do NOT tell the agent how many total iterations exist.
/// This prevents "context pollution" - the agent should complete their task fully
/// without knowing when the loop ends.
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
pub(crate) fn prompt_developer_iteration(
    _iteration: u32,
    _total: u32,
    context: ContextLevel,
) -> String {
    match context {
        ContextLevel::Minimal | ContextLevel::Normal => {
            r#"You are in IMPLEMENTATION MODE. Execute the plan and make progress.

INPUTS TO READ:
1. .agent/PLAN.md - The implementation plan (execute these steps)
2. PROMPT.md - The original requirements (for reference)

YOUR TASK:
Execute the next steps from .agent/PLAN.md that haven't been completed yet.
Work toward satisfying all acceptance checks in PROMPT.md.

GUIDELINES:
- Make meaningful progress in each iteration
- Write clean, idiomatic code following project patterns
- Add tests where appropriate"#
                .to_string()
        }
    }
}

/// Generate reviewer review prompt with minimal context
/// Reviewer should NOT see what was done - just evaluate the code against requirements
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Follows best practices for unbiased code review:
/// - Fresh eyes perspective (minimal context mode)
/// - Output format is intentionally vague to avoid contaminating future runs
pub(crate) fn prompt_reviewer_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)

YOUR TASK:
Evaluate the codebase against the requirements in PROMPT.md.

1. GOAL ALIGNMENT - Does the implementation achieve the stated goal?
2. ACCEPTANCE CHECKS - Verify each check in PROMPT.md passes or fails
3. CODE QUALITY - Check for bugs, error handling, tests, security issues

OUTPUT:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "No issues found." (if everything looks good), OR
- "Issues found." (if you have any concerns)
Do not include any details or additional lines."#
            .to_string(),
        ContextLevel::Normal => r#"You are in REVIEW MODE.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)

YOUR TASK:
Review the repository against PROMPT.md requirements.

1. Check goal alignment - does implementation achieve the stated goal?
2. Verify each acceptance check passes
3. Examine code quality (bugs, error handling, tests, security)

OUTPUT:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "No issues found." (if everything looks good), OR
- "Issues found." (if you have any concerns)
Do not include any details or additional lines."#
            .to_string(),
    }
}

/// Generate reviewer review prompt with language-specific guidelines
///
/// This enhanced version incorporates guidelines from the detected project stack
/// to provide tailored review criteria for the specific language and frameworks.
pub(crate) fn prompt_reviewer_review_with_guidelines(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
) -> String {
    let guidelines_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)
- DO NOT read .agent/STATUS.md or .agent/NOTES.md (you need unbiased perspective)

YOUR TASK:
Evaluate the codebase against the requirements in PROMPT.md.

═══════════════════════════════════════════════════════════════════════════════
1. GOAL ALIGNMENT
═══════════════════════════════════════════════════════════════════════════════
- Does the implementation achieve the stated goal?
- Are there missing features or incomplete work?
- Does the solution match the intent of the requirements?

═══════════════════════════════════════════════════════════════════════════════
2. ACCEPTANCE CHECKS
═══════════════════════════════════════════════════════════════════════════════
Go through EACH acceptance check in PROMPT.md explicitly:
- Verify the check passes or fails
- Note specific evidence for your determination
- Be thorough - every check must be evaluated

═══════════════════════════════════════════════════════════════════════════════
3. CODE QUALITY (Language-Specific)
═══════════════════════════════════════════════════════════════════════════════
{guidelines}

═══════════════════════════════════════════════════════════════════════════════
4. GENERAL CHECKS
═══════════════════════════════════════════════════════════════════════════════
- Bugs or logic errors
- Missing or inadequate error handling
- Missing or inadequate tests
- Security vulnerabilities
- Performance concerns
- Documentation where needed

OUTPUT:
Write findings to .agent/ISSUES.md as a prioritized checklist:
- [ ] Critical: [file:line] Description of issue
- [ ] High: [file:line] Description of issue
- [ ] Medium: [file:line] Description of issue
- [ ] Low: [file:line] Description of issue

Priority Guide:
- Critical: Blocks functionality, security vulnerability, data loss risk
- High: Major bug, acceptance check failure, significant code smell
- Medium: Minor bug, code quality issue, missing edge case handling
- Low: Style issue, minor improvement suggestion, documentation

Be specific about file paths and line numbers.
If no issues found, write "No issues found." to .agent/ISSUES.md"#,
            guidelines = guidelines_section
        ),
        ContextLevel::Normal => format!(
            r#"You are in REVIEW MODE.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)
- .agent/STATUS.md - Current progress state

YOUR TASK:
Review the repository against PROMPT.md requirements.

1. Check goal alignment - does implementation achieve the stated goal?
2. Verify each acceptance check passes
3. Examine code quality with these language-specific checks:

{guidelines}

4. Check for bugs, error handling, tests, security

OUTPUT:
Write findings to .agent/ISSUES.md as a prioritized checklist:
- [ ] Critical: [file:line] Description
- [ ] High: [file:line] Description
- [ ] Medium: [file:line] Description
- [ ] Low: [file:line] Description

Be specific. If no issues, write "No issues found.""#,
            guidelines = guidelines_section
        ),
    }
}

/// Generate comprehensive review prompt with priority-based guidelines
///
/// This enhanced version uses severity-classified guidelines to help agents
/// prioritize their review efforts on the most critical issues first.
pub(crate) fn prompt_comprehensive_review(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
) -> String {
    let priority_section = guidelines.format_for_prompt_with_priorities();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in COMPREHENSIVE REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)
- DO NOT read .agent/STATUS.md or .agent/NOTES.md (you need unbiased perspective)

YOUR TASK:
Perform a thorough code review covering all aspects below.

═══════════════════════════════════════════════════════════════════════════════
1. FUNCTIONAL CORRECTNESS
═══════════════════════════════════════════════════════════════════════════════
- Does the implementation achieve the stated goal in PROMPT.md?
- Are there missing features or incomplete work?
- Does each acceptance check pass? (Evaluate each one explicitly)
- Are there bugs or logic errors that would cause incorrect behavior?

═══════════════════════════════════════════════════════════════════════════════
2. SECURITY ANALYSIS
═══════════════════════════════════════════════════════════════════════════════
- Check for injection vulnerabilities (SQL, command, XSS, etc.)
- Verify input validation on all external data
- Look for hardcoded secrets or credentials
- Check authentication/authorization logic
- Review data sanitization and encoding

═══════════════════════════════════════════════════════════════════════════════
3. PERFORMANCE & RESOURCES
═══════════════════════════════════════════════════════════════════════════════
- Identify potential performance bottlenecks
- Check for resource leaks (file handles, connections, memory)
- Review algorithm complexity for scaling concerns
- Look for unnecessary allocations or copies

═══════════════════════════════════════════════════════════════════════════════
4. CODE MAINTAINABILITY
═══════════════════════════════════════════════════════════════════════════════
- Is error handling comprehensive with meaningful messages?
- Are edge cases handled appropriately?
- Is the code readable and well-organized?
- Are there appropriate tests with good coverage?

═══════════════════════════════════════════════════════════════════════════════
5. LANGUAGE-SPECIFIC CHECKS (Priority-Ordered)
═══════════════════════════════════════════════════════════════════════════════
{priority_section}

OUTPUT:
Write findings to .agent/ISSUES.md as a prioritized checklist:
- [ ] Critical: [file:line] Description - Must fix before merge
- [ ] High: [file:line] Description - Should fix before merge
- [ ] Medium: [file:line] Description - Should address
- [ ] Low: [file:line] Description - Nice to have

CRITICAL issues are blockers: security vulnerabilities, data loss, crashes.
HIGH issues cause significant problems: major bugs, acceptance failures.
MEDIUM issues affect quality: minor bugs, code smells, missing edge cases.
LOW issues are improvements: style, documentation, suggestions.

Be specific about file paths and line numbers.
If no issues found, write "No issues found." to .agent/ISSUES.md"#,
            priority_section = priority_section
        ),
        ContextLevel::Normal => format!(
            r#"You are in COMPREHENSIVE REVIEW MODE.

INPUTS TO READ:
- PROMPT.md - Requirements (Goal and Acceptance checks)
- .agent/STATUS.md - Current progress

REVIEW CHECKLIST:
1. Functional: Does implementation match requirements? All acceptance checks pass?
2. Security: Injection risks? Input validation? No hardcoded secrets?
3. Performance: Bottlenecks? Resource leaks? Scaling concerns?
4. Maintainability: Error handling? Tests? Code clarity?

LANGUAGE-SPECIFIC (Priority-Ordered):
{priority_section}

OUTPUT to .agent/ISSUES.md:
- [ ] Critical: [file:line] Must fix (security, crashes, data loss)
- [ ] High: [file:line] Should fix (bugs, acceptance failures)
- [ ] Medium: [file:line] Address (quality, edge cases)
- [ ] Low: [file:line] Nice to have (style, docs)"#,
            priority_section = priority_section
        ),
    }
}

/// Generate security-focused review prompt
///
/// This prompt emphasizes security analysis above all else, useful for:
/// - Security-sensitive codebases
/// - Code handling user input, authentication, or sensitive data
/// - Pre-deployment security audits
///
/// Focuses on OWASP Top 10 and common vulnerability patterns.
pub(crate) fn prompt_security_focused_review(
    context: ContextLevel,
    guidelines: &ReviewGuidelines,
) -> String {
    let security_section = guidelines.format_for_prompt();

    match context {
        ContextLevel::Minimal => format!(
            r#"You are in SECURITY REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)
- DO NOT read .agent/STATUS.md or .agent/NOTES.md (you need unbiased perspective)

YOUR TASK:
Perform a security-focused review. Security issues are your TOP PRIORITY.

═══════════════════════════════════════════════════════════════════════════════
1. OWASP TOP 10 VULNERABILITIES
═══════════════════════════════════════════════════════════════════════════════
Check for:
- A01 Broken Access Control: Unauthorized access paths, missing auth checks
- A02 Cryptographic Failures: Weak encryption, hardcoded secrets, plain text
- A03 Injection: SQL, command, XSS, LDAP, template injection
- A04 Insecure Design: Missing security requirements, trust boundary violations
- A05 Security Misconfiguration: Default configs, unnecessary features, verbose errors
- A06 Vulnerable Components: Known CVEs, outdated dependencies
- A07 Auth Failures: Weak passwords, session issues, credential exposure
- A08 Data Integrity Failures: Unsafe deserialization, unsigned updates
- A09 Logging Failures: Missing audit trails, sensitive data in logs
- A10 SSRF: Server-side request forgery vulnerabilities

═══════════════════════════════════════════════════════════════════════════════
2. INPUT VALIDATION
═══════════════════════════════════════════════════════════════════════════════
- Are all external inputs validated?
- Is there proper encoding/escaping for output contexts?
- Are file paths checked for traversal attacks?
- Are numeric inputs bounds-checked?

═══════════════════════════════════════════════════════════════════════════════
3. AUTHENTICATION & AUTHORIZATION
═══════════════════════════════════════════════════════════════════════════════
- Are auth checks present on all sensitive endpoints?
- Is password hashing using modern algorithms (bcrypt, argon2)?
- Are sessions properly managed and invalidated?
- Is there proper RBAC/ABAC enforcement?

═══════════════════════════════════════════════════════════════════════════════
4. SECRETS & SENSITIVE DATA
═══════════════════════════════════════════════════════════════════════════════
- No hardcoded credentials, API keys, or secrets?
- Secrets loaded from environment/vault only?
- Sensitive data not logged or exposed in errors?
- Proper encryption at rest and in transit?

═══════════════════════════════════════════════════════════════════════════════
5. LANGUAGE-SPECIFIC SECURITY
═══════════════════════════════════════════════════════════════════════════════
{security_section}

OUTPUT:
Write findings to .agent/ISSUES.md as a prioritized checklist:
- [ ] Critical: [file:line] SECURITY - Description (immediate fix required)
- [ ] High: [file:line] SECURITY - Description (fix before merge)
- [ ] Medium: [file:line] Description (should address)
- [ ] Low: [file:line] Description (nice to have)

All security issues should be marked Critical or High.
Be specific about file paths, line numbers, and exploitation scenarios.
If no issues found, write "No security issues found." to .agent/ISSUES.md"#,
            security_section = security_section
        ),
        ContextLevel::Normal => format!(
            r#"You are in SECURITY REVIEW MODE.

INPUTS TO READ:
- PROMPT.md - Requirements to verify against
- .agent/STATUS.md - Current progress

SECURITY FOCUS:
1. OWASP Top 10: Injection, auth failures, broken access control
2. Input validation: All external data validated and escaped
3. Secrets: No hardcoded credentials, proper secret management
4. Auth/authz: Checks on all sensitive endpoints

LANGUAGE-SPECIFIC:
{security_section}

OUTPUT to .agent/ISSUES.md:
- [ ] Critical: [file:line] SECURITY - Immediate fix required
- [ ] High: [file:line] SECURITY - Fix before merge
- [ ] Medium/Low: Other issues"#,
            security_section = security_section
        ),
    }
}

/// Generate incremental review prompt for reviewing only changed files
///
/// This prompt is optimized for reviewing git diffs, useful for:
/// - Pull request reviews
/// - Post-commit review passes
/// - Focused review of specific changes
///
/// Uses git context to identify what changed.
pub(crate) fn prompt_incremental_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in INCREMENTAL REVIEW MODE with fresh eyes.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)
- Run `git diff HEAD~1` to see what changed in the last commit
- Run `git status` to see current uncommitted changes
- DO NOT read .agent/STATUS.md or .agent/NOTES.md (you need unbiased perspective)

YOUR TASK:
Review ONLY the changed files and lines. Focus on:

═══════════════════════════════════════════════════════════════════════════════
1. CHANGE ANALYSIS
═══════════════════════════════════════════════════════════════════════════════
- What files were modified/added/deleted?
- Do the changes align with the stated goal in PROMPT.md?
- Are there any unrelated changes (scope creep)?

═══════════════════════════════════════════════════════════════════════════════
2. CODE QUALITY (Changed Lines Only)
═══════════════════════════════════════════════════════════════════════════════
- Are the changes well-structured and readable?
- Do new functions/methods follow existing patterns?
- Is error handling appropriate for new code paths?
- Are new edge cases handled?

═══════════════════════════════════════════════════════════════════════════════
3. SECURITY (Changed Lines Only)
═══════════════════════════════════════════════════════════════════════════════
- Do new inputs have validation?
- Are new outputs properly escaped?
- Any new hardcoded secrets or credentials?
- Any new injection vulnerabilities?

═══════════════════════════════════════════════════════════════════════════════
4. INTEGRATION
═══════════════════════════════════════════════════════════════════════════════
- Do changes break existing functionality?
- Are all callers of modified functions updated?
- Are tests updated for changed behavior?

OUTPUT:
Write findings to .agent/ISSUES.md as a prioritized checklist:
- [ ] Critical: [file:line] Description (blocks merge)
- [ ] High: [file:line] Description (should fix)
- [ ] Medium: [file:line] Description (should address)
- [ ] Low: [file:line] Description (nice to have)

Be specific about which changes introduced each issue.
If no issues found, write "No issues found in changed files." to .agent/ISSUES.md"#
            .to_string(),
        ContextLevel::Normal => r#"You are in INCREMENTAL REVIEW MODE.

INPUTS TO READ:
- PROMPT.md - Requirements to verify against
- Run `git diff HEAD~1` and `git status` to see changes
- .agent/STATUS.md - Current progress

REVIEW CHANGED FILES ONLY:
1. Change alignment: Do changes match the goal?
2. Code quality: Is new code well-structured?
3. Security: Are new inputs validated, outputs escaped?
4. Integration: Do changes break anything?

OUTPUT to .agent/ISSUES.md:
- [ ] Critical: [file:line] Blocks merge
- [ ] High: [file:line] Should fix before merge
- [ ] Medium/Low: Address or nice to have"#
            .to_string(),
    }
}

/// Generate fix prompt (applies to either role)
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
pub(crate) fn prompt_fix() -> String {
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

/// Generate prompt for planning phase
/// Agent does a deep dive on PROMPT.md and creates a detailed PLAN.md
///
/// This prompt is designed to be agent-agnostic and follows best practices
/// from Claude Code's plan mode implementation:
/// - Multi-phase workflow (Understanding → Exploration → Design → Review → Final Plan)
/// - Strict read-only constraints during planning
/// - Critical files identification (3-5 files with justifications)
/// - Verification strategy
/// - Clear exit criteria
///
/// Reference: <https://github.com/Piebald-AI/claude-code-system-prompts>
pub(crate) fn prompt_plan() -> String {
    r#"You are in PLANNING MODE. Create a detailed implementation plan.

CRITICAL: This is a READ-ONLY planning task. You are STRICTLY PROHIBITED from:
- Creating, modifying, or deleting any files (except .agent/PLAN.md)
- Running any commands that modify system state
- Making commits or staging changes
- Installing dependencies or packages

You MAY use read-only operations: reading files, searching code, listing directories,
running `git status`, `git log`, `git diff`, or similar read-only commands.

═══════════════════════════════════════════════════════════════════════════════
PHASE 1: UNDERSTANDING
═══════════════════════════════════════════════════════════════════════════════
Read PROMPT.md thoroughly to understand:
- The Goal: What is the desired end state?
- Acceptance Checks: What specific conditions must be satisfied?
- Constraints: Any requirements, limitations, or quality standards mentioned?

If requirements are ambiguous, note them for clarification in the plan.

═══════════════════════════════════════════════════════════════════════════════
PHASE 2: EXPLORATION
═══════════════════════════════════════════════════════════════════════════════
Explore the codebase using read-only tools to understand:
- Current architecture and patterns used
- Relevant existing code that will need changes
- Dependencies and potential impacts
- Similar implementations that can serve as reference
- Test patterns and coverage expectations

Be thorough. Quality exploration leads to better plans.

═══════════════════════════════════════════════════════════════════════════════
PHASE 3: DESIGN
═══════════════════════════════════════════════════════════════════════════════
Design your implementation approach:
- What changes need to be made and in what order?
- Are there multiple valid approaches? Evaluate trade-offs.
- What are the potential risks or challenges?
- How does this integrate with existing code patterns?

═══════════════════════════════════════════════════════════════════════════════
PHASE 4: REVIEW
═══════════════════════════════════════════════════════════════════════════════
Validate your plan against the requirements:
- Does the approach satisfy ALL acceptance checks in PROMPT.md?
- Are there edge cases or error scenarios to handle?
- Is the plan specific enough to implement without ambiguity?
- Have you identified the correct files to modify?

═══════════════════════════════════════════════════════════════════════════════
PHASE 5: WRITE PLAN
═══════════════════════════════════════════════════════════════════════════════
Create .agent/PLAN.md with this structure:

## Summary
One paragraph explaining what will be done and why.

## Implementation Steps
Numbered, actionable steps. Be specific about:
- What each step accomplishes
- Which files are affected
- Dependencies between steps

## Critical Files for Implementation
List 3-5 key files that will be created or modified:
- `path/to/file.rs` - Brief justification for why this file needs changes

## Risks & Mitigations
Challenges identified during exploration and how to handle them.

## Verification Strategy
How to verify acceptance checks are met:
- Specific tests to run
- Manual verification steps
- Success criteria

REMEMBER: Do NOT implement anything. Your only output is .agent/PLAN.md."#
        .to_string()
}

/// Generate prompt for agent to generate a commit message
/// Agent writes the commit message to .agent/commit-message.txt
/// NOTES.md reference is explicitly optional since it may not exist in isolation mode.
pub(crate) fn prompt_generate_commit_message() -> String {
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

/// Role types for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Developer,
    Reviewer,
}

/// Action types for prompts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Plan,
    Iterate,
    Review,
    Fix,
    GenerateCommitMessage,
}

/// Generate a prompt for any agent type
///
/// The optional `guidelines` parameter allows providing language-specific review
/// guidance when the project stack has been detected. When provided, review prompts
/// will include tailored checks for the detected language and frameworks.
pub(crate) fn prompt_for_agent(
    role: Role,
    action: Action,
    context: ContextLevel,
    iteration: Option<u32>,
    total_iterations: Option<u32>,
    guidelines: Option<&ReviewGuidelines>,
) -> String {
    match (role, action) {
        (_, Action::Plan) => prompt_plan(),
        (Role::Developer, Action::Iterate) => prompt_developer_iteration(
            iteration.unwrap_or(1),
            total_iterations.unwrap_or(1),
            context,
        ),
        (_, Action::Review) => {
            if let Some(g) = guidelines {
                prompt_reviewer_review_with_guidelines(context, g)
            } else {
                prompt_reviewer_review(context)
            }
        }
        (_, Action::Fix) => prompt_fix(),
        (_, Action::GenerateCommitMessage) => prompt_generate_commit_message(),
        // Fallback for Reviewer + Iterate (shouldn't happen but be safe)
        (Role::Reviewer, Action::Iterate) => prompt_developer_iteration(
            iteration.unwrap_or(1),
            total_iterations.unwrap_or(1),
            context,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_developer_iteration() {
        let result = prompt_developer_iteration(2, 5, ContextLevel::Normal);
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        assert!(result.contains("IMPLEMENTATION MODE"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_prompt_reviewer_review_fresh_eyes() {
        let result = prompt_reviewer_review(ContextLevel::Minimal);
        assert!(result.contains("fresh eyes"));
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("ISSUES.md"));

        // Should have evaluation sections (now more vague)
        assert!(result.contains("GOAL ALIGNMENT"));
        assert!(result.contains("ACCEPTANCE CHECKS"));
        assert!(result.contains("CODE QUALITY"));

        // Should NOT have detailed priority guide (vague prompts)
        assert!(!result.contains("Priority Guide"));
        // Should NOT reference STATUS.md (isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_prompt_reviewer_review_normal() {
        let result = prompt_reviewer_review(ContextLevel::Normal);
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("REVIEW MODE"));
        assert!(!result.contains("fresh eyes"));
    }

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
    fn test_prompt_for_agent_developer() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            ContextLevel::Normal,
            Some(3),
            Some(10),
            None,
        );
        assert!(result.contains("PROMPT.md"));
    }

    #[test]
    fn test_prompt_for_agent_reviewer() {
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Review,
            ContextLevel::Minimal,
            None,
            None,
            None,
        );
        assert!(result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_plan() {
        let result = prompt_plan();
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("Implementation Steps"));
        assert!(result.contains("Critical Files"));
        assert!(result.contains("Verification Strategy"));

        // Ensure strict read-only constraints are present (Claude Code alignment)
        assert!(result.contains("READ-ONLY"));
        assert!(result.contains("STRICTLY PROHIBITED"));

        // Ensure 5-phase workflow structure (Claude Code alignment)
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        assert!(result.contains("PHASE 2: EXPLORATION"));
        assert!(result.contains("PHASE 3: DESIGN"));
        assert!(result.contains("PHASE 4: REVIEW"));
        assert!(result.contains("PHASE 5: WRITE PLAN"));
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
    fn test_prompt_for_agent_plan() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Plan,
            ContextLevel::Normal,
            None,
            None,
            None,
        );
        assert!(result.contains("PLAN.md"));
    }

    #[test]
    fn test_prompt_for_agent_generate_commit_message() {
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::GenerateCommitMessage,
            ContextLevel::Normal,
            None,
            None,
            None,
        );
        assert!(result.contains("commit-message.txt"));
    }

    #[test]
    fn test_prompts_are_agent_agnostic() {
        // All prompts should be free of agent-specific references
        // to ensure they work with any AI coding assistant
        let agent_specific_terms = [
            "claude", "codex", "opencode", "gemini", "aider", "goose", "cline", "continue",
            "amazon-q", "gpt", "copilot",
        ];

        let prompts_to_check = vec![
            prompt_developer_iteration(1, 5, ContextLevel::Normal),
            prompt_developer_iteration(1, 5, ContextLevel::Minimal),
            prompt_reviewer_review(ContextLevel::Normal),
            prompt_reviewer_review(ContextLevel::Minimal),
            prompt_fix(),
            prompt_plan(),
            prompt_generate_commit_message(),
        ];

        for prompt in prompts_to_check {
            let prompt_lower = prompt.to_lowercase();
            for term in agent_specific_terms {
                assert!(
                    !prompt_lower.contains(term),
                    "Prompt contains agent-specific term '{}': {}",
                    term,
                    &prompt[..prompt.len().min(100)]
                );
            }
        }
    }

    #[test]
    fn test_context_level_from_u8() {
        assert_eq!(ContextLevel::from(0), ContextLevel::Minimal);
        assert_eq!(ContextLevel::from(1), ContextLevel::Normal);
        assert_eq!(ContextLevel::from(2), ContextLevel::Normal);
        assert_eq!(ContextLevel::from(255), ContextLevel::Normal);
    }

    #[test]
    fn test_developer_iteration_minimal_context() {
        let result = prompt_developer_iteration(1, 5, ContextLevel::Minimal);
        // Minimal context should include essential files (not STATUS.md in isolation mode)
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_prompt_for_agent_fix() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Fix,
            ContextLevel::Normal,
            None,
            None,
            None,
        );
        assert!(result.contains("FIX MODE"));
        assert!(result.contains("ISSUES.md"));
    }

    #[test]
    fn test_reviewer_can_use_iterate_action() {
        // Edge case: Reviewer using Iterate action (fallback behavior)
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Iterate,
            ContextLevel::Normal,
            Some(1),
            Some(3),
            None,
        );
        // Should fall back to developer iteration prompt
        assert!(result.contains("IMPLEMENTATION MODE"));
    }

    #[test]
    fn test_prompt_reviewer_review_with_guidelines() {
        use crate::language_detector::ProjectStack;

        // Create a Rust project stack
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Actix".to_string()],
            has_tests: true,
            test_framework: Some("cargo test".to_string()),
            package_manager: Some("Cargo".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        let result = prompt_reviewer_review_with_guidelines(ContextLevel::Minimal, &guidelines);

        // Should contain standard review sections
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("GOAL ALIGNMENT"));
        assert!(result.contains("ACCEPTANCE CHECKS"));
        assert!(result.contains("CODE QUALITY"));
        assert!(result.contains("Language-Specific"));

        // Should contain guidelines content
        assert!(result.contains("SECURITY"));
        assert!(result.contains("PERFORMANCE"));
        assert!(result.contains("AVOID"));
    }

    #[test]
    fn test_prompt_for_agent_with_guidelines() {
        use crate::language_detector::ProjectStack;

        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Django".to_string()],
            has_tests: true,
            test_framework: Some("pytest".to_string()),
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Review,
            ContextLevel::Minimal,
            None,
            None,
            Some(&guidelines),
        );

        // Should use the enhanced prompt with guidelines
        assert!(result.contains("Language-Specific"));
        assert!(result.contains("SECURITY"));
    }

    #[test]
    fn test_prompt_for_agent_without_guidelines() {
        // When no guidelines are provided, should use the standard prompt
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Review,
            ContextLevel::Minimal,
            None,
            None,
            None,
        );

        // Should use standard prompt
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("fresh eyes"));
        // Should NOT contain language-specific section header
        assert!(!result.contains("Language-Specific"));
    }

    #[test]
    fn test_prompt_comprehensive_review() {
        use crate::language_detector::ProjectStack;

        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: true,
            test_framework: Some("cargo test".to_string()),
            package_manager: Some("Cargo".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);
        let result = prompt_comprehensive_review(ContextLevel::Minimal, &guidelines);

        // Should contain structured sections
        assert!(result.contains("COMPREHENSIVE REVIEW MODE"));
        assert!(result.contains("FUNCTIONAL CORRECTNESS"));
        assert!(result.contains("SECURITY ANALYSIS"));
        assert!(result.contains("PERFORMANCE & RESOURCES"));
        assert!(result.contains("CODE MAINTAINABILITY"));
        assert!(result.contains("LANGUAGE-SPECIFIC CHECKS"));

        // Should contain priority indicators from format_for_prompt_with_priorities
        assert!(result.contains("CRITICAL"));
        assert!(result.contains("HIGH"));
        assert!(result.contains("MEDIUM"));
        assert!(result.contains("LOW"));

        // Should contain fresh eyes directive
        assert!(result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_comprehensive_review_normal_context() {
        let guidelines = ReviewGuidelines::default();
        let result = prompt_comprehensive_review(ContextLevel::Normal, &guidelines);

        // Should be shorter and more concise
        assert!(result.contains("COMPREHENSIVE REVIEW MODE"));
        assert!(result.contains("Priority-Ordered"));
        assert!(!result.contains("fresh eyes")); // Normal context doesn't have fresh eyes
    }

    #[test]
    fn test_prompt_security_focused_review_minimal() {
        use crate::language_detector::ProjectStack;

        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Django".to_string()],
            has_tests: true,
            test_framework: Some("pytest".to_string()),
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);
        let result = prompt_security_focused_review(ContextLevel::Minimal, &guidelines);

        // Should contain security-focused content
        assert!(result.contains("SECURITY REVIEW MODE"));
        assert!(result.contains("OWASP TOP 10"));
        assert!(result.contains("Broken Access Control"));
        assert!(result.contains("Injection"));
        assert!(result.contains("Cryptographic Failures"));

        // Should contain input validation section
        assert!(result.contains("INPUT VALIDATION"));
        assert!(result.contains("external inputs validated"));

        // Should contain auth section
        assert!(result.contains("AUTHENTICATION & AUTHORIZATION"));
        assert!(result.contains("bcrypt"));

        // Should contain secrets section
        assert!(result.contains("SECRETS & SENSITIVE DATA"));
        assert!(result.contains("hardcoded credentials"));

        // Should have fresh eyes directive
        assert!(result.contains("fresh eyes"));

        // Should include language-specific section
        assert!(result.contains("LANGUAGE-SPECIFIC SECURITY"));
    }

    #[test]
    fn test_prompt_security_focused_review_normal() {
        let guidelines = ReviewGuidelines::default();
        let result = prompt_security_focused_review(ContextLevel::Normal, &guidelines);

        // Should be more concise
        assert!(result.contains("SECURITY REVIEW MODE"));
        assert!(result.contains("OWASP Top 10"));
        assert!(result.contains("Input validation"));
        assert!(result.contains("Secrets"));
        assert!(!result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_incremental_review_minimal() {
        let result = prompt_incremental_review(ContextLevel::Minimal);

        // Should contain incremental review sections
        assert!(result.contains("INCREMENTAL REVIEW MODE"));
        assert!(result.contains("git diff HEAD~1"));
        assert!(result.contains("git status"));

        // Should focus on changed files only
        assert!(result.contains("CHANGE ANALYSIS"));
        assert!(result.contains("Changed Lines Only"));

        // Should contain code quality section
        assert!(result.contains("CODE QUALITY"));
        assert!(result.contains("well-structured"));

        // Should contain security section
        assert!(result.contains("SECURITY"));
        assert!(result.contains("new inputs have validation"));

        // Should contain integration section
        assert!(result.contains("INTEGRATION"));
        assert!(result.contains("callers of modified functions"));

        // Should have fresh eyes directive
        assert!(result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_incremental_review_normal() {
        let result = prompt_incremental_review(ContextLevel::Normal);

        // Should be more concise
        assert!(result.contains("INCREMENTAL REVIEW MODE"));
        assert!(result.contains("git diff HEAD~1"));
        assert!(result.contains("CHANGED FILES ONLY"));
        assert!(!result.contains("fresh eyes"));
    }

    #[test]
    fn test_security_review_is_agent_agnostic() {
        let guidelines = ReviewGuidelines::default();
        let result = prompt_security_focused_review(ContextLevel::Minimal, &guidelines);
        let result_lower = result.to_lowercase();

        let agent_specific_terms = ["claude", "codex", "opencode", "gemini", "aider", "gpt"];

        for term in agent_specific_terms {
            assert!(
                !result_lower.contains(term),
                "Security review prompt contains agent-specific term '{}'",
                term
            );
        }
    }

    #[test]
    fn test_incremental_review_is_agent_agnostic() {
        let result = prompt_incremental_review(ContextLevel::Minimal);
        let result_lower = result.to_lowercase();

        let agent_specific_terms = ["claude", "codex", "opencode", "gemini", "aider", "gpt"];

        for term in agent_specific_terms {
            assert!(
                !result_lower.contains(term),
                "Incremental review prompt contains agent-specific term '{}'",
                term
            );
        }
    }

    // =========================================================================
    // Vague Prompt Tests (Context Contamination Prevention)
    // =========================================================================

    #[test]
    fn test_prompts_do_not_have_detailed_tracking_language() {
        // Prompts should NOT contain detailed history tracking language
        // to prevent context contamination in future runs
        let detailed_tracking_terms = [
            "iteration number",
            "phase completed",
            "previous iteration",
            "history of",
            "detailed log",
        ];

        let prompts_to_check = vec![
            prompt_developer_iteration(1, 5, ContextLevel::Normal),
            prompt_fix(),
        ];

        for prompt in prompts_to_check {
            let prompt_lower = prompt.to_lowercase();
            for term in detailed_tracking_terms {
                assert!(
                    !prompt_lower.contains(term),
                    "Prompt contains detailed tracking language '{}': {}",
                    term,
                    &prompt[..prompt.len().min(100)]
                );
            }
        }
    }

    #[test]
    fn test_reviewer_review_is_vague() {
        // Reviewer review prompt should NOT have detailed priority levels
        let result = prompt_reviewer_review(ContextLevel::Minimal);

        // Should NOT have structured priority format
        assert!(!result.contains("Priority Guide"));
        assert!(!result.contains("- [ ] Critical:"));
        assert!(!result.contains("- [ ] High:"));
        assert!(!result.contains("[file:line]"));
    }

    #[test]
    fn test_notes_md_references_are_minimal_or_absent() {
        // NOTES.md references should be minimal or absent (isolation mode removes these files)
        let developer_prompt = prompt_developer_iteration(1, 5, ContextLevel::Normal);
        let fix_prompt = prompt_fix();

        // Developer prompt should NOT mention NOTES.md at all (isolation mode)
        assert!(
            !developer_prompt.contains("NOTES.md"),
            "Developer prompt should not reference NOTES.md in isolation mode"
        );

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
