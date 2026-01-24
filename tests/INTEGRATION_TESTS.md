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

## Strict External Dependency Rules

This section defines **non-negotiable rules** for handling external dependencies in tests.
Violations of these rules are blocking issues that must be fixed before code can be merged.

### Rule 1: All External Dependencies MUST Be Mocked

Integration tests **MUST NOT** make real calls to any external system. This includes:

| External Dependency | Requirement |
|---------------------|-------------|
| **AI/LLM APIs** | MUST be mocked. Never call OpenAI, Anthropic, or any AI service |
| **File system** | MUST use `TempDir` for isolation. Never write to real paths |
| **Network/HTTP** | MUST be mocked. No real HTTP calls to external services |
| **Console/stdout** | MUST be captured via `TestPrinter` or similar. No direct `println!` |
| **System clock** | MUST be injectable/mockable for time-dependent tests |
| **Environment variables** | MUST be explicitly set in test, not inherited from system |
| **Databases** | MUST use test instances or in-memory alternatives |
| **External processes** | MUST be mocked or use controlled test fixtures |

### Rule 1.5: NO Process Spawning in Integration Tests

**ABSOLUTE PROHIBITION**: Integration tests MUST NOT spawn ANY external processes, PERIOD.

This is a **CRITICAL VIOLATION** of the integration test style guide. There are NO exceptions.

**FORBIDDEN - Every single external process is prohibited:**

| Prohibited Pattern | Reason |
|-------------------|---------|
| `std::process::Command::new("git")` | Git operations must use git2 library or MockGit |
| `std::process::Command::new("ls")` | File operations must use std::fs or TempDir |
| `std::process::Command::new("cat")` | File reading must use std::fs::read |
| `std::process::Command::new("curl")` | HTTP requests must use mocked HTTP client |
| `std::process::Command::new("wget")` | HTTP requests must use mocked HTTP client |
| `std::process::Command::new("grep")` | Text search must use Rust string methods |
| `std::process::Command::new("find")` | File search must use walkdir or std::fs |
| `std::process::Command::new("echo")` | Use direct string manipulation |
| `std::process::Command::new("mkdir")` | Use std::fs::create_dir |
| `std::process::Command::new("rm")` | Use std::fs::remove_file/remove_dir |
| `std::process::Command::new("cp")` | Use std::fs::copy |
| `std::process::Command::new("mv")` | Use std::fs::rename |
| `std::process::Command::new("touch")` | Use std::fs::File::create |
| `std::process::Command::new("chmod")` | Use std::fs::set_permissions |
| `std::process::Command::new("ps")` | Process info must be mocked |
| `std::process::Command::new("kill")` | Process control must be mocked |
| `std::process::Command::new("cargo")` | Build tests must avoid spawning build processes |
| `std::process::Command::new("rustc")` | Compilation tests must be mocked |
| `std::process::Command::new("npm")` | Package managers must be mocked |
| `std::process::Command::new("yarn")` | Package managers must be mocked |
| `std::process::Command::new("python")` | Script execution must be mocked |
| `std::process::Command::new("node")` | Script execution must be mocked |
| `std::process::Command::new("ralph")` | CLI tests must call app::run() directly with logger injection |
| `std::process::Command::new(ANYTHING)` | NO PROCESS SPAWNING WHATSOEVER |
| `assert_cmd::Command::new(ANYTHING)` | Binary spawning is forbidden for ANY binary |
| `assert_cmd::Command::cargo_bin(ANYTHING)` | Cargo binary spawning is forbidden |
| `std::thread::spawn()` for test execution | All execution must be synchronous and mockable |
| ANY subprocess spawning | Breaks test determinism and isolation |

**The rule is simple: If you're thinking of spawning a process, STOP. There is ALWAYS a better way.**

**Why this is forbidden:**
- Tests must be deterministic (subprocess timing is unpredictable)
- Tests must be fast (subprocess spawn + execution is slow)
- Tests must be isolated (subprocess may affect system state)
- Tests must work in CI (subprocess may not be available)
- Tests must mock at boundaries (subprocess = unmocked external dependency)
- Even CLI binary testing must use direct function calls, not process spawning

**Enforcement:**

