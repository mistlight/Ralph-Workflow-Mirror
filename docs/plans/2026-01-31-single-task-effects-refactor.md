# Single-Task Effects Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor the reducer/effects pipeline so every `Effect` is truly single-task, all branching/policy lives in the reducer, and phases progress via explicit `PipelineEvent`s (no hidden logic in handlers/phases).

**Architecture:** Replace macro-effects like `RunReviewPass/RunFixAttempt/RunDevelopmentIteration/GeneratePlan/GenerateCommitMessage` with explicit, composable effect chains: (1) prepare context files, (2) invoke agent, (3) extract output, (4) validate output, (5) write derived artifacts, (6) archive/cleanup. XSD retry and continuation are reducer-driven and represented only via reducer state + events.

**Tech Stack:** Rust, reducer/event-loop architecture (`ralph-workflow/src/reducer/*`), `Workspace` trait + `MemoryWorkspace` tests, integration tests under `tests/integration_tests/*`.

---

## Non-Negotiable Constraints

- Effects obey `ralph-workflow/src/reducer/effect.rs` “Single-Task Effect Principle”.
- Reducer is the only place allowed to decide: retry, fallback, phase transitions.
- Handlers execute one effect and emit observations via events.
- No new `std::fs` usage outside documented exceptions; use `Workspace`.
- Code style: keep files under 1000 lines by splitting into modules as needed (especially in `ralph-workflow/src/reducer/*`).
- TDD for every behavior change: add a failing test first, watch it fail, then implement.

## High-Level Steps

1) Split oversized reducer modules (>1000 lines) into submodules (no behavior change; keep tests passing).
2) Introduce per-phase progress state + new events/effects (RED -> GREEN).
3) Refactor each macro-effect phase to a single-task effect chain: Review/Fix, Plan, Dev, CommitMessage.
4) Delete legacy macro-effects and any dead code.
5) Add/extend integration tests for end-to-end pipeline + XSD retry edges.
6) Run full verification suite.

## Verification Commands (Must Be Pristine)

```bash
rg -n -U --pcre2 '(?m)^\s*#\s*!?\[\s*(?:(?:allow|expect)\s*\(|cfg_attr\s*\((?:[^()]|\([^()]*\))*?,\s*(?:allow|expect)\s*\()' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .
./tests/integration_tests/compliance_check.sh
./tests/integration_tests/no_test_flags_check.sh
cargo fmt --all --check
cargo clippy -p ralph-workflow --lib --all-features -- -D warnings
cargo clippy -p ralph-workflow-tests --all-targets -- -D warnings
cargo test -p ralph-workflow --lib --all-features
cargo test -p ralph-workflow-tests
cargo build --release
```
