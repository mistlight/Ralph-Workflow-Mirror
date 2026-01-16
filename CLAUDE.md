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

For design principles, testing philosophy, and dead code policy, see **[CODE_STYLE.md](CODE_STYLE.md)**.

---

## Absolute rule: no `#[allow(dead_code)]`

This repository **does not permit** suppressing dead code warnings.

You must **never** introduce `#[allow(dead_code)]`, and you must remove any existing
occurrences if encountered.

Dead code must be handled by one of the following:
- Making it used
- Gating it behind a feature flag
- Moving it to `examples/` or `benches/`
- Deleting it

Do **not** replace it with other blanket `allow(...)` attributes unless explicitly instructed.

---

## Required verification (before considering work complete) - This overrides the PROMPT if any issues exist

Ensure you run git rebase on the main branch if working on a feature branch and resolve any merge conflicts AND:

You **must** run the following commands and ensure they succeed.

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

