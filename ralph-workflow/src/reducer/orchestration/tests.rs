// Orchestration tests for pipeline phase transitions.
//
// Tests for effect determination and phase transitions across all pipeline phases.

use super::*;
use crate::reducer::state::AgentChainState;
use crate::reducer::{reduce, PipelineEvent};

fn create_test_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

// Interrupted phase checkpoint behavior tests
#[path = "tests/interrupted_phase.rs"]
mod interrupted_phase;

// Planning phase effect determination tests
#[path = "tests/planning_phase.rs"]
mod planning_phase;

// Development iteration tests
#[path = "tests/development_phase.rs"]
mod development_phase;

// Review pass and fix tests
#[path = "tests/review_phase.rs"]
mod review_phase;

// Commit message generation tests
#[path = "tests/commit_phase.rs"]
mod commit_phase;

// Complete pipeline flow tests
#[path = "tests/pipeline_flow.rs"]
mod pipeline_flow;

// Retry safety tests (stale XML cleanup on agent failures)
#[path = "tests/retry_cleans_xml.rs"]
mod retry_cleans_xml;
