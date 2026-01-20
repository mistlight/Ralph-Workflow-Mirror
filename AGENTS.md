# AGENTS.md

This repository welcomes automated code assistants (“agents”) and human contributors.
Follow these rules so changes stay safe, consistent, and easy to review.

## Scope & priorities

Agents should optimize for, in order:

1. **Correctness** (tests pass; behavior matches intent)
2. **Maintainability** (clear code; minimal magic)
3. **Consistency** (follow existing patterns; rustfmt/clippy clean)
4. **Small, reviewable diffs** (avoid drive-by refactors)

If any instruction below conflicts with another file (e.g., `CONTRIBUTING.md`), follow the stricter rule.

For design principles, testing philosophy, and dead code policy, see **[CODE_STYLE.md](CODE_STYLE.md)**.

Do not assume anything about external dependency, if you need to interact with an external API, you must use context7, if that fails, research the official documentation by going to the website through playwright.

Do not create ANY files in the root directory or documentation directory unless prompt is about documentation creation. You have to update outdated documentation though.

---

## Integration Tests

**CRITICAL FOR AI AGENTS:** When working with integration tests, you **MUST** follow the integration test style guide.

- **Read first:** Before modifying, adding, or debugging integration tests, read **[tests/INTEGRATION_TESTS.md](tests/INTEGRATION_TESTS.md)**
- **This is mandatory:** The guide defines non-negotiable rules for behavior-based testing, mocking strategy, and when to update tests
- **Key principles:**
  - Test **observable behavior**, not implementation details
  - Mock only at **architectural boundaries** (filesystem, network, external APIs)
  - NEVER use `cfg!(test)` branches or test-only flags in production code
  - When a test fails, fix the implementation unless the expected behavior changed intentionally

**Common agent mistakes to avoid:**
- ❌ Mocking internal functions or helpers - Only mock external dependencies
- ❌ Testing private implementation details - Test through public APIs
- ❌ Adding `#[cfg(test)]` branches in production code - Refactor for dependency injection instead
- ❌ Updating tests because implementation changed - Only update if expected behavior changed
- ❌ Making real API calls to external services - Always mock external dependencies

**Required patterns:**
- Parser tests → Use `TestPrinter` from `ralph_workflow::json_parser::printer`
- File operations → Use `tempfile::TempDir` for isolation
- CLI tests → Use `assert_cmd::Command` for black-box testing

---

# DO NOT OVERRIDE UNLESS THE PROMPT IS ABOUT CLIPPY
## Build & test expectations

Dead code must either be removed or you implement the feature that it needs the dead code

Ensure you run git rebase on the main branch if working on a feature branch and resolve any merge conflicts AND:

Before opening a PR (or marking work “done”), run:

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

# Check for forbidden test flags in production code (cfg!(test), test_mode params, etc.)
# DO NOT MODIFY THIS SCRIPT. If it fails, FIX THE PRODUCTION CODE, not the script.
./tests/integration_tests/no_test_flags_check.sh
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

cargo fmt --all --check
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Lint the main crate (lib only) with all its features
cargo clippy -p ralph-workflow --lib --all-features -- -D warnings
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Lint the separate integration test package
# (test-utils feature is enabled via the integration test crate's Cargo.toml dependency on ralph-workflow)
cargo clippy -p ralph-workflow-tests --all-targets -- -D warnings
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Run the main crate's unit tests with all features
cargo test -p ralph-workflow --lib --all-features
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Run the integration tests package
# (dependency features for ralph-workflow should be enabled via the integration test crate's Cargo.toml)
cargo test -p ralph-workflow-tests
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

# Build release artifacts (default-members only)
cargo build --release
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT
