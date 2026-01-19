//! Integration tests for fault-tolerant rebase operations.
//!
//! These tests verify that the rebase system handles all documented
//! Git rebase failure modes and can recover from interruptions.

pub mod ai_resolution_tests;
pub mod category1_failure_modes;
pub mod category2_failure_modes;
pub mod category3_failure_modes;
pub mod category4_recovery_tests;
pub mod edge_cases;
pub mod state_machine;
