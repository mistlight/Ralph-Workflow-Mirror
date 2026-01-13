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
