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
use ralph_workflow::reducer::effect::{Effect, RecoveryResetType};
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

/// Test that DevFixCompleted does not emit CompletionMarkerEmitted.
///
/// This is a regression test for the bug where TriggerDevFixFlow unconditionally
/// emitted CompletionMarkerEmitted, causing immediate termination instead of recovery.
#[test]
fn test_dev_fix_completion_does_not_terminate() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.dev_fix_attempt_count = 0;

        // Simulate dev-fix completion (first attempt)
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed the issue".to_string()),
        });
        let state = reduce(state, event);

        // Should NOT transition to Interrupted
        assert_eq!(
            state.phase,
            PipelinePhase::AwaitingDevFix,
            "DevFixCompleted should not terminate, should stay in AwaitingDevFix"
        );

        // Should set recovery level
        assert_eq!(state.recovery_escalation_level, 1);
        assert_eq!(state.dev_fix_attempt_count, 1);
    });
}

/// Test that only after 12+ attempts does the pipeline terminate.
#[test]
fn test_termination_only_after_max_attempts() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        // Attempts 1-12 should not terminate
        for i in 1..=12 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: false,
                summary: None,
            });
            state = reduce(state, event);

            assert_eq!(
                state.phase,
                PipelinePhase::AwaitingDevFix,
                "Attempt {} should not terminate",
                i
            );
        }

        // 13th attempt should terminate
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: false,
            summary: None,
        });
        state = reduce(state, event);

        assert_eq!(
            state.phase,
            PipelinePhase::Interrupted,
            "After 13 attempts should terminate"
        );
    });
}

/// End-to-end test: failure → dev-fix → retry succeeds → pipeline continues.
#[test]
fn test_end_to_end_recovery_success() {
    use ralph_workflow::reducer::event::{ErrorEvent, PromptInputEvent};

    with_default_timeout(|| {
        // Start in Development phase
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Development;

        // Simulate AgentChainExhausted error
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });
        let mut state = reduce(state, error_event);

        // Should transition to AwaitingDevFix
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);

        // Lock prompt permissions for orchestration
        state = with_locked_prompt_permissions(state);

        // Orchestration should derive TriggerDevFixFlow
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "AwaitingDevFix should determine TriggerDevFixFlow effect, got {:?}",
            effect
        );

        // Simulate dev-fix completion (success)
        let dev_fix_event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed the issue".to_string()),
        });
        state = reduce(state, dev_fix_event);
        state.dev_fix_triggered = true; // Handler would set this

        // Should stay in AwaitingDevFix with recovery level 1
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(state.recovery_escalation_level, 1);

        // Orchestration should derive AttemptRecovery
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::AttemptRecovery { level: 1, .. }),
            "Should derive AttemptRecovery effect, got {:?}",
            effect
        );

        // Simulate RecoveryAttempted event
        let recovery_event =
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
                level: 1,
                attempt_count: 1,
            });
        state = reduce(state, recovery_event);

        // Should transition back to Development
        assert_eq!(state.phase, PipelinePhase::Development);

        // Simulate successful retry (recovery succeeds)
        let success_event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoverySucceeded {
            level: 1,
            total_attempts: 1,
        });
        state = reduce(state, success_event);

        // Recovery state should be cleared
        assert_eq!(state.dev_fix_attempt_count, 0);
        assert_eq!(state.recovery_escalation_level, 0);
        assert_eq!(state.failed_phase_for_recovery, None);

        // Pipeline should continue normally in Development
        assert_eq!(state.phase, PipelinePhase::Development);
    });
}

