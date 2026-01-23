//! Unit tests for state reduction.
//!
//! These tests verify that the reducer correctly transitions state for each event.
//! Tests are organized by event category and follow TDD principles:
//! 1. Write a failing test that captures the expected behavior
//! 2. Verify the implementation makes it pass
//! 3. Refactor if needed

use super::state::{AgentChainState, CommitState, PipelineState, RebaseState};
use super::state_reduction::reduce;
use crate::reducer::event::{CheckpointTrigger, PipelineEvent, PipelinePhase};
use std::path::PathBuf;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

fn create_state_in_phase(phase: PipelinePhase) -> PipelineState {
    PipelineState {
        phase,
        ..create_test_state()
    }
}

// ============================================================================
// Module 1: Pipeline Lifecycle Events
// ============================================================================

mod pipeline_lifecycle_tests {
    use super::*;

    #[test]
    fn test_pipeline_started_preserves_all_state() {
        let state = create_test_state();
        let original_phase = state.phase;
        let original_iteration = state.iteration;

        let new_state = reduce(state, PipelineEvent::PipelineStarted);

        assert_eq!(new_state.phase, original_phase);
        assert_eq!(new_state.iteration, original_iteration);
    }

    #[test]
    fn test_pipeline_resumed_preserves_all_state() {
        let state = create_test_state();
        let new_state = reduce(
            state.clone(),
            PipelineEvent::PipelineResumed {
                from_checkpoint: true,
            },
        );

        assert_eq!(new_state.phase, state.phase);
        assert_eq!(new_state.iteration, state.iteration);
        assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
    }

    #[test]
    fn test_pipeline_completed_transitions_to_complete_phase() {
        let state = create_state_in_phase(PipelinePhase::FinalValidation);
        let new_state = reduce(state, PipelineEvent::PipelineCompleted);

        assert_eq!(new_state.phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_pipeline_aborted_transitions_to_interrupted() {
        let state = create_state_in_phase(PipelinePhase::Development);
        let new_state = reduce(
            state,
            PipelineEvent::PipelineAborted {
                reason: "User cancelled".to_string(),
            },
        );

        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
    }

    #[test]
    fn test_pipeline_aborted_preserves_progress() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 3,
            reviewer_pass: 0,
            ..create_test_state()
        };
        let new_state = reduce(
            state.clone(),
            PipelineEvent::PipelineAborted {
                reason: "Error".to_string(),
            },
        );

        assert_eq!(new_state.iteration, state.iteration);
        assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
    }
}

// ============================================================================
// Module 2: Planning Phase Events
// ============================================================================

mod planning_phase_tests {
    use super::*;

    #[test]
    fn test_planning_phase_started_sets_planning_phase() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::PlanningPhaseStarted);

        assert_eq!(new_state.phase, PipelinePhase::Planning);
    }

    #[test]
    fn test_planning_phase_completed_transitions_to_development() {
        let state = create_state_in_phase(PipelinePhase::Planning);
        let new_state = reduce(state, PipelineEvent::PlanningPhaseCompleted);

        assert_eq!(new_state.phase, PipelinePhase::Development);
    }

    #[test]
    fn test_plan_generation_started_is_noop() {
        let state = create_test_state();
        let new_state = reduce(
            state.clone(),
            PipelineEvent::PlanGenerationStarted { iteration: 1 },
        );

        assert_eq!(new_state.phase, state.phase);
        assert_eq!(new_state.iteration, state.iteration);
    }

    #[test]
    fn test_plan_generation_completed_transitions_to_development() {
        let state = create_state_in_phase(PipelinePhase::Planning);
        let new_state = reduce(
            state,
            PipelineEvent::PlanGenerationCompleted {
                iteration: 1,
                valid: true,
            },
        );

        assert_eq!(new_state.phase, PipelinePhase::Development);
    }
}

// ============================================================================
// Module 3: Development Phase Events
// ============================================================================

mod development_phase_tests {
    use super::*;

