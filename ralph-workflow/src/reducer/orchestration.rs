//! Orchestration logic for determining next effect.
//!
//! Contains `determine_next_effect()` which decides which effect to execute
//! based on current pipeline state.

use super::event::{CheckpointTrigger, PipelinePhase};
use super::state::{CommitState, PipelineState};

use crate::agents::AgentRole;
use crate::reducer::effect::Effect;

/// Determine the next effect to execute based on current state.
///
/// This function is pure - it only reads state and returns an effect.
/// The actual execution happens in the effect handler.
pub fn determine_next_effect(state: &PipelineState) -> Effect {
    match state.phase {
        PipelinePhase::Planning => {
            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Developer,
                };
            }
            Effect::GeneratePlan {
                iteration: state.iteration,
            }
        }

        PipelinePhase::Development => {
            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Developer,
                };
            }
            if state.agent_chain.is_exhausted() {
                return Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                };
            }
            if state.iteration <= state.total_iterations {
                Effect::RunDevelopmentIteration {
                    iteration: state.iteration,
                }
            } else {
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                }
            }
        }

        PipelinePhase::Review => {
            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Reviewer,
                };
            }
            if state.agent_chain.is_exhausted() {
                return Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                };
            }
            if state.reviewer_pass < state.total_reviewer_passes {
                Effect::RunReviewPass {
                    pass: state.reviewer_pass,
                }
            } else {
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                }
            }
        }

        PipelinePhase::CommitMessage => match state.commit {
            CommitState::NotStarted => Effect::GenerateCommitMessage,
            CommitState::Generated { .. } => Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::PhaseTransition,
            },
            CommitState::Committed { .. } | CommitState::Skipped => Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::PhaseTransition,
            },
            CommitState::Generating { .. } => Effect::GenerateCommitMessage,
        },

        PipelinePhase::FinalValidation => Effect::ValidateFinalState,

        PipelinePhase::Complete | PipelinePhase::Interrupted => Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::state::AgentChainState;

    fn create_test_state() -> PipelineState {
        PipelineState::initial(5, 2)
    }

    #[test]
    fn test_determine_effect_planning_phase() {
        let state = create_test_state();
        let effect = determine_next_effect(&state);
        assert!(matches!(
            effect,
            Effect::InitializeAgentChain {
                role: AgentRole::Developer
            }
        ));
    }

    #[test]
    fn test_determine_effect_planning_with_agents() {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::GeneratePlan { .. }));
    }

    #[test]
    fn test_initial_state_skips_planning_when_zero_developer_iters() {
        // When developer_iters=0, the initial state should skip Planning phase entirely
        let state = PipelineState::initial(0, 2);
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Initial phase should be Review when developer_iters=0 and reviewer_reviews>0"
        );
    }

    #[test]
    fn test_initial_state_skips_to_commit_when_zero_iters_and_reviews() {
        // When both developer_iters=0 and reviewer_reviews=0, skip to CommitMessage
        let state = PipelineState::initial(0, 0);
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Initial phase should be CommitMessage when developer_iters=0 and reviewer_reviews=0"
        );
    }

    #[test]
    fn test_initial_state_starts_planning_when_developer_iters_nonzero() {
        // When developer_iters>0, start in Planning phase as normal
        let state = PipelineState::initial(1, 0);
        assert_eq!(
            state.phase,
            PipelinePhase::Planning,
            "Initial phase should be Planning when developer_iters>0"
        );
    }

    #[test]
    fn test_determine_effect_development_phase_empty_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            agent_chain: AgentChainState::initial(),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(
            effect,
            Effect::InitializeAgentChain {
                role: AgentRole::Developer
            }
        ));
    }

    #[test]
    fn test_determine_effect_development_phase_exhausted_chain() {
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            agent_chain: chain,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_development_phase_with_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::RunDevelopmentIteration { .. }));
    }

    #[test]
    fn test_determine_effect_development_complete() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 6,
            total_iterations: 5,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_review_phase_empty_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            agent_chain: AgentChainState::initial(),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(
            effect,
            Effect::InitializeAgentChain {
                role: AgentRole::Reviewer
            }
        ));
    }

    #[test]
    fn test_determine_effect_review_phase_exhausted_chain() {
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            )
            .with_max_cycles(3);
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            agent_chain: chain,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_review_phase_with_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::RunReviewPass { .. }));
    }

    #[test]
    fn test_determine_effect_review_complete() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 2,
            total_reviewer_passes: 2,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_commit_message_not_started() {
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::NotStarted,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::GenerateCommitMessage));
    }

    #[test]
    fn test_determine_effect_commit_message_generated() {
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Generated {
                message: "test".to_string(),
            },
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_final_validation() {
        let state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::ValidateFinalState));
    }

    #[test]
    fn test_determine_effect_complete() {
        let state = PipelineState {
            phase: PipelinePhase::Complete,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }
}
