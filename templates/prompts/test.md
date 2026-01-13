# Test Coverage: [Area being tested]

> **How to use this template:** This template is for adding or improving test coverage. Good tests catch bugs early and make refactoring safer.

## Goal
[What testing you want to add]
[What gaps currently exist in test coverage]

## Questions to Consider
Before adding tests:

**What to Test:**
- What are the critical paths that must work?
- What edge cases or boundary conditions exist?
- How can the code fail? (null inputs, network errors, invalid data)
- Are there integration points that need coverage?
- What behavior changes would be most dangerous?

**Test Type:**
- Is the focus on unit tests, integration tests, or end-to-end?
- Should tests use mocks or real dependencies?

## Acceptance
- [New tests cover identified gaps]
- [All tests pass (unit + integration)]
- [Edge cases and error paths are covered]
- [Coverage metrics improved]
- [Tests are maintainable and clear]

## Code Quality Specifications

Write clean, maintainable tests:
- Test behavior, not implementation details
- Use descriptive test names that explain what is being tested
- One assertion per test when possible
- Arrange-Act-Assert pattern for clarity
- Tests should be independent and can run in any order
- Use fixtures for common test setup
