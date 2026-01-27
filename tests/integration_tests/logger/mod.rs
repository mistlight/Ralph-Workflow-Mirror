//! Integration tests for Logger functionality.
//!
//! These tests verify that Logger properly formats and writes result events,
//! that Logger flushes correctly after writing, and that written files can be
//! parsed by `extract_result_from_file`.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (file content, parsed output)
//! - Uses `TestLogger` mock at architectural boundary (logging I/O)
//! - Tests are deterministic and black-box (test logger behavior as a user would experience it)

mod json_event_extraction;
mod test_logger_tests;