1. **Timeout wrapper**: The `with_default_timeout()` wrapper automatically detects process spawning and fails the test
2. **Compliance checker**: Script checks for ALL forms of process spawning:
   - `std::process::Command::new`
   - `assert_cmd::Command::new`
   - `assert_cmd::Command::cargo_bin`
   - Any other process spawning patterns
3. **Code review**: Reviewers MUST reject ANY test that spawns ANY process
4. **CI enforcement**: Tests that spawn processes will fail immediately in CI
5. **No exceptions**: There are NO valid use cases for spawning processes in integration tests

**Remember: The prohibition is ABSOLUTE. No processes. No exceptions. Ever.**

**Allowed:**

```rust
// ✅ OK: Use git2 library for git operations
use git2::Repository;
let repo = Repository::open(path)?;

// ✅ OK: Use MockGit for testing git operations
let mock = MockGit::new();
let result = GitOps::commit(&mock, "message", None, None)?;

// ✅ OK: Use TempDir for filesystem isolation
let dir = TempDir::new()?;
std::fs::write(dir.path().join("test.txt"), "content")?;

// ✅ OK: Use run_ralph_cli() for CLI testing (calls app::run() directly)
use crate::common::run_ralph_cli;
use ralph_workflow::logger::output::TestLogger;

let test_logger = TestLogger::new();
run_ralph_cli(&["--version"], &test_logger);
assert!(test_logger.has_log("0.1.0"));
```

**Forbidden (ALL of these are VIOLATIONS):**

```rust
// ❌ FORBIDDEN: Spawning git subprocess
use std::process::Command;
let output = Command::new("git")
    .args(["status", "--porcelain"])
    .output()?;

// ❌ FORBIDDEN: Spawning ls subprocess
let files = Command::new("ls")
    .arg("-la")
    .output()?;

// ❌ FORBIDDEN: Spawning cat subprocess
let content = Command::new("cat")
    .arg("file.txt")
    .output()?;

// ❌ FORBIDDEN: Spawning curl subprocess
let response = Command::new("curl")
    .args(["-s", "https://api.example.com"])
    .output()?;

// ❌ FORBIDDEN: Spawning grep subprocess
let matches = Command::new("grep")
    .args(["-r", "pattern", "."])
    .output()?;

// ❌ FORBIDDEN: Spawning echo subprocess
Command::new("echo")
    .arg("test")
    .spawn()?;

// ❌ FORBIDDEN: Spawning ralph CLI binary process (CRITICAL VIOLATION)
use assert_cmd::Command;
let output = Command::new("ralph")
    .arg("--version")
    .output()?;

// ❌ FORBIDDEN: Spawning ralph via cargo bin
let mut cmd = Command::cargo_bin("ralph")?;
cmd.assert().success();

// ❌ FORBIDDEN: Spawning ANY process via path
let bin_path = env!("CARGO_BIN_EXE_ralph");
let output = Command::new(bin_path)
    .output()?;

// ❌ FORBIDDEN: Using Command::new with ANY argument
let output = Command::new("literally_any_command")
    .output()?;

// ❌ FORBIDDEN: Using thread::spawn for test execution
let handle = thread::spawn(|| {
    // test code that spawns processes
});
handle.join().unwrap();
```

**If you see tests spawning processes:**

1. **For git operations**: Use git2 library or MockGit trait
2. **For file operations** (ls, cat, find, etc.): Use std::fs functions
3. **For HTTP requests** (curl, wget): Use mocked HTTP client traits
4. **For text processing** (grep, sed, awk): Use Rust string methods
5. **For ralph CLI testing**: Use `run_ralph_cli()` which calls app::run() directly
6. **For any other command**: Find the Rust library or mock the behavior

**Common replacements:**

| Instead of... | Use... |
|--------------|--------|
| `Command::new("ls")` | `std::fs::read_dir()` |
| `Command::new("cat")` | `std::fs::read_to_string()` |
| `Command::new("grep")` | String `.contains()`, `.lines().filter()`, regex |
| `Command::new("find")` | `walkdir` crate or recursive `std::fs` |
| `Command::new("curl")` | Mocked HTTP client trait |
| `Command::new("echo")` | Direct string operations |
| `Command::new("mkdir -p")` | `std::fs::create_dir_all()` |
| `Command::new("rm -rf")` | `std::fs::remove_dir_all()` |
| `Command::new("cp")` | `std::fs::copy()` |
| `Command::new("ralph")` | `run_ralph_cli()` helper function |
| `assert_cmd::Command` | Forbidden - use `run_ralph_cli()` for CLI tests |

