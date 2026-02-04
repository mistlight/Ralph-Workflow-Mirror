//! Reducer function for state transitions.
//!
//! Implements pure state reduction - no side effects, exhaustive pattern matching.
//!
//! # Architecture
//!
//! The main `reduce` function routes events to category-specific handlers based
//! on event type, providing type-safe dispatch:
//!
//! | Category     | Handler                    | Responsibility                    |
//! |--------------|----------------------------|-----------------------------------|
//! | Lifecycle    | reduce_lifecycle_event     | Pipeline start/resume/complete    |
//! | Planning     | reduce_planning_event      | Plan generation                   |
//! | Development  | reduce_development_event   | Dev iterations, continuation      |
//! | Review       | reduce_review_event        | Review passes, fix attempts       |
//! | Agent        | reduce_agent_event         | Agent chain, fallback, retries    |
//! | Rebase       | reduce_rebase_event        | Rebase state machine              |
//! | Commit       | reduce_commit_event        | Commit message generation         |
//!
//! Each handler is a pure function that takes state and its specific event type,
//! enabling compile-time verification of exhaustive matching within each category.

use super::event::PipelineEvent;
use super::state::PipelineState;

/// Pure reducer - no side effects, exhaustive match.
///
/// Computes new state by applying an event to current state.
/// This function has zero side effects - all state mutations are explicit.
///
/// # Event Routing
///
/// Events are routed to category-specific reducers based on their type:
///
/// | Category     | Handler                    | Responsibility                    |
/// |--------------|----------------------------|-----------------------------------|
/// | Lifecycle    | reduce_lifecycle_event     | Pipeline start/resume/complete    |
/// | Planning     | reduce_planning_event      | Plan generation                   |
/// | Development  | reduce_development_event   | Dev iterations, continuation      |
/// | Review       | reduce_review_event        | Review passes, fix attempts       |
/// | Agent        | reduce_agent_event         | Agent chain, fallback, retries    |
/// | Rebase       | reduce_rebase_event        | Rebase state machine              |
/// | Commit       | reduce_commit_event        | Commit message generation         |
///
/// Miscellaneous events are handled directly in this function.
pub fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        // Route to category-specific reducers
        PipelineEvent::Lifecycle(e) => lifecycle::reduce_lifecycle_event(state, e),
        PipelineEvent::Planning(e) => planning::reduce_planning_event(state, e),
        PipelineEvent::Development(e) => development::reduce_development_event(state, e),
        PipelineEvent::Review(e) => review::reduce_review_event(state, e),
        PipelineEvent::PromptInput(e) => prompt_input::reduce_prompt_input_event(state, e),
        PipelineEvent::Agent(e) => agent::reduce_agent_event(state, e),
        PipelineEvent::Rebase(e) => rebase::reduce_rebase_event(state, e),
        PipelineEvent::Commit(e) => commit::reduce_commit_event(state, e),
        PipelineEvent::Error(e) => error::reduce_error(&state, &e),

        // Handle miscellaneous events directly
        PipelineEvent::ContextCleaned => PipelineState {
            context_cleaned: true,
            ..state
        },
        PipelineEvent::CheckpointSaved { .. } => {
            let checkpoint_saved_count = state.checkpoint_saved_count.saturating_add(1);
            PipelineState {
                checkpoint_saved_count,
                ..state
            }
        }
        PipelineEvent::FinalizingStarted => PipelineState {
            phase: super::event::PipelinePhase::Finalizing,
            ..state
        },
        PipelineEvent::PromptPermissionsRestored => PipelineState {
            phase: super::event::PipelinePhase::Complete,
            ..state
        },
    }
}

// ============================================================================
// Category-specific reducers (split into modules)
// ============================================================================

#[path = "state_reduction/agent.rs"]
mod agent;
#[path = "state_reduction/commit.rs"]
mod commit;
#[path = "state_reduction/development.rs"]
mod development;
#[path = "state_reduction/error.rs"]
mod error;
#[path = "state_reduction/lifecycle.rs"]
mod lifecycle;
#[path = "state_reduction/planning.rs"]
mod planning;
#[path = "state_reduction/prompt_input.rs"]
mod prompt_input;
#[path = "state_reduction/rebase.rs"]
mod rebase;
#[path = "state_reduction/review.rs"]
mod review;

#[cfg(test)]
#[path = "state_reduction/tests.rs"]
mod tests;