/// End-to-end test: multiple failures → escalate through all levels → terminate.
#[test]
fn test_end_to_end_escalation_and_termination() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.dev_fix_triggered = false;

        // Simulate 12 failed recovery attempts
        for attempt in 1..=12 {
            // Dev-fix runs
            state.dev_fix_triggered = false;
            let effect = determine_next_effect(&state);
            assert!(
                matches!(effect, Effect::TriggerDevFixFlow { .. })
                    || matches!(effect, Effect::AttemptRecovery { .. })
                    || matches!(effect, Effect::EmitRecoveryReset { .. }),
                "Attempt {}: unexpected effect {:?}",
                attempt,
                effect
            );

            // Dev-fix completes (failure)
            state.dev_fix_triggered = true;
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: false,
                summary: None,
            });
            state = reduce(state, event);

            // Should escalate properly
            let expected_level = match attempt {
                1..=3 => 1,
                4..=6 => 2,
                7..=9 => 3,
                _ => 4,
            };
            assert_eq!(
                state.recovery_escalation_level, expected_level,
                "Attempt {} should be at level {}",
                attempt, expected_level
            );
        }

        // After 12 attempts, should still be at level 4 attempting recovery
        // The 13th DevFixCompleted will cause reducer to transition to Interrupted
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(state.dev_fix_attempt_count, 12);
        assert_eq!(state.recovery_escalation_level, 4);

        // Orchestration should still derive recovery effect (level 4 = CompleteReset)
        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                Effect::EmitRecoveryReset {
                    reset_type: RecoveryResetType::CompleteReset,
                    ..
                }
            ),
            "After 12 attempts should still attempt level 4 recovery, got {:?}",
            effect
        );
    });
}

/// Test complete recovery flow: failure → dev-fix → retry succeeds → recovery cleared
#[test]
fn test_complete_recovery_flow_with_success() {
    with_default_timeout(|| {
        // 1. Start with a state that has failed and entered AwaitingDevFix
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);

        // 2. Dev-fix completes (sets attempt_count=1, level=1)
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed planning prompt".to_string()),
        });
        let state = reduce(state, event);
        assert_eq!(state.dev_fix_attempt_count, 1);
        assert_eq!(state.recovery_escalation_level, 1);

        // 3. RecoveryAttempted event transitions back to Planning
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 1,
            attempt_count: 1,
        });
        let state = reduce(state, event);
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix));

        // 4. Simulate Planning completing successfully
        // (In real flow, orchestration would detect this and emit RecoverySucceeded)
        let mut state = with_locked_prompt_permissions(state);
        state.planning_xml_archived_iteration = Some(0);
        state.planning_validated_outcome =
            Some(ralph_workflow::reducer::state::PlanningValidatedOutcome {
                iteration: 0,
                valid: true,
                markdown: None,
            });

        // 5. RecoverySucceeded clears recovery state
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoverySucceeded {
            level: 1,
            total_attempts: 1,
        });
        let state = reduce(state, event);
        assert_eq!(state.dev_fix_attempt_count, 0);
        assert_eq!(state.recovery_escalation_level, 0);
        assert_eq!(state.failed_phase_for_recovery, None);
    });
}

/// Test recovery escalation: Level 1 fails → dev-fix → Level 2 succeeds
#[test]
fn test_recovery_escalation_from_level_1_to_level_2() {
    with_default_timeout(|| {
        // Start in AwaitingDevFix after first recovery attempt failed
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);
        state.dev_fix_attempt_count = 3; // Level 1 failed 3 times
        state.recovery_escalation_level = 1;

        // Dev-fix runs again (attempt 4 → level 2)
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Reset phase state".to_string()),
        });
        let state = reduce(state, event);
        assert_eq!(state.dev_fix_attempt_count, 4);
        assert_eq!(state.recovery_escalation_level, 2); // Escalated to level 2

        // Orchestration should derive EmitRecoveryReset for level 2 (PhaseStart)
        let mut state = with_locked_prompt_permissions(state);
        state.dev_fix_triggered = true;
        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                Effect::EmitRecoveryReset {
                    reset_type: RecoveryResetType::PhaseStart,
                    ..
                }
            ),
            "Should emit recovery reset for level 2, got {:?}",
            effect
        );
    });
}

/// Test full recovery loop with multiple failures and eventual success.
///
/// This test verifies the COMPLETE recovery flow:
/// 1. Initial failure → AwaitingDevFix
/// 2. TriggerDevFixFlow → DevFixCompleted
/// 3. RecoveryAttempted → transition back to failed phase
/// 4. Work fails again (simulated)
/// 5. Back to step 1, escalate recovery level
/// 6. Repeat until eventual success
///
/// This is the definitive test that proves recovery LOOPS rather than terminates.
#[test]
fn test_full_recovery_loop_with_escalation_and_success() {
    use ralph_workflow::reducer::event::{ErrorEvent, PromptInputEvent};

    with_default_timeout(|| {
        // Start in Development phase
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Development;

        // Simulate agent chain exhausted (failure 1)
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });
        state = reduce(state, error_event);

        // Should transition to AwaitingDevFix
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(
            state.failed_phase_for_recovery,
            Some(PipelinePhase::Development)
        );

        // Lock prompt permissions for orchestration
        state = with_locked_prompt_permissions(state);

        // Orchestration should derive TriggerDevFixFlow
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::TriggerDevFixFlow { .. }));

        // Simulate dev-fix completion (attempt 1)
        let dev_fix_event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed issue".to_string()),
        });
        state = reduce(state, dev_fix_event);

        // Should set recovery level 1, stay in AwaitingDevFix
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(state.recovery_escalation_level, 1);
        assert_eq!(state.dev_fix_attempt_count, 1);

        // Mark dev-fix as triggered for orchestration
        state.dev_fix_triggered = true;

        // Orchestration should now derive AttemptRecovery
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::AttemptRecovery { .. }));

        // Simulate RecoveryAttempted (transitions back to Development)
        let recovery_event =
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
                level: 1,
                attempt_count: 1,
            });
        state = reduce(state, recovery_event);

        // Should be back in Development phase
        assert_eq!(state.phase, PipelinePhase::Development);
        assert_eq!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix));

        // *** THIS IS THE CRITICAL PART: Verify we can fail AGAIN and loop back ***

        // Simulate another failure (simulating work didn't succeed)
        let error_event2 = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });
        state = reduce(state, error_event2);

        // Should transition BACK to AwaitingDevFix (recovery attempt 2)
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);

        // Reset dev_fix_triggered for second attempt
        state.dev_fix_triggered = false;

        // Orchestration should derive TriggerDevFixFlow again
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "Should trigger dev-fix again on second failure, got {:?}",
            effect
        );

        // Dev-fix again (attempt 2)
        let dev_fix_event2 = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed issue again".to_string()),
        });
        state = reduce(state, dev_fix_event2);

        // Should increment to attempt 2
        assert_eq!(state.dev_fix_attempt_count, 2);
        assert_eq!(state.recovery_escalation_level, 1); // Still level 1 (attempts 1-3)

        // Mark dev-fix as triggered
        state.dev_fix_triggered = true;

        // Recovery attempt 2
        let recovery_event2 =
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
                level: 1,
                attempt_count: 2,
            });
        state = reduce(state, recovery_event2);

        // Should be back in Development phase again
        assert_eq!(state.phase, PipelinePhase::Development);

        // Simulate final success after recovery
        let success_event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoverySucceeded {
            level: 1,
            total_attempts: 2,
        });
        state = reduce(state, success_event);

        // Recovery state should be cleared
        assert_eq!(state.dev_fix_attempt_count, 0);
        assert_eq!(state.recovery_escalation_level, 0);
        assert_eq!(state.failed_phase_for_recovery, None);

        // Should still be in Development (recovery succeeded)
        assert_eq!(state.phase, PipelinePhase::Development);
    });
}

