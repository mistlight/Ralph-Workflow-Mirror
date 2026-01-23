//! Unit tests for state reduction.
//!
//! Tests are organized by phase to keep files manageable and enable parallel test execution.
//! Each module tests a specific aspect of the reducer's event handling.

// Re-export common types for test modules
pub use crate::reducer::event::{PipelineEvent, PipelinePhase};
pub use crate::reducer::state::{CommitState, PipelineState, RebaseState};
pub use crate::reducer::state_reduction::reduce;

// Test modules organized by phase
mod agent_chain;
mod commit_phase;
mod development_phase;
mod phase_transitions;
mod pipeline_lifecycle;
mod planning_phase;
mod rebase;
mod review_phase;

// ============================================================================
// Shared Test Helpers
// ============================================================================

/// Creates a default test state with 5 iterations and 2 reviewer passes.
pub fn create_test_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

/// Creates a test state in a specific phase.
pub fn create_state_in_phase(phase: PipelinePhase) -> PipelineState {
    PipelineState {
        phase,
        ..create_test_state()
    }
}
