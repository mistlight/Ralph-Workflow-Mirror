//! Integration tests for commit message parsing

// This module exists only to satisfy Cargo's requirement for a library target
// All actual tests are in the subdirectories

// pub mod parsing_test_corpus; // Disabled due to missing test_support module

// Include deduplication integration tests
#[path = "deduplication_integration_tests.rs"]
pub mod deduplication_integration_tests;
