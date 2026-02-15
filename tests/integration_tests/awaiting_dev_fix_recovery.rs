//! Integration test for AwaitingDevFix error handling.
//!
//! Verifies that AwaitingDevFix phase handles dev-fix agent unavailability gracefully
//! without masking the original failure.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{AwaitingDevFixEvent, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

use crate::common::with_locked_prompt_permissions;
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
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
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

/// Test that recovery Level 1 (retry same operation) works correctly.
#[test]
fn test_recovery_level_1_retry_same_operation() {
    with_default_timeout(|| {
        // Given: Pipeline in AwaitingDevFix after first failure
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.dev_fix_attempt_count = 0;

        // When: Dev-fix completes (first attempt)
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed".to_string()),
        });
        let state = reduce(state, event);

        // Then: Should be at level 1, still in AwaitingDevFix
        assert_eq!(state.recovery_escalation_level, 1);
        assert_eq!(state.dev_fix_attempt_count, 1);
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);

        // When: Recovery attempted
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 1,
            attempt_count: 1,
        });
        let state = reduce(state, event);

        // Then: Should transition back to failed phase (Development)
        assert_eq!(state.phase, PipelinePhase::Development);

        // When: Next effect is determined
        let effect = determine_next_effect(&state);

        // Then: Should derive normal development effect (retry same operation)
        assert!(
            matches!(effect, Effect::PrepareDevelopmentContext { .. }),
            "Level 1 recovery should retry same operation, got {:?}",
            effect
        );
    });
}

/// Test that recovery escalates to Level 2 after 3 failed Level 1 attempts.
#[test]
fn test_recovery_escalation_to_level_2() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        // Simulate 3 failed Level 1 attempts
        for i in 1..=3 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: false,
                summary: None,
            });
            state = reduce(state, event);
            assert_eq!(state.recovery_escalation_level, 1);
            assert_eq!(state.dev_fix_attempt_count, i);
        }

        // Fourth attempt should escalate to Level 2
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: false,
            summary: None,
        });
        state = reduce(state, event);

        assert_eq!(state.recovery_escalation_level, 2);
        assert_eq!(state.dev_fix_attempt_count, 4);
    });
}

/// Test that recovery eventually terminates after exhausting all levels.
#[test]
fn test_recovery_terminates_after_max_attempts() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        // Simulate 12 failed attempts
        for _i in 1..=12 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: false,
                summary: None,
            });
            state = reduce(state, event);
        }

        // 13th attempt should terminate
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: false,
            summary: None,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Interrupted);
        assert_eq!(state.dev_fix_attempt_count, 13);
    });
}

/// Test that successful recovery clears recovery state.
#[test]
fn test_successful_recovery_clears_state() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::Development;
        state.dev_fix_attempt_count = 2;
        state.recovery_escalation_level = 1;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        // Recovery succeeds
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
