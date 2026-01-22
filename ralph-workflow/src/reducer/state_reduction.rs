//! Reducer function for state transitions.
//!
//! Implements pure state reduction - no side effects, exhaustive pattern matching.

use super::event::PipelineEvent;
use super::state::{CommitState, PipelineState, RebaseState};

/// Pure reducer - no side effects, exhaustive match.
///
/// Computes new state by applying an event to current state.
/// This function has zero side effects - all state mutations are explicit.
pub fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
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

        PipelineEvent::PlanningPhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Planning,
            ..state
        },

        PipelineEvent::PlanningPhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Development,
            ..state
        },

        PipelineEvent::DevelopmentPhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Development,
            ..state
        },

        PipelineEvent::DevelopmentIterationStarted { iteration } => PipelineState {
            iteration,
            agent_chain: state.agent_chain.reset(),
            ..state
        },

        PipelineEvent::PlanGenerationStarted { .. } => state,

        PipelineEvent::PlanGenerationCompleted { .. } => state,

        PipelineEvent::DevelopmentIterationCompleted {
            iteration,
            output_valid: _output_valid,
        } => {
            let next_iter = iteration + 1;
            let next_phase = if next_iter > state.total_iterations {
                super::event::PipelinePhase::Review
            } else {
                super::event::PipelinePhase::Development
            };

            PipelineState {
                iteration: next_iter,
                phase: next_phase,
                ..state
            }
        }

        PipelineEvent::DevelopmentPhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Review,
            ..state
        },

        PipelineEvent::ReviewPhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Review,
            reviewer_pass: 0,
            ..state
        },

        PipelineEvent::ReviewPassStarted { pass } => PipelineState {
            reviewer_pass: pass,
            agent_chain: state.agent_chain.reset(),
            ..state
        },

        PipelineEvent::ReviewCompleted { pass, .. } => PipelineState {
            reviewer_pass: pass,
            ..state
        },

        PipelineEvent::FixAttemptStarted { .. } => PipelineState {
            agent_chain: state.agent_chain.reset(),
            ..state
        },

        PipelineEvent::FixAttemptCompleted { pass, .. } => {
            let next_pass = pass + 1;
            let next_phase = if next_pass > state.total_reviewer_passes {
                super::event::PipelinePhase::CommitMessage
            } else {
                super::event::PipelinePhase::Review
            };

            PipelineState {
                phase: next_phase,
                reviewer_pass: next_pass,
                ..state
            }
        }

        PipelineEvent::ReviewPhaseCompleted { .. } => PipelineState {
            phase: super::event::PipelinePhase::CommitMessage,
            ..state
        },

        PipelineEvent::AgentInvocationFailed {
            retriable: true, ..
        } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },

        PipelineEvent::AgentFallbackTriggered { to_agent: _, .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent(),
            ..state
        },

        PipelineEvent::AgentChainExhausted { .. } => PipelineState {
            agent_chain: state.agent_chain.start_retry_cycle(),
            ..state
        },

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
            rebase: RebaseState::Conflicted {
                files,
                resolution_attempts: 0,
            },
            ..state
        },

        PipelineEvent::RebaseConflictResolved { files: _ } => PipelineState {
            rebase: match &state.rebase {
                RebaseState::Conflicted { files, .. } => RebaseState::Conflicted {
                    files: files.clone(),
                    resolution_attempts: 0,
                },
                _ => state.rebase.clone(),
            },
            ..state
        },

        PipelineEvent::RebaseSucceeded { new_head, phase: _ } => PipelineState {
            rebase: RebaseState::Completed { new_head },
            ..state
        },

        PipelineEvent::RebaseFailed { phase: _, .. } => PipelineState {
            rebase: RebaseState::NotStarted,
            ..state
        },

        PipelineEvent::RebaseSkipped { phase: _, .. } => PipelineState {
            rebase: RebaseState::Skipped,
            ..state
        },

        PipelineEvent::CommitGenerationStarted => PipelineState {
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: 3,
            },
            ..state
        },

        PipelineEvent::CommitMessageGenerated { message, .. } => PipelineState {
            commit: CommitState::Generated { message },
            ..state
        },

        PipelineEvent::CommitCreated { hash, .. } => PipelineState {
            commit: CommitState::Committed { hash },
            phase: super::event::PipelinePhase::FinalValidation,
            ..state
        },

        PipelineEvent::CommitGenerationFailed { .. } => PipelineState {
            commit: CommitState::NotStarted,
            ..state
        },

        PipelineEvent::CommitSkipped { .. } => PipelineState {
            commit: CommitState::Skipped,
            phase: super::event::PipelinePhase::FinalValidation,
            ..state
        },

        PipelineEvent::CheckpointSaved { .. } => state,

        PipelineEvent::AgentInvocationStarted { .. } => state,
        PipelineEvent::AgentInvocationSucceeded { .. } => state,
        PipelineEvent::AgentInvocationFailed {
            retriable: false, ..
        } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent(),
            ..state
        },
        PipelineEvent::AgentModelFallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        PipelineEvent::AgentRetryCycleStarted { .. } => state,
        PipelineEvent::RebaseAborted { .. } => state,
        PipelineEvent::CommitMessageValidationFailed { .. } => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRole;
    use crate::reducer::event::AgentErrorKind;
    use crate::reducer::event::PipelinePhase;
    use crate::reducer::event::RebasePhase;
    use crate::reducer::AgentChainState;

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
        assert_eq!(new_state.iteration, 3);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    }

    #[test]
    fn test_reduce_development_iteration_complete_moves_to_review() {
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
        assert_eq!(new_state.iteration, 6);
        assert_eq!(new_state.phase, PipelinePhase::Review);
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
}
