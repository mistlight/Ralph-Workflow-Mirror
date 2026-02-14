# Checkpoints and Resume Architecture

This document explains how Ralph persists run state and how `--resume` restores it.

## What a Checkpoint Is

A checkpoint is a serialized snapshot of "enough state to resume" an unattended run without guessing.

The checkpoint is stored at:

- `.agent/checkpoint.json`

Core code:

- Types: `ralph-workflow/src/checkpoint/state/types/checkpoint.rs`
- Serialization: `ralph-workflow/src/checkpoint/state/serialization.rs`
- Validation: `ralph-workflow/src/checkpoint/validation.rs`
- Resume UX + validation plumbing: `ralph-workflow/src/app/resume.rs`

## What the Checkpoint Contains

`PipelineCheckpoint` intentionally captures both progress and "reconstruction" data:

- **Progress**: `phase`, `iteration/total_iterations`, `reviewer_pass/total_reviewer_passes`.
- **Run identity**: `run_id`, optional `parent_run_id`, and `resume_count`.
- **CLI args snapshot**: `cli_args` (so resume uses the same iteration counts, isolation mode, verbosity, etc.).
- **Agent snapshots**: `developer_agent_config`, `reviewer_agent_config`.
- **Rebase state**: `rebase_state` (pre/post rebase progress and conflict status).
- **Validation fingerprints**: working dir, config path/checksum (if any), `PROMPT.md` checksum.
- **Hardened resume (v3+)**: optional `execution_history` and `file_system_state` used to validate/repair resumption.
- **Reducer prompt input state**: `prompt_inputs` for idempotent re-materialization of oversize inputs.

The format is versioned via `CHECKPOINT_VERSION` in `ralph-workflow/src/checkpoint/state/types/snapshots_and_phases.rs`.

## Checkpoint Phase vs Reducer Phase

There are two related-but-not-identical "phase" concepts:

- `checkpoint::PipelinePhase` is a coarse, user-facing phase used in checkpoint summaries.
  - Defined in `ralph-workflow/src/checkpoint/state/types/snapshots_and_phases.rs`.
- `reducer::event::PipelinePhase` is the reducer state machine phase used by orchestration.

When documenting behavior or debugging reducer logic, treat the reducer phase as authoritative.
When resuming or printing status for users, the checkpoint phase is the artifact you will see.

## When Checkpoints Are Written

Checkpoints are written from the app layer when the pipeline reaches states where resuming must be possible.

In particular:

- The `AwaitingDevFix -> Interrupted` flow is designed to emit a completion marker and then persist a checkpoint so a human (or a later run) can resume.
- Interrupt handling (Ctrl+C) saves a checkpoint so the run can continue later.

Reducer code requests checkpoint writes via pipeline effects/events:

- Trigger type: `reducer::CheckpointTrigger` (`ralph-workflow/src/reducer/event/`)
- Effect: `Effect::SaveCheckpoint { trigger: ... }` (`ralph-workflow/src/reducer/effect/types.rs`)

## Resume Flow (CLI)

`--resume` is handled before pipeline execution:

1. Load `.agent/checkpoint.json` from the repo root workspace.
2. Validate the checkpoint against current reality (config/prompt checksums, rebase-in-progress, hardened file state when enabled).
3. If validation succeeds, restore config and context, then re-enter the pipeline.

Resume entrypoint:

- `ralph-workflow/src/app/resume.rs`

## Hardened Resume (FileSystemState)

When enabled (see crate features; default includes `hardened-resume`), the checkpoint may include file state:

- `checkpoint::FileSystemState` (`ralph-workflow/src/checkpoint/file_state/`)

The intent is to detect (and sometimes repair) unsafe divergence between the saved checkpoint and the current working tree, instead of blindly continuing.

## Operational Debugging Tips

- Checkpoint format support is intentionally strict:
  - Supported: v3 (current) and a limited v2 -> v3 in-memory migration when the v2 JSON still matches the current struct shape.
  - Not supported: v1 and pre-v1 formats/phases. These cannot be upgraded automatically.
- If a checkpoint cannot be deserialized due to a version/format mismatch, the CLI intentionally guides you to "start fresh" by backing up and removing `.agent/checkpoint.json`.
- If resume keeps failing validation, start by checking whether a rebase is in progress and whether `.agent/` artifacts were modified externally.
