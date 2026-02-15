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

            if !should_attempt_recovery {
                // Exhausted all recovery attempts - emit completion marker and terminate
                // This is the ONLY path to termination after entering AwaitingDevFix
                return PipelineState {
                    phase: PipelinePhase::Interrupted,
                    previous_phase: Some(state.phase),
                    dev_fix_attempt_count: new_attempt_count,
                    ..state
                };
            }

            // Prepare for recovery attempt at the determined level
            PipelineState {
                dev_fix_attempt_count: new_attempt_count,
                recovery_escalation_level: new_level,
                // Stay in AwaitingDevFix until recovery is attempted
                ..state
            }
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
        AwaitingDevFixEvent::RecoveryAttempted {
            level,
            attempt_count: _,
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

            // Recovery attempt initiated - transition back to failed phase
            let target_phase = state
                .failed_phase_for_recovery
                .unwrap_or(PipelinePhase::Development);

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
