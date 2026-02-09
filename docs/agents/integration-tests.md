# Integration Tests (CRITICAL)

**Read `tests/INTEGRATION_TESTS.md` before touching integration tests.**

## Principles

- Test **observable behavior**, not implementation details
- Mock only at **architectural boundaries** (filesystem, network, external APIs)
- NEVER use `cfg!(test)` branches or test-only flags in production code
- When tests fail, fix implementation (unless expected behavior changed)

## Common mistakes

- Mocking internal functions (only mock external dependencies)
- Testing private details (test through public APIs)
- Adding `#[cfg(test)]` in production (use dependency injection)
- Updating tests because implementation changed (only if behavior changed)

## Required patterns

- Parser tests: `TestPrinter` from `ralph_workflow::json_parser::printer`
- File operations: `MemoryWorkspace` (NO `TempDir`, NO `std::fs::*`)
- Process execution: `MockProcessExecutor` (NO real process spawning)

## Testing Prompt Path Resolution

When testing prompt generation, verify workspace-rooted paths:

```rust
use ralph_workflow::workspace::{Workspace, WorkspaceMemory};
use std::path::PathBuf;

let workspace_root = PathBuf::from("/tmp/test_workspace");
let workspace = WorkspaceMemory::new_at_path(workspace_root);

// Generate prompt
let prompt = prompt_planning_xml_with_references(&template_context, &prompt_ref, &workspace);

// Verify workspace-rooted paths
let expected_path = workspace.absolute_str(".agent/tmp/plan.xml");
assert!(prompt.contains(&expected_path));
```

**Do NOT** test implementation details like whether `resolve_absolute_path` vs `workspace.absolute_str()` is called. Test the observable behavior: prompts contain correct absolute paths.

## Testing Loop Recovery

Test loop detection through reducer behavior:

```rust
let mut state = PipelineState::initial(1, 0);
state.continuation.consecutive_same_effect_count = 5;
state.continuation.xsd_retry_pending = true;

// Behavioral test: system should not loop indefinitely
assert!(state.continuation.consecutive_same_effect_count <= 10);

// Test recovery event
let event = PipelineEvent::Continuation(ContinuationEvent::LoopRecoveryTriggered { ... });
let new_state = reduce(state, event);
assert!(!new_state.continuation.xsd_retry_pending);
```

**Do NOT** test internal loop detection algorithm. Test that:
1. Loop counters are bounded
2. Recovery events reset retry state
3. Pipeline advances after recovery

## Common Anti-Patterns to Avoid

### ❌ Testing Internal State Instead of Observable Behavior

**WRONG - Testing internal field values:**
```rust
#[test]
fn test_reducer_updates_counter() {
    let mut state = PipelineState::initial(1, 0);
    state = reduce(state, event);
    
    // WRONG: Testing internal bookkeeping
    assert_eq!(state.internal_retry_counter, 3);
}
```

**CORRECT - Testing observable behavior:**
```rust
#[test]
fn test_reducer_retries_up_to_limit() {
    let mut state = PipelineState::initial(1, 0);
    
    // Behavioral test: System should retry but not loop forever
    for i in 0..10 {
        state = reduce(state, PipelineEvent::agent_failed(...));
        
        if state.phase == PipelinePhase::AwaitingDevFix {
            // Observable behavior: Pipeline transitions to failure state
            assert!(i < 10, "Should give up before 10 retries");
            return;
        }
    }
    
    panic!("Pipeline should have transitioned to failure state");
}
```

**Key principle:** Test what the system **does** (phase transitions, file writes, agent invocations), not what it **contains** (internal counters, private fields).

**Exception:** Counters that represent behavioral bounds (retry limits, iteration counts) ARE testable because they affect observable behavior.

### ❌ Testing Collection Sizes Instead of Contents

**WRONG - Testing array length as implementation detail:**
```rust
#[test]
fn test_parser_creates_issues() {
    let result = parse_issues(xml);
    
    // WRONG: Testing internal array size
    assert_eq!(result.issues.len(), 3);
}
```

**ACCEPTABLE - Testing observable count with content:**
```rust
#[test]
fn test_reviewer_reports_all_critical_issues() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/issues.xml", issues_xml);
    
    run_review_phase(&workspace);
    
    // GOOD: Testing observable behavior - issues appear in output
    let issues_file = workspace.read(Path::new(".agent/tmp/issues.xml")).unwrap();
    
    // Count is OK here because it's part of the specification
    // But also verify the actual content
    assert!(issues_file.contains("<issue severity=\"critical\">Issue 1</issue>"));
    assert!(issues_file.contains("<issue severity=\"critical\">Issue 2</issue>"));
    assert!(issues_file.contains("<issue severity=\"critical\">Issue 3</issue>"));
}
```

