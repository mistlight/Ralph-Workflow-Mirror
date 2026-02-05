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

fn create_test_state() -> PipelineState {
    PipelineState::initial(5, 2)
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
