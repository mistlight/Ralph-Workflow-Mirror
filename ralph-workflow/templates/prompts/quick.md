# Quick Change: [Brief title]

> **How to use this template:** This template is for small, straightforward changes. Fill in the goal and acceptance criteria below to guide the AI agent.

## Goal
[One-line description of the change]

**EXAMPLE:**
```markdown
## Goal
Update the copyright year in the footer from 2023 to 2024.
```

## Questions to Consider
Quick check before implementing:
- What exactly needs to change?
- Are there any edge cases to consider?
- How will you verify it works?

## Acceptance
- [The change works as expected]
- [No obvious regressions introduced]

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
