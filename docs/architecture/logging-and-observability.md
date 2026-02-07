# Logging and Observability Architecture

## Overview

Ralph's logging infrastructure provides comprehensive observability into pipeline execution through a per-run directory structure. All logs from a single `ralph` invocation are grouped under `.agent/logs-<run_id>/`, making it easy to:

- Share logs as a cohesive artifact
- Correlate logs to a specific run (including across `--resume`)
- Inspect event loop behavior without reconstructing control flow
- Debug pipeline issues with complete context

## Per-Run Log Directory Structure

Each run creates exactly one directory at:

```
.agent/logs-<run_id>/
```

Where `<run_id>` is a UTC timestamp with millisecond precision in the format:

```
YYYY-MM-DD_HH-mm-ss.SSSZ[-NN]
```

The optional `-NN` suffix (e.g., `-01`, `-02`) handles the rare case of multiple runs starting in the same millisecond.

### Directory Layout

```
.agent/
  logs-<run_id>/
    run.json                    # Run metadata (required)
    pipeline.log                # Pipeline execution log (required)
    event_loop.log              # Event loop observability log (required)
    event_loop_trace.jsonl      # Crash-only trace dump (optional)
    agents/                     # Per-agent invocation logs
      planning_1.log
      dev_1.log
      dev_1_a1.log              # Retry attempt
      reviewer_1.log
      commit_1.log
    provider/                   # Provider streaming logs (future)
      claude-stream_dev_1.jsonl
    debug/                      # Future debug artifacts
```

### Run ID Format

The run ID is designed to be:

- **Human-readable**: Clear timestamp format
- **Machine-sortable**: Lexicographic sort == chronological order
- **Filesystem-safe**: No colons; works on macOS, Linux, Windows

Examples:
- `2026-02-06_14-03-27.123Z` (base format)
- `2026-02-06_14-03-27.123Z-01` (with collision counter)

### Collision Handling

If a run directory already exists (e.g., two runs started in the same millisecond), the system appends a zero-padded collision counter:

```rust
// First collision: .agent/logs-2026-02-06_14-03-27.123Z-01/
// Second collision: .agent/logs-2026-02-06_14-03-27.123Z-02/
```

This ensures:
- No overwrites or data loss
- Chronological sorting is preserved within the same millisecond
- Maximum of 99 collisions supported per millisecond

## Run Metadata (run.json)

Each run directory includes a metadata file providing context for debugging and tooling.

### Required Fields

```json
{
  "run_id": "2026-02-06_14-03-27.123Z",
  "started_at_utc": "2026-02-06T14:03:27.123Z",
  "command": "ralph",
  "resume": false,
  "repo_root": "/absolute/path/to/repo",
  "ralph_version": "0.6.3"
}
```

### Optional Fields

```json
{
  "pid": 12345,
  "config_summary": {
    "developer_agent": "claude",
    "reviewer_agent": "claude",
    "total_iterations": 3,
    "total_reviewer_passes": 1
  }
}
```

### When Metadata is Written

Run metadata is written early in pipeline execution (during initialization) to ensure it's available even if the run fails early. The metadata anchors debugging with essential context about how Ralph was invoked.

## Log Types

### Pipeline Log (pipeline.log)

The main execution log containing:
- Phase transitions
- Agent invocations
- Key decisions (retries, fallbacks, continuations)
- User-facing progress messages

**Path**: `.agent/logs-<run_id>/pipeline.log`

**Format**: Human-readable text with timestamps

**Behavior**: Appended on resume (never overwritten)

### Event Loop Log (event_loop.log)

An always-on observability log recording the event loop's progression:

- Which effects ran
- What events were emitted
- Phase/iteration/retry context
- Handler wall time

**Path**: `.agent/logs-<run_id>/event_loop.log`

**Format**: Structured text, one line per effect

**Line Structure**:
```
<seq> ts=<rfc3339> phase=<Phase> effect=<Effect> event=<Event> [extra=[E1,E2]] [ctx=k1=v1,k2=v2] ms=<N>
```

Note: The `ctx` field shows key-value pairs without brackets (e.g., `ctx=k1=v1,k2=v2`), not `[ctx=k1=v1,k2=v2]`.

