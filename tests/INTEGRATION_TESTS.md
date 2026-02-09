# Integration Test Rules

**Read before writing ANY test.** Real filesystem/git → [SYSTEM_TESTS.md](system_tests/SYSTEM_TESTS.md).

## Running Integration Tests

**IMPORTANT:** Always use the explicit package command:

```bash
# Run integration tests (400+ tests) - THIS IS THE DEFAULT FOR CI
cargo test -p ralph-workflow-tests

# DO NOT use just `cargo test` - it runs everything including system tests
# DO NOT confuse with system tests (ralph-workflow-system-tests)
```

If you see significantly fewer tests (e.g., ~130), you may be running
system tests instead. Check your command and ensure you're targeting
`ralph-workflow-tests`.

## Architecture Context

Ralph uses a **reducer architecture** with two effect layers:

| Layer | Mock With | When |
|-------|-----------|------|
| `AppEffect` (CLI) | `MockAppEffectHandler` | Before repo root known |
| `Effect` (Pipeline) | `MemoryWorkspace` + `MockProcessExecutor` | After repo root known |

Test the layer you're in. Don't cross boundaries.

```
Pure logic (reducers, orchestration) → Unit tests, no mocks needed
Effect handlers → Integration tests with mocked I/O
Real filesystem/git → System tests only
```

## Banned

| Banned | Use Instead |
|--------|-------------|
| `TempDir`, `WorkspaceFs` | `MemoryWorkspace` |
| `std::fs::*` | `workspace.read()`, `workspace.write()` |
| `std::process::Command` | `MockProcessExecutor` |
| `cfg!(test)` in prod code | Dependency injection |
| Test file >1000 lines | Split into modules |

## Patterns

### Reducer Tests (Pure - No Mocks)
```rust
#[test]
fn test_state_transition() {
    let state = PipelineState::initial(5, 2);
    let event = PipelineEvent::DevelopmentIterationCompleted { iteration: 1, output_valid: true };
    let new_state = reduce(state, event);
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}
```

### Pipeline Effect Tests
```rust
#[test]
fn test_pipeline_effect() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan");
    let executor = MockProcessExecutor::new()
        .with_agent_result("claude", Ok(AgentCommandResult::success()));
    
    let result = run_phase(&executor, &workspace);
    assert!(workspace.was_written(".agent/output.xml"));
}
```

### CLI Layer Tests
```rust
#[test]
fn test_cli_operation() {
    let mut handler = MockAppEffectHandler::new()
        .with_file("PROMPT.md", "# Goal")
        .with_head_oid("abc123");
    
    run_cli_with_handler(&["--diagnose"], &mut handler).unwrap();
    assert!(handler.was_executed(&AppEffect::GitRequireRepo));
}
```

### Parser Tests
```rust
#[test]
fn test_parser() {
    let workspace = MemoryWorkspace::new_test();
    let printer: SharedPrinter = Rc::new(RefCell::new(TestPrinter::new()));
    let parser = SomeParser::with_printer(colors, verbosity, printer.clone());
    
    parser.parse_stream(BufReader::new(input.as_bytes()), &workspace).unwrap();
    assert!(printer.borrow().get_output().contains("expected"));
}
```

## API Reference

### MemoryWorkspace
```rust
let workspace = MemoryWorkspace::new_test()
    .with_file("path/file.txt", "content")
    .with_dir("path/dir");

workspace.read(Path::new("file.txt"))?;      // -> String
workspace.write(Path::new("file.txt"), "x")?; // creates parents
workspace.exists(Path::new("file.txt"));      // -> bool
workspace.was_written("path");                // test assertion
workspace.get_file("path");                   // -> Option<String>
```

### MockProcessExecutor
```rust
let executor = MockProcessExecutor::new()
    .with_output("git", "output")
    .with_agent_result("claude", Ok(AgentCommandResult::success()))
    .with_agent_result("fail", Ok(AgentCommandResult::failure(1, "err")));

executor.execute_calls();  // commands called
executor.agent_calls();    // agent spawns
```

### MockAppEffectHandler
```rust
let mut handler = MockAppEffectHandler::new()
    .with_file("PROMPT.md", "content")
    .with_head_oid("abc123")
    .with_repo_root("/test/repo");

handler.was_executed(&AppEffect::GitRequireRepo);  // -> bool
handler.get_file("path");                          // -> Option<String>
```

## Rules

- **Black-box only**: Test through public APIs, assert observable outcomes
- **Fix implementation, not tests**: When tests fail, fix the code (unless behavior intentionally changed)
- **Mock at boundaries only**: Filesystem, network, processes - never domain logic
- **Max 1000 lines per file**: Split large test files into focused modules

## Common Anti-Patterns

### ❌ Testing Internal State

**WRONG:**
```rust
assert_eq!(state.internal_retry_counter, 3);
```

**CORRECT:**
```rust
// Test observable behavior: transitions to failure state after retries
assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);
```

### ❌ Testing Array Lengths Without Content

**WRONG:**
```rust
assert_eq!(result.issues.len(), 3);
```

**CORRECT:**
```rust
// Test observable presence of each issue
assert!(output.contains("Issue 1"));
assert!(output.contains("Issue 2"));
assert!(output.contains("Issue 3"));
```

**Note:** Length assertions are acceptable when combined with content checks and when the count is part of the specification.

### ❌ Testing Private Implementation

**WRONG:**
```rust
assert!(parser.has_buffered_tokens());
```

**CORRECT:**
```rust
let output = printer.borrow().get_output();
assert!(output.contains("expected output"));
```

### ❌ Using Real Filesystem

**WRONG:**
```rust
use tempfile::TempDir;
let temp_dir = TempDir::new().unwrap();
```

**CORRECT:**
```rust
let workspace = MemoryWorkspace::new_test()
    .with_file("file.txt", "content");
```

## Detailed Examples from Real Tests

### Testing State Machines (Reducers)

For reducer tests, the **observable behavior IS the state transitions**. Public state fields that are serialized and drive behavior are NOT "internal state" - they're part of the observable contract.

**✅ CORRECT - Testing public state fields that drive behavior:**
```rust
#[test]
fn test_agent_exhaustion_increments_retry_cycle() {
    let state = PipelineState {
        agent_chain: AgentChainState::initial()
            .with_agents(vec!["agent1".to_string()], vec![vec![]], AgentRole::Developer)
            .with_max_cycles(3),
        phase: PipelinePhase::Development,
        ..PipelineState::initial(5, 2)
    };
    
    let new_state = reduce(state, PipelineEvent::agent_chain_exhausted(AgentRole::Developer));
    
    // These are PUBLIC state fields that:
    // 1. Are persisted in checkpoints (observable)
    // 2. Determine when to stop retrying (observable)
    // 3. Affect backoff delays (observable)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 1);
    assert_eq!(new_state.phase, PipelinePhase::Development);
}
```

**❌ WRONG - Testing actual internal/private fields:**
```rust
#[test]
fn test_parser_buffer_size() {
    let parser = SomeParser::new();
    parser.parse(input);
    
    // This is internal state - not part of the public API
    assert_eq!(parser.internal_buffer_size, 1024);  // WRONG!
}
```

### Testing XML Validation (Public API)

**✅ CORRECT - Testing behavioral outcomes with supplementary count checks:**
```rust
#[test]
fn test_review_xml_extracts_all_issues() {
    let xml = r#"<ralph-issues>
        <ralph-issue>Error 1</ralph-issue>
        <ralph-issue>Warning 1</ralph-issue>
        <ralph-issue>Info 1</ralph-issue>
    </ralph-issues>"#;
    
    let result = validate_issues_xml(xml).unwrap();
    
    // Count check is supplementary to content checks
    assert_eq!(result.issues.len(), 3, "Should extract all 3 issues");
    
    // Content checks are the primary assertions
    assert_eq!(result.issues[0], "Error 1");
    assert_eq!(result.issues[1], "Warning 1");
    assert_eq!(result.issues[2], "Info 1");
}
```

**Note:** The `.len()` assertion is acceptable here because:
1. It's testing a public API return value, not internal state
2. It's combined with content checks
3. The count is part of the specification (extract ALL issues)

### Testing Test Utilities (TestLogger, TestPrinter)

When testing test utilities themselves, assertions on counts and internal structure are acceptable because the utility's behavior IS its internal bookkeeping.

**✅ CORRECT - Testing test utility behavior:**
```rust
#[test]
fn test_logger_captures_multiple_messages() {
    let mut logger = TestLogger::new();
    
    writeln!(logger, "Message 1").unwrap();
    writeln!(logger, "Message 2").unwrap();
    
    // Testing the utility's behavior - count IS the observable behavior
    assert_eq!(logger.get_logs().len(), 2);
    assert!(logger.has_log("Message 1"));
    assert!(logger.has_log("Message 2"));
}
```

### Parser Tests - MUST Use TestPrinter

All parser tests MUST use `TestPrinter` to test observable output. Never test internal parser state or buffer contents.

**✅ CORRECT - Parser test with TestPrinter:**
```rust
use ralph_workflow::json_parser::printer::{SharedPrinter, TestPrinter};
use std::rc::Rc;
use std::cell::RefCell;

#[test]
fn test_gemini_parser_streams_deltas() {
    let workspace = MemoryWorkspace::new_test();
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer)
        .with_terminal_mode(TerminalMode::Full);
    
    let input = r#"{"type":"init","session_id":"test","model":"gemini-2.0"}
{"type":"message","role":"assistant","content":"Hello","delta":true}
{"type":"message","role":"assistant","content":" World","delta":true}"#;
    
    parser.parse_stream_for_test(BufReader::new(input.as_bytes()), &workspace).unwrap();
    
    // Test observable output - what the user sees
    let output = test_printer.borrow().get_output();
    assert!(output.contains("Hello"), "Should contain streamed text");
}
```

