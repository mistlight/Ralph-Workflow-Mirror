# Integration Test Philosophy

This document defines the guiding principles for integration tests in this project.
AI agents and human contributors MUST follow these rules when writing, modifying,
or debugging integration tests.

For general testing philosophy and design principles, see [CODE_STYLE.md](../CODE_STYLE.md).

---

## Core Principle: Test Behavior, Not Implementation

Integration tests verify **observable behavior** at system boundaries. They answer:
"Does this system do what users/callers expect?"

Integration tests do NOT verify:
- How the code achieves the behavior internally
- Which internal functions are called
- The order of internal operations
- Internal data structures or state

### The Golden Rule

> **If an integration test fails, the test itself should almost NEVER be updated
> to accommodate a new implementation.**

A failing integration test indicates ONE of these scenarios:

| Scenario | Action |
|----------|--------|
| **Behavior changed intentionally** | Update test to reflect new expected behavior |
| **Test was buggy** | Fix the test (it never correctly tested the behavior) |
| **Implementation has a bug** | Fix the implementation |

**NEVER** update tests solely because the implementation changed. If the behavior
contract is unchanged, a failing test means the new implementation is broken.

### What "Behavior" Means

Behavior is what external observers can see:

- **Inputs** → **Outputs** (function returns, CLI output, API responses)
- **Side effects** (files created, network calls made, state changes)
- **Error conditions** (what errors are raised and when)
- **Invariants** (guarantees that always hold)

