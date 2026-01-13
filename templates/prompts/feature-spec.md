# [Feature Name]

## Goal
[Clear description of what you want to build]

## Questions to Consider
Before implementing, think through:
- What problem does this solve? Who is it for?
- What are the edge cases or error conditions?
- Are there performance, security, or accessibility implications?
- How will this be tested? What does "done" look like?
- Are there breaking changes or migration considerations?
- What dependencies (internal or external) are involved?

## Acceptance Checks
- [Specific, testable condition 1]
- [Specific, testable condition 2]
- [Specific, testable condition 3]

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