/// Test that orchestration emits completion marker after exhausting all recovery attempts.
///
/// This test verifies the catastrophic failure path: after 12+ failed recovery attempts,
/// the orchestration should derive EmitCompletionMarkerAndTerminate effect.
#[test]
fn test_orchestration_emits_completion_after_exhaustion() {
    with_default_timeout(|| {
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.dev_fix_triggered = true;
        state.dev_fix_attempt_count = 13; // Exhausted all 12 attempts
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        // Orchestration should derive EmitCompletionMarkerAndTerminate
        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                Effect::EmitCompletionMarkerAndTerminate {
                    is_failure: true,
                    ..
                }
            ),
            "After 13 attempts, should emit completion marker, got {:?}",
            effect
        );
    });
}

/// Test that recovery loop preserves attempt count when failing again after recovery.
///
/// This is a regression test for the bug where error reducer reset recovery counters
/// on every failure, breaking the recovery loop escalation.
#[test]
fn test_recovery_preserves_attempt_count_on_repeated_failure() {
    use ralph_workflow::reducer::event::{ErrorEvent, PromptInputEvent};

    with_default_timeout(|| {
        // Start with state already in recovery (attempt 1, level 1)
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Development;
        state.previous_phase = Some(PipelinePhase::AwaitingDevFix);
        state.dev_fix_attempt_count = 1;
        state.recovery_escalation_level = 1;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        // Simulate failure AGAIN after recovery attempt
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });
        state = reduce(state, error_event);

        // Should transition to AwaitingDevFix
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);

        // CRITICAL: Should preserve attempt count and level (not reset to 0)
        assert_eq!(
            state.dev_fix_attempt_count, 1,
            "Attempt count should be preserved when failing again in recovery loop"
        );
        assert_eq!(
            state.recovery_escalation_level, 1,
            "Escalation level should be preserved when failing again in recovery loop"
        );
    });
}

/// Test that recovery state reset at each escalation level works correctly.
///
/// This test verifies the core bug fix: RecoveryAttempted reducer actually performs
/// state resets based on escalation level instead of just transitioning phase.
#[test]
fn test_recovery_state_reset_at_each_level() {
    with_default_timeout(|| {
        // Start with a state that has progressed through Planning
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.iteration = 1;
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);
        state.dev_fix_attempt_count = 0;
        state.recovery_escalation_level = 0;

        // Set some Planning phase flags to verify they get cleared
        state.planning_prompt_prepared_iteration = Some(1);
        state.planning_agent_invoked_iteration = Some(1);
        state.planning_xml_extracted_iteration = Some(1);

        // Level 1: Simple retry - should NOT clear flags
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Attempt 1".to_string()),
        });
        state = reduce(state, event);
        assert_eq!(state.recovery_escalation_level, 1);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 1,
            attempt_count: 1,
        });
        state = reduce(state, event);

        // Level 1 should preserve flags
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.planning_prompt_prepared_iteration, Some(1));
        assert_eq!(state.planning_agent_invoked_iteration, Some(1));
        assert_eq!(state.iteration, 1);

        // Simulate failure again, escalate to level 2
        state.phase = PipelinePhase::AwaitingDevFix;
        state.planning_prompt_prepared_iteration = Some(1);
        state.planning_agent_invoked_iteration = Some(1);

        for i in 2..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        // Level 2: Phase reset - should clear phase flags
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
        });
        state = reduce(state, event);

        // Level 2 should clear phase flags but preserve iteration
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.planning_prompt_prepared_iteration, None);
        assert_eq!(state.planning_agent_invoked_iteration, None);
        assert_eq!(state.planning_xml_extracted_iteration, None);
        assert_eq!(state.iteration, 1); // Iteration preserved

        // Simulate failure again, escalate to level 3
        state.phase = PipelinePhase::AwaitingDevFix;
        state.iteration = 2; // Advance iteration

        for i in 5..=7 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 3);

        // Level 3: Iteration reset - should decrement iteration
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 3,
            attempt_count: 7,
        });
        state = reduce(state, event);

        // Level 3 should reset iteration
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 1); // Decremented from 2 to 1
        assert_eq!(state.planning_prompt_prepared_iteration, None);

        // Simulate failure again, escalate to level 4
        state.phase = PipelinePhase::AwaitingDevFix;
        state.iteration = 3; // Advance iteration

        for i in 8..=10 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 4);

        // Level 4: Complete reset - should reset to iteration 0
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 4,
            attempt_count: 10,
        });
        state = reduce(state, event);

        // Level 4 should reset to iteration 0
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0); // Reset to 0
        assert_eq!(state.planning_prompt_prepared_iteration, None);
    });
}