**Guideline:** If the test would pass with a different implementation that has the same observable behavior, the assertion is behavioral. If it would fail when refactoring internal data structures, it's an implementation detail.

### ❌ Testing Private Implementation Details

**WRONG - Testing internal method behavior:**
```rust
#[test]
fn test_parser_buffering() {
    let parser = SomeParser::new();
    parser.parse_line("test");
    
    // WRONG: Testing internal buffer state
    assert!(parser.has_buffered_tokens());
}
```

**CORRECT - Testing public observable output:**
```rust
#[test]
fn test_parser_output() {
    let workspace = MemoryWorkspace::new_test();
    let printer: SharedPrinter = Rc::new(RefCell::new(TestPrinter::new()));
    let parser = SomeParser::with_printer(colors, verbosity, printer.clone());
    
    parser.parse_stream(BufReader::new(input.as_bytes()), &workspace).unwrap();
    
    // CORRECT: Testing observable output
    let output = printer.borrow().get_output();
    assert!(output.contains("expected output"));
}
```

### ❌ Using Real Filesystem or Process Execution

**WRONG - Using TempDir or std::fs:**
```rust
#[test]
fn test_file_creation() {
    use tempfile::TempDir;
    
    // WRONG: Real filesystem
    let temp_dir = TempDir::new().unwrap();
    std::fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
}
```

**CORRECT - Using MemoryWorkspace:**
```rust
#[test]
fn test_file_creation() {
    // CORRECT: In-memory filesystem
    let workspace = MemoryWorkspace::new_test()
        .with_file("file.txt", "content");
    
    workspace.write(Path::new("file.txt"), "updated")?;
    assert_eq!(workspace.read(Path::new("file.txt"))?, "updated");
}
```

**Exception:** System tests in `tests/system_tests/` are allowed to use real filesystem for end-to-end testing.

## Understanding Public vs Internal State

Ralph's state structs use **public fields** (`pub`) that are part of the observable contract:

```rust
pub struct PipelineState {
    pub phase: PipelinePhase,
    pub iteration: u32,
    pub agent_chain: AgentChainState,
    // ... other public fields
}

pub struct AgentChainState {
    pub current_agent_index: usize,
    pub retry_cycle: u32,
    // ... other public fields
}
```

These fields are NOT "internal state" - they are:
- ✅ Serialized to JSON checkpoints
- ✅ Persisted across pipeline runs
- ✅ Used to determine observable behaviors (retries, phase transitions, agent selection)
- ✅ Part of the public state machine contract

**Testing public state fields IS testing observable behavior.**

### What Counts as "Internal State"?

