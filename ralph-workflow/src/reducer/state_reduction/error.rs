//! Error event reduction.
//!
//! Handles error events returned through Err() from effect handlers.
//! Each error type has a specific recovery strategy decided by the reducer.

use crate::reducer::event::ErrorEvent;
use crate::reducer::state::PipelineState;

/// Reduce error events.
///
/// Error events are processed through the reducer identically to success events.
/// The reducer decides the recovery strategy based on the error type.
///
/// # Recovery Strategies
///
/// - **Continuation not supported errors**: These are invariant violations indicating
///   that continuation mode was incorrectly passed to a phase that doesn't support it.
///   The reducer transitions to `PipelinePhase::Interrupted`, and the event loop will
///   observe a terminal state and stop.
///
/// - **Missing inputs errors**: These indicate effect sequencing bugs where a handler
///   was called without required preconditions being met. The reducer transitions to
///   `PipelinePhase::Interrupted`, and the event loop will observe a terminal state
///   and stop.
pub(super) fn reduce_error(state: &PipelineState, error: &ErrorEvent) -> PipelineState {
    match error {
        // Continuation not supported errors are invariant violations
        ErrorEvent::PlanningContinuationNotSupported
        | ErrorEvent::ReviewContinuationNotSupported
        | ErrorEvent::FixContinuationNotSupported
        | ErrorEvent::CommitContinuationNotSupported => {
            // Invariant violations: route through AwaitingDevFix so unattended orchestration
            // always emits a completion marker and dispatches dev-fix, rather than terminating.
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::AwaitingDevFix;
            new_state.dev_fix_triggered = false;
            new_state
        }

        // Missing inputs are handler bugs - route through AwaitingDevFix for remediation.
        ErrorEvent::ReviewInputsNotMaterialized { .. }
        | ErrorEvent::PlanningInputsNotMaterialized { .. }
        | ErrorEvent::DevelopmentInputsNotMaterialized { .. }
        | ErrorEvent::CommitInputsNotMaterialized { .. }
        | ErrorEvent::CommitAgentNotInitialized { .. }
        | ErrorEvent::ValidatedPlanningMarkdownMissing { .. }
        | ErrorEvent::ValidatedDevelopmentOutcomeMissing { .. }
        | ErrorEvent::ValidatedReviewOutcomeMissing { .. }
        | ErrorEvent::ValidatedFixOutcomeMissing { .. }
        | ErrorEvent::ValidatedCommitOutcomeMissing { .. } => {
            // Invariant violations: route through AwaitingDevFix so the pipeline never
            // exits early and a completion marker is reliably written.
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::AwaitingDevFix;
            new_state.dev_fix_triggered = false;
            new_state
        }

        // Fix prompt file missing is recoverable - tmp artifacts can be cleaned between checkpoints.
        // Clear the "prepared" flag so orchestration re-runs PrepareFixPrompt.
        ErrorEvent::FixPromptMissing => {
            let mut new_state = state.clone();
            new_state.fix_prompt_prepared_pass = None;
            new_state
        }

        // Unknown agent lookup is recoverable - advance the agent chain to preserve
        // unattended-mode fallback behavior.
        ErrorEvent::AgentNotFound { .. } => {
            let mut new_state = state.clone();
            new_state.agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
            new_state.continuation = crate::reducer::state::ContinuationState {
                xsd_retry_count: 0,
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: false,
                same_agent_retry_count: 0,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation.clone()
            };
            new_state
        }

        // Missing prompt files are recoverable - tmp artifacts can be cleaned between checkpoints.
        // Clear the corresponding "prepared" flag so the event loop will regenerate the prompt.
        ErrorEvent::PlanningPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.planning_prompt_prepared_iteration = None;
            new_state
        }
        ErrorEvent::DevelopmentPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.development_prompt_prepared_iteration = None;
            new_state
        }
        ErrorEvent::ReviewPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.review_prompt_prepared_pass = None;
            new_state
        }
        ErrorEvent::CommitPromptMissing { .. } => {
            let mut new_state = state.clone();
            new_state.commit_prompt_prepared = false;
            new_state
        }

        // Workspace/Git operation failures must not cause early pipeline termination.
        // Route these through AwaitingDevFix so TriggerDevFixFlow writes the completion marker
        // and unattended orchestration can reliably detect completion.
        ErrorEvent::WorkspaceReadFailed { .. }
        | ErrorEvent::WorkspaceWriteFailed { .. }
        | ErrorEvent::WorkspaceCreateDirAllFailed { .. }
        | ErrorEvent::WorkspaceRemoveFailed { .. }
        | ErrorEvent::GitAddAllFailed { .. } => {
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::AwaitingDevFix;
            new_state.dev_fix_triggered = false;
            // Capture the failed phase for recovery
            new_state.failed_phase_for_recovery = Some(state.phase);
            // Reset recovery counters for new failure
            new_state.dev_fix_attempt_count = 0;
            new_state.recovery_escalation_level = 0;
            new_state
        }

        // Agent chain exhausted - transition to AwaitingDevFix for remediation attempt
        // instead of immediately terminating
        ErrorEvent::AgentChainExhausted { .. } => {
            // Transition to AwaitingDevFix phase
            // This signals orchestration to invoke the development agent to diagnose
            // and fix the pipeline failure before deciding whether to proceed or terminate
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::AwaitingDevFix;
            new_state.dev_fix_triggered = false; // Reset flag for new AwaitingDevFix phase

            // Capture the failed phase for recovery
            new_state.failed_phase_for_recovery = Some(state.phase);

            // CRITICAL: Only reset recovery counters if this is a NEW failure
            // (not already in recovery loop). If previous_phase is AwaitingDevFix,
            // we're failing AGAIN after recovery attempt - keep counters to continue
            // escalation. This enables the recovery loop instead of resetting to level 1.
            if state.previous_phase != Some(PipelinePhase::AwaitingDevFix) {
                // First failure - reset counters
                new_state.dev_fix_attempt_count = 0;
                new_state.recovery_escalation_level = 0;
            }
            // else: Already in recovery loop - keep existing attempt_count and level
            // to continue escalation on next dev-fix completion

            new_state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::state::ContinuationState;

    #[test]
    fn test_reduce_continuation_not_supported_errors_route_to_awaiting_dev_fix() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![
            ErrorEvent::PlanningContinuationNotSupported,
            ErrorEvent::ReviewContinuationNotSupported,
            ErrorEvent::FixContinuationNotSupported,
            ErrorEvent::CommitContinuationNotSupported,
        ];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            assert_eq!(
                new_state.phase,
                crate::reducer::event::PipelinePhase::AwaitingDevFix
            );
            assert!(
                !new_state.dev_fix_triggered,
                "expected dev_fix_triggered reset when routing to AwaitingDevFix"
            );
        }
    }

    #[test]
    fn test_reduce_missing_inputs_errors_route_to_awaiting_dev_fix() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![ErrorEvent::ReviewInputsNotMaterialized { pass: 1 }];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            assert_eq!(
                new_state.phase,
                crate::reducer::event::PipelinePhase::AwaitingDevFix
            );
            assert!(
                !new_state.dev_fix_triggered,
                "expected dev_fix_triggered reset when routing to AwaitingDevFix"
            );
        }
    }

    #[test]
    fn test_reduce_fix_prompt_missing_is_recoverable_by_clearing_prepared_flag() {
        use crate::reducer::event::PipelinePhase;

        let mut state =
            PipelineState::initial_with_continuation(0, 1, ContinuationState::default());
        state.phase = PipelinePhase::Review;
        state.fix_prompt_prepared_pass = Some(0);

        let new_state = reduce_error(&state, &ErrorEvent::FixPromptMissing);

        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.fix_prompt_prepared_pass, None);
    }

    #[test]
    fn test_reduce_agent_not_found_advances_agent_chain_instead_of_terminating() {
        use crate::agents::AgentRole;
        use crate::reducer::event::PipelinePhase;
        use crate::reducer::state::AgentChainState;

        let mut state =
            PipelineState::initial_with_continuation(1, 0, ContinuationState::default());
        state.phase = PipelinePhase::Development;
        state.agent_chain = AgentChainState::initial().with_agents(
            vec!["missing".to_string(), "fallback".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        let new_state = reduce_error(
            &state,
            &ErrorEvent::AgentNotFound {
                agent: "missing".to_string(),
            },
        );

        assert_eq!(new_state.phase, PipelinePhase::Development);
        assert_eq!(new_state.agent_chain.current_agent_index, 1);
    }

    #[test]
    fn test_reduce_agent_chain_exhausted_transitions_to_awaiting_dev_fix() {
        use crate::agents::AgentRole;
        use crate::reducer::event::PipelinePhase;

        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());
        assert_eq!(state.phase, PipelinePhase::Planning);

        let error = ErrorEvent::AgentChainExhausted {
            role: AgentRole::Developer,
            phase: PipelinePhase::Development,
            cycle: 3,
        };

        let new_state = reduce_error(&state, &error);

        // Should transition to AwaitingDevFix phase for remediation
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(
            new_state.previous_phase,
            Some(state.phase),
            "previous_phase should be recorded for dev-fix transitions"
        );
    }

    #[test]
    fn test_reduce_workspace_failures_transition_to_awaiting_dev_fix_and_set_previous_phase() {
        use crate::reducer::event::{PipelinePhase, WorkspaceIoErrorKind};

        let mut state =
            PipelineState::initial_with_continuation(1, 1, ContinuationState::default());
        state.phase = PipelinePhase::Review;

        let error = ErrorEvent::WorkspaceWriteFailed {
            path: ".agent/tmp/out.txt".to_string(),
            kind: WorkspaceIoErrorKind::Other,
        };

        let new_state = reduce_error(&state, &error);
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(
            new_state.previous_phase,
            Some(state.phase),
            "previous_phase should be recorded for awaiting-dev-fix transitions"
        );
        assert!(
            !new_state.dev_fix_triggered,
            "dev_fix_triggered should be reset on awaiting-dev-fix transitions"
        );
    }

    #[test]
    fn test_agent_chain_exhausted_preserves_recovery_state_when_already_in_recovery() {
        use crate::agents::AgentRole;
        use crate::reducer::event::PipelinePhase;

        // Set up state that's already in recovery loop
        let mut state =
            PipelineState::initial_with_continuation(1, 1, ContinuationState::default());
        state.phase = PipelinePhase::Development;
        state.previous_phase = Some(PipelinePhase::AwaitingDevFix); // Key: already in recovery
        state.dev_fix_attempt_count = 2;
        state.recovery_escalation_level = 1;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);

        // Simulate failure again during recovery
        let error = ErrorEvent::AgentChainExhausted {
            role: AgentRole::Developer,
            phase: PipelinePhase::Development,
            cycle: 3,
        };

        let new_state = reduce_error(&state, &error);

        // Should transition to AwaitingDevFix
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);

        // CRITICAL: Should preserve recovery state (not reset to 0)
        assert_eq!(
            new_state.dev_fix_attempt_count, 2,
            "dev_fix_attempt_count should be preserved when already in recovery loop"
        );
        assert_eq!(
            new_state.recovery_escalation_level, 1,
            "recovery_escalation_level should be preserved when already in recovery loop"
        );
    }

    #[test]
    fn test_agent_chain_exhausted_resets_recovery_state_on_first_failure() {
        use crate::agents::AgentRole;
        use crate::reducer::event::PipelinePhase;

        // Set up state that's NOT in recovery (first failure)
        let mut state =
            PipelineState::initial_with_continuation(1, 1, ContinuationState::default());
        state.phase = PipelinePhase::Development;
        state.previous_phase = Some(PipelinePhase::Planning); // Not AwaitingDevFix
                                                              // Simulate stale recovery state from previous recovery
        state.dev_fix_attempt_count = 5;
        state.recovery_escalation_level = 2;
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);

        // Simulate first failure (not in recovery)
        let error = ErrorEvent::AgentChainExhausted {
            role: AgentRole::Developer,
            phase: PipelinePhase::Development,
            cycle: 3,
        };

        let new_state = reduce_error(&state, &error);

        // Should transition to AwaitingDevFix
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);

        // Should reset recovery state (new failure, not recovery loop)
        assert_eq!(
            new_state.dev_fix_attempt_count, 0,
            "dev_fix_attempt_count should be reset on first failure"
        );
        assert_eq!(
            new_state.recovery_escalation_level, 0,
            "recovery_escalation_level should be reset on first failure"
        );
    }
}
