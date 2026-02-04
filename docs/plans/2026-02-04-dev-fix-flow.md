# Dev-Fix Flow Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Dispatch the development agent on failure and ensure completion markers emit with regression coverage.

**Architecture:** Add a dev-fix dispatch path in the TriggerDevFixFlow handler that builds a prompt, invokes the developer agent, and emits DevFixTriggered/DevFixCompleted with completion marker emission. Update mock handler behavior and tests to assert dev-fix dispatch and completion markers in the failure event loop.

**Tech Stack:** Rust, reducer/effect handler, integration tests with MemoryWorkspace and MockProcessExecutor

---

### Task 1: Add failing regression test for dev-fix dispatch

**Files:**
- Modify: `tests/integration_tests/reducer_fault_tolerance/failure_completion_marker.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_failed_status_dispatches_dev_fix_agent_and_emits_completion_marker() {
    // Arrange: state in AwaitingDevFix and event loop with MainEffectHandler
    // Act: run event loop
    // Assert: completion marker exists and MockProcessExecutor recorded an agent spawn
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow-tests test_failed_status_dispatches_dev_fix_agent_and_emits_completion_marker`
Expected: FAIL because TriggerDevFixFlow does not dispatch dev agent.

---

### Task 2: Implement dev-fix dispatch in TriggerDevFixFlow

**Files:**
- Modify: `ralph-workflow/src/reducer/handler/mod.rs`

**Step 1: Write minimal implementation**

```rust
// Build a dev-fix prompt (with safe fallbacks for PROMPT/PLAN)
// Invoke developer agent via invoke_agent
// Emit DevFixTriggered + DevFixCompleted + CompletionMarkerEmitted
```

**Step 2: Run test to verify it passes**

Run: `cargo test -p ralph-workflow-tests test_failed_status_dispatches_dev_fix_agent_and_emits_completion_marker`
Expected: PASS

---

### Task 3: Update mock handler and existing tests for new dev-fix events

**Files:**
- Modify: `ralph-workflow/src/reducer/mock_effect_handler/mock_handler.rs`
- Modify: `tests/integration_tests/reducer_fault_tolerance/failure_completion_marker.rs`
- Modify: `tests/integration_tests/reducer_fault_tolerance/continuation_exhaustion.rs`
- Modify: `tests/integration_tests/reducer_error_handling.rs`

**Step 1: Update test expectations**

```rust
// Replace DevFixSkipped expectations with DevFixTriggered/DevFixCompleted
```

**Step 2: Run focused tests**

Run: `cargo test -p ralph-workflow-tests reducer_fault_tolerance::failure_completion_marker`
Expected: PASS

---

### Task 4: Run relevant test suite

**Step 1: Run integration tests covering failure handling**

Run: `cargo test -p ralph-workflow-tests reducer_fault_tolerance`
Expected: PASS

---
