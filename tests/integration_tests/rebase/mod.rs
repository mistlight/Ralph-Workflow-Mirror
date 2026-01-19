//! Integration tests for fault-tolerant rebase operations.
//!
//! These tests verify that the rebase system handles all documented
//! Git rebase failure modes and can recover from interruptions.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (git state, commit history, working directory)
//! - Uses `TempDir` for filesystem isolation
//! - Tests are deterministic and black-box (test rebase as a user would experience it)

pub mod ai_resolution_tests;
pub mod category1_failure_modes;
pub mod category2_failure_modes;
pub mod category3_failure_modes;
pub mod category4_recovery_tests;
pub mod category5_unknown_failures;
pub mod edge_cases;
pub mod state_machine;
