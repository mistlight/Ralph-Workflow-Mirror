//! Phase-specific effect orchestration.
//!
//! This module contains pure orchestration logic for determining the next effect
//! based on the current pipeline state. All functions are deterministic and perform
//! no I/O operations.
//!
//! # Architecture
//!
//! Each phase module implements a `determine_*_effect()` function that:
//! - Takes `&PipelineState` as input
//! - Returns an `Effect` to execute next
//! - Performs NO I/O or side effects
//! - Is purely deterministic
//!
//! # Priority Order
//!
//! The main `determine_next_effect_for_phase()` function is called by the
//! higher-level orchestration layer in `xsd_retry.rs`, which handles:
//!
//! 1. **Continuation cleanup** - Write pending continuation context
//! 2. **Retry logic** - Same-agent retry after timeout/failure
//! 3. **XSD retry** - Re-invoke agent after XSD validation failure
//! 4. **Continuation** - Re-invoke agent with continuation prompt
//! 5. **Normal progression** - Call phase-specific orchestration (this module)
//!
//! # Phase Modules
//!
//! - `planning` - Planning phase orchestration
//! - `development` - Development phase orchestration (including Analysis agent)
//! - `review` - Review phase orchestration (including Fix agent)
//! - `commit` - Commit phase orchestration
//!
//! # Special Cases
//!
//! - FinalValidation phase → CheckUncommittedChangesBeforeTermination (safety check), then ValidateFinalState
//! - Finalizing phase → RestorePromptPermissions effect
//! - AwaitingDevFix phase → TriggerDevFixFlow effect
//! - Complete/Interrupted phase → CheckUncommittedChangesBeforeTermination (safety check), then SaveCheckpoint
//!
//! ## Pre-Termination Safety Check
//!
//! Before any pipeline termination (Complete, Interrupted, or after FinalValidation),
//! the orchestration derives a `CheckUncommittedChangesBeforeTermination` effect to
//! ensure no work is lost:
//!
//! - If uncommitted changes exist → route to CommitMessage phase
//! - If working directory is clean → emit PreTerminationCommitChecked and proceed
//! - If git snapshot fails → route to AwaitingDevFix for recovery
//!
//! **THE ONLY EXCEPTION:** User-initiated Ctrl+C (interrupted_by_user=true) skips
//! this check because the user explicitly chose to interrupt. All other termination
//! paths (AwaitingDevFix exhaustion, completion marker emission, etc.) MUST commit
//! uncommitted work before terminating.

mod commit;
mod development;
mod planning;
mod review;

use crate::reducer::effect::Effect;
use crate::reducer::event::PipelinePhase;
use crate::reducer::state::PipelineState;

