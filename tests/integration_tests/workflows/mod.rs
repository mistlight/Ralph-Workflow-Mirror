//! Workflow integration tests
//!
//! This module contains tests for the complete workflow including:
//! - Prompt backup and restore (backup.rs)
//! - Cleanup and error recovery (cleanup.rs)
//! - Commit behavior tests (commit_tests.rs)
//! - Config and initialization (config.rs)
//! - PLAN workflow tests (plan.rs)
//! - Review workflow tests (review.rs)
//! - Full workflow requirements (requirements.rs)

pub mod backup;
pub mod cleanup;
pub mod commit_tests;
pub mod config;
pub mod plan;
pub mod requirements;
pub mod review;
