# Architecture Docs Index

These documents describe the *current* architecture of the `ralph-workflow` crate.

- Canonical behavior is the Rust code in `ralph-workflow/src/`.
- RFCs in `docs/RFC/` are historical; do not treat them as canonical.

## Bird's-Eye View

Ralph is an unattended agent orchestrator built around a reducer-driven event loop.

```
state --orchestrate--> effect --handle--> event --reduce--> next_state
```

- `PipelineState` is the single source of truth and the checkpoint payload.
- Orchestration is pure: `state -> next Effect`.
- Reducers are pure: `(state, event) -> next_state`.
- Effect handlers are impure: they do I/O (agents, git, filesystem) and emit events.

Two effect layers exist because repo-root discovery happens before a `Workspace` can exist:

- CLI layer (`AppEffect`): pre-repo-root setup; allowed to use `std::fs`.
- Pipeline layer (`Effect`): post-repo-root execution; filesystem I/O must go through `Workspace`.

End-to-end lifecycle (configured counts drive the loops):

- Planning -> Development (developer + verification) -> Commit, repeated for configured iterations
- Review -> Fix -> Commit, repeated for configured review passes
- Final validation / finalization

## What A Developer Usually Needs First

When you read the Rust, there are a few concepts that unlock most of the system:

- The pipeline is a state machine, not ad-hoc control flow; everything important is encoded in `PipelineState`.
- Handlers must not contain policy (retry/fallback/budgets); policy must be reducer-visible state.
- Files in `.agent/` are orchestrator-written artifacts; agents don't own those writes.
- Checkpoint/resume is not a "nice-to-have"; it is part of the non-terminating unattended design.

If any of those feel surprising, start with `event-loop-and-reducers.md` and `effect-system.md`.

## Subsystems (At A Glance)

The repo has multiple "architectures" working together:

- **CLI/App bootstrapping**: parse args, load config/registry, discover repo root, create `Workspace`, handle `--init/--diagnose/--resume`.
- **Reducer pipeline**: pure orchestration + pure reduction + impure effect handling.
- **Agents + prompts**: agent registry/chain selection, prompt templates, and provider-specific execution.
- **Streaming output**: NDJSON parsing + rendering with terminal capability gating + dedup.
- **Git operations**: libgit2 (no git CLI) for diff baselines, commits, and rebase.
- **Persistence**: checkpoints (and hardened validation) for resume.

This README indexes the deeper docs that cover each subsystem.

## Key Artifacts (Mental Model)

Ralph creates and manages a small set of canonical files under `.agent/`:

- `.agent/PLAN.md`: plan for the current development iteration (pipeline-written).
- `.agent/ISSUES.md`: issues found during a review pass (pipeline-written).
- `.agent/checkpoint.json`: resume snapshot written for interrupted runs.
- `.agent/tmp/`: transient scratch space (including extracted XML).
- `.agent/logs-<run_id>/`: per-run log directory containing:
  - `run.json`: run metadata (timestamp, command, version)
  - `pipeline.log`: main pipeline execution log
  - `event_loop.log`: event loop observability log
  - `event_loop_trace.jsonl`: detailed trace (written on failure/iteration cap)
  - `agents/`: per-agent invocation logs

Agents may read these artifacts, but pipeline code owns their lifecycle.

## How To Extend (Common Change Paths)

- Adding/changing pipeline behavior: update reducer state/events/effects first, then handler I/O.
  - Start with: `event-loop-and-reducers.md` + `pipeline-lifecycle.md`.
- Adding a new agent or changing provider behavior: agent config/registry + prompts + NDJSON parsing.
  - Start with: `agents-and-prompts.md` + `streaming-and-parsers.md`.
- Changing persistence/resume behavior: checkpoint types/validation and resume UX.
  - Start with: `checkpoint-and-resume.md`.
- Changing git/rebase behavior: libgit2 helpers and rebase orchestration.
  - Start with: `git-and-rebase.md`.

## Debugging Where It "Got Stuck"

- State-machine issues: `.agent/logs-<run_id>/event_loop_trace.jsonl` (written on internal failures / iteration cap).
- Event loop behavior: `.agent/logs-<run_id>/event_loop.log` (always-on human-readable effect/event log).
- Resume issues: inspect `.agent/checkpoint.json` and resume validation output.
- Agent execution: check `.agent/logs-<run_id>/agents/` and provider streaming output paths.

## Start Here

- `codebase-tour.md` - entrypoints, module map, and how a run flows through the system.

## Pipeline Core

- `pipeline-lifecycle.md` - end-to-end lifecycle (Planning -> Development -> verification -> Commit -> Review/Fix loops).
- `event-loop-and-reducers.md` - reducer/event-loop contract (pure reducers/orchestrator, descriptive events, terminal semantics).
- `effect-system.md` - effect layers + filesystem rules (`AppEffect` vs pipeline `Effect`, `Workspace` requirements).

## Agents, Prompts, and Output

- `agents-and-prompts.md` - agent registry/config, agent chains/fallback, prompt generation, and where parsing hooks in.
- `streaming-and-parsers.md` - streaming NDJSON parsing, terminal modes, deduplication, provider quirks.

## Persistence and Recovery

- `checkpoint-and-resume.md` - checkpoint contents, save triggers, resume validation, and hardened resume.

## Git Semantics

- `git-and-rebase.md` - baseline diffs, commits, git wrapper/hooks, and libgit2 rebase flow.
