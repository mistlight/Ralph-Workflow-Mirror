//! System tests for fault-tolerant rebase operations.
//!
//! These tests verify that the rebase system handles all documented
//! Git rebase failure modes and can recover from interruptions.
//!
//! # System Test Guidelines
//!
//! These tests are in `system_tests` (not `integration_tests`) because they
//! require **real git operations** that cannot be mocked:
//! - Real git repository initialization via `git2`
//! - Real file system operations for conflict simulation
//! - Real rebase/merge operations to test recovery
//!
//! See **[SYSTEM_TESTS.md](../SYSTEM_TESTS.md)** for guidelines.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (git state, commit history, working directory)
//! - Uses `TempDir` for filesystem isolation (allowed in system tests)
//! - Tests are deterministic and black-box (test rebase as a user would experience it)

pub mod ai_resolution_tests;
pub mod category1_failure_modes;
pub mod category2_failure_modes;
pub mod category3_failure_modes;
pub mod category4_recovery_tests;
pub mod category5_unknown_failures;
pub mod edge_cases;
pub mod state_machine;
