# Debug Triage: [Issue Title]

> **How to use this template:** This template is for investigating and debugging production issues or complex bugs. Use it when you need to systematically diagnose a problem.

## Goal
[Clear description of the issue that needs to be debugged]

**Tips for a good debug goal:**
- ✅ "Investigate why user sessions are expiring prematurely"
- ✅ "Diagnose memory leak in background worker process"
- ❌ "Fix the crash" (too vague)

**EXAMPLE:**
```markdown
## Goal
Investigate and diagnose why the API server returns 500 errors when processing large payloads (>10MB) specifically under high concurrency.
```

## Questions to Consider
Before debugging, gather information:

**Symptoms:**
- What exactly is happening? (be specific)
- When does it occur? (timing, frequency, patterns)
- Who/what is affected? (users, features, environments)
- What are the error messages, stack traces, or logs?

**Reproduction:**
- Can you reproduce the issue consistently?
- What are the minimal steps to reproduce?
- What conditions are required? (data, state, environment)

**Scope:**
- Is this a regression? Did it work before?
- What changed recently? (code, config, dependencies, environment)
- Is this isolated or affecting multiple areas?

**Impact:**
- How severe is this? (critical, high, medium, low)
- What's the business impact? (revenue, users, operations)
- Is there a workaround?

## Acceptance Checks
- [Root cause identified and documented]
- [Reproduction steps documented (if reproducible)]
- [Affected components/systems identified]
- [Recommended fix or workaround documented]
- [Preventive measures suggested to avoid recurrence]

## Investigation Notes

### What I've Already Tried
[List steps already taken to investigate or fix]

**EXAMPLE:**
```markdown
### What I've Already Tried
1. Checked application logs - found "OutOfMemoryError" at 2024-01-15 14:32:00
2. Reproduced locally with same payload size - no error
3. Checked staging environment - error occurs
4. Compared config between staging and production - staging has 1/4 memory
```

### Relevant Logs/Error Messages
[Paste relevant logs, stack traces, error messages]

### Environment
[Production, staging, dev?]
[OS, version, dependencies]
[Configuration differences]

## Code Quality Specifications

**Debugging Best Practices:**
- Start with the simplest explanation (Occam's razor)
- Form hypotheses and test them systematically
- Document everything you try (even what doesn't work)
- Use data and evidence over assumptions
- Check recent changes first (code, config, dependencies)
- Look for patterns and correlations

**What to Gather:**
- Full error messages and stack traces
- Relevant logs (application, system, database)
- Metrics and monitoring data (CPU, memory, latency)
- Database state (if applicable)
- Network traces (if applicable)

**Common Debugging Approaches:**
- Binary search: isolate when/where the issue occurs
- Minimize reproduction: reduce to smallest reproducible case
- Add logging: instrument code to gather more data
- Use debuggers: step through code to observe state
- Check assumptions: verify what you "know" to be true
