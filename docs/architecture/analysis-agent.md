# Analysis Agent Architecture

## Purpose

The analysis agent provides objective verification of development progress by comparing git diff against PLAN.md, eliminating reliance on potentially polluted self-reports from development agents.

## Problem Statement

Previously, the pipeline relied on the development agent's self-report (via `development_result.xml`) to determine task completion status. However, due to context pollution—where the agent has accumulated conversation history, intermediate states, and assumptions—this self-report may not accurately reflect the actual changes made to the codebase. The development agent may claim completion when work is incomplete, or may mischaracterize the scope of changes.

## Solution

After each development iteration completes, a fresh, independent analysis agent is launched with:
1. The original PLAN.md (what was intended)
2. The git diff since pipeline start (what actually changed)
3. A specific task: Compare these and generate an objective progress report

This analysis agent has NO context from the development phase, ensuring an unbiased assessment based purely on observable code changes.

## Timing

**CRITICAL**: Analysis runs after EVERY development iteration, not just the final one. This provides continuous verification throughout the development phase.

The analysis agent is invoked:
- After EVERY `InvokeDevelopmentAgent` effect completes
- Regardless of iteration count
- Even during continuation attempts (when status=partial)

## Flow

```
Development Phase (iteration N)
  ↓
InvokeDevelopmentAgent (dev agent executes code)
  ↓
DevelopmentAgentInvoked event
  ↓
Orchestrator checks: dev_agent_invoked == Some(N) && analysis_agent_invoked != Some(N)
  ↓
InvokeAnalysisAgent (analysis agent compares diff vs PLAN)
  ↓
AnalysisAgentInvoked event
  ↓
ExtractDevelopmentXml (reads development_result.xml produced by analysis)
  ↓
ValidateDevelopmentXml
  ↓
ArchiveDevelopmentXml
  ↓
ApplyDevelopmentOutcome (status: completed/partial/failed)
```

## Architecture Changes

### Before (Development Agent Self-Report)
- Development agent writes `development_result.xml` (self-report, potentially polluted)
- Development agent must both implement code AND summarize its work
- Development agent prompts include XML output requirements

### After (Analysis Agent Objective Assessment)
- Analysis agent writes `development_result.xml` (objective assessment, context-free)
- Development agents focus solely on code execution
- Development agent prompts simplified (no XML output requirement)
- Analysis agent specializes in assessment, not implementation

### Infrastructure Reuse

The analysis agent **reuses ALL existing infrastructure**:
- `development_result.xsd` schema (unchanged)
- `extract_development_result_xml()` function (unchanged)
- `validate_development_result_xml()` function (unchanged)
- Effects: `ExtractDevelopmentXml`, `ValidateDevelopmentXml`, `ArchiveDevelopmentXml` (unchanged)
- Events: `DevelopmentXmlExtracted`, `DevelopmentXmlValidated`, `DevelopmentXmlArchived` (unchanged)
- State field: `development_validated_outcome` (unchanged, now populated by analysis agent)

### New Components

1. **Effect**: `InvokeAnalysisAgent { iteration: u32 }`
2. **Event**: `AnalysisAgentInvoked { iteration: u32 }`
3. **State field**: `analysis_agent_invoked_iteration: Option<u32>`
4. **Handler**: `invoke_analysis_agent()` in `reducer/handler/analysis.rs`
5. **Prompt**: `generate_analysis_prompt()` in `prompts/analysis/system_prompt.rs`

## Iteration Counter Invariant

**CRITICAL**: Analysis does NOT increment `state.iteration`.

Only the commit phase (via `compute_post_commit_transition` in `commit.rs:244`) increments the iteration counter. This ensures:
- `-D 3` means exactly 3 planning cycles
- Analysis is verification, not a development step
- Continuation stays within same iteration

## Empty Diff Handling

The analysis agent ALWAYS runs, even if git diff is empty. The analysis agent receives:
- PLAN.md content (what was intended)
- git diff output (what actually changed, may be empty)