**❌ WRONG - Testing parser internal state:**
```rust
#[test]
fn test_parser_buffers_deltas() {
    let parser = GeminiParser::new();
    parser.parse_delta("Hello");
    
    // WRONG - testing internal buffer state
    assert_eq!(parser.delta_buffer.len(), 1);
    assert_eq!(parser.delta_buffer[0], "Hello");
}
```

### Testing Loop Detection and Metrics

When testing metrics or counters, focus on the behavioral outcome (does the system stop looping?) rather than the counter value itself.

**✅ CORRECT - Testing loop detection behavior:**
```rust
#[test]
fn test_system_prevents_infinite_loops() {
    let mut state = PipelineState::initial(10, 2);
    
    // Simulate many repeated events
    for i in 0..20 {
        state = reduce(state, PipelineEvent::agent_failed(AgentRole::Developer, "error"));
        
        if state.phase == PipelinePhase::AwaitingDevFix {
            // Observable behavior: system detected loop and stopped
            assert!(i < 20, "Should detect loop before 20 iterations");
            return;
        }
    }
    
    panic!("System should have detected loop and transitioned to failure state");
}
```

**❌ WRONG - Testing internal loop counter:**
```rust
#[test]
fn test_loop_counter_increments() {
    let mut state = PipelineState::initial(5, 2);
    state = reduce(state, PipelineEvent::agent_failed(AgentRole::Developer, "error"));
    
    // WRONG - testing internal counter implementation
    assert_eq!(state.internal_loop_counter, 1);
}
```

## When Length Assertions Are Acceptable

Length assertions (`.len()`) are acceptable when:

1. **Testing public API return values:**
   ```rust
   // Extracting issues from XML is the API contract
   let issues = validate_issues_xml(xml).unwrap().issues;
   assert_eq!(issues.len(), 3);  // OK - part of specification
   ```

2. **Combined with content checks:**
   ```rust
   assert_eq!(issues.len(), 2);  // Count
   assert_eq!(issues[0], "Issue 1");  // Content
   assert_eq!(issues[1], "Issue 2");  // Content
   ```

3. **Testing test utilities:**
   ```rust
   assert_eq!(logger.get_logs().len(), 2);  // OK - testing utility itself
   ```

4. **When count is observable user-facing behavior:**
   ```rust
   // User sees "Found 3 issues" in output
   assert_eq!(extracted_issues.len(), 3);
   ```

Length assertions are NOT acceptable when:

1. Testing internal collection sizes that don't affect observable behavior
2. Testing without corresponding content checks
3. Testing implementation details (e.g., buffer sizes, cache sizes)

## Reducer State Fields: Public vs Internal

Ralph's `PipelineState` and related state structs use **public fields** (`pub`) that are:
- Serialized to JSON checkpoints
- Persisted across pipeline runs
- Used to determine observable behaviors (retries, phase transitions, agent selection)

These fields are NOT "internal state" - they're part of the public state machine contract. Testing them is testing observable behavior.

**Public state fields (OK to test):**
- `phase: PipelinePhase`
- `iteration: u32`
- `agent_chain.retry_cycle: u32`
- `agent_chain.current_agent_index: usize`
- `continuation.invalid_output_attempts: u32`

**Actual internal state (NOT OK to test - doesn't exist in our codebase):**
- Private fields (e.g., `internal_buffer: Vec<u8>`)
- Non-serialized transient state
- Implementation details not in checkpoints

If it's in the checkpoint JSON, it's observable. If it's public and drives behavior, it's observable. Test it.

---

## Test Naming Guidelines

Test names should describe **observable behavior**, not implementation details:

**Good test names (behavior-focused):**
- `test_agent_fallback_after_internal_error_retry_exhaustion` - describes what happens
- `test_pipeline_stops_after_reaching_retry_limit` - describes observable outcome
- `test_parser_outputs_complete_message` - describes visible behavior

**Bad test names (implementation-focused):**
- `test_buffer_fills_correctly` - implementation detail
- `test_counter_increments` - internal bookkeeping
- `test_cache_invalidation` - internal mechanism

**Exception:** Test names containing "internal_error" are acceptable when testing how the system behaves when internal errors occur (the error type is observable, not the internal implementation).

## Audit Script

Run `bash scripts/audit_tests.sh` from repo root to verify compliance with these guidelines.

The script checks for:
- `cfg!(test)` usage in production code
- Real filesystem usage (`std::fs`, `TempDir`)
- Real process execution
- Files over 1000 lines
- Internal field assertions
- Parser tests using TestPrinter/VirtualTerminal
- MemoryWorkspace and MockProcessExecutor usage
- Length assertions without content checks
- Implementation-focused test names
- Integration guide references

**As of 2026-02-08: All 771 integration tests comply with behavioral testing principles.**

Key metrics from last audit:
- ✅ 771 integration tests passing
- ✅ 266 MemoryWorkspace usages (no real filesystem)
- ✅ 35 MockProcessExecutor usages (no real process execution)
- ✅ All parser tests use TestPrinter or VirtualTerminal
- ✅ No files over 1000 lines
- ✅ Length assertions combined with content checks
- ✅ 94 integration guide references across test files
