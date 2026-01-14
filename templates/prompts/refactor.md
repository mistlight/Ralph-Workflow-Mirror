# Refactor: [What is being refactored]

> **How to use this template:** This template is for improving existing code without changing its behavior. Refactoring is about changing code structure to make it more maintainable while preserving functionality.

## Goal
[What improvement you want to achieve]
[Why the current code needs refactoring]

**Common refactoring goals:**
- Reduce complexity or improve readability
- Eliminate code duplication
- Improve performance without changing behavior
- Make code easier to test or maintain
- Update to use newer patterns or APIs

## Questions to Consider
Before refactoring:

**Scope & Risk:**
- What specific problems exist with the current code? (complexity, duplication, coupling)
- What is the scope of this refactoring? (single function, module, or system-wide)
- Is the risk justified by the benefit?
- Will this affect public APIs or interfaces?

**Verification:**
- How will you verify behavior is unchanged?
- Are there existing tests that cover the affected code?
- What tests should be added before refactoring?

**Impact:**
- Are there performance implications of the refactoring?
- Could this introduce new bugs or edge cases?

## Refactor vs. Rewrite
- **Refactor:** Change structure, keep behavior. Small, incremental steps.
- **Rewrite:** Build from scratch. Higher risk, harder to verify.

When in doubt, prefer refactoring with small incremental changes.

## Acceptance
- [Code is cleaner/more maintainable]
- [Behavior is unchanged - all existing tests pass]
- [No changes to public APIs without deprecation]
- [Documentation updated if needed]
- [Code follows project style guidelines]

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

**Refactoring Best Practices:**
- Make small, incremental changes - commit after each logical step
- Run tests after every change to catch behavior regressions immediately
- Use the compiler/linter as your safety net (let Rust guide you)
- Preserve error handling behavior - don't swallow errors
- Keep performance characteristics in mind - some refactorings can be slower
- Update documentation as you go, not as an afterthought

**Behavior Preservation Verification:**
- Before starting, ensure comprehensive test coverage of the affected code
- If tests don't exist, write them first (test-driven refactoring)
- Use `git diff` to review changes before committing
- Consider using mutation testing or fuzzing for critical code paths
- Verify logging and monitoring still work as expected
- Check that error messages remain helpful and accurate
