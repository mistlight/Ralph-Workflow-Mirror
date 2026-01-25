# Test Rules (Unit & Integration)

**For AI Agents: Read this ENTIRE file before writing or modifying ANY test.**

These rules apply to **ALL tests** - both unit tests and integration tests.

## ABSOLUTE PROHIBITIONS

Tests run in isolated containers with NO filesystem or network access.

| BANNED | USE INSTEAD |
|--------|-------------|
| `TempDir` | `MemoryWorkspace` |
| `WorkspaceFs` | `MemoryWorkspace` |
| `std::fs::*` | `workspace.read()`, `workspace.write()` |
| `std::process::Command` | `MockProcessExecutor` |
| `assert_cmd::Command` | Direct function calls with mocks |
| Real HTTP/network calls | Mocked HTTP traits |
| `cfg!(test)` in prod code | Dependency injection |
| Large test files (>1000 lines) | Split into focused modules |

## Required Imports

```rust
use ralph_workflow::workspace::MemoryWorkspace;
use ralph_workflow::executor::MockProcessExecutor;
use crate::test_timeout::with_default_timeout;  // integration tests only
```

## Test Patterns

### Parser Tests

```rust
#[test]
fn test_parser_behavior() {
    let workspace = MemoryWorkspace::new_test();
    let printer: SharedPrinter = Rc::new(RefCell::new(TestPrinter::new()));
    let parser = SomeParser::with_printer(colors, verbosity, printer.clone());
    
    let input = r#"{"type":"event"}"#;
    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader, &workspace).unwrap();
    
    let output = printer.borrow().get_output();
    assert!(output.contains("expected"));
}
```

### File Operation Tests

```rust
#[test]
fn test_file_operations() {
    let workspace = MemoryWorkspace::new_test()
        .with_file("/test/input.txt", "content");
    
    let result = some_function(&workspace);
    
    assert!(workspace.was_written("/test/output.txt"));
}
```

### Agent/Process Tests

```rust
#[test]
fn test_agent_execution() {
    let workspace = MemoryWorkspace::new_test();
    let executor = MockProcessExecutor::new()
        .with_output("git", "")
        .with_agent_result("echo", Ok(AgentCommandResult::success()));
    
    let result = run_pipeline(&executor, &workspace);
    assert!(result.is_ok());
}
```

## MemoryWorkspace API

```rust
let workspace = MemoryWorkspace::new_test();                    // root: /test/repo
let workspace = MemoryWorkspace::new_test()
    .with_file("/path/file.txt", "content")
    .with_dir("/path/dir");

workspace.read(Path::new("/test/file.txt"))?;                   // -> String
workspace.write(Path::new("/test/file.txt"), "content")?;       // creates parents
workspace.exists(Path::new("/test/file.txt"));                  // -> bool
workspace.read_dir(Path::new("/test/dir"))?;                    // -> Vec<DirEntry>

// Test assertions
workspace.was_written("/path");                                 // -> bool
workspace.get_file("/path");                                    // -> Option<String>
```

## MockProcessExecutor API

```rust
let executor = MockProcessExecutor::new()
    .with_output("git", "output")
    .with_agent_result("echo", Ok(AgentCommandResult::success()))
    .with_agent_result("fail", Ok(AgentCommandResult::failure(1, "error")));

executor.execute_calls();   // what was called
executor.agent_calls();     // agent spawn calls
```

## Test File Size Rule

**Max 1000 lines per test file.** Split large test files into focused modules:

```
tests/integration_tests/
├── parser/
│   ├── mod.rs
│   ├── codex_tests.rs      # ≤200 lines
│   ├── gemini_tests.rs     # ≤200 lines
│   └── claude_tests.rs     # ≤200 lines
```

## Quick Reference

| Forbidden | Allowed |
|-----------|---------|
| `TempDir::new()` | `MemoryWorkspace::new_test()` |
| `std::fs::write()` | `workspace.write()` |
| `std::fs::read_to_string()` | `workspace.read()` |
| `Command::new("anything")` | `MockProcessExecutor` |
| Test file >1000 lines | Split into modules |
