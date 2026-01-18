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

For integration test guidance (behavior-based testing, mocking strategy, when to update tests), see **[tests/INTEGRATION_TESTS.md](tests/INTEGRATION_TESTS.md)**.
This **MUST** be followed when dealing with integration tests

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
cargo fmt --all --check

# Lint the main crate (lib only) with all its features
cargo clippy -p ralph-workflow --lib --all-features -- -D warnings

# Lint the separate integration test package
# (test-utils feature is enabled via the integration test crate's Cargo.toml dependency on ralph-workflow)
cargo clippy -p ralph-workflow-tests --all-targets -- -D warnings

# Run the main crate's unit tests with all features
cargo test -p ralph-workflow --lib --all-features

# Run the integration tests package
# (dependency features for ralph-workflow should be enabled via the integration test crate's Cargo.toml)
cargo test -p ralph-workflow-tests

# Build release artifacts (default-members only)
cargo build --release
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT
