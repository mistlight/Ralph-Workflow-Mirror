# Bug Fix: [Issue Title]

> **How to use this template:** This template helps you report bugs in a way that enables AI agents to fix them effectively. Provide as much detail as possible about reproduction steps and expected behavior.

## Goal
[One-line description of what needs to be fixed]

## Questions to Consider
Before fixing, investigate:
- Can you consistently reproduce the bug? What are the exact steps?
- What is the expected behavior vs. actual behavior?
- When does this occur? (specific conditions, inputs, or environments)
- Is this a regression? Did it work before? What changed?
- What is the root cause, not just the symptom?
- Are there similar issues elsewhere in the codebase?
- What tests should be added to prevent this from recurring?

## How to Write a Good Bug Report
Include the following information when possible:

**Reproduction Steps:**
Be specific and numbered:
1. Go to...
2. Click on...
3. See error...

**Context:**
- Error messages (copy the full text)
- Logs or stack traces
- Screenshots (if applicable to UI bugs)
- Environment (OS, browser, version, etc.)

## Issue
[Description of the bug]

**Steps to Reproduce:**
1. [First step]
2. [Second step]
3. [Third step]

**Actual Behavior:**
[What actually happens - include error messages]

**Expected Behavior:**
[What should happen instead]

**EXAMPLE:**
```markdown
## Issue
User login fails when password contains special characters like `@` or `#`.

**Steps to Reproduce:**
1. Navigate to login page
2. Enter username: test@example.com
3. Enter password: P@ssw0rd#123
4. Click login button

**Actual Behavior:**
Returns error "Invalid credentials" even with correct password.
Console shows: "URI malformed" error in URL encoding.

**Expected Behavior:**
User should be able to login with any valid password including special characters.
```

## Acceptance
- [Bug is fixed and no longer occurs]
- [Reproduction test case added to prevent regressions]
- [All existing tests pass]
- [No regressions in related functionality]
- [Error handling is robust with clear messages]

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

**Bug Fix Best Practices:**
- Fix the root cause, not just the symptom
- Add a regression test before fixing the bug (test should fail, then pass after fix)
- Consider similar code paths that may have the same vulnerability
- Ensure error messages are clear and actionable
- Handle edge cases that may have led to the bug
- Document the fix if the bug was subtle or non-obvious

**Testing Strategy for Bug Fixes:**
- Write a test case that reproduces the exact bug
- Test the fix with valid inputs to ensure no regression
- Test edge cases around the bug (boundary values, null/empty, etc.)
- Verify error handling paths are robust
- Consider adding property-based tests for related logic
