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

## Build & test expectations

Ensure you run git rebase on the main branch if working on a feature branch and resolve any merge conflicts AND:

Before opening a PR (or marking work “done”), run:

```bash
# THIS IS VERY IMPORTANT!!!! THESE TWO COMMANDS MUST PRODUCE NO OUTPUT!!! NOTHING AT ALL SHOULD DISPLAY WITH THIS COMMAND
rg -n -U --pcre2 '(?x)
  \#\s*!?\[\s*
  (allow|expect)
  \s*\(
    [^()\]]*
    (?:\([^()\]]*\)[^()\]]*)*
  \)
  \s*\]
' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

rg -n -U --pcre2 '(?x)
  \#\s*!?\[\s*cfg_attr\s*\(
    [^()]*?
    \b(allow|expect)\s*\(
  ' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE, 
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE, 
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT

