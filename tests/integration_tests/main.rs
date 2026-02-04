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
//! - **No process spawning:** Tests must NOT spawn external processes
//!
//! Key patterns used in these tests:
//! - **Parser tests:** Use `TestPrinter` from `ralph_workflow::json_parser::printer`
//! - **File operations:** Use `MemoryWorkspace` for isolation (NOT `TempDir`)
//! - **CLI tests:** Use `run_ralph_cli_injected()` which calls `app::run_with_config()` directly
//! - **Process execution:** Use `MockProcessExecutor` (never spawn real processes in tests)
//!
//! Tests requiring real git/filesystem operations are in `tests/system_tests/`.
//! See `tests/system_tests/SYSTEM_TESTS.md` for those guidelines.

mod agent_spawn_errors;
mod cli;
mod commit;
mod common;
mod deduplication;
mod development_xml_validation;
mod event_loop_trace_dump;
mod fix_xml_validation;
mod gemini_parser_tests;
mod git;
mod logger;
mod opencode_parser_tests;
mod reducer_agent_fallback;
mod reducer_effect_invariants;
mod reducer_error_handling;
mod reducer_fault_tolerance;
mod reducer_hidden_behavior;
mod reducer_legacy_rejection;
mod reducer_rebase_state_machine;
mod reducer_resume_tests;
mod reducer_state_machine;
mod review_output_validation;
mod review_xml_validation;
mod test_count_guard;
mod test_timeout;
mod test_traits;
mod ui_events;
mod workflows;
