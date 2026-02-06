# AGENTS.md

ALWAYS USE test-driven-development!

## Non-Negotiables

- **TDD is required for all code changes.** No production code without a failing test first.
- **Verification is required for ANY code change** (prod code or tests): run `docs/agents/verification.md` before PR/completion.
- **Architecture reading is REQUIRED** before any pipeline/reducer/behavioral change: `CODE_STYLE.md` (Architecture), `docs/architecture/event-loop-and-reducers.md`, `docs/architecture/effect-system.md`.
- **Integration test guide is REQUIRED reading** before writing/changing tests: `docs/agents/integration-tests.md` and `tests/INTEGRATION_TESTS.md`.
- **Do not introduce tech debt.** If the alternative is adding/keeping tech debt, **prefer refactor** even when it makes the diff larger; do not leave deprecated/unused code behind.

This repository welcomes automated code assistants ("agents") and human contributors.
Follow these rules so changes stay safe, consistent, and easy to review.

## Priorities (in order)

1. **Correctness** - tests pass, behavior matches intent
2. **Maintainability** - clear code, no magic
3. **Consistency** - follow existing patterns, rustfmt/clippy clean
4. **Small diffs** - keep changes focused *if possible*; if the alternative is adding/keeping tech debt, **prefer refactor** even when it makes the diff larger

If instructions conflict with other files (e.g., `CONTRIBUTING.md`), follow the **stricter** rule.

See **[CODE_STYLE.md](CODE_STYLE.md)** for design principles and testing philosophy.

If you change **pipeline behavior** (phases, retries/fallback, effect sequencing, checkpoint/resume, or any reducer/event/effect shape), the reducer/effect architecture reading is **REQUIRED**: `CODE_STYLE.md` (Architecture section), `docs/architecture/event-loop-and-reducers.md`, `docs/architecture/effect-system.md`.

## Where The Details Live

- Filesystem I/O rules (Workspace vs `std::fs`, exceptions): `docs/agents/workspace-trait.md`
- Integration test rules and patterns: `docs/agents/integration-tests.md` and `tests/INTEGRATION_TESTS.md`
- Required verification commands (must produce **NO OUTPUT**): `docs/agents/verification.md`
- Custom lints (dylint), env vars, troubleshooting: `docs/tooling/dylint.md`

---

## File Creation Rules

- **NO temporary .md files** in root or doc folders
- **NO new files** in root/doc directories unless explicitly about documentation
- **DO** update outdated documentation when encountered
- **ALL temporary files MUST go in `tmp/` at the repo root** (gitignored); use a unique subdir like `tmp/ralph-workflow-*` if needed

---

## External Dependencies

Never assume API behavior. Research order:
1. Use context7
2. If that fails, check official docs via playwright

---

## YOLO Mode (CRITICAL)

All agents MUST run with YOLO mode enabled (`--dangerously-skip-permissions` for Claude CLI, `--yes` for Aider).

**Why:** Ralph is a fully automated pipeline. All roles (Developer, Reviewer, Commit) write XML to `.agent/tmp/`. Without write permissions, the XSD retry mechanism fails.

**Configuration:** Every agent needs `yolo_flag` in `agents.toml`:
- Claude CLI: `--dangerously-skip-permissions`
- Aider: `--yes`
- Claude Code: Usually no flag needed

---

## Integration Tests (CRITICAL)

Read `tests/INTEGRATION_TESTS.md` (and `docs/agents/integration-tests.md`) before touching integration tests.

---

## Workspace Trait (CRITICAL)

Read `docs/agents/workspace-trait.md` before doing any filesystem I/O.

---

## No `#[allow(dead_code)]`

**Never** introduce or keep `#[allow(dead_code)]` (or deprecated code). Consider Dead Code removal to be EVEN MORE IMPORTANT! than the actual current task. It is very important no dead code exists.

Handle dead code by:
- Making it used
- Implementing the feature now (no time constraints)
- Gating behind a feature flag
- Moving to `examples/` or `benches/`
- Deleting it

---

## Required Verification (before PR/completion)

Run the commands in `docs/agents/verification.md`. All commands must produce **NO OUTPUT**.

**Additional verification for metrics changes:**

When changing iteration/retry/continuation/fallback logic, run the metrics tests to ensure metrics remain accurate and no drift occurs:

```bash
# Metrics unit tests
cargo test --lib reducer::state_reduction::tests::metrics

# Metrics integration tests
cargo test --test '*' iteration_counter
cargo test --test '*' continuation_budget
cargo test --test '*' summary_consistency
```

All tests must pass with NO OUTPUT (warnings or failures).

**Additional verification for logging changes:**

When changing per-run logging infrastructure, event loop logging, or log file paths, run the logging tests to ensure the logging system remains correct:

```bash
# Per-run logging infrastructure tests
cargo test --test '*' logging_per_run

# Event loop trace dump tests
cargo test --test '*' event_loop_trace_dump
```

All tests must pass with NO OUTPUT (warnings or failures).

---

## Custom Lints (dylint)

See `docs/tooling/dylint.md`.
