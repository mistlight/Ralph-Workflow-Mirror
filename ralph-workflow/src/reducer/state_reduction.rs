//! Reducer function for state transitions.
//!
//! Implements pure state reduction - no side effects, exhaustive pattern matching.
//!
//! # Architecture
//!
//! The main `reduce` function delegates to category-specific handlers for
//! better organization and maintainability:
//!
//! - `reduce_pipeline_lifecycle` - Pipeline start/stop/completion events
//! - `reduce_planning_event` - Planning phase events
//! - `reduce_development_event` - Development iteration events
//! - `reduce_review_event` - Review pass and fix events
//! - `reduce_agent_event` - Agent invocation and chain events
//! - `reduce_rebase_event` - Rebase operation events
//! - `reduce_commit_event` - Commit generation and creation events
//!
//! Each handler is a pure function that takes state and returns new state.

use super::event::PipelineEvent;
use super::state::{CommitState, ContinuationState, PipelineState, RebaseState};

/// Pure reducer - no side effects, exhaustive match.
///
/// Computes new state by applying an event to current state.
/// This function has zero side effects - all state mutations are explicit.
///
/// Delegates to category-specific handlers for better organization.
pub fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match &event {
        // Pipeline lifecycle events
        PipelineEvent::PipelineStarted
        | PipelineEvent::PipelineResumed { .. }
        | PipelineEvent::PipelineCompleted
        | PipelineEvent::PipelineAborted { .. } => reduce_pipeline_lifecycle(state, event),

        // Planning events
        PipelineEvent::PlanningPhaseStarted
        | PipelineEvent::PlanningPhaseCompleted
        | PipelineEvent::PlanGenerationStarted { .. }
        | PipelineEvent::PlanGenerationCompleted { .. } => reduce_planning_event(state, event),

        // Development events
        PipelineEvent::DevelopmentPhaseStarted
        | PipelineEvent::DevelopmentIterationStarted { .. }
        | PipelineEvent::DevelopmentIterationCompleted { .. }
        | PipelineEvent::DevelopmentPhaseCompleted
        | PipelineEvent::DevelopmentIterationContinuationTriggered { .. }
        | PipelineEvent::DevelopmentIterationContinuationSucceeded { .. } => {
            reduce_development_event(state, event)
        }

        // Review events
        PipelineEvent::ReviewPhaseStarted
        | PipelineEvent::ReviewPassStarted { .. }
        | PipelineEvent::ReviewCompleted { .. }
        | PipelineEvent::FixAttemptStarted { .. }
        | PipelineEvent::FixAttemptCompleted { .. }
        | PipelineEvent::ReviewPhaseCompleted { .. } => reduce_review_event(state, event),

        // Agent events
        PipelineEvent::AgentInvocationStarted { .. }
        | PipelineEvent::AgentInvocationSucceeded { .. }
        | PipelineEvent::AgentInvocationFailed { .. }
        | PipelineEvent::AgentFallbackTriggered { .. }
        | PipelineEvent::AgentChainExhausted { .. }
        | PipelineEvent::AgentModelFallbackTriggered { .. }
        | PipelineEvent::AgentRetryCycleStarted { .. }
        | PipelineEvent::AgentChainInitialized { .. } => reduce_agent_event(state, event),

        // Rebase events
        PipelineEvent::RebaseStarted { .. }
        | PipelineEvent::RebaseConflictDetected { .. }
        | PipelineEvent::RebaseConflictResolved { .. }
        | PipelineEvent::RebaseSucceeded { .. }
        | PipelineEvent::RebaseFailed { .. }
        | PipelineEvent::RebaseSkipped { .. }
        | PipelineEvent::RebaseAborted { .. } => reduce_rebase_event(state, event),

        // Commit events
        PipelineEvent::CommitGenerationStarted
        | PipelineEvent::CommitMessageGenerated { .. }
        | PipelineEvent::CommitCreated { .. }
        | PipelineEvent::CommitGenerationFailed { .. }
        | PipelineEvent::CommitSkipped { .. }
        | PipelineEvent::CommitMessageValidationFailed { .. } => reduce_commit_event(state, event),

        // Miscellaneous events
        PipelineEvent::ContextCleaned => PipelineState {
            context_cleaned: true,
            ..state
        },
        PipelineEvent::CheckpointSaved { .. } => state,
        PipelineEvent::FinalizingStarted => PipelineState {
            phase: super::event::PipelinePhase::Finalizing,
            ..state
        },
        PipelineEvent::PromptPermissionsRestored => PipelineState {
            phase: super::event::PipelinePhase::Complete,
            ..state
        },
    }
}

