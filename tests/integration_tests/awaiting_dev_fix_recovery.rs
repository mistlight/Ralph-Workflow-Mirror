//! Integration test for AwaitingDevFix error handling.
//!
//! Verifies that AwaitingDevFix phase handles dev-fix agent unavailability gracefully
//! without masking the original failure.

use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{AwaitingDevFixEvent, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

/// Test that pipeline transitions to AwaitingDevFix on failure.
#[test]
fn test_transitions_to_awaiting_dev_fix_on_failure() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::event::{ErrorEvent, PromptInputEvent};

        let state = PipelineState::initial(1, 0);

        // Simulate failure that should trigger AwaitingDevFix
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Planning,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Planning,
                cycle: 3,
            },
        });

        let new_state = reduce(state, error_event);

        // Should transition to AwaitingDevFix
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Planning));
    });
}

/// Test that TriggerDevFixFlow effect is determined for AwaitingDevFix phase.
#[test]
fn test_dev_fix_flow_effect_determined() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        // Should determine TriggerDevFixFlow effect
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "AwaitingDevFix should determine TriggerDevFixFlow effect, got {:?}",
            effect
        );
    });
}

/// Test that dev-fix flow completes and writes completion marker.
#[test]
fn test_dev_fix_completion_marker_emitted() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;

        // Simulate completion marker emission
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
            is_failure: true,
        });
        let new_state = reduce(state, event);

        // Should transition to Interrupted after completion marker
        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
    });
}

/// Test that DevFixAgentUnavailable event is handled gracefully.
///
/// This verifies that when dev-fix agent cannot run (quota/usage limit),
/// the pipeline doesn't hard-fail but rather logs and continues to completion.
#[test]
fn test_dev_fix_agent_unavailable_handled_gracefully() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        // Simulate dev-fix agent unavailable
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixAgentUnavailable {
            failed_phase: PipelinePhase::Planning,
            reason: "usage limit exceeded".to_string(),
        });
        let new_state = reduce(state, event);

        // Should remain in AwaitingDevFix (waiting for completion marker)
        // NOT transition to Interrupted immediately (that happens after completion marker)
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
    });
}

/// Test that dev-fix success path works correctly.
#[test]
fn test_dev_fix_success_path() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        // Dev-fix triggered
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixTriggered {
            failed_phase: PipelinePhase::Planning,
            failed_role: AgentRole::Developer,
        });
        let state = reduce(state, event);

        // Dev-fix completed successfully
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed the issue".to_string()),
        });
        let state = reduce(state, event);

        // Completion marker emitted
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
            is_failure: false,
        });
        let final_state = reduce(state, event);

        // Should transition to Interrupted
        assert_eq!(final_state.phase, PipelinePhase::Interrupted);
    });
}

/// Test that dev-fix failure path writes failure completion marker.
#[test]
fn test_dev_fix_failure_path() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        // Dev-fix completed unsuccessfully
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: false,
            summary: None,
        });
        let state = reduce(state, event);

        // Failure completion marker emitted
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
            is_failure: true,
        });
        let final_state = reduce(state, event);

        // Should transition to Interrupted
        assert_eq!(final_state.phase, PipelinePhase::Interrupted);
    });
}
