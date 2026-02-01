# Event Loop Trace Dump Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Capture the last N event-loop steps and a final state snapshot to `.agent/tmp/` when the loop hits max iterations or panics.

**Architecture:** Add a lightweight ring buffer in `event_loop.rs` that records each effect/event/state summary. On max-iteration or panic, dump the buffer as JSONL via `Workspace` (no direct `std::fs`).

**Tech Stack:** Rust, reducer event loop, `Workspace` trait, JSONL serialization.

---

### Task 1: Add failing test for max-iteration trace dump

**Files:**
- Modify: `ralph-workflow/src/app/event_loop.rs`
- Test: `tests/integration_tests/reducer_hidden_behavior.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_event_loop_dumps_trace_on_max_iterations() {
    use ralph_workflow::app::event_loop::{run_event_loop_with_handler, EventLoopConfig};
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelineEvent;
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::workspace::MemoryWorkspace;

    // Build a PhaseContext with MemoryWorkspace from existing test fixture.
    // Use a mock handler that always returns the same event to force looping.
    // Configure max_iterations to 3.
    // After run, assert a trace file exists in `.agent/tmp/` and contains >= 3 entries.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow test_event_loop_dumps_trace_on_max_iterations`

Expected: FAIL (trace file missing).

**Step 3: Write minimal implementation**

Add trace buffer + dump hook in the event loop (no `std::fs`).

**Step 4: Run test to verify it passes**

Run: `cargo test -p ralph-workflow test_event_loop_dumps_trace_on_max_iterations`

Expected: PASS.

**Step 5: Commit**

```bash
git add ralph-workflow/src/app/event_loop.rs tests/integration_tests/reducer_hidden_behavior.rs
git commit -m "feat: dump event loop trace on max iterations"
```

---

### Task 2: Add failing test for panic-path trace dump

**Files:**
- Modify: `ralph-workflow/src/app/event_loop.rs`
- Test: `tests/integration_tests/reducer_hidden_behavior.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_event_loop_dumps_trace_on_panic() {
    use ralph_workflow::app::event_loop::{run_event_loop_with_handler, EventLoopConfig};
    use ralph_workflow::workspace::MemoryWorkspace;

    // Use a handler that panics during execute.
    // Assert the run returns completed=false and a trace dump is written.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow test_event_loop_dumps_trace_on_panic`

Expected: FAIL (trace file missing).

**Step 3: Write minimal implementation**

Hook trace dumping into panic recovery path in `run_event_loop`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ralph-workflow test_event_loop_dumps_trace_on_panic`

Expected: PASS.

**Step 5: Commit**

```bash
git add ralph-workflow/src/app/event_loop.rs tests/integration_tests/reducer_hidden_behavior.rs
git commit -m "feat: persist event loop trace on panic"
```

---

### Task 3: Implement ring buffer and JSONL dump

**Files:**
- Modify: `ralph-workflow/src/app/event_loop.rs`
- Modify: `ralph-workflow/src/phases/context.rs` (if helper needed for tests)

**Step 1: Write the failing test (buffer size cap)**

```rust
#[test]
fn test_event_trace_buffer_keeps_last_n_entries() {
    // Construct buffer with capacity 3, push 5, assert only last 3 retained.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow test_event_trace_buffer_keeps_last_n_entries`

Expected: FAIL (type missing).

**Step 3: Write minimal implementation**

```rust
struct EventTraceEntry {
    iteration: usize,
    effect: String,
    event: String,
    phase: String,
    xsd_retry_pending: bool,
    xsd_retry_count: u32,
    invalid_output_attempts: u32,
    agent_index: usize,
    model_index: usize,
    retry_cycle: u32,
}

struct EventTraceBuffer {
    capacity: usize,
    entries: std::collections::VecDeque<EventTraceEntry>,
}
```

Dump with `workspace.write(Path::new(".agent/tmp/event_loop_trace.jsonl"), contents)`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ralph-workflow test_event_trace_buffer_keeps_last_n_entries`

Expected: PASS.

**Step 5: Commit**

```bash
git add ralph-workflow/src/app/event_loop.rs
git commit -m "feat: add event loop trace buffer"
```

---

### Task 4: Integrate trace buffer into event loop

**Files:**
- Modify: `ralph-workflow/src/app/event_loop.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_trace_entry_includes_effect_and_event() {
    // Run one loop iteration with a mock handler and assert JSONL includes effect/event names.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow test_trace_entry_includes_effect_and_event`

Expected: FAIL (fields missing).

**Step 3: Write minimal implementation**

Record effect, result.event, and state summary each iteration (including additional events).

**Step 4: Run test to verify it passes**

Run: `cargo test -p ralph-workflow test_trace_entry_includes_effect_and_event`

Expected: PASS.

**Step 5: Commit**

```bash
git add ralph-workflow/src/app/event_loop.rs tests/integration_tests/reducer_hidden_behavior.rs
git commit -m "feat: record event loop trace entries"
```

---

### Task 5: Tighten logging and docs

**Files:**
- Modify: `ralph-workflow/src/app/event_loop.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_max_iterations_logs_trace_path() {
    // Capture logs and assert they include the trace file path.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow test_max_iterations_logs_trace_path`

Expected: FAIL (log missing).

**Step 3: Write minimal implementation**

Update warning message to include trace path when dump succeeds.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ralph-workflow test_max_iterations_logs_trace_path`

Expected: PASS.

**Step 5: Commit**

```bash
git add ralph-workflow/src/app/event_loop.rs tests/integration_tests/reducer_hidden_behavior.rs
git commit -m "chore: log event loop trace path on exhaustion"
```
