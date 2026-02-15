//! Tests for reducer state transitions during failure handling.
//!
//! Verifies that the reducer correctly transitions through phases when
//! handling AgentChainExhausted errors:
//! 1. Planning/Development → AwaitingDevFix (on error)
//! 2. AwaitingDevFix → Interrupted (after marker emission)
//! 3. Interrupted → Complete (after checkpoint saved)
//!
//! These tests focus on the pure reducer logic and orchestration,
//! without testing full event loop execution.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

#[test]
fn test_failure_status_triggers_awaiting_dev_fix_not_immediate_exit() {
    with_default_timeout(|| {
        // Given: Pipeline in Development phase
        let state = PipelineState::initial(2, 1);

        // When: AgentChainExhausted occurs during Development
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 5,
            },
        });

        let new_state = reduce(state, error_event);

        // Then: Should transition to AwaitingDevFix, NOT Interrupted
        assert_eq!(
            new_state.phase,
            PipelinePhase::AwaitingDevFix,
            "Should enter AwaitingDevFix phase for remediation attempt"
        );

        // And: Should NOT be complete yet (needs to process dev-fix flow)
        assert!(
            !new_state.is_complete(),
            "Should not be complete in AwaitingDevFix phase"
        );

        // When: TriggerDevFixFlow effect is processed (simulated)
        let after_trigger_state = reduce(
            new_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixTriggered {
                    failed_phase: PipelinePhase::Development,
                    failed_role: AgentRole::Developer,
                },
            ),
        );

        let after_fix_state = reduce(
            after_trigger_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixCompleted {
                    success: false,
                    summary: None,
                },
            ),
        );

        // When: CompletionMarkerEmitted event is processed
        let interrupted_state = reduce(
            after_fix_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure: true,
                },
            ),
        );

        // Then: Should be in Interrupted phase
        assert_eq!(interrupted_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            interrupted_state.previous_phase,
            Some(PipelinePhase::AwaitingDevFix)
        );

        // And: Next effect should be the pre-termination safety check.
        // Completion marker emission is not an exception (only Ctrl+C is).
        let next_effect = determine_next_effect(&interrupted_state);
        assert!(
            matches!(
                next_effect,
                Effect::CheckUncommittedChangesBeforeTermination
            ),
            "Expected CheckUncommittedChangesBeforeTermination for Interrupted phase, got {:?}",
            next_effect
        );
    });
}

#[test]
fn test_interrupted_from_dev_fix_is_complete_before_checkpoint() {
    with_default_timeout(|| {
        // This test validates the fix for the "Pipeline exited without completion marker" bug.
        // It verifies that when transitioning from AwaitingDevFix to Interrupted,
        // is_complete() returns true even before SaveCheckpoint executes.

        let mut state = PipelineState::initial(1, 1);

        // Simulate the transition path: Planning → AwaitingDevFix → Interrupted
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        // After TriggerDevFixFlow completes and CompletionMarkerEmitted event is processed
        let after_marker_state = reduce(
            state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure: true,
                },
            ),
        );

        // Verify state transitioned to Interrupted
        assert_eq!(after_marker_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            after_marker_state.previous_phase,
            Some(PipelinePhase::AwaitingDevFix)
        );
        assert_eq!(after_marker_state.checkpoint_saved_count, 0);

        // CRITICAL: is_complete() should return true even without checkpoint
        // because we came from AwaitingDevFix (completion marker already written)
        assert!(
            after_marker_state.is_complete(),
            "BUG: is_complete() should return true for Interrupted phase from AwaitingDevFix, \
             even without checkpoint, because completion marker was already written. \
             This is the fix for 'Pipeline exited without completion marker'."
        );

        // Verify next effect is the pre-termination safety check.
        // Completion marker emission is not an exception (only Ctrl+C is).
        let next_effect = determine_next_effect(&after_marker_state);
        assert!(
            matches!(
                next_effect,
                Effect::CheckUncommittedChangesBeforeTermination
            ),
            "Next effect should be CheckUncommittedChangesBeforeTermination, got {:?}",
            next_effect
        );
    });
}
