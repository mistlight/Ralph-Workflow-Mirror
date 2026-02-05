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
            // Invariant violations: terminate cleanly by transitioning to Interrupted.
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::Interrupted;
            new_state
        }

        // Missing inputs are handler bugs - these should be caught by effect sequencing
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
            // Invariant violations: terminate cleanly by transitioning to Interrupted.
            use crate::reducer::event::PipelinePhase;
            let mut new_state = state.clone();
            new_state.previous_phase = Some(state.phase);
            new_state.phase = PipelinePhase::Interrupted;
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
            new_state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::state::ContinuationState;

    #[test]
    fn test_reduce_continuation_not_supported_errors_transition_to_interrupted() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![
            ErrorEvent::PlanningContinuationNotSupported,
            ErrorEvent::ReviewContinuationNotSupported,
            ErrorEvent::FixContinuationNotSupported,
            ErrorEvent::CommitContinuationNotSupported,
        ];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            // Terminate cleanly
            assert_eq!(
                new_state.phase,
                crate::reducer::event::PipelinePhase::Interrupted
            );
        }
    }

    #[test]
    fn test_reduce_missing_inputs_errors_transition_to_interrupted() {
        let state = PipelineState::initial_with_continuation(1, 1, ContinuationState::default());

        let errors = vec![ErrorEvent::ReviewInputsNotMaterialized { pass: 1 }];

        for error in errors {
            let new_state = reduce_error(&state, &error);
            // Terminate cleanly
            assert_eq!(
                new_state.phase,
                crate::reducer::event::PipelinePhase::Interrupted
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
}