**Migration Examples:**

```rust
// Before (forbidden):
use std::process::Command;
let output = Command::new("git")
    .args(["commit", "-m", "test"])
    .current_dir(dir)
    .output()?;
assert!(output.status.success());

// After (allowed - git2 library):
use git2::{Repository, Signature};
let repo = Repository::open(dir)?;
let sig = Signature::now("Test User", "test@example.com")?;
let tree_id = repo.index()?.write_tree()?;
let tree = repo.find_tree(tree_id)?;
let parent = repo.head()?.peel_to_commit()?;
repo.commit(Some("HEAD"), &sig, &sig, "test", &tree, &[&parent])?;

// After (allowed - MockGit for integration tests):
use ralph_workflow::git_helpers::{GitOps, MockGit};
let mock = MockGit::new();
let result = GitOps::commit(&mock, "test", None, None)?;
assert!(matches!(result, CommitResult::Success(_)));
```

**FINAL EMPHASIS ON PROCESS SPAWNING:**

⚠️ **THERE ARE NO EXCEPTIONS TO THIS RULE** ⚠️

- You CANNOT spawn `ls` to list files - use `std::fs::read_dir()`
- You CANNOT spawn `cat` to read files - use `std::fs::read_to_string()`
- You CANNOT spawn `ralph` to test the CLI - use `run_ralph_cli()`
- You CANNOT spawn `curl` for HTTP - use mocked HTTP clients
- You CANNOT spawn `git` for version control - use git2 library
- You CANNOT spawn ANYTHING, EVER, FOR ANY REASON

**If you think you need to spawn a process, you are wrong. There is always a better way.**

**This is not a guideline. This is not a preference. This is an ABSOLUTE REQUIREMENT.**

**Why this matters:**
- Tests must be deterministic and reproducible
- Tests must not incur costs (API calls cost money)
- Tests must not depend on network availability
- Tests must not pollute the developer's environment
- Tests must run in CI without special credentials

### Rule 2: No Test-Only Flags in Production Code

**FORBIDDEN** patterns in production code:

```rust
// ❌ FORBIDDEN: Test-only conditional branches
if cfg!(test) {
    // test behavior
} else {
    // real behavior
}

// ❌ FORBIDDEN: Test mode flags
fn process_data(data: &str, test_mode: bool) {
    if test_mode {
        // skip external calls
    }
}

// ❌ FORBIDDEN: Environment-based test detection
if std::env::var("RUNNING_TESTS").is_ok() {
    return mock_response();
}

// ❌ FORBIDDEN: Feature flags solely for testing
#[cfg(feature = "testing")]
fn do_something() { /* test version */ }

#[cfg(not(feature = "testing"))]
fn do_something() { /* real version */ }
```

**Why this is forbidden:**
- Production code paths must be testable as-is
- Test-only branches add untested code paths to production
- These patterns hide design problems (poor dependency injection)
- They make it unclear what code actually runs in production

### Rule 3: Use Dependency Injection for Testability

If code needs external dependencies, **refactor to accept them as parameters**:

```rust
// ❌ BAD: Hardcoded dependency
fn fetch_ai_response(prompt: &str) -> Result<String> {
    let client = AnthropicClient::new();  // Hardcoded!
    client.complete(prompt)
}

// ✅ GOOD: Dependency injection via trait
trait AiClient {
    fn complete(&self, prompt: &str) -> Result<String>;
}

fn fetch_ai_response(client: &dyn AiClient, prompt: &str) -> Result<String> {
    client.complete(prompt)
}

// In tests: use MockAiClient
// In production: use RealAnthropicClient
```

```rust
// ❌ BAD: Direct filesystem access
fn save_results(data: &str) -> Result<()> {
    std::fs::write("/var/data/results.txt", data)  // Hardcoded path!
}

// ✅ GOOD: Path injection
fn save_results(path: &Path, data: &str) -> Result<()> {
    std::fs::write(path, data)
}

// In tests: use TempDir path
// In production: use configured path
```

