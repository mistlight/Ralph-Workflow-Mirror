//! Integration tests for error event processing through the reducer.
//!
//! These tests verify that error events returned through Err() from effect handlers
//! are properly extracted and processed through the reducer, allowing the reducer
//! to decide recovery strategy based on the specific error type.
//!
//! ## Module Summary
//!
//! Tests the error handling architecture where effect handlers return typed error events
//! that are processed through the reducer, not bypassed via Err(). Verifies downcasting
//! roundtrip and reducer state transitions for different error types.

use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

#[test]
fn test_error_events_processed_through_reducer() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 1);

        // Error events should reduce to unchanged state (invariant violations)
        let error_event = PipelineEvent::Error(ErrorEvent::ReviewInputsNotMaterialized { pass: 1 });
        let new_state = reduce(state.clone(), error_event);

        // State unchanged - these errors are invariant violations
        assert_eq!(new_state.phase, state.phase);
    });
}

#[test]
fn test_error_event_downcast_roundtrip() {
    with_default_timeout(|| {
        let error = ErrorEvent::ReviewInputsNotMaterialized { pass: 1 };
        let anyhow_err: anyhow::Error = error.clone().into();

        // Should be able to downcast back to ErrorEvent
        let extracted = anyhow_err.downcast_ref::<ErrorEvent>();
        assert!(extracted.is_some());
        assert!(matches!(
            extracted.unwrap(),
            ErrorEvent::ReviewInputsNotMaterialized { pass: 1 }
        ));
    });
}

#[test]
fn test_agent_chain_exhausted_transitions_to_interrupted() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRole;

        let state = PipelineState::initial(1, 1);
        assert_eq!(state.phase, PipelinePhase::Planning);

        let error_event = PipelineEvent::Error(ErrorEvent::AgentChainExhausted {
            role: AgentRole::Developer,
            phase: PipelinePhase::Development,
            cycle: 3,
        });

        let new_state = reduce(state, error_event);

        // Should transition to Interrupted phase
        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
    });
}

#[test]
fn test_continuation_not_supported_errors_leave_state_unchanged() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 1);

        let errors = vec![
            ErrorEvent::PlanningContinuationNotSupported,
            ErrorEvent::ReviewContinuationNotSupported,
            ErrorEvent::FixContinuationNotSupported,
            ErrorEvent::CommitContinuationNotSupported,
        ];

        for error in errors {
            let error_event = PipelineEvent::Error(error);
            let new_state = reduce(state.clone(), error_event);

            // State should not change - these are invariant violations
            assert_eq!(new_state.phase, state.phase);
        }
    });
}
