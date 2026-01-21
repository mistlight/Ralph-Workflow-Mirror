//! Orchestration logic for determining next effect.
//!
//! Contains `determine_next_effect()` which decides which effect to execute
//! based on current pipeline state.

use super::event::{CheckpointTrigger, PipelinePhase};
use super::state::{CommitState, PipelineState};

use crate::reducer::effect::Effect;

/// Determine the next effect to execute based on current state.
///
/// This function is pure - it only reads state and returns an effect.
/// The actual execution happens in the effect handler.
pub fn determine_next_effect(state: &PipelineState) -> Effect {
    match state.phase {
        PipelinePhase::Planning => Effect::GeneratePlan {
            iteration: state.iteration,
        },

        PipelinePhase::Development => {
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
            if state.reviewer_pass <= state.total_reviewer_passes {
                Effect::RunReviewPass {
                    pass: state.reviewer_pass,
                }
            } else {
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                }
            }
        }

        PipelinePhase::CommitMessage => match &state.commit {
            CommitState::NotStarted => Effect::GenerateCommitMessage,
            CommitState::Generating { .. } => Effect::GenerateCommitMessage,
            CommitState::Generated { message } => Effect::CreateCommit {
                message: message.clone(),
            },
            CommitState::Committed { .. } | CommitState::Skipped => Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::PhaseTransition,
            },
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

    fn create_test_state() -> PipelineState {
        PipelineState::initial(5, 2)
    }

    #[test]
    fn test_determine_effect_planning_phase() {
        let state = create_test_state();
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::GeneratePlan { .. }));
    }

    #[test]
    fn test_determine_effect_development_phase() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
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
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_review_phase() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::RunReviewPass { .. }));
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
        assert!(matches!(effect, Effect::CreateCommit { .. }));
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