```rust
// ❌ BAD: Direct println
fn report_status(status: &str) {
    println!("Status: {}", status);  // Untestable!
}

// ✅ GOOD: Writer injection
fn report_status(writer: &mut dyn Write, status: &str) -> std::io::Result<()> {
    writeln!(writer, "Status: {}", status)
}

// In tests: use Vec<u8> or TestPrinter
// In production: use stdout()
```

### Rule 4: Allowed Testing Infrastructure

The following patterns ARE allowed because they exist solely in test code:

```rust
// ✅ OK: Test-only helper structs (in test modules only)
#[cfg(test)]
mod tests {
    struct MockClient { /* ... */ }
}

// ✅ OK: Test utilities exposed via feature flag (for integration test crate)
#[cfg(feature = "test-utils")]
pub mod test_utils {
    pub struct TestPrinter { /* ... */ }
}

// ✅ OK: Trait implementations only used in tests
#[cfg(test)]
impl AiClient for MockClient { /* ... */ }
```

The distinction is:
- **Forbidden**: Test conditionals that change production code behavior
- **Allowed**: Test infrastructure that only exists in test builds

### Decision Tree: Making Code Testable

```
Need to test code that uses external dependency?
    │
    ├─► Is the dependency behind a trait/interface?
    │       NO → Refactor to introduce trait/interface
    │       YES ↓
    │
    ├─► Can the dependency be injected?
    │       NO → Refactor to accept dependency as parameter
    │       YES ↓
    │
    └─► Create mock implementation of trait for tests
            │
            ├─► Mock goes in test code only (not production)
            └─► Real implementation used in production
```

### Enforcement

These rules are enforced by:

1. **Code review** - Reviewers must reject test-only flags in production code
2. **CI checks** - Automated scripts check for forbidden patterns
3. **Test isolation** - Tests that make real external calls will fail in CI

#### Automated Test Flag Checker

A compliance checker script validates that production code does not contain forbidden test flags:

```bash
# DO NOT MODIFY THIS SCRIPT. If it fails, FIX THE PRODUCTION CODE, not the script.
./tests/integration_tests/no_test_flags_check.sh
```

The checker validates:
- No `cfg!(test)` runtime detection in production code
- No `#[cfg(not(test))]` conditional compilation (creates untested code paths)
- No `#[cfg(feature = "testing")]` dual implementations
- No test mode boolean parameters (`test_mode`, `is_test`, `is_testing`, etc.)
- No skip/bypass boolean parameters (`skip_validation`, `skip_auth`, `skip_verify`, etc.)
- No mock/fake/stub boolean parameters (`mock_mode`, `use_mock`, `fake_mode`, etc.)
- No test-related environment variables (`RUNNING_TEST`, `IS_TEST`, `TEST_MODE`, etc.)
- No skip/bypass environment variables (`SKIP_AUTH`, `SKIP_VALIDATION`, etc.)
- No mock environment variables (`MOCK_*`, `FAKE_*`, `STUB_*`, etc.)
- No disable security environment variables (`DISABLE_AUTH`, `DISABLE_SSL`, etc.)

**AI AGENTS:** This script is protected. DO NOT modify it to bypass checks. If the script
fails, the production code has a design problem that must be fixed with proper dependency
injection, not by changing the enforcement script.

**If you find existing code that violates these rules**, fix it as part of your change
or file an issue to track the technical debt.

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
│   │   └── mod.rs           # run_ralph_cli() - calls app::run() directly
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

#### Pattern 2: CLI Testing with run_ralph_cli()

Used when testing the CLI as a black box without spawning processes:

```rust
use std::fs;
use tempfile::TempDir;

use crate::common::{mock_executor_with_success, run_ralph_cli};
use crate::test_timeout::with_default_timeout;

#[test]
fn test_cli_behavior() {
    with_default_timeout(|| {
        // 1. Set up isolated environment
        let dir = TempDir::new().unwrap();

        // 2. Create any required fixtures
        fs::write(dir.path().join("input.txt"), "test content").unwrap();

        // 3. Set test environment to avoid interactive prompts
        std::env::set_var("RALPH_INTERACTIVE", "0");

        // 4. Run CLI directly via app::run() (no process spawning)
        let executor = mock_executor_with_success();
        let result = run_ralph_cli(&["--some-flag", "input.txt"], executor);

        // 5. Assert on OBSERVABLE BEHAVIOR (exit code, file side effects)
        assert!(result.is_ok(), "CLI should succeed");
        assert!(dir.path().join("output.txt").exists(), "Should create output file");
    });
}
```

