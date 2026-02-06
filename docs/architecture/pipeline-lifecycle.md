# Pipeline Lifecycle (Plan -> Build -> Verify -> Commit -> Review)

This document describes the *end-to-end* behavior of Ralph's pipeline: how it moves through Planning, Development, result verification (analysis), Commit, and the Review/Fix cycles.

If you are looking for the generic reducer/event-loop mechanics, see `event-loop-and-reducers.md`.
If you are looking for effect-handler layering and filesystem rules, see `effect-system.md`.

## The Big Picture: Nested Loops

Ralph runs two nested loops, each driven by reducer-visible counters in `PipelineState`:

- **Development iterations** (`iteration` / `total_iterations`): repeated Plan -> Build -> Verify -> Commit.
- **Review passes** (`reviewer_pass` / `total_reviewer_passes`): repeated Review; when issues are found, run Fix -> Commit before the next pass.

The pipeline is deterministic: each step is a single `Effect` derived from state; each effect produces a `PipelineEvent`; the reducer applies that event to produce the next state.

## Start State

Initial phase selection is derived from the configured iteration counts (see `PipelineState::initial_with_continuation` in `ralph-workflow/src/reducer/state/pipeline.rs`):

- If `total_iterations > 0`: start in `Planning`.
- If `total_iterations == 0` and `total_reviewer_passes > 0`: start in `Review`.
- If both are `0`: start in `CommitMessage` (typically becomes a no-op commit and then completes).

Internally, `iteration` and `reviewer_pass` are *0-based counters*. The pipeline runs work while `iteration < total_iterations` (and similarly for `reviewer_pass < total_reviewer_passes`).

## Development Iteration: Plan -> Build -> Verify -> Commit

One development iteration is the unit of progress that ends in a commit. The core idea is:

1. **Planning** writes the plan for the current iteration (`.agent/PLAN.md`).
2. **Development** runs the developer agent to edit the repo.
3. **Result verification** runs the analysis agent to produce an objective `development_result.xml`.
4. **Commit** creates (or skips) a commit for that iteration and advances the iteration counter.

The authoritative sequencing lives in `ralph-workflow/src/reducer/orchestration/phase_effects.rs` and the reducer transitions live in `ralph-workflow/src/reducer/state_reduction/`.

### Planning (per-iteration)

Planning produces a valid plan *before* development starts:

- Effects (typical): `PreparePlanningPrompt` -> `InvokePlanningAgent` -> `ExtractPlanningXml` -> `ValidatePlanningXml` -> `WritePlanningMarkdown` -> `ArchivePlanningXml` -> `ApplyPlanningOutcome`.
- If planning output is invalid, the reducer keeps the phase in `Planning` and uses XSD retries / agent fallback (this is reducer-visible via `ContinuationState`).
- Planning does not advance `iteration`.

### Development (per-iteration)

Development is intentionally split into two roles:

- **Developer agent**: edits code.
- **Analysis agent**: verifies what changed vs the plan.

#### Developer invocation

Typical effect sequence for a given `iteration`:

`PrepareDevelopmentContext` -> `PrepareDevelopmentPrompt` -> `InvokeDevelopmentAgent`

#### Result verification (analysis agent)

After *every* developer invocation (including continuation attempts), orchestration invokes the analysis agent:

`InvokeAnalysisAgent` -> `ExtractDevelopmentXml` -> `ValidateDevelopmentXml` -> `ArchiveDevelopmentXml` -> `ApplyDevelopmentOutcome`

Key properties:

- Analysis runs **after** `InvokeDevelopmentAgent` and **before** XML extraction.
- Analysis does **not** increment `iteration`.
- The analysis agent writes `.agent/tmp/development_result.xml` by comparing the current git diff against `.agent/PLAN.md`.

See also: `analysis-agent.md` for the analysis step's invariants.

### Continuation Loop (within an iteration)

Applying the validated development outcome produces a status:

