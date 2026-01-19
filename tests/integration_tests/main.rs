//! Integration tests for ralph-workflow
//!
//! This is the main entry point for all integration tests.
//! Each module is declared here as a submodule.

mod cli;
mod codex_parser_tests;
mod commit;
mod common;
mod deduplication;
mod gemini_parser_tests;
mod git;
mod logger;
mod opencode_parser_tests;
mod test_timeout;
mod test_traits;
mod workflows;
