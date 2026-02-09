//! Resume integration tests.
//!
//! Tests verify that resume functionality works correctly with reducer state machine.
//! Tests cover resume at all pipeline phases (planning, development, review, commit).
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::checkpoint::PipelineCheckpoint;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::state::PipelineState;

#[test]
fn test_pipeline_state_from_checkpoint_at_planning() {
    with_default_timeout(|| {
        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::Planning,
            iteration: 0,
            reviewer_pass: 0,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0);
    });
}

#[test]
fn test_pipeline_state_from_checkpoint_at_development() {
    with_default_timeout(|| {
        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::Development,
            iteration: 3,
            reviewer_pass: 0,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        assert_eq!(state.phase, PipelinePhase::Development);
        assert_eq!(state.iteration, 3);
    });
}

#[test]
fn test_pipeline_state_from_checkpoint_at_review() {
    with_default_timeout(|| {
        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::Review,
            iteration: 5,
            reviewer_pass: 1,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        assert_eq!(state.phase, PipelinePhase::Review);
        assert_eq!(state.reviewer_pass, 1);
        assert_eq!(state.iteration, 5);
    });
}

#[test]
fn test_pipeline_state_from_checkpoint_at_commit() {
    with_default_timeout(|| {
        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::CommitMessage,
            iteration: 5,
            reviewer_pass: 2,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        assert_eq!(state.phase, PipelinePhase::CommitMessage);
        assert_eq!(state.reviewer_pass, 2);
        assert_eq!(state.iteration, 5);
    });
}

#[test]
fn test_pipeline_state_from_complete_checkpoint() {
    with_default_timeout(|| {
        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::Complete,
            iteration: 5,
            reviewer_pass: 2,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        assert_eq!(state.phase, PipelinePhase::Complete);
    });
}

#[test]
fn test_resume_continues_from_correct_iteration() {
    with_default_timeout(|| {
        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::Development,
            iteration: 2,
            reviewer_pass: 0,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        assert_eq!(state.iteration, 2);
        assert_eq!(state.total_iterations, 5);
    });
}

#[test]
fn test_resume_continues_from_correct_reviewer_pass() {
    with_default_timeout(|| {
        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::Review,
            iteration: 5,
            reviewer_pass: 1,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        assert_eq!(state.reviewer_pass, 1);
        assert_eq!(state.total_reviewer_passes, 2);
    });
}

#[test]
fn test_agent_chain_preserved_across_resume() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::AgentChainState;

        let checkpoint = PipelineCheckpoint {
            phase: PipelinePhase::Development,
            iteration: 2,
            reviewer_pass: 0,
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..Default::default()
        };

        let state = PipelineState::from(checkpoint.clone());

        if let AgentChainState {
            agents,
            current_agent_index,
            models_per_agent,
            current_model_index,
            ..
        } = &state.agent_chain
        {
            assert!(!agents.is_empty());
            assert!(current_agent_index < agents.len());
            assert!(
                current_model_index
                    < models_per_agent
                        .get(current_agent_index)
                        .map_or(&vec![], |v| v)
                        .len()
            );
        }
    });
}