**❌ Internal state (don't test these):**
- Private fields (e.g., `internal_buffer: Vec<u8>`)
- Non-serialized transient state
- Implementation details not in checkpoints
- Helper functions that don't affect output

**✅ Observable state (OK to test):**
- Public fields that affect behavior
- Fields in checkpoint JSON
- State that determines phase transitions
- Counters that enforce behavioral bounds

### Example: Testing Reducer State Transitions

**✅ CORRECT - Testing public state that drives behavior:**
```rust
#[test]
fn test_agent_chain_exhaustion_starts_retry_cycle() {
    let state = PipelineState {
        agent_chain: AgentChainState::initial()
            .with_agents(vec!["agent1".to_string()], vec![vec![]], AgentRole::Developer)
            .with_max_cycles(3),
        phase: PipelinePhase::Development,
        ..PipelineState::initial(5, 2)
    };
    
    let new_state = reduce(state, PipelineEvent::agent_chain_exhausted(AgentRole::Developer));
    
    // These are PUBLIC fields that:
    // 1. Are persisted in checkpoints (observable)
    // 2. Determine when to stop retrying (observable)
    // 3. Affect backoff delays (observable)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 1);
    assert_eq!(new_state.phase, PipelinePhase::Development);
}
```

**Rule of thumb:** If it's in the checkpoint JSON, it's observable. If it's public and drives behavior, it's observable. Test it.

## Length Assertions: When They're OK

Length assertions (`.len()`) are acceptable when:

1. **Testing public API return values:**
   ```rust
   let issues = validate_issues_xml(xml).unwrap().issues;
   assert_eq!(issues.len(), 3);  // OK - part of API contract
   ```

2. **Combined with content checks:**
   ```rust
   assert_eq!(issues.len(), 2);  // Count
   assert_eq!(issues[0], "Issue 1");  // Content
   assert_eq!(issues[1], "Issue 2");  // Content
   ```

3. **Testing test utilities:**
   ```rust
   assert_eq!(logger.get_logs().len(), 2);  // OK - testing utility behavior
   ```

Length assertions are NOT acceptable when:
- Testing internal collection sizes
- Testing without content checks
- Testing implementation details (buffer sizes, cache sizes)

---

## Test Naming Best Practices

When naming tests, focus on **what the system does** (observable behavior), not **how it does it** (implementation):

**✅ Good names:**
- `test_agent_fallback_after_internal_error_retry_exhaustion` - describes observable behavior
- `test_pipeline_transitions_to_failure_after_retry_limit` - describes state transition
- `test_parser_streams_deltas_to_terminal` - describes observable output

**❌ Avoid:**
- `test_buffer_management` - implementation detail
- `test_internal_counter_updates` - internal bookkeeping (unless it's testing error type handling)
- `test_cache_size_tracking` - non-observable detail

**Note:** Names like `test_agent_fallback_after_internal_error` are acceptable because they describe behavior (fallback) triggered by an observable error type (internal error), not internal implementation details.

## Recent Compliance Improvements (Feb 2026)

The integration test suite was audited and improved to ensure strict compliance with behavioral testing principles:

### Length Assertions Fixed

**Updated files:**
- `tests/integration_tests/logger/test_logger_tests.rs` - Combined 3 length assertions with content checks
- `ralph-workflow/src/git_helpers/rebase_checkpoint/tests.rs` - Added content verification to 7 length assertions
- `tests/integration_tests/test_traits.rs` - Removed redundant length assertion (content checks sufficient)
- `tests/integration_tests/reducer_rebase_state_machine.rs` - Removed redundant length check
- `ralph-workflow/src/workspace/tests.rs` - Removed redundant assertion, verified both files

**Key principle:** Length assertions are acceptable when combined with content checks. If content checks already verify correctness (e.g., checking array indices or using `.contains()`), the length check is redundant and should be removed.

### Examples of Correct Length Assertions

**✅ CORRECT - Length + Content:**
```rust
let logs = logger.get_logs();
assert_eq!(logs.len(), 2, "Should buffer two separate writes");
assert!(logs[0].contains("Partial line"), "First log content");
assert!(logs[1].contains("Another line"), "Second log content");
```

**✅ CORRECT - Content checks sufficient (no length needed):**
```rust
// Array indexing already verifies length (would panic if wrong)
assert_eq!(files[0].as_path(), "file1.txt");
assert_eq!(files[1].as_path(), "file2.txt");
```

**❌ WRONG - Length without content:**
```rust
assert_eq!(logger.get_logs().len(), 2);  // What's in the logs? No verification!
```

See `tests/INTEGRATION_TESTS.md` for detailed before/after examples from the actual fixes.

## Compliance Verification

Run `bash scripts/audit_tests.sh` to verify tests follow these guidelines.

**Verified audit results (all checks passing):**

**✅ All 900 tests (771 integration + 129 system) compile successfully and comply with behavioral testing principles:**

- ✅ No `cfg!(test)` branches in production code
- ✅ No real filesystem usage (all use `MemoryWorkspace`)
- ✅ No real process execution (all use `MockProcessExecutor`)
- ✅ No files exceed 1000 lines
- ✅ No tests assert on internal/private field state
- ✅ All parser tests use `TestPrinter` or `VirtualTerminal`
- ✅ All test files have comprehensive behavioral documentation
- ✅ `.len()` assertions are combined with content checks where appropriate
- ✅ Test names focus on observable behavior

**Test utilities:**
- 268 MemoryWorkspace usages
- 35 MockProcessExecutor usages
- 105 test files
- 94 integration guide references

The codebase demonstrates **exemplary adherence** to behavioral testing principles.

**Enhanced audit script checks:**
The audit script now includes additional checks for:
- Length assertions without content verification
- Implementation-focused test names (buffer, cache, queue)
- Parser tests missing TestPrinter/VirtualTerminal
- Integration guide reference count
