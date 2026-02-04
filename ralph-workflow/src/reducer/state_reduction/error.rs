//! Error event reduction.
//!
//! Handles error events returned through Err() from effect handlers.
//! Each error type has a specific recovery strategy decided by the reducer.

use crate::reducer::event::ErrorEvent;
use crate::reducer::state::PipelineState;

/// Reduce error events.
///
/// Error events are processed through the reducer identically to success events.
/// The reducer decides the recovery strategy based on the error type.
///
/// # Recovery Strategies
///
/// - **Continuation not supported errors**: These are invariant violations indicating
///   that continuation mode was incorrectly passed to a phase that doesn't support it.
///   The reducer transitions to `PipelinePhase::Interrupted`, and the event loop will
///   observe a terminal state and stop.
///
/// - **Missing inputs errors**: These indicate effect sequencing bugs where a handler
///   was called without required preconditions being met. The reducer transitions to
///   `PipelinePhase::Interrupted`, and the event loop will observe a terminal state
///   and stop.
pub(super) fn reduce_error(state: &PipelineState, error: &ErrorEvent) -> PipelineState {
    match error {
        // Continuation not supported errors are invariant violations
        ErrorEvent::PlanningContinuationNotSupported
        | ErrorEvent::ReviewContinuationNotSupported
        | ErrorEvent::FixContinuationNotSupported
        | ErrorEvent::CommitContinuationNotSupported => {
            // Invariant violations: terminate cleanly by transitioning to Interrupted.
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::Interrupted;
            new_state
        }

        // Missing inputs are handler bugs - these should be caught by effect sequencing
        ErrorEvent::ReviewInputsNotMaterialized { .. }
        | ErrorEvent::PlanningInputsNotMaterialized { .. }
        | ErrorEvent::DevelopmentInputsNotMaterialized { .. }
        | ErrorEvent::CommitInputsNotMaterialized { .. }
        | ErrorEvent::ValidatedPlanningMarkdownMissing { .. }
        | ErrorEvent::ValidatedDevelopmentOutcomeMissing { .. }
        | ErrorEvent::ValidatedReviewOutcomeMissing { .. }
        | ErrorEvent::ValidatedFixOutcomeMissing { .. }
        | ErrorEvent::FixPromptMissing
        | ErrorEvent::AgentNotFound { .. } => {
            // Invariant violations: terminate cleanly by transitioning to Interrupted.
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::Interrupted;
            new_state
        }

        // Missing prompt files are recoverable - tmp artifacts can be cleaned between checkpoints.
        // Clear the corresponding "prepared" flag so the event loop will regenerate the prompt.
        ErrorEvent::PlanningPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.planning_prompt_prepared_iteration = None;
            new_state
        }
        ErrorEvent::DevelopmentPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.development_prompt_prepared_iteration = None;
            new_state
        }
        ErrorEvent::ReviewPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.review_prompt_prepared_pass = None;
            new_state
        }
        ErrorEvent::CommitPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.commit_prompt_prepared = false;
            new_state
        }

        // Workspace operation failures are treated as terminal.
        ErrorEvent::WorkspaceReadFailed { .. }
        | ErrorEvent::WorkspaceWriteFailed { .. }
        | ErrorEvent::WorkspaceCreateDirAllFailed { .. }
        | ErrorEvent::WorkspaceRemoveFailed { .. } => {
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::Interrupted;
            new_state
        }

        // Agent chain exhausted - this is a terminal condition
        // The reducer transitions to Interrupted phase to signal pipeline termination
        ErrorEvent::AgentChainExhausted { .. } => {
            // Transition to Interrupted phase
            // This signals the event loop that the pipeline should terminate
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::Interrupted;
            new_state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::state::ContinuationState;

    #[test]
    fn test_reduce_continuation_not_supported_errors_transition_to_interrupted() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![
            ErrorEvent::PlanningContinuationNotSupported,
            ErrorEvent::ReviewContinuationNotSupported,
            ErrorEvent::FixContinuationNotSupported,
            ErrorEvent::CommitContinuationNotSupported,
        ];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            // Terminate cleanly
            assert_eq!(
                new_state.phase,
                crate::reducer::event::PipelinePhase::Interrupted
            );
        }
    }

    #[test]
    fn test_reduce_missing_inputs_errors_transition_to_interrupted() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![
            ErrorEvent::ReviewInputsNotMaterialized { pass: 1 },
            ErrorEvent::FixPromptMissing,
        ];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            // Terminate cleanly
            assert_eq!(
                new_state.phase,
                crate::reducer::event::PipelinePhase::Interrupted
            );
        }
    }

    #[test]
    fn test_reduce_agent_chain_exhausted_transitions_to_interrupted() {
        use crate::agents::AgentRole;
        use crate::reducer::event::PipelinePhase;

        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());
        assert_eq!(state.phase, PipelinePhase::Planning);

        let error = ErrorEvent::AgentChainExhausted {
            role: AgentRole::Developer,
            phase: PipelinePhase::Development,
            cycle: 3,
        };

        let new_state = reduce_error(&state, &error);

        // Should transition to Interrupted phase
        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            new_state.previous_phase,
            Some(state.phase),
            "previous_phase should be recorded for interrupted transitions"
        );
    }

    #[test]
    fn test_reduce_workspace_failures_transition_to_interrupted_and_set_previous_phase() {
        use crate::reducer::event::{PipelinePhase, WorkspaceIoErrorKind};

        let mut state =
            PipelineState::initial_with_continuation(1, 1, ContinuationState::default());
        state.phase = PipelinePhase::Review;

        let error = ErrorEvent::WorkspaceWriteFailed {
            path: ".agent/tmp/out.txt".to_string(),
            kind: WorkspaceIoErrorKind::Other,
        };

        let new_state = reduce_error(&state, &error);
        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            new_state.previous_phase,
            Some(state.phase),
            "previous_phase should be recorded for interrupted transitions"
        );
    }
}