    #[test]
    fn test_development_phase_started_sets_development_phase() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::DevelopmentPhaseStarted);

        assert_eq!(new_state.phase, PipelinePhase::Development);
    }

    #[test]
    fn test_development_iteration_started_sets_iteration() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationStarted { iteration: 3 },
        );

        assert_eq!(new_state.iteration, 3);
    }

    #[test]
    fn test_development_iteration_started_resets_agent_chain() {
        let state = create_test_state();
        // Note: We'll test that agent_chain gets reset by checking indices
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationStarted { iteration: 1 },
        );

        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    }

    #[test]
    fn test_development_iteration_completed_increments_iteration() {
        let state = PipelineState {
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
    }

    #[test]
    fn test_development_iteration_completed_stays_in_development_when_more_iterations() {
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

        assert_eq!(new_state.phase, PipelinePhase::Development);
        assert_eq!(new_state.iteration, 3);
    }

    #[test]
    fn test_development_iteration_completed_transitions_to_review_when_done() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 4,
            total_iterations: 5,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 4,
                output_valid: true,
            },
        );

        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.iteration, 5);
    }

    #[test]
    fn test_development_iteration_completed_with_zero_total_iterations() {
        let state = PipelineState {
            iteration: 0,
            total_iterations: 0,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 0,
                output_valid: true,
            },
        );

        // 0 + 1 = 1, 1 >= 0, so should transition to Review
        assert_eq!(new_state.phase, PipelinePhase::Review);
    }

    #[test]
    fn test_development_phase_completed_transitions_to_review() {
        let state = create_state_in_phase(PipelinePhase::Development);
        let new_state = reduce(state, PipelineEvent::DevelopmentPhaseCompleted);

        assert_eq!(new_state.phase, PipelinePhase::Review);
    }
}

// ============================================================================
// Module 4: Review Phase Events (Critical for Bug Fixes)
// ============================================================================

mod review_phase_tests {
    use super::*;

    #[test]
    fn test_review_phase_started_sets_review_phase() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::ReviewPhaseStarted);

        assert_eq!(new_state.phase, PipelinePhase::Review);
    }

    #[test]
    fn test_review_phase_started_resets_reviewer_pass_to_zero() {
        let state = PipelineState {
            reviewer_pass: 5,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::ReviewPhaseStarted);

        assert_eq!(new_state.reviewer_pass, 0);
    }

    #[test]
    fn test_review_phase_started_clears_issues_flag() {
        let state = PipelineState {
            review_issues_found: true,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::ReviewPhaseStarted);

        assert!(!new_state.review_issues_found);
    }

    #[test]
    fn test_review_pass_started_sets_pass() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::ReviewPassStarted { pass: 2 });

        assert_eq!(new_state.reviewer_pass, 2);
    }

    #[test]
    fn test_review_pass_started_clears_issues_flag() {
        let state = PipelineState {
            review_issues_found: true,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::ReviewPassStarted { pass: 0 });

        assert!(!new_state.review_issues_found);
    }

    #[test]
    fn test_review_completed_with_no_issues_increments_pass() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 3,
            review_issues_found: false,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::ReviewCompleted {
                pass: 0,
                issues_found: false,
            },
        );

        assert_eq!(new_state.reviewer_pass, 1);
        assert!(!new_state.review_issues_found);
    }

    #[test]
    fn test_review_completed_with_issues_stays_on_same_pass() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 3,
            review_issues_found: false,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::ReviewCompleted {
                pass: 0,
                issues_found: true,
            },
        );

        // Should stay on pass 0 to allow fix attempt
        assert_eq!(new_state.reviewer_pass, 0);
        assert!(new_state.review_issues_found);
        assert_eq!(new_state.phase, PipelinePhase::Review);
    }

    #[test]
    fn test_review_completed_on_last_pass_with_no_issues_transitions_to_commit() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            review_issues_found: false,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::ReviewCompleted {
                pass: 1,
                issues_found: false,
            },
        );

        // 1 + 1 = 2, 2 >= 2, should transition to CommitMessage
        assert_eq!(new_state.reviewer_pass, 2);
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_review_completed_on_last_pass_with_issues_stays_in_review() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            review_issues_found: false,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::ReviewCompleted {
                pass: 1,
                issues_found: true,
            },
        );

        // Should stay on pass 1 for fix attempt
        assert_eq!(new_state.reviewer_pass, 1);
        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert!(new_state.review_issues_found);
    }

    #[test]
    fn test_fix_attempt_started_is_noop() {
        let state = PipelineState {
            review_issues_found: true,
            ..create_test_state()
        };
        let new_state = reduce(state.clone(), PipelineEvent::FixAttemptStarted { pass: 0 });

        assert_eq!(new_state.phase, state.phase);
        assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
    }

    #[test]
    fn test_fix_attempt_completed_clears_issues_flag() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: true,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 0,
                changes_made: true,
            },
        );

        assert!(!new_state.review_issues_found);
    }

    #[test]
    fn test_fix_attempt_completed_on_mid_pass_increments_and_stays_in_review() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: true,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 0,
                changes_made: true,
            },
        );

        // Fix attempt increments pass and stays in Review for next review pass
        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.reviewer_pass, 1);
        assert!(!new_state.review_issues_found); // Flag cleared after fix
    }

    #[test]
    fn test_fix_attempt_completed_on_last_pass_transitions_to_commit() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            review_issues_found: true,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 1,
                changes_made: true,
            },
        );

        // Last pass: 1 + 1 = 2, 2 >= 2, should transition to CommitMessage
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert_eq!(new_state.reviewer_pass, 2);
    }
}

