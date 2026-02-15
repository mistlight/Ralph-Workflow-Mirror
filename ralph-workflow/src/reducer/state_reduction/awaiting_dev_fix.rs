//! AwaitingDevFix event reduction.
//!
//! Handles events during the failure remediation phase.

use crate::reducer::event::{AwaitingDevFixEvent, PipelinePhase};
use crate::reducer::state::PipelineState;

/// Reduce AwaitingDevFix events.
///
/// This phase handles pipeline failure remediation by tracking the dev-fix
/// flow state and transitioning to Interrupted after completion marker emission.
pub(super) fn reduce_awaiting_dev_fix_event(
    state: PipelineState,
    event: AwaitingDevFixEvent,
) -> PipelineState {
    match event {
        AwaitingDevFixEvent::DevFixTriggered { .. } => {
            // Record that dev-fix was triggered, stay in AwaitingDevFix phase
            PipelineState {
                dev_fix_triggered: true,
                ..state
            }
        }
        AwaitingDevFixEvent::DevFixSkipped { .. } => {
            // Dev-fix was skipped, prepare for termination
            state
        }
        AwaitingDevFixEvent::DevFixCompleted {
            success: _,
            summary: _,
        } => {
            // Dev-fix attempt completed. Decide whether to:
            // 1. Attempt recovery at current level
            // 2. Escalate to next recovery level
            // 3. Give up and terminate (only after exhausting all levels)

            let new_attempt_count = state.dev_fix_attempt_count + 1;

            // Determine recovery escalation level based on attempt count
            // Level 1 (attempts 1-3): Retry same operation
            // Level 2 (attempts 4-6): Reset to phase start
            // Level 3 (attempts 7-9): Reset iteration counter
            // Level 4 (attempts 10+): Reset to iteration 0
            let new_level = match new_attempt_count {
                1..=3 => 1,
                4..=6 => 2,
                7..=9 => 3,
                _ => 4,
            };

            // If dev-fix reports success, we can optimistically attempt recovery
            // If it reports failure, we still attempt recovery but may need to escalate faster
            let should_attempt_recovery = new_attempt_count <= 12; // Max 12 attempts total

            // Prepare for recovery attempt at the determined level.
            //
            // IMPORTANT: Do not transition to Interrupted directly here.
            // Termination (if any) must happen through the single termination path:
            // AwaitingDevFix orchestration derives Effect::EmitCompletionMarkerAndTerminate,
            // then the reducer transitions to Interrupted when CompletionMarkerEmitted is reduced.
            // This keeps the completion marker semantics consistent and ensures the marker
            // includes the correct "reason" string.
            let updated = PipelineState {
                dev_fix_attempt_count: new_attempt_count,
                recovery_escalation_level: new_level,
                // Stay in AwaitingDevFix until recovery is attempted
                ..state
            };

            if !should_attempt_recovery {
                return updated;
            }

            updated
        }
        AwaitingDevFixEvent::DevFixAgentUnavailable { .. } => {
            // Dev-fix agent unavailable (quota/usage limit), prepare for termination
            // Completion marker already written, pipeline will terminate gracefully
            state
        }
        AwaitingDevFixEvent::CompletionMarkerEmitted { .. } => {
            // Completion marker emitted, transition to Interrupted
            PipelineState {
                phase: PipelinePhase::Interrupted,
                previous_phase: Some(state.phase),
                ..state
            }
        }
        AwaitingDevFixEvent::CompletionMarkerWriteFailed { .. } => {
            // Marker write failed; stay in AwaitingDevFix so orchestration can retry.
            state
        }
        AwaitingDevFixEvent::RecoveryAttempted {
            level,
            attempt_count: _,
            target_phase,
        } => {
            // Recovery state transitions documented for clarity:
            //
            // Level 1: Retry same operation (attempts 1-3)
            //   - No state reset, just transition back to failed phase
            //   - Orchestration will derive the same effect that failed
            //   - Example: If InvokeAgent failed, retry InvokeAgent
            //
            // Level 2: Reset to phase start (attempts 4-6)
            //   - Clear all phase-specific progress flags
            //   - Orchestration starts the phase from scratch
            //   - Preserves: iteration counter, reviewer_pass, other phases
            //   - Example: Clear development_agent_invoked_iteration, restart from PrepareDevelopmentContext
            //
            // Level 3: Reset iteration (attempts 7-9)
            //   - Decrement iteration counter (floor at 0)
            //   - Clear Planning/Development/Commit flags
            //   - Transition to Planning phase to redo iteration
            //   - Preserves: reviewer_pass, total_iterations
            //
            // Level 4: Complete reset (attempts 10+)
            //   - Reset iteration to 0
            //   - Clear Planning/Development/Commit flags
            //   - Transition to Planning phase for full restart
            //   - Preserves: reviewer_pass, total_iterations

            // Base state with phase transition
            let mut new_state = PipelineState {
                phase: target_phase,
                previous_phase: Some(PipelinePhase::AwaitingDevFix),
                // Keep recovery tracking fields so we can escalate if this fails
                ..state
            };

            // Apply state reset based on escalation level
            new_state = match level {
                1 => {
                    // Level 1: Simple retry - just transition back, no state reset
                    new_state
                }
                2 => {
                    // Level 2: Reset to phase start - clear phase-specific progress flags
                    new_state.clear_phase_flags(target_phase)
                }
                3 => {
                    // Level 3: Reset iteration counter - decrement iteration and restart from Planning
                    new_state.reset_iteration()
                }
                _ => {
                    // Level 4+: Complete reset - reset to iteration 0, restart from Planning
                    new_state.reset_to_iteration_zero()
                }
            };

            // Recovery must also reset agent-chain state.
            //
            // If the original failure was agent-chain exhaustion, leaving the chain exhausted
            // would cause immediate re-failure on the next orchestration cycle.
            //
            // Semantics:
            // - Always reset for Level 2+ recovery (phase/iteration resets imply fresh work).
            // - Also reset for Level 1 if the chain is already exhausted.
            if level >= 2 || new_state.agent_chain.is_exhausted() {
                let role = new_state.agent_chain.current_role;
                new_state.agent_chain = new_state.agent_chain.reset_for_role(role);
            }

            new_state
        }
        AwaitingDevFixEvent::RecoveryEscalated {
            from_level: _,
            to_level,
            reason: _,
        } => {
            // Recovery escalated - update level, stay in AwaitingDevFix
            PipelineState {
                recovery_escalation_level: to_level,
                ..state
            }
        }
        AwaitingDevFixEvent::RecoverySucceeded {
            level: _,
            total_attempts: _,
        } => {
            // Recovery succeeded - clear recovery state and resume normal operation
            PipelineState {
                dev_fix_attempt_count: 0,
                recovery_escalation_level: 0,
                failed_phase_for_recovery: None,
                // Stay in current phase (which should be the recovered phase)
                ..state
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRole;
    use crate::reducer::event::{AwaitingDevFixEvent, PipelineEvent, PipelinePhase};
    use crate::reducer::reduce;
    use crate::reducer::state::AgentChainState;

    #[test]
    fn dev_fix_completed_does_not_directly_interrupt_when_attempts_exhausted() {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.dev_fix_attempt_count = 12;
        state.recovery_escalation_level = 4;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        let new_state = reduce(
            state,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: false,
                summary: Some("attempt 13".to_string()),
            }),
        );

        assert_eq!(
            new_state.phase,
            PipelinePhase::AwaitingDevFix,
            "expected to remain in AwaitingDevFix so orchestration can emit completion marker"
        );
        assert_eq!(new_state.dev_fix_attempt_count, 13);
    }

    #[test]
    fn recovery_attempted_uses_event_target_phase_not_state_snapshot() {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.recovery_escalation_level = 2;
        state.dev_fix_attempt_count = 4;

        let new_state = reduce(
            state,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
                level: 2,
                attempt_count: 4,
                target_phase: PipelinePhase::Planning,
            }),
        );

        assert_eq!(new_state.phase, PipelinePhase::Planning);
    }

    #[test]
    fn recovery_attempted_resets_agent_chain_when_exhausted() {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::AwaitingDevFix;

        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["dev".to_string()],
                vec![vec!["model".to_string()]],
                AgentRole::Developer,
            )
            .with_max_cycles(1);
        chain.retry_cycle = 1;
        chain.current_agent_index = 0;
        chain.current_model_index = 0;
        assert!(chain.is_exhausted());
        state.agent_chain = chain;

        let new_state = reduce(
            state,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
                level: 1,
                attempt_count: 1,
                target_phase: PipelinePhase::Development,
            }),
        );

        assert!(!new_state.agent_chain.is_exhausted());
        assert_eq!(new_state.agent_chain.retry_cycle, 0);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    }
}
