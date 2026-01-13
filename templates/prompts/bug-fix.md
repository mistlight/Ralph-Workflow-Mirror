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