// ============================================================================
// Module 5: Commit Phase Events
// ============================================================================

mod commit_phase_tests {
    use super::*;
    use crate::reducer::event::CheckpointTrigger;

    #[test]
    fn test_commit_generation_started_is_noop() {
        let state = create_test_state();
        let new_state = reduce(state.clone(), PipelineEvent::CommitGenerationStarted);

        assert_eq!(new_state.phase, state.phase);
    }

    #[test]
    fn test_commit_message_generated_sets_commit_to_generated() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitMessageGenerated {
                message: "feat: add feature".to_string(),
                attempt: 1,
            },
        );

        assert!(matches!(new_state.commit, CommitState::Generated { .. }));
    }

    #[test]
    fn test_commit_message_generated_stores_message() {
        let state = create_test_state();
        let message = "fix: resolve bug".to_string();
        let new_state = reduce(
            state,
            PipelineEvent::CommitMessageGenerated {
                message: message.clone(),
                attempt: 1,
            },
        );

        if let CommitState::Generated {
            message: stored_msg,
        } = new_state.commit
        {
            assert_eq!(stored_msg, message);
        } else {
            panic!("Expected CommitState::Generated");
        }
    }

    // Removed test_commit_message_validation_failed_is_noop - it's now test_commit_message_validation_failed_retries

    #[test]
    fn test_commit_created_sets_commit_to_committed() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: "abc123".to_string(),
                message: "feat: test".to_string(),
            },
        );

        assert!(matches!(new_state.commit, CommitState::Committed { .. }));
    }

    #[test]
    fn test_commit_created_transitions_to_final_validation() {
        let state = create_state_in_phase(PipelinePhase::CommitMessage);
        let new_state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: "abc123".to_string(),
                message: "feat: test".to_string(),
            },
        );

        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    }

    #[test]
    fn test_commit_created_stores_hash() {
        let state = create_test_state();
        let hash = "abc123def456".to_string();
        let new_state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: hash.clone(),
                message: "test".to_string(),
            },
        );

        if let CommitState::Committed { hash: stored_hash } = new_state.commit {
            assert_eq!(stored_hash, hash);
        } else {
            panic!("Expected CommitState::Committed");
        }
    }

    #[test]
    fn test_commit_message_validation_failed_retries() {
        let state = PipelineState {
            commit: CommitState::Generated {
                message: "test".to_string(),
            },
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::CommitMessageValidationFailed {
                reason: "Invalid format".to_string(),
                attempt: 1,
            },
        );

        // Should retry with incremented attempt
        assert!(matches!(
            new_state.commit,
            CommitState::Generating { attempt: 2, .. }
        ));
    }

    #[test]
    fn test_commit_message_validation_failed_exhausts_all_agents() {
        use super::super::state::MAX_VALIDATION_RETRY_ATTEMPTS;
        use crate::agents::AgentRole;

        // Setup: On last agent (index 2 of 3 agents)
        let base_state = create_test_state();
        let state = PipelineState {
            agent_chain: base_state
                .agent_chain
                .with_agents(
                    vec![
                        "commit-agent-1".to_string(),
                        "commit-agent-2".to_string(),
                        "commit-agent-3".to_string(),
                    ],
                    vec![vec![], vec![], vec![]],
                    AgentRole::Commit,
                )
                .switch_to_next_agent()
                .switch_to_next_agent(), // Move to last agent (index 2)
            commit: CommitState::Generating {
                attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
                max_attempts: MAX_VALIDATION_RETRY_ATTEMPTS,
            },
            ..base_state
        };

        // Verify we're on the last agent and retry_cycle is 0
        assert_eq!(state.agent_chain.current_agent_index, 2);
        assert_eq!(state.agent_chain.retry_cycle, 0);

        let new_state = reduce(
            state,
            PipelineEvent::CommitMessageValidationFailed {
                reason: "Invalid format".to_string(),
                attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
            },
        );

        // When we try to advance from last agent, switch_to_next_agent() wraps around:
        // - Index wraps back to 0
        // - Retry cycle increments to 1
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.retry_cycle, 1);

        // Since we wrapped around (exhausted all agents in this cycle), should give up
        assert!(matches!(new_state.commit, CommitState::NotStarted));
    }

    #[test]
    fn test_commit_message_validation_failed_with_single_agent() {
        use super::super::state::MAX_VALIDATION_RETRY_ATTEMPTS;
        use crate::agents::AgentRole;

        // Setup: Only 1 commit agent
        let base_state = create_test_state();
        let state = PipelineState {
            agent_chain: base_state.agent_chain.with_agents(
                vec!["commit-agent-1".to_string()],
                vec![vec![]],
                AgentRole::Commit,
            ),
            commit: CommitState::Generating {
                attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
                max_attempts: MAX_VALIDATION_RETRY_ATTEMPTS,
            },
            ..base_state
        };

        let new_state = reduce(
            state,
            PipelineEvent::CommitMessageValidationFailed {
                reason: "Invalid format".to_string(),
                attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
            },
        );

        // No more agents to fallback to - should give up
        assert!(matches!(new_state.commit, CommitState::NotStarted));
    }

    #[test]
    fn test_commit_skipped_sets_commit_to_skipped() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "No changes".to_string(),
            },
        );

        assert!(matches!(new_state.commit, CommitState::Skipped));
    }

    #[test]
    fn test_commit_skipped_transitions_to_final_validation() {
        let state = create_state_in_phase(PipelinePhase::CommitMessage);
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "No changes".to_string(),
            },
        );

        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    }

    #[test]
    fn test_checkpoint_saved_preserves_all_state() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 3,
            reviewer_pass: 1,
            commit: CommitState::Generated {
                message: "test".to_string(),
            },
            ..create_test_state()
        };
        let new_state = reduce(
            state.clone(),
            PipelineEvent::CheckpointSaved {
                trigger: CheckpointTrigger::PhaseTransition,
            },
        );

        assert_eq!(new_state.phase, state.phase);
        assert_eq!(new_state.iteration, state.iteration);
        assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
        assert!(matches!(new_state.commit, CommitState::Generated { .. }));
    }
}

