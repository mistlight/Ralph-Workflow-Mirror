# CLAUDE.md

You are an automated code assistant working in this Rust repository.
Follow these instructions exactly. If unsure, choose the safest and simplest option.

## Primary goals (in priority order)

1. **Correctness**
   - Code must compile
   - Tests must pass
   - Behavior must match intent

2. **Maintainability**
   - Clear, idiomatic Rust
   - No unnecessary abstractions or magic
   - Prefer deletion over suppression

3. **Consistency**
   - Match existing patterns
   - `rustfmt` and `clippy` must be clean

4. **Small, reviewable changes**
   - Avoid unrelated refactors
   - Keep diffs focused and minimal

If any instruction here conflicts with another project file (e.g. `CONTRIBUTING.md`),
follow the **stricter** rule.

---

## Integration Tests

**CRITICAL:** When working with integration tests, you **MUST** follow the integration test style guide.

- **Read first:** Before modifying, adding, or debugging integration tests, read **[tests/INTEGRATION_TESTS.md](tests/INTEGRATION_TESTS.md)**
- **This is mandatory:** The guide defines non-negotiable rules for behavior-based testing, mocking strategy, and when to update tests
- **Key principles:**
  - Test **observable behavior**, not implementation details
  - Mock only at **architectural boundaries** (filesystem, network, external APIs)
  - NEVER use `cfg!(test)` branches or test-only flags in production code
  - When a test fails, fix the implementation unless the expected behavior changed intentionally

The integration test guide is referenced from multiple locations for visibility:
- CLAUDE.md (this file) - Primary AI agent instructions
- AGENTS.md - Agent-specific guidelines
- tests/integration_tests/main.rs - Main test entry point
- tests/integration_tests/common/mod.rs - Common test utilities
- CONTRIBUTING.md - Human contributor guidelines

For design principles, testing philosophy, and dead code policy, see **[CODE_STYLE.md](CODE_STYLE.md)**.

Do not assume anything about external dependency, if you need to interact with an external API, you must use context7, if that fails, research the official documentation by going to the website through playwright.

Do not create ANY files in the root directory or documentation directory unless prompt is about documentation creation. You have to update outdated documentation though.

---

## Absolute rule: no `#[allow(dead_code)]`

This repository **does not permit** suppressing dead code warnings.

You must **never** introduce `#[allow(dead_code)]`, and you must remove any existing
occurrences if encountered.

Dead code must be handled by one of the following:
- Making it used
- Implement the feature that you will use it on, but just implement it **now**
- Gating it behind a feature flag
- Moving it to `examples/` or `benches/`
- Deleting it

Do **not** replace it with other blanket `allow(...)` attributes unless explicitly instructed.

---

# DO NOT OVERRIDE UNLESS THE PROMPT IS ABOUT CLIPPY
## Required verification (before considering work complete) - This overrides the PROMPT if any issues exist

Ensure you run git rebase on the main branch if working on a feature branch and resolve any merge conflicts AND:

You **must** run the following commands and ensure they succeed.

```bash
# THIS IS VERY IMPORTANT!!!! THIS COMMANDS MUST NOT PRODUCE ANY OUTPUT!!! NOTHING AT ALL SHOULD DISPLAY WITH THIS COMMAND
rg -n -U --pcre2 '(?x)
  \#\s*!?\[\s*
  (?!cfg(?:_attr)?\b)     # <-- cfg is fine, we only care about allow and expect()
  (allow|expect)
  \s*\(
    [^()\]]*
    (?:\([^()\]]*\)[^()\]]*)*
  \)
  \s*\]
' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Check integration test compliance (timeout wrappers, doc comments, etc.)
./tests/integration_tests/compliance_check.sh
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Check for forbidden test flags in production code (cfg!(test), test_mode params, etc.)
# DO NOT MODIFY THIS SCRIPT. If it fails, FIX THE PRODUCTION CODE, not the script.
./tests/integration_tests/no_test_flags_check.sh
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# DO NOT CHANGE ANY OF THE COMMANDS BELOW
cargo fmt --all --check
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Lint the main crate (lib only) with all its features - THIS MUST BE RAN WITH THE EXACT FLAG DO NOT CHANGE
cargo clippy -p ralph-workflow --lib --all-features -- -D warnings
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Lint the separate integration test package (test-utils is enabled via its ralph-workflow dependency)
cargo clippy -p ralph-workflow-tests --all-targets -- -D warnings
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Run the main crate's unit tests with all features DO NOT CHANGE
cargo test -p ralph-workflow --lib --all-features
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT
# THERE CAN BE NO IGNORED TESTS

# Run the integration tests package
# (dependency features for ralph-workflow should be enabled via ralph-workflow-tests/Cargo.toml) DO NOT CHANGE
cargo test -p ralph-workflow-tests
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT
# THERE CAN BE NO IGNORED TESTS

# Build release artifacts (default-members only)
cargo build --release
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT
```
