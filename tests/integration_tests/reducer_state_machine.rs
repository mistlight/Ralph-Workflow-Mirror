//! Reducer state machine integration tests.
//!
//! These tests verify that reducer state machine handles all transitions correctly
//! in real pipeline execution. Tests verify actual state changes through event
//! emission and reduce() function, not just unit tests of individual transitions.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (state machine transitions)
//! - Tests are deterministic and isolated
//! - Tests verify that reducer produces correct state for each event

use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::event::{
    AgentErrorKind, CheckpointTrigger, PipelineEvent, PipelinePhase, RebasePhase,
};
use ralph_workflow::reducer::state::{AgentChainState, CommitState, PipelineState, RebaseState};

use crate::test_timeout::with_default_timeout;

fn create_initial_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

fn create_state_with_agent_chain() -> PipelineState {
    PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        ),
        ..PipelineState::initial(5, 2)
    }
}

fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
    ralph_workflow::reducer::state_reduction::reduce(state, event)
}

#[test]
fn test_planning_to_development_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::PlanningPhaseCompleted);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_development_phase_starts() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::DevelopmentPhaseStarted);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_development_to_review_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::DevelopmentPhaseCompleted);
        assert_eq!(new_state.phase, PipelinePhase::Review);
    });
}

#[test]
fn test_review_phase_starts() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::ReviewPhaseStarted);
        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.reviewer_pass, 0);
    });
}

#[test]
fn test_review_to_commit_message_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::ReviewPhaseCompleted { early_exit: false },
        );
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    });
}

#[test]
fn test_commit_message_to_final_validation_on_commit() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: "abc123".to_string(),
                message: "test commit".to_string(),
            },
        );
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    });
}

#[test]
fn test_commit_message_to_final_validation_on_skip() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "no changes".to_string(),
            },
        );
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    });
}

#[test]
fn test_pipeline_complete_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::PipelineCompleted);
        assert_eq!(new_state.phase, PipelinePhase::Complete);
    });
}

#[test]
fn test_development_iteration_increments() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 2,
                output_valid: true,
            },
        );
        assert_eq!(new_state.iteration, 3);
    });
}

#[test]
fn test_development_iteration_complete_moves_to_review() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 5,
            total_iterations: 5,
            ..create_initial_state()
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
    });
}

#[test]
fn test_development_iteration_started_resets_agent_chain() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain();
        state = reduce(
            state,
            PipelineEvent::AgentFallbackTriggered {
                role: AgentRole::Developer,
                from_agent: "agent1".to_string(),
                to_agent: "agent2".to_string(),
            },
        );
        assert_eq!(state.agent_chain.current_agent().unwrap(), "agent2");
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationStarted { iteration: 2 },
        );
        assert_eq!(new_state.iteration, 2);
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
    });
}

#[test]
fn test_review_pass_increments() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 1,
                changes_made: true,
            },
        );
        assert_eq!(new_state.reviewer_pass, 2);
    });
}

#[test]
fn test_review_pass_complete_moves_to_commit_message() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 2,
            total_reviewer_passes: 2,
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 2,
                changes_made: true,
            },
        );
        assert_eq!(new_state.reviewer_pass, 3);
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    });
}

#[test]
fn test_agent_chain_resets_on_new_iteration() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain();
        state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );
        assert_eq!(state.agent_chain.current_model_index, 1);
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationStarted { iteration: 2 },
        );
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    });
}

#[test]
fn test_agent_chain_advances_on_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
        assert_eq!(new_state.agent_chain.current_model_index, 1);
    });
}

#[test]
fn test_agent_chain_advances_on_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Authentication,
                retriable: false,
            },
        );
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent2");
    });
}

#[test]
fn test_agent_chain_handles_model_fallback_event() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let new_state = reduce(
            state,
            PipelineEvent::AgentModelFallbackTriggered {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                from_model: "model1".to_string(),
                to_model: "model2".to_string(),
            },
        );
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
        assert_eq!(new_state.agent_chain.current_model_index, 1);
    });
}

#[test]
fn test_agent_chain_handles_agent_fallback_event() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let new_state = reduce(
            state,
            PipelineEvent::AgentFallbackTriggered {
                role: AgentRole::Developer,
                from_agent: "agent1".to_string(),
                to_agent: "agent2".to_string(),
            },
        );
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent2");
    });
}

#[test]
fn test_agent_chain_starts_retry_cycle_on_exhaustion() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let new_state = reduce(
            state,
            PipelineEvent::AgentChainExhausted {
                role: AgentRole::Developer,
            },
        );
        assert_eq!(new_state.agent_chain.retry_cycle, 1);
    });
}

#[test]
fn test_agent_chain_resets_on_review_pass_started() {
    with_default_timeout(|| {
        let mut state = PipelineState {
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec!["model1".to_string(), "model2".to_string()]],
                AgentRole::Reviewer,
            ),
            ..create_initial_state()
        };
        state = reduce(
            state,
            PipelineEvent::AgentFallbackTriggered {
                role: AgentRole::Reviewer,
                from_agent: "agent1".to_string(),
                to_agent: "agent2".to_string(),
            },
        );
        assert_eq!(state.agent_chain.current_agent().unwrap(), "agent2");
        let new_state = reduce(state, PipelineEvent::ReviewPassStarted { pass: 2 });
        assert_eq!(new_state.reviewer_pass, 2);
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
    });
}