// ============================================================================
// Module 6: Agent Chain Events
// ============================================================================

mod agent_chain_tests {
    use super::*;
    use crate::agents::AgentRole;

    #[test]
    fn test_agent_chain_initialized_for_developer() {
        let state = create_test_state();
        let agents = vec!["agent1".to_string(), "agent2".to_string()];

        let new_state = reduce(
            state,
            PipelineEvent::AgentChainInitialized {
                agents: agents.clone(),
                role: AgentRole::Developer,
            },
        );

        assert_eq!(new_state.agent_chain.agents, agents);
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    }

    #[test]
    fn test_agent_chain_initialized_for_reviewer() {
        let state = create_test_state();
        let agents = vec!["reviewer1".to_string()];

        let new_state = reduce(
            state,
            PipelineEvent::AgentChainInitialized {
                agents: agents.clone(),
                role: AgentRole::Reviewer,
            },
        );

        assert_eq!(new_state.agent_chain.agents, agents);
    }

    #[test]
    fn test_agent_invocation_started_resets_agent_chain() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationStarted {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                model: Some("model1".to_string()),
            },
        );

        // AgentInvocationStarted resets the agent chain
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    }

    #[test]
    fn test_agent_invocation_succeeded_preserves_indices() {
        let state = create_test_state();
        let new_state = reduce(
            state.clone(),
            PipelineEvent::AgentInvocationSucceeded {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
            },
        );

        assert_eq!(
            new_state.agent_chain.current_agent_index,
            state.agent_chain.current_agent_index
        );
        assert_eq!(
            new_state.agent_chain.current_model_index,
            state.agent_chain.current_model_index
        );
    }

    #[test]
    fn test_agent_invocation_failed_with_retriable_advances_model() {
        let base_state = create_test_state();
        let state = PipelineState {
            agent_chain: base_state.agent_chain.with_agents(
                vec!["agent1".to_string()],
                vec![vec!["model1".to_string(), "model2".to_string()]],
                AgentRole::Developer,
            ),
            ..base_state
        };

        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: crate::reducer::event::AgentErrorKind::Timeout,
                retriable: true,
            },
        );

        // Should advance to next model (0 -> 1)
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.current_model_index, 1);
    }

    #[test]
    fn test_agent_fallback_triggered_switches_agent() {
        let base_state = create_test_state();
        let state = PipelineState {
            agent_chain: base_state.agent_chain.with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec!["model1".to_string()], vec!["model2".to_string()]],
                AgentRole::Developer,
            ),
            ..base_state
        };

        let new_state = reduce(
            state,
            PipelineEvent::AgentFallbackTriggered {
                role: AgentRole::Developer,
                from_agent: "agent1".to_string(),
                to_agent: "agent2".to_string(),
            },
        );

        // Should switch to next agent (0 -> 1) and reset model (0)
        assert_eq!(new_state.agent_chain.current_agent_index, 1);
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    }

    #[test]
    fn test_agent_chain_exhausted_increments_retry_cycle() {
        let state = create_test_state();
        let initial_retry_cycle = state.agent_chain.retry_cycle;

        let new_state = reduce(
            state,
            PipelineEvent::AgentChainExhausted {
                role: AgentRole::Developer,
            },
        );

        assert_eq!(new_state.agent_chain.retry_cycle, initial_retry_cycle + 1);
    }

    #[test]
    fn test_agent_chain_exhausted_resets_indices() {
        let base_state = create_test_state();
        let state = PipelineState {
            agent_chain: base_state.agent_chain.with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![
                    vec!["model1".to_string(), "model2".to_string()],
                    vec!["model3".to_string()],
                ],
                AgentRole::Developer,
            ),
            ..base_state
        };

        // Manually set indices to non-zero
        let state = PipelineState {
            agent_chain: state
                .agent_chain
                .advance_to_next_model()
                .switch_to_next_agent(),
            ..state
        };

        let new_state = reduce(
            state,
            PipelineEvent::AgentChainExhausted {
                role: AgentRole::Developer,
            },
        );

        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    }
}

