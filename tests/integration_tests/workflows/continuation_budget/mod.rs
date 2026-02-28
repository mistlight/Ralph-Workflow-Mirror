//! Integration tests for continuation budget enforcement.
//!
//! Verifies that the reducer enforces configured continuation limits
//! for both development and fix phases.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (budget enforcement, state transitions)
//! - Tests are deterministic and isolated
//! - Tests behavior, not implementation details

mod core_enforcement;
mod defaults_and_boundaries;
mod regression_and_guards;
