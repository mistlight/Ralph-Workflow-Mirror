# Integration Test Rules

**Read before writing ANY test.** Real filesystem/git → [SYSTEM_TESTS.md](system_tests/SYSTEM_TESTS.md).

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
