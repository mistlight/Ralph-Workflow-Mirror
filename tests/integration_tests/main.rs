//! Integration tests for ralph-workflow
//!
//! This is the main entry point for all integration tests.
//! Each module is declared here as a submodule.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All integration tests MUST follow the style guide defined in
//! **[INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Before writing, modifying, or debugging any integration test, you MUST read
//! that document. It defines non-negotiable rules for:
//!
//! - **Behavior-based testing:** Test observable behavior, not implementation
//! - **Mocking strategy:** Mock only at architectural boundaries (filesystem, network)
//! - **When to update tests:** Only update when expected behavior changes
//! - **Forbidden patterns:** No `cfg!(test)` branches in production code
//!
//! Key patterns used in these tests:
//! - **Parser tests:** Use `TestPrinter` from `ralph_workflow::json_parser::printer`
//! - **File operations:** Use `tempfile::TempDir` for isolation
//! - **CLI tests:** Use `assert_cmd::Command` for black-box testing
//!
//! See individual test modules for examples of proper integration test structure.

mod agent_spawn_errors;
mod cli;
mod codex_parser_tests;
mod commit;
mod common;
mod deduplication;
mod development_xml_validation;
mod fix_xml_validation;
mod gemini_parser_tests;
mod git;
mod logger;
mod opencode_parser_tests;
mod rebase;
mod reducer_fault_tolerance;
mod reducer_rebase_state_machine;
mod reducer_resume_tests;
mod reducer_state_machine;
mod review_xml_validation;
mod review_xsd_retry_session;
mod test_timeout;
mod test_traits;
mod workflows;