#[test]
fn test_agent_chain_resets_on_fix_attempt_started() {
    with_default_timeout(|| {
        let mut state = PipelineState {
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec!["model1".to_string(), "model2".to_string()]],
                AgentRole::Reviewer,
            ),
            reviewer_pass: 1,
            ..create_initial_state()
        };
        state = reduce(
            state,
            PipelineEvent::AgentFallbackTriggered {
                role: AgentRole::Reviewer,
                from_agent: "agent1".to_string(),
                to_agent: "agent2".to_string(),
            },
        );
        assert_eq!(state.agent_chain.current_agent().unwrap(), "agent2");
        let new_state = reduce(state, PipelineEvent::FixAttemptStarted { pass: 1 });
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
    });
}

#[test]
fn test_rebase_started_transitions_to_in_progress() {
    with_default_timeout(|| {
        let state = PipelineState {
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseStarted {
                phase: RebasePhase::Initial,
                target_branch: "main".to_string(),
            },
        );
        assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
    });
}

#[test]
fn test_rebase_conflict_detected_transitions_to_conflicted() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::RebaseConflictDetected {
                files: vec!["file1.txt".into(), "file2.txt".into()],
            },
        );
        if let RebaseState::Conflicted { files, .. } = &new_state.rebase {
            assert_eq!(files.len(), 2);
        } else {
            panic!("Expected Conflicted state");
        }
    });
}

#[test]
fn test_rebase_succeeded_transitions_to_completed() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseSucceeded {
                phase: RebasePhase::Initial,
                new_head: "def456".to_string(),
            },
        );
        assert!(matches!(new_state.rebase, RebaseState::Completed { .. }));
    });
}

#[test]
fn test_rebase_failed_transitions_to_not_started() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseFailed {
                phase: RebasePhase::Initial,
                reason: "conflict".to_string(),
            },
        );
        assert!(matches!(new_state.rebase, RebaseState::NotStarted));
    });
}

#[test]
fn test_rebase_skipped_transitions_to_skipped() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseSkipped {
                phase: RebasePhase::Initial,
                reason: "up to date".to_string(),
            },
        );
        assert!(matches!(new_state.rebase, RebaseState::Skipped));
    });
}

#[test]
fn test_rebase_aborted_leaves_state_unchanged() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::RebaseAborted {
                phase: RebasePhase::Initial,
                restored_to: "abc123".to_string(),
            },
        );
        assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
    });
}

#[test]
fn test_commit_generation_started_transitions_to_generating() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::CommitGenerationStarted);
        if let CommitState::Generating {
            attempt,
            max_attempts,
        } = new_state.commit
        {
            assert_eq!(attempt, 1);
            assert_eq!(max_attempts, 3);
        } else {
            panic!("Expected Generating state");
        }
    });
}

#[test]
fn test_commit_message_generated_transitions_to_generated() {
    with_default_timeout(|| {
        let state = PipelineState {
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: 3,
            },
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::CommitMessageGenerated {
                message: "test commit".to_string(),
                attempt: 1,
            },
        );
        assert!(matches!(new_state.commit, CommitState::Generated { .. }));
    });
}

#[test]
fn test_commit_created_transitions_to_committed() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: "abc123".to_string(),
                message: "test commit".to_string(),
            },
        );
        assert!(matches!(new_state.commit, CommitState::Committed { .. }));
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    });
}

#[test]
fn test_commit_generation_failed_transitions_to_not_started() {
    with_default_timeout(|| {
        let state = PipelineState {
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: 3,
            },
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::CommitGenerationFailed {
                reason: "error".to_string(),
            },
        );
        assert!(matches!(new_state.commit, CommitState::NotStarted));
    });
}

#[test]
fn test_commit_skipped_transitions_to_skipped() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "no changes".to_string(),
            },
        );
        assert!(matches!(new_state.commit, CommitState::Skipped));
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    });
}

#[test]
fn test_commit_message_validation_failed_leaves_state_unchanged() {
    with_default_timeout(|| {
        let state = PipelineState {
            commit: CommitState::Generated {
                message: "test".to_string(),
            },
            ..create_initial_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::CommitMessageValidationFailed {
                reason: "invalid".to_string(),
                attempt: 1,
            },
        );
        assert!(matches!(new_state.commit, CommitState::Generated { .. }));
    });
}

#[test]
fn test_checkpoint_saved_leaves_state_unchanged() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let original_phase = state.phase;
        let new_state = reduce(
            state,
            PipelineEvent::CheckpointSaved {
                trigger: CheckpointTrigger::PhaseTransition,
            },
        );
        assert_eq!(new_state.phase, original_phase);
    });
}

#[test]
fn test_informational_events_leave_state_unchanged() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let events = vec![
            PipelineEvent::AgentInvocationStarted {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                model: Some("model1".to_string()),
            },
            PipelineEvent::AgentInvocationSucceeded {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
            },
            PipelineEvent::AgentRetryCycleStarted {
                role: AgentRole::Developer,
                cycle: 1,
            },
        ];
        for event in events {
            let new_state = reduce(state.clone(), event);
            assert_eq!(new_state.phase, state.phase);
            assert_eq!(
                new_state.agent_chain.current_agent(),
                state.agent_chain.current_agent()
            );
        }
    });
}
