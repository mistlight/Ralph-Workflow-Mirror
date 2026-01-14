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

---

## Build & test expectations

Before opening a PR (or marking work “done”), run:

```bash
# THIS MUST BE EMPTY
grep -RIn --include='*.rs' --exclude-dir target --exclude-dir .git \
  'allow\s*(\s*dead_code\s*)' .
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

