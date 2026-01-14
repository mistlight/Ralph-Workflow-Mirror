# [Feature Name]

> **How to use this template:** This template is for implementing new features. The sections below help you think through the design and provide clear acceptance criteria for the AI agent.

## Goal
[Clear description of what you want to build]

**Tips for a good goal:**
- ✅ "Add user authentication with email/password"
- ✅ "Implement search with filters for date and category"
- ❌ "Improve user experience" (too vague)
- ❌ "Fix the thing" (doesn't say what)

## Questions to Consider
Before implementing, think through:

**Clarity:**
- What problem does this solve? Who is it for?
- What does "done" look like? How will you test it?

**Edge Cases:**
- What happens with invalid input?
- What about empty states or zero results?
- Are there concurrency or threading implications?

**Impact:**
- Are there performance, security, or accessibility implications?
- Are there breaking changes or migration considerations?
- What dependencies (internal or external) are involved?

**Security & Error Handling:**
- Are there potential security vulnerabilities (injection, XSS, authentication)?
- How should errors be handled and communicated to users?
- What sensitive data is involved and how should it be protected?
- Are there rate limiting or resource exhaustion concerns?

**Compatibility:**
- Will this require database migrations or schema changes?
- Are backward compatibility requirements (APIs, file formats)?
- Will this require changes to dependent services or clients?

## Acceptance Checks
- [Specific, testable condition 1]
- [Specific, testable condition 2]
- [Specific, testable condition 3]

**Tips for acceptance criteria:**
- Make them specific and measurable
- Focus on behavior, not implementation
- Include error cases and edge cases

## Constraints
- [Any limitations or requirements]
- [Performance requirements, if applicable]
- [Compatibility notes]

## Context
[Relevant background information]
[Why this change is needed]
[Impact on existing code]

## Implementation Notes (Optional)
[Architecture considerations]
[Potential approaches]
[Files/modules likely affected]

## Code Quality Specifications

Write clean, maintainable code:
- Single responsibility: one reason to change per function/class
- Small units: functions < 30 lines, classes < 300 lines
- Clear names that reveal intent
- Early returns; minimize nesting depth
- Explicit error handling; no silent failures
- No magic numbers; extract constants
- DRY: extract duplicated logic
- Validate at boundaries; trust internal data
- Test behavior, not implementation

**Feature Implementation Best Practices:**
- Start with the simplest working solution, optimize only if needed
- Prefer standard library solutions over external dependencies
- Add logging at key points (entry/exit of major functions, errors)
- Use types to make invalid states unrepresentable
- Document non-obvious design decisions in comments
- Consider the API ergonomics - is it pleasant to use?

**Security Considerations:**
- Validate all user input at system boundaries
- Sanitize data before display (prevent XSS)
- Use parameterized queries to prevent SQL injection
- Follow the principle of least privilege for permissions
- Never log sensitive data (passwords, tokens, PII)
- Consider rate limiting for public-facing features
