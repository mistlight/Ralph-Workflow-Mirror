# Code Review: [PR/Change Title]

> **How to use this template:** This template is conducting thorough code reviews of pull requests or code changes. It helps provide structured, actionable feedback to improve code quality.

## Goal
[Clear description of what code is being reviewed and what level of scrutiny is needed]

**Tips for a good review goal:**
- ✅ "Review authentication changes for security issues"
- ✅ "Review API refactoring for breaking changes"
- ❌ "Review this code" (too vague)

**EXAMPLE:**
```markdown
## Goal
Review the user session management changes for security vulnerabilities, thread safety, and proper error handling.
```

## Questions to Consider
Before the review, understand:

**Scope:**
- What files/changes are included in this review?
- Is this a refactor, new feature, bug fix, or breaking change?
- What is the context and background of this change?

**Code Quality:**
- Is the code readable and maintainable?
- Are there appropriate abstractions or is it over/under-engineered?
- Does it follow project conventions and style guidelines?
- Are there any obvious bugs or logic errors?

**Security:**
- Are there potential security vulnerabilities (injection, XSS, authentication issues)?
- Is sensitive data properly protected?
- Are there proper input validation and output sanitization?

**Testing:**
- Are there adequate tests for the new code?
- Do existing tests need to be updated?
- Are edge cases covered?

**Performance:**
- Are there performance concerns (N+1 queries, inefficient algorithms)?
- Does this change scale appropriately?

## Acceptance Checks
- [All high-priority issues identified and documented]
- [Security concerns flagged if present]
- [Code style and consistency issues noted]
- [Suggestions for improvements provided]
- [Overall assessment documented (approve, request changes, comment only)]

## Context
[Link to PR or diff]
[Background on why this change is being made]
[Any specific areas of concern to focus on]

## Code Quality Specifications

**Review Best Practices:**
- Be constructive: explain the issue and suggest improvements
- Focus on what matters: security, correctness, maintainability
- Ask questions when intent is unclear
- Celebrate good patterns and practices
- Separate must-fix from nice-to-have feedback

**What to Look For:**
- Single responsibility: one reason to change per function/class
- Small units: functions < 30 lines, classes < 300 lines
- Clear names that reveal intent
- Early returns; minimize nesting depth
- Explicit error handling; no silent failures
- No magic numbers; extract constants
- DRY: extract duplicated logic
- Validate at boundaries; trust internal data

**Security Checklist:**
- Validate all user input at system boundaries
- Sanitize data before display (prevent XSS)
- Use parameterized queries to prevent SQL injection
- Follow the principle of least privilege for permissions
- Never log sensitive data (passwords, tokens, PII)