/// Test that Level 2 recovery correctly clears Development phase flags.
#[test]
fn test_recovery_clears_development_flags() {
    with_default_timeout(|| {
        // Start in Development phase with progress flags set
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.iteration = 2;

        // Set Development phase flags
        state.development_context_prepared_iteration = Some(2);
        state.development_prompt_prepared_iteration = Some(2);
        state.development_agent_invoked_iteration = Some(2);
        state.analysis_agent_invoked_iteration = Some(2);
        state.development_xml_extracted_iteration = Some(2);

        // Escalate to level 2 (phase reset)
        for i in 1..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        // Level 2: Phase reset should clear Development flags
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
        });
        state = reduce(state, event);

        // Verify Development flags are cleared
        assert_eq!(state.phase, PipelinePhase::Development);
        assert_eq!(state.development_context_prepared_iteration, None);
        assert_eq!(state.development_prompt_prepared_iteration, None);
        assert_eq!(state.development_agent_invoked_iteration, None);
        assert_eq!(state.analysis_agent_invoked_iteration, None);
        assert_eq!(state.development_xml_extracted_iteration, None);
        assert_eq!(state.iteration, 2); // Iteration preserved at level 2
    });
}

/// Test that Level 2 recovery correctly clears Review phase flags.
#[test]
fn test_recovery_clears_review_flags() {
    with_default_timeout(|| {
        // Start in Review phase with progress flags set
        let mut state = PipelineState::initial(1, 3);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Review);
        state.reviewer_pass = 1;

        // Set Review phase flags
        state.review_context_prepared_pass = Some(1);
        state.review_prompt_prepared_pass = Some(1);
        state.review_agent_invoked_pass = Some(1);
        state.review_issues_xml_extracted_pass = Some(1);
        state.review_issues_found = true;
        state.fix_prompt_prepared_pass = Some(1);
        state.fix_agent_invoked_pass = Some(1);

        // Escalate to level 2 (phase reset)
        for i in 1..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        // Level 2: Phase reset should clear Review flags
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
        });
        state = reduce(state, event);

        // Verify Review flags are cleared
        assert_eq!(state.phase, PipelinePhase::Review);
        assert_eq!(state.review_context_prepared_pass, None);
        assert_eq!(state.review_prompt_prepared_pass, None);
        assert_eq!(state.review_agent_invoked_pass, None);
        assert_eq!(state.review_issues_xml_extracted_pass, None);
        assert!(!state.review_issues_found);
        assert_eq!(state.fix_prompt_prepared_pass, None);
        assert_eq!(state.fix_agent_invoked_pass, None);
        assert_eq!(state.reviewer_pass, 1); // Pass preserved at level 2
    });
}

/// Test that Level 2 recovery correctly clears CommitMessage phase flags.
#[test]
fn test_recovery_clears_commit_flags() {
    with_default_timeout(|| {
        // Start in CommitMessage phase with progress flags set
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::CommitMessage);
        state.iteration = 3;

        // Set CommitMessage phase flags
        state.commit_prompt_prepared = true;
        state.commit_diff_prepared = true;
        state.commit_diff_empty = false;
        state.commit_diff_content_id_sha256 = Some("abc123".to_string());
        state.commit_agent_invoked = true;
        state.commit_xml_cleaned = true;

        // Escalate to level 2 (phase reset)
        for i in 1..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        // Level 2: Phase reset should clear CommitMessage flags
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
        });
        state = reduce(state, event);

        // Verify CommitMessage flags are cleared
        assert_eq!(state.phase, PipelinePhase::CommitMessage);
        assert!(!state.commit_prompt_prepared);
        assert!(!state.commit_diff_prepared);
        assert!(!state.commit_diff_empty);
        assert_eq!(state.commit_diff_content_id_sha256, None);
        assert!(!state.commit_agent_invoked);
        assert!(!state.commit_xml_cleaned);
        assert_eq!(state.iteration, 3); // Iteration preserved at level 2
    });
}

