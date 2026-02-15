//! Basic reducer/orchestration expectations for AwaitingDevFix.

use crate::common::with_locked_prompt_permissions;
use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{AwaitingDevFixEvent, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

/// Test that pipeline transitions to AwaitingDevFix on failure.
#[test]
fn transitions_to_awaiting_dev_fix_on_failure() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::event::{ErrorEvent, PromptInputEvent};

        let state = PipelineState::initial(1, 0);

        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Planning,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Planning,
                cycle: 3,
            },
        });

        let new_state = reduce(state, error_event);

        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Planning));
    });
}

/// Test that TriggerDevFixFlow effect is determined for AwaitingDevFix phase.
#[test]
fn awaiting_dev_fix_derives_trigger_dev_fix_flow() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "AwaitingDevFix should determine TriggerDevFixFlow effect, got {:?}",
            effect
        );
    });
}

/// CompletionMarkerEmitted transitions AwaitingDevFix -> Interrupted.
#[test]
fn completion_marker_emitted_transitions_to_interrupted() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
            is_failure: true,
        });
        let new_state = reduce(state, event);

        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
    });
}

/// Test that DevFixAgentUnavailable does not terminate immediately.
#[test]
fn dev_fix_agent_unavailable_does_not_interrupt() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixAgentUnavailable {
            failed_phase: PipelinePhase::Planning,
            reason: "usage limit exceeded".to_string(),
        });
        let new_state = reduce(state, event);

        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
    });
}

/// Test that successful recovery clears recovery state.
#[test]
fn recovery_succeeded_clears_recovery_tracking() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::Development;
        state.dev_fix_attempt_count = 2;
        state.recovery_escalation_level = 1;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoverySucceeded {
            level: 1,
            total_attempts: 2,
        });
        state = reduce(state, event);

        assert_eq!(state.dev_fix_attempt_count, 0);
        assert_eq!(state.recovery_escalation_level, 0);
        assert_eq!(state.failed_phase_for_recovery, None);
    });
}