**Example**:
```
1 ts=2026-02-06T14:03:27.123Z phase=Development effect=InvokePrompt event=PromptCompleted ms=1234
2 ts=2026-02-06T14:03:28.456Z phase=Development effect=WriteFile event=FileWritten ctx=file=PLAN.md ms=12
```

**Redaction Requirements**:
- Must never include full prompt contents
- Must never include model outputs
- Must never include git diffs
- Must never include secrets/tokens/credentials
- Errors must be sanitized (message only, no embedded payloads)

### Event Loop Trace (event_loop_trace.jsonl)

A bounded ring buffer snapshot written only on:
- Internal failure
- Iteration cap reached
- Unrecoverable handler errors
- Panics

**Path**: `.agent/logs-<run_id>/event_loop_trace.jsonl`

**Format**: NDJSON (newline-delimited JSON)

**Behavior**: Only written on failure/iteration-cap (not during normal execution)

### Agent Invocation Logs

Per-phase, per-agent invocation logs with simplified naming.

**Path**: `.agent/logs-<run_id>/agents/<phase>_<index>[_aN].log`

**Naming Convention**:
- First attempt: `<phase>_<index>.log` (e.g., `planning_1.log`, `dev_1.log`)
- Retry attempts: `<phase>_<index>_aN.log` (e.g., `dev_1_a1.log`, `dev_1_a2.log`)

**Log Header**: Each agent log includes a header with:
```
# Ralph Agent Invocation Log
# Role: Development
# Agent: claude
# Model Index: 0
# Attempt: 0
# Phase: Development
# Timestamp: 2026-02-06T14:03:27.123Z
```

**Rationale**: Agent identity is recorded in the log header (not the filename) because logs are already grouped per-run. This simplifies filename management while preserving all necessary metadata.

### Provider Logs (future)

Provider streaming artifacts (NDJSON/JSONL capture) will be written under:

```
.agent/logs-<run_id>/provider/<provider>-stream_<phase>_<index>.jsonl
```

**Status**: Infrastructure exists but not yet used in production.

## Resume Semantics

### Fresh Run

1. Generate new `run_id` with current UTC timestamp
2. Create run log directory (`.agent/logs-<run_id>/`)
3. Write `run.json` with `resume: false`
4. All logs written to new run directory

### Resume (`--resume`)

1. Load checkpoint (`.agent/checkpoint.json`)
2. Extract `run_id` from checkpoint
3. Continue using same run log directory (`.agent/logs-<run_id>/`)
4. Append to existing logs (`pipeline.log`, `event_loop.log`)
5. Write `run.json` with `resume: true` (if missing or updating metadata)

### Legacy Resume (from old checkpoint format)

If resuming from a checkpoint without `run_id`:
1. Generate new `run_id`
2. Record in `run.json` that this is a resume-from-legacy run
3. Continue with new run directory

**Note**: Directory recreation is automatic if deleted (preserves run_id).

## Canonical Orchestrator Artifacts (Not Moved)

The following files remain in their original locations (not under the run log directory):

- `.agent/PLAN.md` - Implementation plan
- `.agent/ISSUES.md` - Code review issues
- `.agent/STATUS.md` - Pipeline status
- `.agent/NOTES.md` - Additional notes
- `.agent/commit-message.txt` - Generated commit message
- `.agent/checkpoint.json` - Checkpoint for resume
- `.agent/tmp/*.xml` - XSD validation scratch files

**Rationale**: These files are correctness-critical artifacts used by the reducer/orchestrator, not observability logs. They must remain in stable, well-known locations for the pipeline to function correctly.

## Architecture Integration

### Reducer/Effect Boundary

Per-run logging strictly follows Ralph's reducer-driven architecture:

- **Reducers remain pure**: No logging, no time access, no filesystem I/O
- **Orchestrators remain pure**: No logging; they only choose the next `Effect`
- **All I/O stays inside effect handlers**: The event loop driver and effect handlers are the only writers

### RunLogContext

The `RunLogContext` struct is created once per run in the impure layer (effect-handling layer) and passed to all effect handlers. It:

- Owns the `run_id`
- Resolves run-relative paths (e.g., `pipeline.log`, `agents/...`, `event_loop.log`)
- Uses `Workspace` trait for filesystem operations (no `std::fs` in pipeline layer)
- Ensures directory creation is explicit (via early effect or dedicated "ensure logging" effect)

### Event Loop Integration

The event loop driver emits `event_loop.log` entries *after* each effect handler returns an `EffectResult`:

```rust
// Pseudocode
let start = Instant::now();
let result = handler.handle(effect, ctx);
let duration_ms = start.elapsed().as_millis();

event_loop_logger.log_effect(LogEffectParams {
    phase: state.phase,
    effect: effect_name,
    primary_event: result.primary_event,
    extra_events: result.extra_events,
    duration_ms,
    context: build_context(&state),
});
```

This ensures the log reflects the actual (effect → events) boundary defined by the architecture.

## Error Handling

### Run Directory Creation Failure

If the run log root cannot be created, Ralph must:
- Fail early with a clear error message
- Include attempted path and underlying OS error
- Not attempt to fall back to legacy locations

### Individual Log Write Failures

During execution, individual log write failures should:
- Be reported to the pipeline log (best-effort)
- Not corrupt pipeline correctness (the pipeline should continue when safe)
- Use `Workspace::append_bytes()` for append-only operations

### Trace Dump Failures

If event loop trace dump fails:
- Log the error to pipeline log
- Continue execution (trace is observability, not correctness)

## Performance Considerations

- `event_loop.log` writes are append-only and O(1) per loop iteration
- Logging should not meaningfully change runtime for typical runs
- Avoid serializing large state (effect names and event names only)
- Use bounded ring buffer for trace (not unbounded growth)

## Backward Compatibility

### Migration from Legacy Logs

- New versions stop writing logs to `.agent/logs/`
- Tooling/tests that read `.agent/logs/pipeline.log` must locate the current run's log via:
  - The checkpoint's `run_id` field
  - Optional pointer file (`.agent/current_run.txt`) containing `run_id`

### Agent Log Naming Migration

Existing agent log filename conventions that embedded agent/model identity are replaced by simplified per-run names. Identity metadata is recorded in log file headers instead.

## Tooling Integration

### Finding Current Run Logs

**Option 1: Via Checkpoint**
```bash
RUN_ID=$(jq -r .run_id .agent/checkpoint.json)
PIPELINE_LOG=".agent/logs-${RUN_ID}/pipeline.log"
```

**Option 2: Via Current Run Pointer (if implemented)**
```bash
RUN_ID=$(cat .agent/current_run.txt)
PIPELINE_LOG=".agent/logs-${RUN_ID}/pipeline.log"
```

**Option 3: Lexicographically Latest**
```bash
LATEST_RUN=$(ls -1d .agent/logs-* | sort | tail -n1)
PIPELINE_LOG="${LATEST_RUN}/pipeline.log"
```

### Sharing Logs

To share logs for a specific run:
```bash
tar -czf logs.tar.gz .agent/logs-<run_id>/
```

All logs from that run are in a single directory, making sharing trivial.

### Analyzing Event Loop Behavior

```bash
# Count effects by type
grep -oP 'effect=\K\w+' .agent/logs-<run_id>/event_loop.log | sort | uniq -c

# Find slow effects (>1000ms)
awk '$NF ~ /^ms=/ && substr($NF, 4) > 1000' .agent/logs-<run_id>/event_loop.log

# Track phase transitions
grep -oP 'phase=\K\w+' .agent/logs-<run_id>/event_loop.log | uniq
```

## Testing

### Integration Tests

- `tests/integration_tests/logging_per_run.rs`: Per-run logging infrastructure
  - Run directory format and collision handling
  - Resume continuity
  - Event loop log structure
  - Redaction requirements
  - No legacy logs created
  - Agent log headers

- `tests/integration_tests/event_loop_trace_dump.rs`: Event loop trace dump

### Unit Tests

- `ralph-workflow/src/logging/run_log_context.rs`: RunLogContext path resolution
- `ralph-workflow/src/logging/run_id.rs`: RunId format and collision counter
- `ralph-workflow/src/logging/event_loop_logger.rs`: EventLoopLogger formatting

## Related Documentation

- [Event Loop and Reducers](./event-loop-and-reducers.md) - How event loop integrates with reducers
- [Effect System](./effect-system.md) - How effects drive I/O
- [Workspace Trait](../agents/workspace-trait.md) - Filesystem abstraction

## Future Extensions

- Provider streaming log capture (infrastructure exists, not yet used)
- Debug artifacts directory (reserved for future use)
- Configurable log retention policies
- Log aggregation and analysis tools
