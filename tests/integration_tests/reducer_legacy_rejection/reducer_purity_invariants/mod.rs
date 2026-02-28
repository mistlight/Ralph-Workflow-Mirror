//! Integration tests for reducer purity invariants.
//!
//! Verifies that the reducer follows pure functional principles:
//! - State transitions are purely driven by events through the reducer
//! - Effect determination is based solely on reducer state
//! - No direct state mutation outside the reducer
//! - All control flow happens via events, not side effects
//!
//! # Integration Test Compliance
//!
//! These tests follow [../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md):
//! - Test observable behavior: phase transitions, effect emission
//! - Pure reducer tests require no mocks
//! - Verify deterministic state transitions

mod control_flow;
mod effects_and_phases;
