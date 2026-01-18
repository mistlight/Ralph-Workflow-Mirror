//! Integration tests for Logger functionality.
//!
//! These tests verify that Logger properly formats and writes result events,
//! that Logger flushes correctly after writing, and that written files can be
//! parsed by `extract_result_from_file`.

mod json_event_extraction;
