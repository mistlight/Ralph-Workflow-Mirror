//! Integration test template
//!
//! This file serves as a template for writing new integration tests.
//! Copy this file and rename it to match your test module.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All integration tests MUST follow the style guide defined in
//! **[INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Before writing any integration test, you MUST read that document. It defines
//! non-negotiable rules for behavior-based testing, mocking strategy, and when
//! to update tests.
//!
//! **Timeout Enforcement:** ALL integration tests MUST be wrapped with
//! `with_default_timeout()` to prevent indefinite test hangs. This is enforced
//! by automated compliance checking.
//!
//! # Checklist for New Tests
//!
//! Before writing a new integration test, verify:
//!
//! - [ ] **Testing behavior**: Does this test verify observable behavior, not implementation?
//! - [ ] **Black-box**: Could this test pass with a completely different internal implementation?
//! - [ ] **Mocking boundaries**: Am I only mocking external dependencies (filesystem, network)?
//! - [ ] **No internal knowledge**: Does the test avoid importing internal/private modules?
//! - [ ] **Deterministic**: Will this test produce the same result every time?
//! - [ ] **Isolated**: Does this test clean up after itself and not affect other tests?
//! - [ ] **Timeout protection**: Is the test wrapped with `with_default_timeout()`?
//!
//! # Common Patterns
//!
//! ## Pattern 1: Parser Tests with TestPrinter
//!
//! Used when testing streaming/parsing behavior without actual I/O:
//!
//! ```rust
//! use std::cell::RefCell;
//! use std::io::Cursor;
//! use std::rc::Rc;
//!
//! use ralph_workflow::json_parser::printer::{SharedPrinter, TestPrinter};
//! use ralph_workflow::json_parser::ClaudeParser;
//! use ralph_workflow::cli::args::Verbosity;
//! use ralph_workflow::colors::Colors;
//!
//! use crate::test_timeout::with_default_timeout;
//!
//! /// Test that [SCENARIO] produces [EXPECTED BEHAVIOR].
//! ///
//! /// This verifies that when [CONDITION], the system [OBSERVABLE OUTCOME].
//! #[test]
//! fn test_scenario_produces_expected_behavior() {
//!     with_default_timeout(|| {
//!         // Setup: Create test printer and parser
//!         let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
//!         let printer: SharedPrinter = test_printer.clone();
//!         let parser = ClaudeParser::with_printer(Colors::new(), Verbosity::Normal, printer);
//!
//!         // Input: Construct the scenario
//!         let input = r#"{"type":"stream_event",...}"#;
//!
//!         // Execute: Run through real code path
//!         let cursor = Cursor::new(input);
//!         parser.parse_stream(std::io::BufReader::new(cursor))
//!             .expect("parse_stream should succeed");
//!
//!         // Assert: Verify OBSERVABLE behavior
//!         let printer_ref = test_printer.borrow();
//!         let output = printer_ref.get_output();
//!
//!         // Good: Assert on what the user would see
//!         assert!(output.contains("expected content"), "Should render expected content");
//!
//!         // Good: Assert on behavioral metrics
//!         let metrics = parser.streaming_metrics();
//!         assert_eq!(metrics.some_count, expected, "Should track expected metric");
//!
//!         // Bad: Don't assert on internal state
//!         // assert_eq!(parser.internal_buffer.len(), 5); // WRONG!
//!     });
//! }
//! ```
//!
//! ## Pattern 2: CLI Tests with MemoryWorkspace
//!
//! Used when testing the CLI without real filesystem operations:
//!
//! ```rust
//! use std::path::Path;
//!
//! use ralph_workflow::workspace::MemoryWorkspace;
//! use ralph_workflow::executor::MockProcessExecutor;
//!
//! use crate::common::run_ralph_cli_injected;
//! use crate::test_timeout::with_default_timeout;
//!
//! /// Test that [CLI SCENARIO] produces [EXPECTED BEHAVIOR].
//! ///
//! /// This verifies that when [CONDITION], the CLI [OBSERVABLE OUTCOME].
//! #[test]
//! fn test_cli_scenario_produces_expected_behavior() {
//!     with_default_timeout(|| {
//!         // Setup: Create in-memory workspace with test files
//!         let workspace = MemoryWorkspace::new_test()
//!             .with_file(".agent/config.toml", "[agent]\nname = \"test\"")
//!             .with_file("PROMPT.md", "Test prompt content");
//!
//!         // Setup: Create mock executor for process simulation
//!         let executor = MockProcessExecutor::new()
//!             .with_output("git", "main")
//!             .with_agent_result("claude", Ok(AgentCommandResult::success()));
//!
//!         // Execute: Run CLI with injected dependencies
//!         let result = run_ralph_cli_injected(
//!             &["--some-flag", "value"],
//!             executor,
//!             &workspace,
//!         );
//!
//!         // Assert: Verify OBSERVABLE behavior (return value, file side effects)
//!         assert!(result.is_ok(), "CLI should succeed");
//!         assert!(workspace.was_written(".agent/output.txt"),
//!             "Should create expected output file");
//!
//!         // Assert: Verify file content if needed
//!         let content = workspace.get_file(".agent/output.txt").unwrap();
//!         assert!(content.contains("expected"), "Output should contain expected content");
//!     });
//! }
//! ```
//!
//! ## Pattern 3: File Operation Tests with MemoryWorkspace
//!
//! Used when testing file-based operations without real I/O:
//!
//! ```rust
//! use std::path::Path;
//!
//! use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
//!
//! use crate::test_timeout::with_default_timeout;
//!
//! /// Test that [FILE SCENARIO] produces [EXPECTED BEHAVIOR].
//! ///
//! /// This verifies that when [CONDITION], the file system [OBSERVABLE OUTCOME].
//! #[test]
//! fn test_file_operation_produces_expected_behavior() {
//!     with_default_timeout(|| {
//!         // Setup: Create in-memory workspace with initial files
//!         let workspace = MemoryWorkspace::new_test()
//!             .with_file("input.txt", "initial content");
//!
//!         // Execute: Perform file operations through workspace trait
//!         workspace.write(Path::new("output.txt"), "processed content").unwrap();
//!
//!         // Assert: Verify OBSERVABLE file system state
//!         assert!(workspace.exists(Path::new("output.txt")));
//!         let content = workspace.read(Path::new("output.txt")).unwrap();
//!         assert_eq!(content, "processed content", "Should write file correctly");
//!
//!         // Assert: Verify using test helpers
//!         assert!(workspace.was_written("output.txt"));
//!     });
//! }
//! ```
//!
//! ## Pattern 4: For Real Git/Filesystem Tests
//!
//! If your test REQUIRES real filesystem or git operations (e.g., testing
//! actual git rebase behavior, file permissions, symlinks), it belongs in
//! `tests/system_tests/`. See `tests/system_tests/SYSTEM_TESTS.md`.
//!
//! System tests are NOT part of CI and run separately as sanity checks.
//!
//! # Anti-Patterns to Avoid
//!
//! | Anti-Pattern | Why It's Wrong | Fix |
//! |--------------|----------------|-----|
//! | `TempDir` in integration tests | Real I/O, slow, non-deterministic | Use `MemoryWorkspace::new_test()` |
//! | `std::fs::*` in integration tests | Real I/O, tests affect each other | Use `workspace.read()`/`write()` |
//! | Mocking internal functions | Tests implementation, not behavior | Use integration boundary mocks |
//! | Asserting on log messages | Logs are not part of behavior contract | Assert on outputs/side effects |
//! | Testing private functions | Private = implementation detail | Test through public API |
//! | Brittle string matching | Ties test to exact formatting | Use semantic assertions |
//! | Shared mutable state | Tests affect each other | Use `MemoryWorkspace`, reset state |
//! | `cfg!(test)` in production | Adds untested code paths | Use dependency injection |
//! | Test file >1000 lines | Hard to maintain | Split into focused modules |
//!
//! # When to Update Tests
//!
//! **Valid reasons:**
//! 1. Intentional behavior change (document WHY in commit message)
//! 2. Test was incorrect (fix the test bug)
//! 3. Test was flaky (make it deterministic)
//!
//! **Invalid reasons:**
//! - "The implementation changed" (but behavior didn't)
//! - "The test is failing after my refactor" (refactors shouldn't change behavior)
//!
//! # Module Organization
//!
//! Integration tests are organized by feature/area:
//! - `workflows/`: End-to-end workflow tests (mocked)
//! - `deduplication/`: Parser deduplication tests
//! - `cli/`: CLI argument and output tests (mocked)
//! - `commit/`: Commit message generation tests
//! - `logger/`: Logging and event extraction tests
//!
//! Tests requiring real git/filesystem operations are in `tests/system_tests/`:
//! - `system_tests/rebase/`: Rebase operation tests (real git)
//! - `system_tests/git/`: Git operation tests (real git)
//! - `system_tests/workspace_fs/`: WorkspaceFs tests (real filesystem)
//!
//! Place your test in the appropriate module directory.

// TODO: Remove this comment and replace with your actual test code
// Your test imports go here
// use crate::common::run_ralph_cli;
// use ralph_workflow::...

// Your test functions go here
// #[test]
// fn test_your_feature_here() {
//     // ...
// }
