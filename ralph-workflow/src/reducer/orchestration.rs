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

            // After development iteration completes, clean up context (PLAN.md, etc.)
            if !state.context_cleaned && state.iteration > 0 {
                return Effect::CleanupContext;
            }

            if state.iteration < state.total_iterations {
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

            // If review found issues, run fix attempt
            if state.review_issues_found {
                return Effect::RunFixAttempt {
                    pass: state.reviewer_pass,
                };
            }

            // Otherwise, run next review pass or complete phase
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
            CommitState::Generated { ref message } => Effect::CreateCommit {
                message: message.clone(),
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
    use crate::reducer::{reduce, PipelineEvent};

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
    fn test_planning_phase_transitions_to_development_after_completion() {
        // Create state in Planning phase with agents initialized
        let mut state = PipelineState {
            phase: PipelinePhase::Planning,
            iteration: 1,
            total_iterations: 5,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };

        // Simulate plan generation completing
        state = reduce(
            state,
            PipelineEvent::PlanGenerationCompleted {
                iteration: 1,
                valid: true,
            },
        );

        // After plan generation completes, phase should transition to Development
        assert_eq!(
            state.phase,
            PipelinePhase::Development,
            "Phase should transition to Development after PlanGenerationCompleted"
        );

        // Orchestration should now return RunDevelopmentIteration, not GeneratePlan
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunDevelopmentIteration { .. }),
            "Expected RunDevelopmentIteration, got {:?}",
            effect
        );
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
    fn test_development_runs_exactly_n_iterations() {
        // When total_iterations=5, should run iterations 0,1,2,3,4 (5 total)
        let mut state = PipelineState::initial(5, 0);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Track which iterations actually run
        let mut iterations_run = Vec::new();

        // Simulate the development phase
        while state.phase == PipelinePhase::Planning || state.phase == PipelinePhase::Development {
            let effect = determine_next_effect(&state);

            match effect {
                Effect::GeneratePlan { iteration } => {
                    // Complete planning
                    state = reduce(
                        state,
                        PipelineEvent::PlanGenerationCompleted {
                            iteration,
                            valid: true,
                        },
                    );
                }
                Effect::RunDevelopmentIteration { iteration } => {
                    iterations_run.push(iteration);
                    // Complete the iteration
                    state = reduce(
                        state,
                        PipelineEvent::DevelopmentIterationCompleted {
                            iteration,
                            output_valid: true,
                        },
                    );
                }
                Effect::SaveCheckpoint { .. } => {
                    // Phase complete
                    break;
                }
                Effect::InitializeAgentChain { .. } => {
                    state = reduce(
                        state,
                        PipelineEvent::AgentChainInitialized {
                            role: AgentRole::Developer,
                            agents: vec!["claude".to_string()],
                        },
                    );
                }
                _ => panic!("Unexpected effect: {:?}", effect),
            }
        }

        // Should run exactly 5 iterations (0,1,2,3,4), not 6 (0,1,2,3,4,5)
        assert_eq!(
            iterations_run.len(),
            5,
            "Should run exactly 5 iterations, ran: {:?}",
            iterations_run
        );
        assert_eq!(
            iterations_run,
            vec![0, 1, 2, 3, 4],
            "Should run iterations 0-4"
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Should transition to Review after 5 iterations"
        );
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
    fn test_review_triggers_fix_when_issues_found() {
        // Create state in Review phase with issues found
        let mut state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: false,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };

        // Initially should run review pass
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunReviewPass { pass: 0 }),
            "Expected RunReviewPass, got {:?}",
            effect
        );

        // Simulate review completing with issues found
        state = reduce(
            state,
            PipelineEvent::ReviewCompleted {
                pass: 0,
                issues_found: true,
            },
        );

        // State should now have issues_found flag set
        assert!(
            state.review_issues_found,
            "review_issues_found should be true"
        );

        // Orchestration should now trigger fix attempt
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunFixAttempt { pass: 0 }),
            "Expected RunFixAttempt after issues found, got {:?}",
            effect
        );

        // After fix completes, flag should be reset and move to next pass
        state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 0,
                changes_made: true,
            },
        );

        assert!(
            !state.review_issues_found,
            "review_issues_found should be reset after fix"
        );
        assert_eq!(state.reviewer_pass, 1, "Should increment to next pass");
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Should stay in Review phase"
        );
    }

    #[test]
    fn test_complete_pipeline_flow_with_planning_dev_review_commit() {
        // Test the COMPLETE flow: Planning → Development → Review → Fix → Commit → FinalValidation
        let mut state = PipelineState::initial(2, 1); // 2 dev iterations, 1 review pass
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let mut phase_sequence = Vec::new();
        let mut iterations_run = Vec::new();
        let mut review_passes_run = Vec::new();

        // Simulate complete pipeline execution
        let max_steps = 50; // Safety limit to prevent infinite loops
        for step in 0..max_steps {
            phase_sequence.push(state.phase);
            let effect = determine_next_effect(&state);

            match effect {
                Effect::InitializeAgentChain { role } => {
                    state = reduce(
                        state,
                        PipelineEvent::AgentChainInitialized {
                            role,
                            agents: vec!["claude".to_string()],
                        },
                    );
                }
                Effect::GeneratePlan { iteration } => {
                    state = reduce(
                        state,
                        PipelineEvent::PlanGenerationCompleted {
                            iteration,
                            valid: true,
                        },
                    );
                }
                Effect::RunDevelopmentIteration { iteration } => {
                    iterations_run.push(iteration);
                    state = reduce(
                        state,
                        PipelineEvent::DevelopmentIterationCompleted {
                            iteration,
                            output_valid: true,
                        },
                    );
                }
                Effect::RunReviewPass { pass } => {
                    review_passes_run.push(pass);
                    state = reduce(
                        state,
                        PipelineEvent::ReviewCompleted {
                            pass,
                            issues_found: true, // Simulate finding issues
                        },
                    );
                }
                Effect::RunFixAttempt { pass } => {
                    state = reduce(
                        state,
                        PipelineEvent::FixAttemptCompleted {
                            pass,
                            changes_made: true,
                        },
                    );
                }
                Effect::GenerateCommitMessage => {
                    state = reduce(
                        state,
                        PipelineEvent::CommitMessageGenerated {
                            message: "test commit".to_string(),
                            attempt: 1,
                        },
                    );
                }
                Effect::CreateCommit { .. } => {
                    state = reduce(
                        state,
                        PipelineEvent::CommitCreated {
                            hash: "abc123".to_string(),
                            message: "test commit".to_string(),
                        },
                    );
                }
                Effect::ValidateFinalState => {
                    state = reduce(state, PipelineEvent::PipelineCompleted);
                }
                Effect::SaveCheckpoint { .. } => {
                    // Phase transition checkpoint - continue
                    if state.phase == PipelinePhase::Complete {
                        break;
                    }
                }
                _ => panic!("Unexpected effect at step {}: {:?}", step, effect),
            }

            if state.phase == PipelinePhase::Complete {
                break;
            }
        }

        // Verify the complete flow
        assert_eq!(
            iterations_run,
            vec![0, 1],
            "Should run exactly 2 development iterations"
        );
        assert_eq!(
            review_passes_run,
            vec![0],
            "Should run exactly 1 review pass"
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Complete,
            "Pipeline should complete"
        );

        // Verify phase progression
        assert!(
            phase_sequence.contains(&PipelinePhase::Planning),
            "Should go through Planning"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::Development),
            "Should go through Development"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::Review),
            "Should go through Review"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::CommitMessage),
            "Should go through CommitMessage"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::FinalValidation),
            "Should go through FinalValidation"
        );
    }

    #[test]
    fn test_pipeline_flow_skip_planning_when_zero_iterations() {
        // When developer_iters=0, should skip Planning and Development entirely
        let mut state = PipelineState::initial(0, 2); // 0 dev iterations, 2 review passes
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Should start in Review when developer_iters=0"
        );

        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        let mut review_passes = Vec::new();
        let max_steps = 30;

        for _ in 0..max_steps {
            let effect = determine_next_effect(&state);

            match effect {
                Effect::InitializeAgentChain { role } => {
                    state = reduce(
                        state,
                        PipelineEvent::AgentChainInitialized {
                            role,
                            agents: vec!["claude".to_string()],
                        },
                    );
                }
                Effect::RunReviewPass { pass } => {
                    review_passes.push(pass);
                    state = reduce(
                        state,
                        PipelineEvent::ReviewCompleted {
                            pass,
                            issues_found: false, // No issues
                        },
                    );
                }
                Effect::GenerateCommitMessage => {
                    state = reduce(
                        state,
                        PipelineEvent::CommitMessageGenerated {
                            message: "test".to_string(),
                            attempt: 1,
                        },
                    );
                }
                Effect::CreateCommit { .. } => {
                    state = reduce(
                        state,
                        PipelineEvent::CommitCreated {
                            hash: "abc".to_string(),
                            message: "test".to_string(),
                        },
                    );
                }
                Effect::ValidateFinalState => {
                    state = reduce(state, PipelineEvent::PipelineCompleted);
                    break;
                }
                Effect::SaveCheckpoint { .. } => {
                    if state.phase == PipelinePhase::Complete {
                        break;
                    }
                }
                _ => panic!("Unexpected effect: {:?}", effect),
            }
        }

        assert_eq!(review_passes, vec![0, 1], "Should run 2 review passes");
        assert_eq!(state.phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_review_runs_exactly_n_passes() {
        // Similar to development iteration test, verify review passes count
        let mut state = PipelineState::initial(0, 3); // 0 dev, 3 review passes
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        let mut passes_run = Vec::new();
        let max_steps = 30;

        for _ in 0..max_steps {
            let effect = determine_next_effect(&state);

            match effect {
                Effect::InitializeAgentChain { role } => {
                    state = reduce(
                        state,
                        PipelineEvent::AgentChainInitialized {
                            role,
                            agents: vec!["claude".to_string()],
                        },
                    );
                }
                Effect::RunReviewPass { pass } => {
                    passes_run.push(pass);
                    state = reduce(
                        state,
                        PipelineEvent::ReviewCompleted {
                            pass,
                            issues_found: false, // No issues, no fix needed
                        },
                    );
                }
                Effect::SaveCheckpoint { .. } => {
                    // Review complete
                    break;
                }
                _ => break,
            }
        }

        assert_eq!(
            passes_run.len(),
            3,
            "Should run exactly 3 review passes, ran: {:?}",
            passes_run
        );
        assert_eq!(passes_run, vec![0, 1, 2], "Should run passes 0-2");
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Should transition to CommitMessage after reviews"
        );
    }

    #[test]
    fn test_review_skips_fix_when_no_issues() {
        // Create state in Review phase
        let mut state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: false,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };

        // Run review pass
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::RunReviewPass { pass: 0 }));

        // Review completes with NO issues
        state = reduce(
            state,
            PipelineEvent::ReviewCompleted {
                pass: 0,
                issues_found: false,
            },
        );

        assert!(
            !state.review_issues_found,
            "review_issues_found should be false"
        );

        assert_eq!(
            state.reviewer_pass, 1,
            "Should increment to next pass when no issues"
        );

        // Should run next review pass (pass 1), NOT fix attempt
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunReviewPass { pass: 1 }),
            "Expected RunReviewPass pass 1 when no issues, got {:?}",
            effect
        );
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
                message: "test commit message".to_string(),
            },
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        match effect {
            Effect::CreateCommit { message } => {
                assert_eq!(message, "test commit message");
            }
            _ => panic!("Expected CreateCommit effect, got {:?}", effect),
        }
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
