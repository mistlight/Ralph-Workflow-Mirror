//! Integration tests for error event processing through the reducer.
//!
//! These tests verify that error events returned through `Err()` from effect handlers
//! are properly extracted and processed through the reducer, allowing the reducer
//! to decide recovery strategy based on the specific error type.
//!
//! ## Module Summary
//!
//! Tests the error handling architecture where effect handlers return typed error events
//! that are processed through the reducer, not bypassed via `Err()`. Verifies downcasting
//! roundtrip and reducer state transitions for different error types.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

use crate::common::with_locked_prompt_permissions;
use crate::test_timeout::with_default_timeout;

#[test]
fn test_error_events_processed_through_reducer() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 1);

        // Error events returned through Err() should be processed through the reducer.
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: state.phase,
            error: ErrorEvent::ReviewInputsNotMaterialized { pass: 1 },
        });
        let new_state = reduce(state, error_event);

        // Invariant violations must route through AwaitingDevFix so the pipeline never exits
        // early and always emits a completion marker.
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
    });
}

#[test]
fn test_error_event_downcast_roundtrip() {
    with_default_timeout(|| {
        let error = ErrorEvent::ReviewInputsNotMaterialized { pass: 1 };
        let anyhow_err: anyhow::Error = error.into();

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
        use ralph_workflow::reducer::determine_next_effect;
        use ralph_workflow::reducer::effect::Effect;
        use ralph_workflow::reducer::event::AwaitingDevFixEvent;

        let state = with_locked_prompt_permissions(PipelineState::initial(1, 1));
        assert_eq!(state.phase, PipelinePhase::Planning);

        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: state.phase,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });

        let new_state = reduce(state, error_event);

        // Should transition to AwaitingDevFix phase (not directly to Interrupted)
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Planning));

        // Orchestration should determine TriggerDevFixFlow effect
        let effect = determine_next_effect(&new_state);
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "Expected TriggerDevFixFlow effect, got {effect:?}"
        );

        // After dev-fix flow, state should transition to Interrupted
        let after_trigger_state = reduce(
            new_state,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixTriggered {
                failed_phase: PipelinePhase::Planning,
                failed_role: AgentRole::Developer,
            }),
        );
        assert_eq!(after_trigger_state.phase, PipelinePhase::AwaitingDevFix);

        let after_fix_state = reduce(
            after_trigger_state,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: false,
                summary: None,
            }),
        );

        let final_state = reduce(
            after_fix_state,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
                is_failure: true,
            }),
        );

        // CompletionMarkerEmitted transitions to Interrupted
        assert_eq!(final_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            final_state.previous_phase,
            Some(PipelinePhase::AwaitingDevFix)
        );
    });
}

#[test]
fn test_continuation_not_supported_errors_route_to_awaiting_dev_fix() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 1);

        let errors = vec![
            ErrorEvent::PlanningContinuationNotSupported,
            ErrorEvent::ReviewContinuationNotSupported,
            ErrorEvent::FixContinuationNotSupported,
            ErrorEvent::CommitContinuationNotSupported,
        ];

        for error in errors {
            let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
                phase: state.phase,
                error,
            });
            let new_state = reduce(state.clone(), error_event);

            // Invariant violations must route through AwaitingDevFix so the pipeline never exits
            // early and always emits a completion marker.
            assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        }
    });
}