Analysis agent determines:
- Empty diff + plan satisfied = `status="completed"` (no changes needed)
- Empty diff + plan requires changes = `status="failed"` (dev agent didn't execute)

This prevents false positives (claiming success when nothing happened).

## XSD Retry

When the analysis agent produces invalid XML, the same XSD retry infrastructure is used:
1. Validation fails with specific error message
2. Error written to `.agent/tmp/development_xsd_error.txt`
3. Invalid XML preserved in `.agent/tmp/development_result.xml`
4. Analysis agent re-invoked with XSD error context in prompt
5. Retry count tracked in `state.continuation.xsd_retry_count`

The orchestrator logic ensures XSD retry re-invokes the analysis agent (not the development agent).

## Agent Fallback

The analysis agent uses the same agent chain fallback mechanism as development agents:
1. Invalid output attempts tracked in `state.continuation.invalid_output_attempts`
2. When threshold exceeded (typically 3-4 attempts), agent chain advances
3. Next agent in chain is used for subsequent analysis attempts
4. This provides resilience against agent-specific issues

## State Machine Integration

### Orchestration Logic

In `reducer/orchestration/phase_effects.rs` (Development phase, lines 172-182):

```rust
// After EVERY development iteration, invoke analysis agent to verify results
if state.development_agent_invoked_iteration == Some(state.iteration)
    && state.analysis_agent_invoked_iteration != Some(state.iteration)
{
    return Effect::InvokeAnalysisAgent {
        iteration: state.iteration,
    };
}
```

This guard ensures:
- Development agent has completed for this iteration
- Analysis agent has NOT yet run for this iteration
- Works for continuations: When continuation resets `analysis_agent_invoked_iteration` to `None`, orchestrator re-invokes analysis

### State Reduction

In `reducer/state_reduction/development.rs` (line 83-90):

```rust
DevelopmentEvent::AnalysisAgentInvoked { iteration } => PipelineState {
    analysis_agent_invoked_iteration: Some(iteration),
    continuation: state.continuation.clear_xsd_retry_pending(),
    ..state
}
```

Key points:
- Records that analysis ran for this iteration
- Clears XSD retry flag (if set) so orchestration proceeds to XML extraction
- Does NOT modify `state.iteration` (preserves invariant)

### Continuation Handling

In `reducer/state_reduction/development.rs` (line 286-291):

```rust
DevelopmentEvent::ContinuationTriggered { .. } => {
    // Reset analysis tracking so analysis runs again after next dev agent invocation
    analysis_agent_invoked_iteration: None,
    // ... other state updates
}
```

When continuation is triggered, `analysis_agent_invoked_iteration` is reset so that the next development agent invocation will be followed by a fresh analysis.

## Testing

Comprehensive integration tests verify all aspects:

1. **Basic flow**: Analysis runs after every development iteration
2. **Sequencing**: Analysis doesn't run before dev agent completes
3. **Idempotency**: Analysis doesn't run twice for same iteration
4. **State updates**: `AnalysisAgentInvoked` event correctly updates state
5. **Iteration invariant**: Analysis does NOT increment iteration counter
6. **Continuation**: Continuation resets analysis tracking
7. **XSD retry**: Invalid XML triggers analysis agent retry (not dev agent retry)
8. **Empty diff**: Correctly handles both "no changes needed" and "dev agent failed" scenarios
9. **Agent fallback**: Uses same fallback mechanism as dev agents
10. **End-to-end**: Complete pipeline flow from dev -> analysis -> extract -> validate

See: `tests/integration_tests/workflows/analysis.rs`

## Implementation Files

### Primary Files
- `ralph-workflow/src/reducer/handler/analysis.rs` - Effect handler
- `ralph-workflow/src/prompts/analysis/system_prompt.rs` - Prompt generation
- `ralph-workflow/src/reducer/event/development.rs` - Event definition
- `ralph-workflow/src/reducer/effect/types.rs` - Effect definition
- `ralph-workflow/src/reducer/state/pipeline.rs` - State field
- `ralph-workflow/src/reducer/state_reduction/development.rs` - Event handling
- `ralph-workflow/src/reducer/orchestration/phase_effects.rs` - Orchestration logic

### Test Files
- `tests/integration_tests/workflows/analysis.rs` - Comprehensive integration tests
- `tests/integration_tests/development_xml_validation.rs` - XML validation tests

## Future Work

### Prompt Template Migration

Currently, the analysis prompt uses a hardcoded `format!` string (`system_prompt.rs:55-100`) instead of a `.txt` template file. Per the TODO comment at line 46:

```rust
// TODO THIS NEEDS to be migrated to .txt this is not conforming, prompt strings should almost
// never be allowed since users won't be able to edit them when we add prompt editing in the
// future
```

This is **known technical debt**. The current implementation works correctly. Migration to the template system should be done in a follow-up task:

1. Create `prompts/analysis/system_prompt.txt` template file
2. Use template engine with `{{PLAN}}`, `{{DIFF}}`, `{{ITERATION}}` variables
3. Update `generate_analysis_prompt` to use template renderer
4. Update tests to verify template rendering

## See Also

- [Effect System Architecture](effect-system.md) - General effect/event/reducer architecture
- [Development Phase](../phases/development.md) - Development phase flow
- [Integration Testing Guide](../../tests/INTEGRATION_TESTS.md) - Testing philosophy
