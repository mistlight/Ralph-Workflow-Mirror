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
//! /// Test that [SCENARIO] produces [EXPECTED BEHAVIOR].
//! ///
//! /// This verifies that when [CONDITION], the system [OBSERVABLE OUTCOME].
//! #[test]
//! fn test_scenario_produces_expected_behavior() {
//!     // Setup: Create test printer and parser
//!     let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
//!     let printer: SharedPrinter = test_printer.clone();
//!     let parser = ClaudeParser::with_printer(Colors::new(), Verbosity::Normal, printer);
//!
//!     // Input: Construct the scenario
//!     let input = r#"{"type":"stream_event",...}"#;
//!
//!     // Execute: Run through real code path
//!     let cursor = Cursor::new(input);
//!     parser.parse_stream(std::io::BufReader::new(cursor))
//!         .expect("parse_stream should succeed");
//!
//!     // Assert: Verify OBSERVABLE behavior
//!     let printer_ref = test_printer.borrow();
//!     let output = printer_ref.get_output();
//!
//!     // Good: Assert on what the user would see
//!     assert!(output.contains("expected content"), "Should render expected content");
//!
//!     // Good: Assert on behavioral metrics
//!     let metrics = parser.streaming_metrics();
//!     assert_eq!(metrics.some_count, expected, "Should track expected metric");
//!
//!     // Bad: Don't assert on internal state
//!     // assert_eq!(parser.internal_buffer.len(), 5); // WRONG!
//! }
//! ```
//!
//! ## Pattern 2: CLI Tests with assert_cmd
//!
//! Used when testing the CLI binary as a black box:
//!
//! ```rust
//! use tempfile::TempDir;
//! use predicates::prelude::*;
//!
//! use crate::common::ralph_cmd;
//!
//! /// Test that [CLI SCENARIO] produces [EXPECTED BEHAVIOR].
//! ///
//! /// This verifies that when [CONDITION], the CLI [OBSERVABLE OUTCOME].
//! #[test]
//! fn test_cli_scenario_produces_expected_behavior() {
//!     // Setup: Create isolated environment
//!     let dir = TempDir::new().unwrap();
//!
//!     // Setup: Create any required fixtures
//!     std::fs::write(dir.path().join("input.txt"), "test content").unwrap();
//!
//!     // Execute: Run the CLI as a subprocess (true black-box test)
//!     let mut cmd = ralph_cmd();
//!     cmd.current_dir(dir.path())
//!         .env("SOME_CONFIG", "value")      // Control environment
//!         .arg("--some-flag")
//!         .arg("input.txt");
//!
//!     // Assert: Verify OBSERVABLE BEHAVIOR
//!     cmd.assert()
//!         .success()                                    // Exit code
//!         .stdout(predicate::str::contains("expected")); // Output
//!
//!     // Assert: Verify SIDE EFFECTS (files created, etc.)
//!     assert!(dir.path().join("output.txt").exists(), "Should create output file");
//! }
//! ```
//!
//! # Anti-Patterns to Avoid
//!
//! | Anti-Pattern | Why It's Wrong | Fix |
//! |--------------|----------------|-----|
//! | Mocking internal functions | Tests implementation, not behavior | Refactor code or use integration boundary |
//! | Asserting on log messages | Logs are not part of the behavior contract | Assert on outputs/side effects instead |
//! | Testing private functions | Private = implementation detail | Test through public API |
//! | Brittle string matching | Ties test to exact formatting | Use semantic assertions |
//! | Shared mutable state | Tests affect each other | Use `TempDir`, reset state |
//! | `cfg!(test)` in production | Adds untested code paths | Use dependency injection |
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
//! - `workflows/`: End-to-end workflow tests
//! - `deduplication/`: Parser deduplication tests
//! - `cli/`: CLI argument and output tests
//! - `commit/`: Commit-related tests
//! - `rebase/`: Rebase operation tests
//! - `git/`: Git operation tests
//!
//! Place your test in the appropriate module directory.

// TODO: Remove this comment and replace with your actual test code
// Your test imports go here
// use crate::common::ralph_cmd;
// use ralph_workflow::...

// Your test functions go here
// #[test]
// fn test_your_feature_here() {
//     // ...
// }