// ============================================================================
// Category-specific reducers
// ============================================================================

/// Handle pipeline lifecycle events.
fn reduce_pipeline_lifecycle(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::PipelineStarted => state,
        PipelineEvent::PipelineResumed { .. } => state,
        PipelineEvent::PipelineCompleted => PipelineState {
            phase: super::event::PipelinePhase::Complete,
            ..state
        },
        PipelineEvent::PipelineAborted { .. } => PipelineState {
            phase: super::event::PipelinePhase::Interrupted,
            ..state
        },
        _ => state,
    }
}

/// Handle planning phase events.
fn reduce_planning_event(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::PlanningPhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Planning,
            ..state
        },
        PipelineEvent::PlanningPhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Development,
            ..state
        },
        PipelineEvent::PlanGenerationStarted { .. } => state,
        PipelineEvent::PlanGenerationCompleted { .. } => PipelineState {
            phase: super::event::PipelinePhase::Development,
            ..state
        },
        _ => state,
    }
}

/// Handle development phase events.
fn reduce_development_event(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::DevelopmentPhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Development,
            ..state
        },
        PipelineEvent::DevelopmentIterationStarted { iteration } => PipelineState {
            iteration,
            agent_chain: state.agent_chain.reset(),
            // Reset continuation state when starting a new iteration
            continuation: state.continuation.reset(),
            ..state
        },
        PipelineEvent::DevelopmentIterationCompleted {
            iteration,
            output_valid,
        } => {
            if output_valid {
                // After a successful dev iteration, go to CommitMessage phase to create a commit.
                PipelineState {
                    phase: super::event::PipelinePhase::CommitMessage,
                    previous_phase: Some(super::event::PipelinePhase::Development),
                    iteration,
                    commit: super::state::CommitState::NotStarted,
                    context_cleaned: false,
                    // Reset continuation state on successful completion
                    continuation: ContinuationState::new(),
                    ..state
                }
            } else {
                // Output was not valid enough to proceed to commit; stay in Development.
                PipelineState {
                    phase: super::event::PipelinePhase::Development,
                    iteration,
                    ..state
                }
            }
        }
        PipelineEvent::DevelopmentPhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Review,
            // Reset continuation state when phase completes
            continuation: ContinuationState::new(),
            ..state
        },
        PipelineEvent::DevelopmentIterationContinuationTriggered {
            iteration: _,
            status,
            summary,
            files_changed,
            next_steps,
        } => {
            // Trigger continuation with context from the previous attempt
            PipelineState {
                continuation: state.continuation.trigger_continuation(
                    status,
                    summary,
                    files_changed,
                    next_steps,
                ),
                ..state
            }
        }
        PipelineEvent::DevelopmentIterationContinuationSucceeded {
            iteration: _,
            total_continuation_attempts: _,
        } => {
            // Continuation succeeded; proceed to CommitMessage and reset continuation state.
            PipelineState {
                phase: super::event::PipelinePhase::CommitMessage,
                previous_phase: Some(super::event::PipelinePhase::Development),
                commit: super::state::CommitState::NotStarted,
                context_cleaned: false,
                continuation: ContinuationState::new(),
                ..state
            }
        }
        _ => state,
    }
}