/// Test that Level 3 recovery correctly handles iteration counter floor at zero.
#[test]
fn test_recovery_iteration_reset_floor_at_zero() {
    with_default_timeout(|| {
        // Start at iteration 0
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);
        state.iteration = 0;

        // Escalate to level 3 (iteration reset)
        for i in 1..=7 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 3);

        // Level 3: Iteration reset should stay at 0 (floor)
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 3,
            attempt_count: 7,
        });
        state = reduce(state, event);

        // Verify iteration stays at 0 (saturating_sub prevents underflow)
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0);
    });
}

/// Test end-to-end recovery loop with multiple attempts at same level.
///
/// This test verifies the complete recovery loop from failure to dev-fix to recovery
/// attempt, simulating the actual pipeline flow with multiple failed recovery attempts
/// at the same escalation level.
#[test]
fn test_end_to_end_recovery_loop_with_multiple_attempts() {
    with_default_timeout(|| {
        // Simulate a pipeline that fails in Development phase
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.development_agent_invoked_iteration = Some(1);
        state.development_xml_extracted_iteration = Some(1);

        // Simulate failure transitioning to AwaitingDevFix
        // (This would normally come from error reducer, we simulate the transition)
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.dev_fix_attempt_count = 0;
        state.recovery_escalation_level = 0;

        // --- Recovery Attempt 1 (Level 1) ---
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed import path".to_string()),
        });
        state = reduce(state, event);
        assert_eq!(state.dev_fix_attempt_count, 1);
        assert_eq!(state.recovery_escalation_level, 1);
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 1,
            attempt_count: 1,
        });
        state = reduce(state, event);
        assert_eq!(state.phase, PipelinePhase::Development);
        assert_eq!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix));
        // Level 1 should preserve flags
        assert_eq!(state.development_agent_invoked_iteration, Some(1));

        // Simulate work executing and failing again
        state.phase = PipelinePhase::AwaitingDevFix;

        // --- Recovery Attempt 2 (Level 1) ---
        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Fixed permission issue".to_string()),
        });
        state = reduce(state, event);
        assert_eq!(state.dev_fix_attempt_count, 2);
        assert_eq!(state.recovery_escalation_level, 1);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 1,
            attempt_count: 2,
        });
        state = reduce(state, event);
        assert_eq!(state.phase, PipelinePhase::Development);

        // Simulate work executing and failing again
        state.phase = PipelinePhase::AwaitingDevFix;

        // --- Skip to Recovery Attempt 4 (Level 2 - Phase Reset) ---
        for i in 3..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Fix attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.dev_fix_attempt_count, 4);
        assert_eq!(state.recovery_escalation_level, 2);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
        });
        state = reduce(state, event);
        assert_eq!(state.phase, PipelinePhase::Development);
        // Level 2 should clear Development flags
        assert_eq!(state.development_agent_invoked_iteration, None);
        assert_eq!(state.development_xml_extracted_iteration, None);
        assert_eq!(state.iteration, 1); // Iteration preserved

        // Verify recovery state is still active (not cleared)
        assert_eq!(state.dev_fix_attempt_count, 4);
        assert_eq!(state.recovery_escalation_level, 2);
        assert_eq!(
            state.failed_phase_for_recovery,
            Some(PipelinePhase::Development)
        );
    });
}
