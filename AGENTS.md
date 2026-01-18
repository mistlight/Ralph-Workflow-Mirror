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

cargo fmt --all
# Check lib with all features (including test-utils)
cargo clippy -p ralph-workflow --lib --all-features -- -D warnings
# Check bins without test-utils feature (binary doesn't use test utilities)
cargo clippy -p ralph-workflow --bins -- -D warnings
# Run lib tests with all features
cargo test --lib --all-features
# Run integration tests (test-utils is enabled via their Cargo.toml)
cargo test -p ralph-workflow-tests
cargo build --release
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