/// Handle review phase events.
fn reduce_review_event(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::ReviewPhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Review,
            reviewer_pass: 0,
            review_issues_found: false,
            ..state
        },
        PipelineEvent::ReviewPassStarted { pass } => PipelineState {
            reviewer_pass: pass,
            review_issues_found: false,
            agent_chain: state.agent_chain.reset(),
            ..state
        },
        PipelineEvent::ReviewCompleted { pass, issues_found } => {
            let next_pass = if issues_found { pass } else { pass + 1 };
            let next_phase = if !issues_found && next_pass >= state.total_reviewer_passes {
                super::event::PipelinePhase::CommitMessage
            } else {
                state.phase
            };
            PipelineState {
                phase: next_phase,
                reviewer_pass: next_pass,
                review_issues_found: issues_found,
                ..state
            }
        }
        PipelineEvent::FixAttemptStarted { .. } => PipelineState {
            agent_chain: state.agent_chain.reset(),
            ..state
        },
        PipelineEvent::FixAttemptCompleted { pass, .. } => PipelineState {
            phase: super::event::PipelinePhase::CommitMessage,
            previous_phase: Some(super::event::PipelinePhase::Review),
            reviewer_pass: pass,
            review_issues_found: false,
            commit: super::state::CommitState::NotStarted,
            ..state
        },
        PipelineEvent::ReviewPhaseCompleted { .. } => PipelineState {
            phase: super::event::PipelinePhase::CommitMessage,
            ..state
        },
        _ => state,
    }
}

/// Handle agent-related events.
fn reduce_agent_event(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::AgentInvocationStarted { .. } => state,
        PipelineEvent::AgentInvocationSucceeded { .. } => state,
        PipelineEvent::AgentInvocationFailed {
            retriable: true, ..
        } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        PipelineEvent::AgentInvocationFailed {
            retriable: false, ..
        } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent(),
            ..state
        },
        PipelineEvent::AgentFallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent(),
            ..state
        },
        PipelineEvent::AgentChainExhausted { .. } => PipelineState {
            agent_chain: state.agent_chain.start_retry_cycle(),
            ..state
        },
        PipelineEvent::AgentModelFallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        PipelineEvent::AgentRetryCycleStarted { .. } => state,
        PipelineEvent::AgentChainInitialized { role, agents } => {
            let models_per_agent = agents.iter().map(|_| vec![]).collect();
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .with_agents(agents, models_per_agent, role)
                    .reset_for_role(role),
                ..state
            }
        }
        _ => state,
    }
}

/// Handle rebase-related events.
fn reduce_rebase_event(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::RebaseStarted {
            target_branch,
            phase: _,
        } => PipelineState {
            rebase: RebaseState::InProgress {
                original_head: state.current_head(),
                target_branch,
            },
            ..state
        },
        PipelineEvent::RebaseConflictDetected { files } => PipelineState {
            rebase: match &state.rebase {
                RebaseState::InProgress {
                    original_head,
                    target_branch,
                } => RebaseState::Conflicted {
                    original_head: original_head.clone(),
                    target_branch: target_branch.clone(),
                    files,
                    resolution_attempts: 0,
                },
                _ => state.rebase.clone(),
            },
            ..state
        },
        PipelineEvent::RebaseConflictResolved { .. } => PipelineState {
            rebase: match &state.rebase {
                RebaseState::Conflicted {
                    original_head,
                    target_branch,
                    ..
                } => RebaseState::InProgress {
                    original_head: original_head.clone(),
                    target_branch: target_branch.clone(),
                },
                _ => state.rebase.clone(),
            },
            ..state
        },
        PipelineEvent::RebaseSucceeded { new_head, .. } => PipelineState {
            rebase: RebaseState::Completed { new_head },
            ..state
        },
        PipelineEvent::RebaseFailed { .. } => PipelineState {
            rebase: RebaseState::NotStarted,
            ..state
        },
        PipelineEvent::RebaseSkipped { .. } => PipelineState {
            rebase: RebaseState::Skipped,
            ..state
        },
        PipelineEvent::RebaseAborted { .. } => state,
        _ => state,
    }
}

/// Handle commit-related events.
fn reduce_commit_event(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::CommitGenerationStarted => PipelineState {
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: super::state::MAX_VALIDATION_RETRY_ATTEMPTS,
            },
            ..state
        },
        PipelineEvent::CommitMessageGenerated { message, .. } => PipelineState {
            commit: CommitState::Generated { message },
            ..state
        },
        PipelineEvent::CommitCreated { hash, .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            PipelineState {
                commit: CommitState::Committed { hash },
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                context_cleaned: false,
                ..state
            }
        }
        PipelineEvent::CommitGenerationFailed { .. } => PipelineState {
            commit: CommitState::NotStarted,
            ..state
        },
        PipelineEvent::CommitSkipped { .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            PipelineState {
                commit: CommitState::Skipped,
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                context_cleaned: false,
                ..state
            }
        }
        PipelineEvent::CommitMessageValidationFailed { attempt, .. } => {
            reduce_commit_validation_failed(state, attempt)
        }
        _ => state,
    }
}