- `completed`: iteration succeeds and proceeds to commit.
- `partial` / `failed`: iteration may enter a continuation loop.

Continuation is a reducer-controlled retry that stays inside the same `iteration`:

1. The reducer emits `DevelopmentEvent::ContinuationTriggered { .. }`.
2. Orchestration writes continuation context (so the next prompt can reference what happened).
3. Development context/prompt are re-prepared in `PromptMode::Continuation`.
4. Developer agent runs again.
5. Analysis runs again.

This repeats until either:

- A continuation succeeds (status becomes `completed`) and we proceed to commit, or
- Continuation budget is exhausted, which triggers agent fallback and may escalate to `AwaitingDevFix` if the agent chain is exhausted and work is still incomplete.

The mechanics live in `ralph-workflow/src/reducer/state_reduction/development.rs` (see `OutcomeApplied`, `ContinuationTriggered`, `ContinuationSucceeded`, `ContinuationBudgetExhausted`).

### Commit (completes a dev iteration)

After a successful development iteration, the pipeline enters `CommitMessage` and tries to produce a commit:

- If the computed diff is empty, the commit is skipped (`SkipCommit`).
- Otherwise the commit agent generates a message (XML), the reducer validates it, and the handler creates the commit.

Only the post-commit transition advances the top-level counters:

- If `previous_phase == Development`: `iteration` increments and phase transitions to `Planning` (if more iterations remain) or to `Review` / `FinalValidation`.
- If `previous_phase == Review`: `reviewer_pass` increments and phase transitions to `Review` (if passes remain) or to `FinalValidation`.

See `compute_post_commit_transition` in `ralph-workflow/src/reducer/state_reduction/commit.rs`.

## Review Phase: Review -> Fix -> Commit (repeat)

Review runs after all development iterations complete (when configured) and is driven by `reviewer_pass`.

### Review pass

Typical effect sequence:

`PrepareReviewContext` -> `PrepareReviewPrompt` -> `InvokeReviewAgent` -> `ExtractReviewIssuesXml` -> `ValidateReviewIssuesXml` -> `WriteIssuesMarkdown` -> `ArchiveReviewIssuesXml` -> `ApplyReviewOutcome`

The review agent produces `.agent/tmp/issues.xml` (validated) and `.agent/ISSUES.md` (written by the pipeline).

### Fix attempt (only when issues found)

If review finds issues, the phase remains `Review` but `review_issues_found` drives orchestration into a fix attempt:

`PrepareFixPrompt` -> `InvokeFixAgent` -> `ExtractFixResultXml` -> `ValidateFixResultXml` -> `ArchiveFixResultXml` -> `ApplyFixOutcome`

Fix attempts can themselves continue (a fix-specific continuation budget), and on completion the pipeline transitions to `CommitMessage` with `previous_phase = Review`.

After the fix commit, `reviewer_pass` increments and review continues until all passes are complete.

## Finalization and Completion

After the last configured loop completes, the pipeline transitions through:

- `FinalValidation`: final state validation effect.
- `Finalizing`: cleanup effects (for example restoring prompt permissions).
- `Complete`: terminal success.

## Failure Handling: AwaitingDevFix -> Interrupted

Ralph is designed to route terminal failures through a non-early-exit path:

`AwaitingDevFix` -> `TriggerDevFixFlow` (writes completion marker, optional dev-fix agent) -> `Interrupted` -> `SaveCheckpoint`

Terminal semantics for the event loop are implemented by `PipelineState::is_complete()` (see `ralph-workflow/src/reducer/state/pipeline.rs`).

## Where To Look in Code

- Orchestration (state -> next effect): `ralph-workflow/src/reducer/orchestration/phase_effects.rs`
- Reducers (state + event -> state): `ralph-workflow/src/reducer/state_reduction/`
- State + counters + terminal semantics: `ralph-workflow/src/reducer/state/pipeline.rs`
- Event loop driver: `ralph-workflow/src/app/event_loop.rs`
