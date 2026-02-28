//! Comprehensive orchestration tests for pipeline phase transitions.
//!
//! This module contains systematic tests for ALL phase transitions and state management
//! in the reducer-based pipeline architecture. These tests verify that:
//! - Each phase correctly determines the next effect based on state
//! - State transitions happen correctly when events are applied
//! - Iteration/pass counts are respected (no off-by-one errors)
//! - Phase transitions occur at the right time
//! - The complete pipeline flows from Planning → Development → Review → Commit → Complete

use super::orchestration::determine_next_effect;
use super::state_reduction::reduce;
use crate::agents::AgentRole;
use crate::reducer::effect::Effect;
use crate::reducer::event::{DevelopmentEvent, PipelineEvent, PipelinePhase};
use crate::reducer::state::{PipelineState, PromptMode};

#[must_use]
fn create_test_state() -> PipelineState {
    let mut state = PipelineState::initial(5, 2);
    // Most tests in this module assume permissions were locked at startup
    state.prompt_permissions.locked = true;
    state.prompt_permissions.restore_needed = true;
    state
}

/// Helper to create initial state with locked permissions (for mid-pipeline test scenarios)
#[must_use]
fn initial_with_locked_permissions(dev_iters: u32, review_passes: u32) -> PipelineState {
    let mut state = PipelineState::initial(dev_iters, review_passes);
    state.prompt_permissions.locked = true;
    state.prompt_permissions.restore_needed = true;
    state
}

// Review phase single-task effect chain tests
#[path = "orchestration_tests/review_phase_effects.rs"]
mod review_phase_effects;

// Fix chain single-task effect tests
#[path = "orchestration_tests/fix_chain_effects.rs"]
mod fix_chain_effects;

// Planning phase orchestration tests
#[path = "orchestration_tests/planning_phase.rs"]
mod planning_phase;

// Development phase orchestration tests
#[path = "orchestration_tests/development_phase.rs"]
mod development_phase;

// Review phase orchestration tests
#[path = "orchestration_tests/review_phase.rs"]
mod review_phase;

// Commit phase orchestration tests
#[path = "orchestration_tests/commit_phase.rs"]
mod commit_phase;

// Complete pipeline flow tests
#[path = "orchestration_tests/pipeline_flow.rs"]
mod pipeline_flow;