**Key points:**
- Tests the CLI as users would experience it (black-box)
- Uses `TempDir` for filesystem isolation
- Calls `app::run()` directly without spawning subprocesses (process spawning is FORBIDDEN)
- Environment variables control configuration (no internal mocking)
- Asserts on Result return type and file side effects

---

## Timeout Enforcement

### Rule: All Tests MUST Have Timeouts

**ALL integration tests MUST be wrapped with `with_default_timeout()` (5 seconds) to prevent indefinite test hangs.**

```rust
use crate::test_timeout::with_default_timeout;

#[test]
fn test_example_behavior() {
    with_default_timeout(|| {
        // test code here
    });
}
```

**Why this is required:**

- Integration tests may involve external I/O (filesystem, subprocess execution)
- Tests that hang block the entire test suite and CI/CD pipelines
- A 5-second timeout prevents indefinite waits while allowing reasonable test execution time
- Without timeouts, a single hung test can waste hours of CI resources

**Enforcement:**

- All existing tests use `with_default_timeout()` wrapper
- New tests without the timeout wrapper will be flagged in code review
- The `test_timeout` module provides the timeout implementation

### Automated Enforcement

A compliance checker script validates that all tests use timeout wrappers:

```bash
./tests/integration_tests/compliance_check.sh
```

The checker validates:
- All `#[test]` functions are wrapped with `with_default_timeout()`
- Timeout wrapper is the first statement in the test body
- No test code executes before timeout protection

CI runs this check automatically to prevent non-compliant tests from being merged.

**See also:** `tests/integration_tests/test_timeout.rs` for timeout implementation details.

---

## Writing New Integration Tests

### Checklist

Before writing a new integration test, verify:

- [ ] **Testing behavior**: Does this test verify observable behavior, not implementation?
- [ ] **Black-box**: Could this test pass with a completely different internal implementation?
- [ ] **Mocking boundaries**: Am I only mocking external dependencies (filesystem, network)?
- [ ] **No internal knowledge**: Does the test avoid importing internal/private modules?
- [ ] **NO PROCESS SPAWNING**: Does this test avoid ALL use of Command::new(), assert_cmd, etc?
- [ ] **Deterministic**: Will this test produce the same result every time?
- [ ] **Isolated**: Does this test clean up after itself and not affect other tests?
- [ ] **Timeout protection**: Is the test wrapped with `with_default_timeout()`?

### Anti-Patterns to Avoid

| Anti-Pattern | Why It's Wrong | Fix |
|--------------|----------------|-----|
| Spawning ANY external process | Non-deterministic, slow, breaks isolation | Use Rust libraries or mocks |
| Using Command::new() for ANYTHING | Process spawning is FORBIDDEN | Use std::fs, git2, HTTP mocks, etc. |
| Using assert_cmd for ralph testing | Process spawning is FORBIDDEN | Use run_ralph_cli() helper |
| Mocking internal functions | Tests implementation, not behavior | Refactor code or use integration boundary |
| Asserting on log messages | Logs are not part of the behavior contract | Assert on outputs/side effects instead |
| Testing private functions | Private = implementation detail | Test through public API |
| Brittle string matching | Ties test to exact formatting | Use semantic assertions |
| Shared mutable state | Tests affect each other | Use `TempDir`, reset state |

### Example: Adding a New Deduplication Test

```rust
use crate::test_timeout::with_default_timeout;

/// Test that [SPECIFIC SCENARIO] produces [EXPECTED BEHAVIOR].
///
/// This verifies that when [CONDITION], the system [OBSERVABLE OUTCOME].
#[test]
fn test_specific_scenario_expected_behavior() {
    with_default_timeout(|| {
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
    });
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
3. **NO PROCESS SPAWNING EVER** - Not ls, not cat, not curl, not ralph, NOTHING
4. **Failing test = behavior mismatch** - Fix implementation, not tests
5. **Use real code paths** - TestPrinter replaces I/O, not logic
6. **All tests must have timeouts** - Wrap with `with_default_timeout()`