Behavior is NOT:
- Internal variable names or types
- Which helper functions are called
- Memory layout or performance characteristics (unless explicitly guaranteed)
- Logging output (unless it's part of the public contract)

---

## Mocking Strategy

### What to Mock (External Dependencies)

Mock at **true architectural boundaries**:

- External services (APIs, third-party integrations)
- Filesystem operations (use `TempDir` for isolation)
- Network/HTTP calls
- System clock, randomness
- Databases, message queues

### What NOT to Mock (Internal Code)

**DO NOT** mock:

- Domain logic
- Internal helper functions
- Collaborators within the same module/crate
- Pure or deterministic functions

### Signs of Over-Mocking

If you find yourself needing to mock internal code to write a test, this indicates:

1. **Poor boundaries** - The code lacks clear separation between I/O and logic
2. **Wrong test level** - Consider whether this should be a unit test instead
3. **Coupling** - The code is too tightly coupled to implementation details

The fix is to refactor the production code, not to add more mocks.

---

## When to Update Integration Tests

### Valid Reasons to Update a Test

1. **Intentional behavior change**: The expected behavior has changed as part of
   a feature or fix. Document WHY in the commit message.

2. **Test was incorrect**: The test never correctly verified the intended behavior.
   This is a test bug fix.

3. **Test was flaky**: The test had race conditions or environmental dependencies.
   Fix the test to be deterministic.

### Invalid Reasons to Update a Test

- "The implementation changed" (but behavior didn't)
- "The test is failing after my refactor" (refactors shouldn't change behavior)
- "It's easier to change the test than fix the code"

### Decision Tree

```
Test is failing
    │
    ├─► Did the EXPECTED BEHAVIOR change intentionally?
    │       YES → Update test to match new behavior
    │       NO  ↓
    │
    ├─► Was the test itself buggy/flaky?
    │       YES → Fix the test
    │       NO  ↓
    │
    └─► The implementation has a bug → Fix the implementation
```

---

## Test Architecture

### Directory Structure

```
tests/
├── integration_tests/       # Main integration test package
│   ├── main.rs              # Test entry point, declares all modules
│   ├── common/              # Shared test utilities
│   │   └── mod.rs           # ralph_cmd(), ralph_bin_path()
│   ├── workflows/           # Workflow integration tests
│   │   ├── review.rs        # Review workflow tests
│   │   ├── config.rs        # Configuration tests
│   │   └── ...
│   ├── deduplication/       # Parser deduplication tests
│   │   └── mod.rs           # Uses TestPrinter pattern
│   ├── cli/                 # CLI argument and output tests
│   └── ...
├── deduplication_integration_tests/
│   └── fixtures/            # Real log files for testing
└── Cargo.toml               # Test package configuration
```

### Common Patterns

#### Pattern 1: TestPrinter for Parser Testing

Used when testing streaming/parsing behavior without actual I/O:

```rust
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

use ralph_workflow::json_parser::printer::{SharedPrinter, TestPrinter};
use ralph_workflow::json_parser::ClaudeParser;

#[test]
fn test_parser_behavior() {
    // 1. Create TestPrinter (captures output instead of printing)
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    
    // 2. Create parser with the test printer
    let parser = ClaudeParser::with_printer(colors, verbosity, printer);
    
    // 3. Feed input through the REAL code path
    let input = r#"{"type":"stream_event",...}"#;
    let cursor = Cursor::new(input);
    parser.parse_stream(std::io::BufReader::new(cursor))
        .expect("parse_stream should succeed");
    
    // 4. Assert on OBSERVABLE OUTPUT (behavior)
    let output = test_printer.borrow().get_output();
    assert!(output.contains("expected text"), "Should produce expected output");
    
    // 5. Assert on OBSERVABLE METRICS (behavior)
    let metrics = parser.streaming_metrics();
    assert_eq!(metrics.total_deltas, 5, "Should process 5 deltas");
}
```

**Key points:**
- Uses the REAL `parse_stream()` code path (not a test-only path)
- Asserts on observable output, not internal state
- TestPrinter is an architectural boundary mock (replaces stdout)

#### Pattern 2: CLI Testing with assert_cmd

Used when testing the CLI binary as a black box:

```rust
use tempfile::TempDir;
use predicates::prelude::*;

use crate::common::ralph_cmd;

#[test]
fn test_cli_behavior() {
    // 1. Set up isolated environment
    let dir = TempDir::new().unwrap();
    
    // 2. Create any required fixtures
    std::fs::write(dir.path().join("input.txt"), "test content").unwrap();
    
    // 3. Run the CLI as a subprocess (true black-box test)
    let mut cmd = ralph_cmd();
    cmd.current_dir(dir.path())
        .env("SOME_CONFIG", "value")      // Control environment
        .arg("--some-flag")
        .arg("input.txt");
    
    // 4. Assert on OBSERVABLE BEHAVIOR
    cmd.assert()
        .success()                                    // Exit code
        .stdout(predicate::str::contains("expected")); // Output
    
    // 5. Assert on SIDE EFFECTS (files created, etc.)
    assert!(dir.path().join("output.txt").exists(), "Should create output file");
}
```

**Key points:**
- Tests the binary as users would invoke it
- Uses `TempDir` for filesystem isolation
- Environment variables control configuration (no internal mocking)
- Asserts on exit code, stdout/stderr, and file side effects

---

## Writing New Integration Tests

### Checklist

Before writing a new integration test, verify:

- [ ] **Testing behavior**: Does this test verify observable behavior, not implementation?
- [ ] **Black-box**: Could this test pass with a completely different internal implementation?
- [ ] **Mocking boundaries**: Am I only mocking external dependencies (filesystem, network)?
- [ ] **No internal knowledge**: Does the test avoid importing internal/private modules?
- [ ] **Deterministic**: Will this test produce the same result every time?
- [ ] **Isolated**: Does this test clean up after itself and not affect other tests?

### Anti-Patterns to Avoid

| Anti-Pattern | Why It's Wrong | Fix |
|--------------|----------------|-----|
| Mocking internal functions | Tests implementation, not behavior | Refactor code or use integration boundary |
| Asserting on log messages | Logs are not part of the behavior contract | Assert on outputs/side effects instead |
| Testing private functions | Private = implementation detail | Test through public API |
| Brittle string matching | Ties test to exact formatting | Use semantic assertions |
| Shared mutable state | Tests affect each other | Use `TempDir`, reset state |

### Example: Adding a New Deduplication Test

```rust
/// Test that [SPECIFIC SCENARIO] produces [EXPECTED BEHAVIOR].
///
/// This verifies that when [CONDITION], the system [OBSERVABLE OUTCOME].
#[test]
fn test_specific_scenario_expected_behavior() {
    // Setup: Create test printer and parser
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = ClaudeParser::with_printer(Colors::new(), Verbosity::Normal, printer);

    // Input: Construct the scenario
    let events = [
        // ... events that trigger the scenario
    ];
    let input = events.join("\n");
    
    // Execute: Run through real code path
    let cursor = Cursor::new(input);
    parser.parse_stream(std::io::BufReader::new(cursor))
        .expect("parse_stream should succeed");

    // Assert: Verify OBSERVABLE behavior
    let printer_ref = test_printer.borrow();
    let output = printer_ref.get_output();
    
    // Good: Assert on what the user would see
    assert!(output.contains("expected content"), "Should render expected content");
    
    // Good: Assert on behavioral metrics
    let metrics = parser.streaming_metrics();
    assert_eq!(metrics.some_count, expected, "Should track expected metric");
    
    // Bad: Don't assert on internal state
    // assert_eq!(parser.internal_buffer.len(), 5); // WRONG!
}
```

---

## Cross-References

- **[CODE_STYLE.md](../CODE_STYLE.md)** - Design principles, black-box testing philosophy, mocking discipline
- **[AGENTS.md](../AGENTS.md)** - Build commands, CI expectations
- **[tests/integration_tests/deduplication/mod.rs](integration_tests/deduplication/mod.rs)** - Well-documented example tests

---

## Summary

1. **Test behavior, not implementation** - If it's not observable, don't test it
2. **Mock boundaries, not internals** - Filesystem/network yes, helper functions no
3. **Failing test = behavior mismatch** - Fix implementation, not tests
4. **Use real code paths** - TestPrinter replaces I/O, not logic
