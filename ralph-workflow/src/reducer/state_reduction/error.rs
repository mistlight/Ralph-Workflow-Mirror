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
///   The state remains unchanged and the event loop will terminate on Err.
///
/// - **Missing inputs errors**: These indicate effect sequencing bugs where a handler
///   was called without required preconditions being met. The state remains unchanged
///   and the event loop will terminate on Err.
pub(super) fn reduce_error(state: &PipelineState, error: &ErrorEvent) -> PipelineState {
    match error {
        // Continuation not supported errors are invariant violations
        ErrorEvent::PlanningContinuationNotSupported
        | ErrorEvent::ReviewContinuationNotSupported
        | ErrorEvent::FixContinuationNotSupported
        | ErrorEvent::CommitContinuationNotSupported => {
            // These should never happen - continuation mode should not be passed to these phases
            // State remains unchanged - event loop will terminate on Err
            state.clone()
        }

        // Missing inputs are handler bugs - these should be caught by effect sequencing
        ErrorEvent::ReviewInputsNotMaterialized { .. } | ErrorEvent::FixPromptMissing => {
            // These indicate effect sequencing bugs
            // State remains unchanged - event loop will terminate on Err
            state.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::state::ContinuationState;

    #[test]
    fn test_reduce_continuation_not_supported_errors_no_state_change() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![
            ErrorEvent::PlanningContinuationNotSupported,
            ErrorEvent::ReviewContinuationNotSupported,
            ErrorEvent::FixContinuationNotSupported,
            ErrorEvent::CommitContinuationNotSupported,
        ];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            // State should not change - event loop will terminate on Err
            assert_eq!(new_state.phase, state.phase);
        }
    }

    #[test]
    fn test_reduce_missing_inputs_errors_no_state_change() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![
            ErrorEvent::ReviewInputsNotMaterialized { pass: 1 },
            ErrorEvent::FixPromptMissing,
        ];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            // State should not change - event loop will terminate on Err
            assert_eq!(new_state.phase, state.phase);
        }
    }
}
