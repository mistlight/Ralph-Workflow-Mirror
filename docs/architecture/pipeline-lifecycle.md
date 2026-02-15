# Pipeline Lifecycle (Plan -> Build -> Verify -> Commit -> Review)

This document describes the *end-to-end* behavior of Ralph's pipeline: how it moves through Planning, Development, result verification (analysis), Commit, and the Review/Fix cycles.

If you are looking for the generic reducer/event-loop mechanics, see `event-loop-and-reducers.md`.
If you are looking for effect-handler layering and filesystem rules, see `effect-system.md`.
If you are looking for checkpoint/resume persistence details, see `checkpoint-and-resume.md`.
If you are looking for git baseline/rebase behavior, see `git-and-rebase.md`.

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

## Failure Handling: AwaitingDevFix Recovery Loop

Ralph routes terminal failures through an **escalating recovery loop** (not a one-shot attempt) designed for unattended operation:

1. **Failure detected** → Transition to `AwaitingDevFix` phase
2. **TriggerDevFixFlow** → Invoke dev-fix agent to diagnose and fix the issue
3. **DevFixCompleted** → Increment attempt count, determine escalation level (1-4)
4. **RecoveryAttempted** → Transition back to failed phase, attempt recovery at determined level
5. **Work executes** → Phase orchestration retries the work that failed
6. **IF work succeeds** → Phase orchestration detects recovery success
7. **EmitRecoverySuccess** → Handler emits `RecoverySucceeded` event
8. **Recovery state cleared** → Reset `dev_fix_attempt_count`, `recovery_escalation_level`, `failed_phase_for_recovery`
9. **Normal operation resumes** → Continue from the recovered phase
10. **IF work fails AGAIN** → **LOOP BACK TO STEP 1** with preserved recovery state
11. **Recovery state preserved** → Keep attempt count and escalation level for continued escalation
12. **Only after 12+ attempts** → `CompletionMarkerEmitted` → `Interrupted` → `SaveCheckpoint`

### Critical: This is a LOOP, Not One-Shot

The recovery flow is designed to **repeatedly attempt recovery with escalating strategies**. 
Each failure loops back to step 1 with an incremented attempt count and potentially higher 
escalation level. The pipeline will retry the same work multiple times, with progressively 
more aggressive resets, before giving up.

**Recovery state preservation is critical:** When a failure occurs while already in recovery 
(previous_phase == AwaitingDevFix), the error reducer preserves `dev_fix_attempt_count` and 
`recovery_escalation_level` instead of resetting them. This enables escalation across multiple 
recovery cycles rather than resetting to level 1 on each failure.

**This is NOT:**
- Run dev-fix once → terminate
- Try recovery once → give up
- Reset counters on each failure

**This IS:**
- Run dev-fix → retry work → if fails, run dev-fix again → retry work with reset → 
  if fails, run dev-fix again with bigger reset → repeat up to 12 times → only then terminate

### Escalation Levels

The recovery hierarchy implements progressively more aggressive reset strategies:

- **Level 1** (attempts 1-3): Retry the same operation that failed
- **Level 2** (attempts 4-6): Reset to phase start (clear phase-specific progress)
- **Level 3** (attempts 7-9): Reset iteration counter, restart from Planning
- **Level 4** (attempts 10+): Reset to iteration 0, complete restart

This ensures the pipeline is truly **non-terminating by default** for unattended operation, only exiting after exhausting all recovery strategies.

### Example: Recovery Loop with Escalation

```
Iteration 0: Development agent fails (GitAddAllFailed)
  ↓
AwaitingDevFix: TriggerDevFixFlow (attempt 1)
  ↓
DevFixCompleted: "Fixed permission issue"
  ↓
RecoveryAttempted: Level 1 (retry same operation)
  ↓
Development: Try git add again... FAILS AGAIN (still has issue)
  ↓
AwaitingDevFix: TriggerDevFixFlow (attempt 2) ← LOOP BACK (counters preserved)
  ↓
DevFixCompleted: "Fixed path issue"
  ↓
RecoveryAttempted: Level 1 (retry same operation)
  ↓
Development: Try git add again... FAILS AGAIN
  ↓
AwaitingDevFix: TriggerDevFixFlow (attempt 3) ← LOOP BACK (counters preserved)
  ↓
DevFixCompleted: "Fixed file encoding"
  ↓
RecoveryAttempted: Level 1 (retry same operation)
  ↓
Development: Try git add again... FAILS AGAIN
  ↓
AwaitingDevFix: TriggerDevFixFlow (attempt 4) ← LOOP BACK, ESCALATE
  ↓
DevFixCompleted: "Reset phase state"
  ↓
RecoveryAttempted: Level 2 (reset to phase start) ← ESCALATED
  ↓
Development: Restart entire Development phase from scratch... SUCCESS
  ↓
EmitRecoverySuccess: Clear recovery state, resume normal operation
  ↓
Continue to CommitMessage phase
```

The loop can execute up to 12 times before termination.

### Recovery Success Detection

Recovery is considered successful when:
- `previous_phase == Some(AwaitingDevFix)` (just returned from recovery)
- Phase-specific work completes (e.g., Planning XML archived, Development XML archived)
- Orchestration emits `EmitRecoverySuccess` effect before applying phase outcome
- Reducer clears recovery state (attempt_count=0, level=0, failed_phase=None)

Each phase orchestration module checks `is_recovery_state_active(state)` after its archive step completes to detect successful recovery.

Terminal semantics for the event loop are implemented by `PipelineState::is_complete()` (see `ralph-workflow/src/reducer/state/pipeline.rs`).

Checkpoint persistence for interrupted runs is implemented in the CLI/app layer and written to `.agent/checkpoint.json` (see `checkpoint-and-resume.md`).

## Where To Look in Code

- Orchestration (state -> next effect): `ralph-workflow/src/reducer/orchestration/phase_effects.rs`
- Reducers (state + event -> state): `ralph-workflow/src/reducer/state_reduction/`
- State + counters + terminal semantics: `ralph-workflow/src/reducer/state/pipeline.rs`
- Event loop driver: `ralph-workflow/src/app/event_loop.rs`
