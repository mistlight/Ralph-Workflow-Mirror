# Event Loop and Reducer Architecture

This document describes Ralph's pipeline event loop and reducer architecture: how `PipelineState`, `PipelineEvent`, and `Effect` work together, and the best practices that keep reducers pure and effects isolated.

If you are looking specifically for effect-handler layering and filesystem rules (`AppEffect` vs pipeline `Effect`, `Workspace` requirements, `std::fs` exceptions), see `effect-system.md`.

## Core Contract

Ralph's pipeline is driven by an explicit event loop with a strict separation of concerns:

- `PipelineState`: immutable snapshot of "where we are" (this is the pipeline's app-state).
- `Effect`: an intention to perform I/O (run an agent, write a checkpoint, run git, etc.).
- `PipelineEvent`: a fact about something that happened (success, failure, data produced). In code this is an umbrella enum that wraps category events.
- `Reducer`: pure function that applies an event to the current state.
- `Orchestrator`: pure function that derives the next effect from the current state.
- `EffectHandler`: impure executor that performs the effect and reports the outcome as `EffectResult` (primary reducer event + optional additional events + optional UI events).

The pipeline must make progress by cycling through these steps:

```
state --orchestrate--> effect --handle--> event --reduce--> next_state
```

### Why this is non-negotiable

- Predictable execution: the same state + the same sequence of events produces the same result.
- Testability: reducers and orchestrators test without filesystem, network, or git.
- Debuggability: the event log explains what happened without reverse-engineering control flow.
- Resume/checkpoint: state is the checkpoint; resume is just load state + continue.

## The Event Loop

At a high level (pipeline layer):

1. Orchestrate: inspect the current `PipelineState` and choose the next `Effect`.
2. Handle: execute the effect in the `EffectHandler` (I/O happens here).
3. Emit: return an `EffectResult` (primary `PipelineEvent`, optional `additional_events`, optional `ui_events`).
4. Reduce: compute the next state by applying the primary event and then any additional events, in order.
5. Repeat until the state reaches a terminal condition (`PipelineState::is_complete()`).

Two rules prevent subtle bugs:

- The event loop must not "fake" events for certain effects (no synthetic substitutions).
- The handler is responsible for the side effect itself; the reducer is responsible for decisions.

This includes checkpointing: `Effect::SaveCheckpoint` must still execute through the handler even when persistence is disabled. (If persistence is disabled, the handler can skip writing files, but the effect still runs.)

## PipelineState (The App State)

`PipelineState` is the canonical application state for the pipeline reducer architecture.

- It is the single source of truth for pipeline progress.
- It is the checkpoint payload: serialize to JSON to resume later.
- It is reducer-owned: state transitions only happen by applying reducer events.

### What belongs in state

State should include the minimum information needed to:

- deterministically derive the next `Effect`
- explain why the pipeline is in its current phase
- safely resume after interruption

In practice this means a lot of "single-task sequencing" fields (for example: "prompt prepared for iteration N", "xml extracted for pass P", "validated outcome stored", etc.). These flags keep orchestration deterministic and prevent handlers from bundling policy decisions into I/O.

### What must not be state

- mutable caches of external reality (filesystem, git status, network)
- hidden control flags that are not driven by events
- anything that would make `reduce(state, event)` depend on time, environment, or I/O

## Terminal State Semantics

The event loop terminates based on `PipelineState::is_complete()`, not on orchestration returning "no effect".

Current terminal semantics (see `ralph-workflow/src/reducer/state/pipeline.rs`):

- `PipelinePhase::Complete` is always terminal.
- `PipelinePhase::Interrupted` is terminal when either:
  - at least one checkpoint has been saved (`checkpoint_saved_count > 0`), or
  - we transitioned from `PipelinePhase::AwaitingDevFix` (completion marker already emitted).

The `AwaitingDevFix -> Interrupted` path is intentionally considered terminal even before the checkpoint write, because external orchestration observes termination via the completion marker. The event loop still ensures `Effect::SaveCheckpoint` runs next for persistence.

## Event and Effect Shapes (Current)

Ralph uses a few structural patterns that are important when you add new behavior.

### PipelineEvent is category-based

`PipelineEvent` wraps category-specific enums so the reducer can do type-safe routing:

- `LifecycleEvent` (frozen)
- `PlanningEvent`
- `DevelopmentEvent`
- `ReviewEvent`
- `CommitEvent`
- `AgentEvent`
- `RebaseEvent`
- `PromptInputEvent`
- `AwaitingDevFixEvent`

Category routing keeps each reducer module exhaustively matched within its own domain.

### UIEvent is separate from PipelineEvent

Effect handlers may emit UI-only events for rendering/logging. These do not affect state, do not go into checkpoints, and must not be required for correctness.

### Effects are intentionally "single-task"

Pipeline effects are granular (prepare prompt, invoke agent, extract XML, validate XML, write markdown, archive, apply outcome, etc.).

An effect should do one type of I/O and then report an outcome event. Avoid effects that mix multiple responsibilities (for example: invoke agent + parse output + transition phase).

## Non-Terminating Failure Handling

The pipeline is designed to keep running and route internal failures through the state machine instead of exiting early.

- Terminal internal failure conditions transition the state to `PipelinePhase::AwaitingDevFix`.
- Orchestration derives a dev-fix flow effect, which emits a completion marker and then transitions to `Interrupted`.
- The event loop should exit with `completed=true` after the checkpoint is saved for the interrupted state.

## Orchestration: Priority Order

Orchestration is a pure function from state to the next effect (`determine_next_effect(&PipelineState) -> Effect`). It intentionally encodes a priority order so that recovery/cleanup always preempts phase work.

The current priority ordering is documented in code (see `ralph-workflow/src/reducer/orchestration/xsd_retry.rs`) and includes, roughly:

1. Continuation context cleanup
2. Same-agent retry pending (transient invocation failures)
3. XSD retry pending (invalid XML output)
4. Continuation pending (valid output but incomplete work)
5. Rebase in progress
6. Agent-chain exhaustion / backoff waiting
7. Phase-specific effects (the normal single-task sequence)

Do not implement hidden retries or fallback loops inside handlers; retries/fallback must be reducer-visible state so the orchestrator can remain pure and deterministic.

## Handler Error Recovery (Downcasting)

Handlers normally return `Ok(EffectResult { .. })`. When a handler needs to surface a failure that should still be handled by the state machine, it returns `Err(ErrorEvent::... .into())`.

The event loop attempts to downcast typed `ErrorEvent` values out of `anyhow::Error` and re-emit them as a reducer event (`PipelineEvent::PromptInput(PromptInputEvent::HandlerError { .. })`). This keeps recovery logic in reducers without forcing new top-level `PipelineEvent` variants.

Handler panics are also treated as internal failures: the event loop catches them and routes through the same non-terminating failure flow.

## Debuggability: Event Loop Trace

The event loop keeps a bounded in-memory ring buffer of recent (effect, event, phase, retry counters) entries. When the loop encounters an internal failure or hits its iteration cap, it writes a trace to:

- `.agent/tmp/event_loop_trace.jsonl`

This is designed to make "stuck" pipelines diagnosable without reconstructing control flow.

## Where This Lives in Code

The exact file layout can evolve, but conceptually Ralph keeps these concerns separate:

- Event loop driver: `ralph-workflow/src/app/event_loop.rs`
- Orchestration (state -> next effect): `ralph-workflow/src/reducer/orchestration/`
- Reduction (state + event -> next state): `ralph-workflow/src/reducer/state_reduction/`
- Effects (intent enum) and handler trait: `ralph-workflow/src/reducer/effect*`
- Effect handler implementations (I/O): `ralph-workflow/src/reducer/handler/`

## Best Practices: Events vs Decisions

### Never add decision-events to lifecycle

`LifecycleEvent` is intentionally frozen so effect handlers cannot introduce new "control" events.

If you need to represent a new observation or failure, add it to the appropriate phase/category event and let the reducer decide what to do.

### Events must be descriptive facts

An emitted event should answer: "what happened" (and with what observable data), not "what should we do next".

Good events are:

- Past-tense, factual, and specific
- Stable over time (their meaning does not depend on hidden context)
- Carry the data needed for future decisions

Bad events are:

- Imperative or policy-shaped ("advance", "retry", "should")
- Encoding decisions that belong in the reducer

### Decisions belong in reducers (pure functions)

Reducers should encode the decision logic that turns facts into state transitions:

- retry vs fallback vs abort
- phase transitions
- iteration bookkeeping
- counters/limits
- which "next step" is enabled by state

The orchestrator then translates the resulting state into the next effect.

### Concrete naming guidance

Prefer events shaped like:

- `SomethingStarted { .. }`
- `SomethingSucceeded { .. }`
- `SomethingFailed { reason, .. }`
- `SomethingDetected { .. }`
- `SomethingCompleted { .. }`

Avoid events shaped like:

- `TryNextX`
- `ShouldRetry`
- `AdvanceToNextX`
- `DecideX`

#### Migration examples

Before (decision-shaped):

- `AdvanceToNextAgent`
- `RetryCommitGeneration`
- `SkipCheckpointWrite`

After (fact-shaped + reducer decision):

- `AgentEvent::InvocationFailed { role, agent, error_kind, retriable, .. }`
- `CommitEvent::MessageValidationFailed { reason, attempt }`
- `CheckpointPersisted` / `CheckpointWriteSkipped { reason }` (when you need observability)

In the "after" model:

- The handler reports the outcome (including classification data like `retriable`).
- The reducer updates state (advance chain, increment attempt, transition phase).
- The orchestrator sees the updated state and chooses the next effect.

## Best Practices: Reducers

Reducers must be deterministic and side-effect free.

- No filesystem, git, network, environment, time, randomness, or logging
- No mutation of shared global state
- No hidden coupling to config: decisions should be driven by values already present in `PipelineState` or carried in events

### Prefer typed error-events over `Err` when recoverable

When a failure should be handled by the state machine, represent it as a typed reducer event (often via `ErrorEvent` or a category event like `PlanningEvent::OutputValidationFailed`).

Reserve returning `Err` for truly unrecoverable failures. The event loop has a recovery path where certain handler errors are downcast back into typed error events and reduced.

### Reducer-friendly event design

If the reducer needs to decide something, the event should include the inputs to that decision.

Example: if fallback policy depends on whether an agent failure is retriable, the event should carry `retriable: bool` (or a structured `error_kind`) rather than forcing the reducer to re-derive it.

### TDD for reducers

When adding or changing reducer behavior:

1. Write a unit test for `reduce(state, event) -> new_state` capturing the decision.
2. Run the test and confirm it fails for the right reason.
3. Implement the minimal state transition in the reducer.
4. Add follow-up tests for edge cases (limits, phase boundaries, retries).

## Best Practices: Effects and Handlers

Effects are intentions; handlers are execution.

- Effects should be named as verbs/intents: `RunRebase`, `CreateCommit`, `SaveCheckpoint`, `InvokeAgent`.
- Handlers should not contain high-level pipeline policy (like "how many times to retry").
- Handlers should translate outcomes into events, not mutate pipeline state directly.

When a handler must implement a local safety rule (for example, "checkpointing disabled so do not write files"), it should still execute through the handler and return an event; the event loop must not bypass the effect.

## Migration Guide: From Decision-Actions to Descriptive Events

If you see control flow that looks like "if X then emit ActionY", it is often a sign the event model is too decision-shaped.

Recommended migration approach:

1. Identify the decision in the handler/orchestrator (retry, fallback, phase change).
2. Replace any decision-shaped event with a fact-shaped outcome event.
3. Move the decision into the reducer by updating `PipelineState` fields (attempt counters, chain position, phase).
4. Ensure the orchestrator derives the next effect only from state.
5. Add reducer unit tests that cover the policy explicitly.

## See Also

- `effect-system.md`

## Historical Notes

The RFCs in `docs/RFC/` are kept for historical interest only. Do not treat them as canonical.

- `../RFC/RFC-004-reducer-based-pipeline-architecture.md` (historical design)
- `../RFC/RFC-005-event-loop-savecheckpoint-bypass.md` (historical incident writeup)