/// Compute phase transition after a commit (used by CommitCreated and CommitSkipped).
fn compute_post_commit_transition(
    state: &PipelineState,
) -> (super::event::PipelinePhase, u32, u32) {
    match state.previous_phase {
        Some(super::event::PipelinePhase::Development) => {
            let next_iter = state.iteration + 1;
            if next_iter >= state.total_iterations {
                (
                    super::event::PipelinePhase::Review,
                    next_iter,
                    state.reviewer_pass,
                )
            } else {
                (
                    super::event::PipelinePhase::Planning,
                    next_iter,
                    state.reviewer_pass,
                )
            }
        }
        Some(super::event::PipelinePhase::Review) => {
            let next_pass = state.reviewer_pass + 1;
            if next_pass >= state.total_reviewer_passes {
                (
                    super::event::PipelinePhase::FinalValidation,
                    state.iteration,
                    next_pass,
                )
            } else {
                (
                    super::event::PipelinePhase::Review,
                    state.iteration,
                    next_pass,
                )
            }
        }
        _ => (
            super::event::PipelinePhase::FinalValidation,
            state.iteration,
            state.reviewer_pass,
        ),
    }
}

/// Handle commit message validation failure with retry logic.
fn reduce_commit_validation_failed(state: PipelineState, attempt: u32) -> PipelineState {
    let next_attempt = attempt + 1;
    let max_attempts = super::state::MAX_VALIDATION_RETRY_ATTEMPTS;

    if next_attempt <= max_attempts {
        PipelineState {
            commit: CommitState::Generating {
                attempt: next_attempt,
                max_attempts,
            },
            ..state
        }
    } else {
        // Exceeded max attempts with current agent - try next agent
        let old_agent_index = state.agent_chain.current_agent_index;
        let old_retry_cycle = state.agent_chain.retry_cycle;
        let new_agent_chain = state.agent_chain.switch_to_next_agent();

        let wrapped_around = new_agent_chain.retry_cycle > old_retry_cycle;
        let advanced_to_next =
            new_agent_chain.current_agent_index != old_agent_index && !wrapped_around;

        if advanced_to_next {
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::Generating {
                    attempt: 1,
                    max_attempts,
                },
                ..state
            }
        } else {
            // All agents exhausted - reset so orchestration can handle
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::NotStarted,
                ..state
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRole;
    use crate::reducer::event::AgentErrorKind;
    use crate::reducer::event::PipelinePhase;
    use crate::reducer::event::RebasePhase;
    use crate::reducer::state::AgentChainState;

    fn create_test_state() -> PipelineState {
        PipelineState {
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec!["model1".to_string(), "model2".to_string()]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(5, 2)
        }
    }

    #[test]
    fn test_reduce_pipeline_started() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::PipelineStarted);
        assert_eq!(new_state.phase, PipelinePhase::Planning);
    }

    #[test]
    fn test_reduce_pipeline_completed() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::PipelineCompleted);
        assert_eq!(new_state.phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_reduce_development_iteration_completed() {
        // DevelopmentIterationCompleted transitions to CommitMessage phase
        // The iteration counter stays the same; it gets incremented by CommitCreated
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 2,
                output_valid: true,
            },
        );
        // Iteration stays at 2 (incremented by CommitCreated later)
        assert_eq!(new_state.iteration, 2);
        // Goes to CommitMessage phase to create a commit
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        // Previous phase stored for return after commit
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Development));
    }

    #[test]
    fn test_reduce_development_iteration_complete_goes_to_commit() {
        // Even on last iteration, DevelopmentIterationCompleted goes to CommitMessage
        // The transition to Review happens after CommitCreated
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 5,
            total_iterations: 5,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 5,
                output_valid: true,
            },
        );
        // Iteration stays at 5 (incremented by CommitCreated later)
        assert_eq!(new_state.iteration, 5);
        // Goes to CommitMessage phase first
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_reduce_agent_fallback_to_next_model() {
        let state = create_test_state();
        let initial_agent = state.agent_chain.current_agent().unwrap().clone();
        let initial_model_index = state.agent_chain.current_model_index;

        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: initial_agent.clone(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );

        assert_ne!(
            new_state.agent_chain.current_model_index,
            initial_model_index
        );
    }

    #[test]
    fn test_reduce_rebase_started() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::RebaseStarted {
                phase: RebasePhase::Initial,
                target_branch: "main".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
    }

    #[test]
    fn test_reduce_rebase_succeeded() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::RebaseSucceeded {
                phase: RebasePhase::Initial,
                new_head: "abc123".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::Completed { .. }));
    }

    #[test]
    fn test_reduce_commit_generation_started() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::CommitGenerationStarted);

        assert!(matches!(new_state.commit, CommitState::Generating { .. }));
    }

    #[test]
    fn test_reduce_commit_created() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: "abc123".to_string(),
                message: "test commit".to_string(),
            },
        );

        assert!(matches!(new_state.commit, CommitState::Committed { .. }));
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    }

    #[test]
    fn test_reduce_all_agent_failure_scenarios() {
        let state = create_test_state();
        let initial_agent_index = state.agent_chain.current_agent_index;
        let initial_model_index = state.agent_chain.current_model_index;
        let agent_name = state.agent_chain.current_agent().unwrap().clone();

        let network_error_state = reduce(
            state.clone(),
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: agent_name.clone(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );
        assert_eq!(
            network_error_state.agent_chain.current_agent_index,
            initial_agent_index
        );
        assert!(network_error_state.agent_chain.current_model_index > initial_model_index);

        let auth_error_state = reduce(
            state.clone(),
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: agent_name.clone(),
                exit_code: 1,
                error_kind: AgentErrorKind::Authentication,
                retriable: false,
            },
        );
        assert!(auth_error_state.agent_chain.current_agent_index > initial_agent_index);
        assert_eq!(
            auth_error_state.agent_chain.current_model_index,
            initial_model_index
        );

        let internal_error_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: agent_name,
                exit_code: 139,
                error_kind: AgentErrorKind::InternalError,
                retriable: false,
            },
        );
        assert!(internal_error_state.agent_chain.current_agent_index > initial_agent_index);
    }

    #[test]
    fn test_reduce_rebase_full_state_machine() {
        let mut state = create_test_state();

        state = reduce(
            state,
            PipelineEvent::RebaseStarted {
                phase: RebasePhase::Initial,
                target_branch: "main".to_string(),
            },
        );
        assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

        state = reduce(
            state,
            PipelineEvent::RebaseConflictDetected {
                files: vec![std::path::PathBuf::from("file1.txt")],
            },
        );
        assert!(matches!(state.rebase, RebaseState::Conflicted { .. }));

        state = reduce(
            state,
            PipelineEvent::RebaseConflictResolved {
                files: vec![std::path::PathBuf::from("file1.txt")],
            },
        );
        assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

        state = reduce(
            state,
            PipelineEvent::RebaseSucceeded {
                phase: RebasePhase::Initial,
                new_head: "def456".to_string(),
            },
        );
        assert!(matches!(state.rebase, RebaseState::Completed { .. }));
    }

    #[test]
    fn test_reduce_commit_full_state_machine() {
        let mut state = create_test_state();

        state = reduce(state, PipelineEvent::CommitGenerationStarted);
        assert!(matches!(state.commit, CommitState::Generating { .. }));

        state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: "abc123".to_string(),
                message: "test commit".to_string(),
            },
        );
        assert!(matches!(state.commit, CommitState::Committed { .. }));
    }

    #[test]
    fn test_reduce_phase_transitions() {
        let mut state = create_test_state();

        state = reduce(state, PipelineEvent::PlanningPhaseCompleted);
        assert_eq!(state.phase, PipelinePhase::Development);

        state = reduce(state, PipelineEvent::DevelopmentPhaseStarted);
        assert_eq!(state.phase, PipelinePhase::Development);

        state = reduce(state, PipelineEvent::DevelopmentPhaseCompleted);
        assert_eq!(state.phase, PipelinePhase::Review);

        state = reduce(state, PipelineEvent::ReviewPhaseStarted);
        assert_eq!(state.phase, PipelinePhase::Review);

        state = reduce(
            state,
            PipelineEvent::ReviewPhaseCompleted { early_exit: false },
        );
        assert_eq!(state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_reduce_agent_chain_exhaustion() {
        let state = PipelineState {
            agent_chain: AgentChainState::initial()
                .with_agents(
                    vec!["agent1".to_string()],
                    vec![vec!["model1".to_string()]],
                    AgentRole::Developer,
                )
                .with_max_cycles(3),
            ..create_test_state()
        };

        let exhausted_state = reduce(
            state,
            PipelineEvent::AgentChainExhausted {
                role: AgentRole::Developer,
            },
        );

        assert_eq!(exhausted_state.agent_chain.current_agent_index, 0);
        assert_eq!(exhausted_state.agent_chain.current_model_index, 0);
        assert_eq!(exhausted_state.agent_chain.retry_cycle, 1);
    }

    #[test]
    fn test_reduce_agent_fallback_triggers_fallback_event() {
        let state = create_test_state();
        let agent = state.agent_chain.current_agent().unwrap().clone();

        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: agent.clone(),
                exit_code: 1,
                error_kind: AgentErrorKind::Authentication,
                retriable: false,
            },
        );

        assert!(new_state.agent_chain.current_agent_index > 0);
    }

    #[test]
    fn test_reduce_model_fallback_triggers_fallback_event() {
        let state = create_test_state();
        let initial_model_index = state.agent_chain.current_model_index;
        let agent_name = state.agent_chain.current_agent().unwrap().clone();

        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: agent_name,
                exit_code: 1,
                error_kind: AgentErrorKind::RateLimit,
                retriable: true,
            },
        );

        assert!(new_state.agent_chain.current_model_index > initial_model_index);
    }

    #[test]
    fn test_reduce_finalizing_started() {
        let state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::FinalizingStarted);
        assert_eq!(new_state.phase, PipelinePhase::Finalizing);
    }

    #[test]
    fn test_reduce_prompt_permissions_restored() {
        let state = PipelineState {
            phase: PipelinePhase::Finalizing,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::PromptPermissionsRestored);
        assert_eq!(new_state.phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_reduce_finalization_full_flow() {
        let mut state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..create_test_state()
        };

        // FinalValidation -> Finalizing
        state = reduce(state, PipelineEvent::FinalizingStarted);
        assert_eq!(state.phase, PipelinePhase::Finalizing);

        // Finalizing -> Complete
        state = reduce(state, PipelineEvent::PromptPermissionsRestored);
        assert_eq!(state.phase, PipelinePhase::Complete);
    }

    /// Test the complete finalization flow from FinalValidation through effects.
    ///
    /// This tests the orchestration + reduction path:
    /// 1. FinalValidation phase -> ValidateFinalState effect
    /// 2. ValidateFinalState effect -> FinalizingStarted event
    /// 3. FinalizingStarted event -> Finalizing phase
    /// 4. Finalizing phase -> RestorePromptPermissions effect
    /// 5. RestorePromptPermissions effect -> PromptPermissionsRestored event
    /// 6. PromptPermissionsRestored event -> Complete phase
    #[test]
    fn test_finalization_orchestration_integration() {
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        use crate::reducer::orchestration::determine_next_effect;

        // Start in FinalValidation
        let initial_state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..PipelineState::initial(5, 2)
        };

        let mut handler = MockEffectHandler::new(initial_state.clone());

        // Step 1: Determine effect for FinalValidation
        let effect1 = determine_next_effect(&initial_state);
        assert!(
            matches!(effect1, crate::reducer::effect::Effect::ValidateFinalState),
            "FinalValidation should emit ValidateFinalState effect"
        );

        // Step 2: Execute effect, get event
        let result1 = handler.execute_mock(effect1);
        assert!(
            matches!(result1.event, PipelineEvent::FinalizingStarted),
            "ValidateFinalState should return FinalizingStarted"
        );

        // Step 3: Reduce state with event
        let state2 = reduce(initial_state, result1.event);
        assert_eq!(state2.phase, PipelinePhase::Finalizing);
        assert!(!state2.is_complete(), "Finalizing should not be complete");

        // Step 4: Determine effect for Finalizing
        let effect2 = determine_next_effect(&state2);
        assert!(
            matches!(
                effect2,
                crate::reducer::effect::Effect::RestorePromptPermissions
            ),
            "Finalizing should emit RestorePromptPermissions effect"
        );

        // Step 5: Execute effect, get event
        let result2 = handler.execute_mock(effect2);
        assert!(
            matches!(result2.event, PipelineEvent::PromptPermissionsRestored),
            "RestorePromptPermissions should return PromptPermissionsRestored"
        );

        // Step 6: Reduce state with event
        let final_state = reduce(state2, result2.event);
        assert_eq!(final_state.phase, PipelinePhase::Complete);
        assert!(final_state.is_complete(), "Complete should be complete");

        // Verify effects were captured
        let effects = handler.captured_effects();
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            effects[0],
            crate::reducer::effect::Effect::ValidateFinalState
        ));
        assert!(matches!(
            effects[1],
            crate::reducer::effect::Effect::RestorePromptPermissions
        ));
    }

    // =========================================================================
    // Continuation event handling tests
    // =========================================================================

    #[test]
    fn test_continuation_triggered_updates_state() {
        use crate::reducer::state::DevelopmentStatus;

        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationContinuationTriggered {
                iteration: 1,
                status: DevelopmentStatus::Partial,
                summary: "Did work".to_string(),
                files_changed: Some(vec!["src/main.rs".to_string()]),
                next_steps: Some("Continue".to_string()),
            },
        );

        assert!(new_state.continuation.is_continuation());
        assert_eq!(
            new_state.continuation.previous_status,
            Some(DevelopmentStatus::Partial)
        );
        assert_eq!(
            new_state.continuation.previous_summary,
            Some("Did work".to_string())
        );
        assert_eq!(
            new_state.continuation.previous_files_changed,
            Some(vec!["src/main.rs".to_string()])
        );
        assert_eq!(
            new_state.continuation.previous_next_steps,
            Some("Continue".to_string())
        );
        assert_eq!(new_state.continuation.continuation_attempt, 1);
    }

    #[test]
    fn test_continuation_triggered_with_failed_status() {
        use crate::reducer::state::DevelopmentStatus;

        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationContinuationTriggered {
                iteration: 1,
                status: DevelopmentStatus::Failed,
                summary: "Build failed".to_string(),
                files_changed: None,
                next_steps: Some("Fix errors".to_string()),
            },
        );

        assert!(new_state.continuation.is_continuation());
        assert_eq!(
            new_state.continuation.previous_status,
            Some(DevelopmentStatus::Failed)
        );
        assert_eq!(
            new_state.continuation.previous_summary,
            Some("Build failed".to_string())
        );
        assert!(new_state.continuation.previous_files_changed.is_none());
    }

    #[test]
    fn test_continuation_succeeded_resets_state() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );
        assert!(state.continuation.is_continuation());

        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationContinuationSucceeded {
                iteration: 1,
                total_continuation_attempts: 2,
            },
        );

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.continuation.continuation_attempt, 0);
        assert!(new_state.continuation.previous_status.is_none());
    }

    #[test]
    fn test_iteration_started_resets_continuation() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );
        assert!(state.continuation.is_continuation());

        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationStarted { iteration: 2 },
        );

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.iteration, 2);
    }

    #[test]
    fn test_iteration_completed_resets_continuation() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );

        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 1,
                output_valid: true,
            },
        );

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_development_phase_completed_resets_continuation() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );

        let new_state = reduce(state, PipelineEvent::DevelopmentPhaseCompleted);

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.phase, PipelinePhase::Review);
    }

    #[test]
    fn test_multiple_continuation_triggers_accumulate() {
        use crate::reducer::state::DevelopmentStatus;

        let state = create_test_state();

        // First continuation
        let state = reduce(
            state,
            PipelineEvent::DevelopmentIterationContinuationTriggered {
                iteration: 1,
                status: DevelopmentStatus::Partial,
                summary: "First attempt".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );
        assert_eq!(state.continuation.continuation_attempt, 1);

        // Second continuation
        let state = reduce(
            state,
            PipelineEvent::DevelopmentIterationContinuationTriggered {
                iteration: 1,
                status: DevelopmentStatus::Partial,
                summary: "Second attempt".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );
        assert_eq!(state.continuation.continuation_attempt, 2);
        assert_eq!(
            state.continuation.previous_summary,
            Some("Second attempt".to_string())
        );
    }
}