pub(in crate::reducer::orchestration) fn determine_next_effect_for_phase(
    state: &PipelineState,
) -> Effect {
    match state.phase {
        PipelinePhase::Planning => planning::determine_planning_effect(state),
        PipelinePhase::Development => development::determine_development_effect(state),
        PipelinePhase::Review => review::determine_review_effect(state),
        PipelinePhase::CommitMessage => commit::determine_commit_effect(state),
        PipelinePhase::FinalValidation => {
            // SAFETY CHECK: Ensure no uncommitted work before finalization
            // This check happens before FinalizingStarted, ensuring all work is committed
            // before the pipeline enters its terminal sequence (Finalizing -> Complete)
            if !state.pre_termination_commit_checked {
                return Effect::CheckUncommittedChangesBeforeTermination;
            }

            Effect::ValidateFinalState
        }
        PipelinePhase::Finalizing => Effect::RestorePromptPermissions,
        PipelinePhase::AwaitingDevFix => {
            // Completion marker emission must preempt recovery-loop effects.
            // If the marker write failed, we must keep retrying deterministically until
            // the marker is successfully written (CompletionMarkerEmitted).
            if state.completion_marker_pending {
                return Effect::EmitCompletionMarkerAndTerminate {
                    is_failure: state.completion_marker_is_failure,
                    reason: state.completion_marker_reason.clone(),
                };
            }

            // If dev-fix already triggered and recovery state is set, attempt recovery
            if state.dev_fix_triggered && state.recovery_escalation_level > 0 {
                // Derive the appropriate recovery effect based on escalation level
                if state.recovery_escalation_level == 1 {
                    // Level 1: Simple retry - emit RecoveryAttempted to transition back
                    Effect::AttemptRecovery {
                        level: state.recovery_escalation_level,
                        attempt_count: state.dev_fix_attempt_count,
                    }
                } else {
                    // Level 2+: Requires state reset - use EmitRecoveryReset
                    use crate::reducer::effect::RecoveryResetType;
                    let (reset_type, target_phase) = match state.recovery_escalation_level {
                        2 => (
                            RecoveryResetType::PhaseStart,
                            state
                                .failed_phase_for_recovery
                                .unwrap_or(PipelinePhase::Development),
                        ),
                        3 => (RecoveryResetType::IterationReset, PipelinePhase::Planning),
                        _ => (RecoveryResetType::CompleteReset, PipelinePhase::Planning),
                    };
                    Effect::EmitRecoveryReset {
                        reset_type,
                        target_phase,
                    }
                }
            } else {
                // First time in AwaitingDevFix or dev-fix not yet triggered
                let failed_phase = state
                    .failed_phase_for_recovery
                    .or(state.previous_phase)
                    .unwrap_or(PipelinePhase::Development);
                let failed_phase = if failed_phase == PipelinePhase::AwaitingDevFix {
                    PipelinePhase::Development
                } else {
                    failed_phase
                };
                Effect::TriggerDevFixFlow {
                    failed_phase,
                    failed_role: state.agent_chain.current_role,
                    retry_cycle: state.agent_chain.retry_cycle,
                }
            }
        }
        PipelinePhase::Complete | PipelinePhase::Interrupted => {
            use crate::reducer::event::CheckpointTrigger;

            // EXCEPTION: User-initiated Ctrl+C (interrupted_by_user=true) skips safety check
            // User explicitly chose to interrupt, so we respect that decision
            if state.interrupted_by_user {
                // On Interrupted, check if restoration is pending before checkpoint
                if state.phase == PipelinePhase::Interrupted
                    && state.prompt_permissions.restore_needed
                    && !state.prompt_permissions.restored
                {
                    return Effect::RestorePromptPermissions;
                }

                return Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::Interrupt,
                };
            }

            // SAFETY CHECK: Ensure no uncommitted work before termination
            // This applies to ALL other termination paths:
            // - AwaitingDevFix exhaustion → Interrupted
            // - Completion marker emission → Interrupted
            // - Normal completion → Complete
            if !state.pre_termination_commit_checked {
                return Effect::CheckUncommittedChangesBeforeTermination;
            }

            // Safety check passed - proceed with normal termination
            if state.phase == PipelinePhase::Interrupted
                && state.prompt_permissions.restore_needed
                && !state.prompt_permissions.restored
            {
                return Effect::RestorePromptPermissions;
            }

            Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::Interrupt,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRole;

    #[test]
    fn trigger_dev_fix_flow_prefers_failed_phase_for_recovery_over_previous_phase() {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::AwaitingDevFix);
        state.failed_phase_for_recovery = Some(PipelinePhase::CommitMessage);
        state.dev_fix_triggered = false;
        state.recovery_escalation_level = 0;

        state.agent_chain.current_role = AgentRole::Developer;
        state.agent_chain.retry_cycle = 7;

        let effect = determine_next_effect_for_phase(&state);

        match effect {
            Effect::TriggerDevFixFlow {
                failed_phase,
                failed_role,
                retry_cycle,
            } => {
                assert_eq!(failed_phase, PipelinePhase::CommitMessage);
                assert_eq!(failed_role, AgentRole::Developer);
                assert_eq!(retry_cycle, 7);
            }
            other => panic!("expected TriggerDevFixFlow, got: {other:?}"),
        }
    }

    #[test]
    fn trigger_dev_fix_flow_never_reports_awaiting_dev_fix_as_failed_phase() {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::AwaitingDevFix);
        state.failed_phase_for_recovery = None;
        state.dev_fix_triggered = false;
        state.recovery_escalation_level = 0;

        let effect = determine_next_effect_for_phase(&state);

        match effect {
            Effect::TriggerDevFixFlow { failed_phase, .. } => {
                assert_ne!(failed_phase, PipelinePhase::AwaitingDevFix);
            }
            other => panic!("expected TriggerDevFixFlow, got: {other:?}"),
        }
    }

    #[test]
    fn awaiting_dev_fix_derives_completion_marker_effect_when_pending() {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.completion_marker_pending = true;
        state.completion_marker_is_failure = true;
        state.completion_marker_reason = Some("safety_valve".to_string());

        let effect = determine_next_effect_for_phase(&state);

        assert!(
            matches!(
                effect,
                Effect::EmitCompletionMarkerAndTerminate {
                    is_failure: true,
                    ref reason
                } if reason.as_deref() == Some("safety_valve")
            ),
            "expected EmitCompletionMarkerAndTerminate when completion_marker_pending is set, got: {effect:?}"
        );
    }

    // Dev-fix agent selection is enforced by the TriggerDevFixFlow handler.
    // Orchestration intentionally preserves the original failed role in the effect params.

    #[test]
    fn user_interrupt_skips_pre_termination_safety_check() {
        // Test that when interrupted_by_user=true (Ctrl+C), the safety check is skipped
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Interrupted;
        state.interrupted_by_user = true; // Key: user-initiated interrupt
        state.pre_termination_commit_checked = false; // Safety check NOT done

        let effect = determine_next_effect_for_phase(&state);

        // Should skip safety check and go directly to SaveCheckpoint
        match effect {
            Effect::SaveCheckpoint { trigger } => {
                assert_eq!(trigger, crate::reducer::event::CheckpointTrigger::Interrupt);
            }
            other => panic!(
                "Expected SaveCheckpoint effect when interrupted_by_user=true, got {:?}. \
                 User interrupt should skip pre-termination safety check.",
                other
            ),
        }
    }

    #[test]
    fn programmatic_interrupt_requires_pre_termination_safety_check() {
        // Test that when interrupted_by_user=false (programmatic interrupt),
        // the safety check is REQUIRED before termination
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Interrupted;
        state.interrupted_by_user = false; // Key: NOT user-initiated
        state.pre_termination_commit_checked = false; // Safety check NOT done

        let effect = determine_next_effect_for_phase(&state);

        // Should derive safety check effect BEFORE proceeding to termination
        match effect {
            Effect::CheckUncommittedChangesBeforeTermination => {
                // Expected - safety check required
            }
            other => panic!(
                "Expected CheckUncommittedChangesBeforeTermination when interrupted_by_user=false, got {:?}. \
                 Programmatic interrupts must commit uncommitted work before terminating.",
                other
            ),
        }
    }

    #[test]
    fn complete_phase_requires_pre_termination_safety_check() {
        // Test that normal completion requires safety check
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Complete;
        state.interrupted_by_user = false;
        state.pre_termination_commit_checked = false;

        let effect = determine_next_effect_for_phase(&state);

        // Should derive safety check before completion
        match effect {
            Effect::CheckUncommittedChangesBeforeTermination => {
                // Expected - safety check required
            }
            other => panic!(
                "Expected CheckUncommittedChangesBeforeTermination before Complete, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn final_validation_requires_pre_termination_safety_check() {
        // Test that FinalValidation requires safety check before proceeding
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::FinalValidation;
        state.pre_termination_commit_checked = false;

        let effect = determine_next_effect_for_phase(&state);

        // Should derive safety check before final validation
        match effect {
            Effect::CheckUncommittedChangesBeforeTermination => {
                // Expected - safety check required
            }
            other => panic!(
                "Expected CheckUncommittedChangesBeforeTermination before FinalValidation, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn safety_check_allows_proceed_after_checked() {
        // Test that after safety check completes, pipeline proceeds normally
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Interrupted;
        state.interrupted_by_user = false;
        state.pre_termination_commit_checked = true; // Safety check DONE

        let effect = determine_next_effect_for_phase(&state);

        // Should proceed to checkpoint save
        match effect {
            Effect::SaveCheckpoint { .. } => {
                // Expected - proceed after safety check
            }
            other => panic!(
                "Expected SaveCheckpoint after safety check completes, got {:?}",
                other
            ),
        }
    }
}