// ============================================================================
// Module 7: Rebase Events
// ============================================================================

mod rebase_tests {
    use super::*;
    use crate::reducer::event::RebasePhase;
    use std::path::PathBuf;

    #[test]
    fn test_rebase_started_sets_in_progress() {
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
    fn test_rebase_started_stores_target_branch() {
        let state = create_test_state();
        let target = "develop".to_string();
        let new_state = reduce(
            state,
            PipelineEvent::RebaseStarted {
                phase: RebasePhase::Initial,
                target_branch: target.clone(),
            },
        );

        if let RebaseState::InProgress {
            target_branch,
            original_head: _,
        } = new_state.rebase
        {
            assert_eq!(target_branch, target);
        } else {
            panic!("Expected RebaseState::InProgress");
        }
    }

    #[test]
    fn test_rebase_conflict_detected_transitions_to_conflicted() {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseConflictDetected {
                files: vec![PathBuf::from("file.rs")],
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::Conflicted { .. }));
    }

    #[test]
    fn test_rebase_conflict_detected_stores_files() {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_test_state()
        };
        let files = vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")];
        let new_state = reduce(
            state,
            PipelineEvent::RebaseConflictDetected {
                files: files.clone(),
            },
        );

        if let RebaseState::Conflicted {
            target_branch: _,
            original_head: _,
            files: stored_files,
            resolution_attempts: _,
        } = new_state.rebase
        {
            assert_eq!(stored_files, files);
        } else {
            panic!("Expected RebaseState::Conflicted");
        }
    }

    #[test]
    fn test_rebase_conflict_resolved_transitions_to_in_progress() {
        let state = PipelineState {
            rebase: RebaseState::Conflicted {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
                files: vec![PathBuf::from("file.rs")],
                resolution_attempts: 0,
            },
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseConflictResolved {
                files: vec![PathBuf::from("file.rs")],
            },
        );

        // After resolving conflict, should transition back to InProgress
        if let RebaseState::InProgress {
            target_branch,
            original_head,
        } = new_state.rebase
        {
            assert_eq!(target_branch, "main");
            assert_eq!(original_head, "abc123");
        } else {
            panic!(
                "Expected RebaseState::InProgress, got {:?}",
                new_state.rebase
            );
        }
    }

    #[test]
    fn test_rebase_succeeded_transitions_to_completed() {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_test_state()
        };
        let new_head_hash = "def456".to_string();
        let new_state = reduce(
            state,
            PipelineEvent::RebaseSucceeded {
                phase: RebasePhase::Initial,
                new_head: new_head_hash.clone(),
            },
        );

        if let RebaseState::Completed { new_head } = new_state.rebase {
            assert_eq!(new_head, new_head_hash);
        } else {
            panic!(
                "Expected RebaseState::Completed, got {:?}",
                new_state.rebase
            );
        }
    }

    #[test]
    fn test_rebase_failed_resets_to_not_started() {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseFailed {
                phase: RebasePhase::Initial,
                reason: "Merge conflict".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::NotStarted));
    }

    #[test]
    fn test_rebase_aborted_is_noop() {
        let state = PipelineState {
            rebase: RebaseState::Conflicted {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
                files: vec![PathBuf::from("file.rs")],
                resolution_attempts: 2,
            },
            ..create_test_state()
        };
        let new_state = reduce(
            state.clone(),
            PipelineEvent::RebaseAborted {
                phase: RebasePhase::Initial,
                restored_to: "abc123".to_string(),
            },
        );

        // RebaseAborted is currently a no-op - state is preserved
        assert!(matches!(new_state.rebase, RebaseState::Conflicted { .. }));
    }

    #[test]
    fn test_rebase_skipped_transitions_to_skipped() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::RebaseSkipped {
                phase: RebasePhase::Initial,
                reason: "Not needed".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::Skipped));
    }
}
