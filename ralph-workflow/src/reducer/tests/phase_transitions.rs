//! Tests for phase transitions and state preservation across the pipeline.

use super::*;

#[test]
fn test_complete_pipeline_phase_flow() {
    let mut state = create_test_state();

    // Planning → Development
    state = reduce(state, PipelineEvent::PlanningPhaseStarted);
    assert_eq!(state.phase, PipelinePhase::Planning);

    state = reduce(state, PipelineEvent::PlanningPhaseCompleted);
    assert_eq!(state.phase, PipelinePhase::Development);

    // Development → Review
    state = reduce(state, PipelineEvent::DevelopmentPhaseStarted);
    assert_eq!(state.phase, PipelinePhase::Development);

    state = reduce(state, PipelineEvent::DevelopmentPhaseCompleted);
    assert_eq!(state.phase, PipelinePhase::Review);

    // Review → CommitMessage
    state = reduce(state, PipelineEvent::ReviewPhaseStarted);
    assert_eq!(state.phase, PipelinePhase::Review);

    state = reduce(
        state,
        PipelineEvent::ReviewPhaseCompleted { early_exit: false },
    );
    assert_eq!(state.phase, PipelinePhase::CommitMessage);

    // CommitMessage → FinalValidation
    state = reduce(
        state,
        PipelineEvent::CommitCreated {
            hash: "abc123".to_string(),
            message: "test".to_string(),
        },
    );
    assert_eq!(state.phase, PipelinePhase::FinalValidation);

    // FinalValidation → Complete
    state = reduce(state, PipelineEvent::PipelineCompleted);
    assert_eq!(state.phase, PipelinePhase::Complete);
}

#[test]
fn test_phase_transitions_preserve_iteration_count() {
    let mut state = PipelineState {
        iteration: 3,
        total_iterations: 5,
        ..create_test_state()
    };

    let original_iteration = state.iteration;

    // Transition through phases
    state = reduce(state, PipelineEvent::PlanningPhaseStarted);
    assert_eq!(state.iteration, original_iteration);

    state = reduce(state, PipelineEvent::PlanningPhaseCompleted);
    assert_eq!(state.iteration, original_iteration);

    state = reduce(state, PipelineEvent::DevelopmentPhaseStarted);
    assert_eq!(state.iteration, original_iteration);
}

#[test]
fn test_phase_transitions_preserve_reviewer_pass() {
    let mut state = PipelineState {
        reviewer_pass: 2,
        total_reviewer_passes: 3,
        ..create_test_state()
    };

    let original_pass = state.reviewer_pass;

    // Transition to commit phase
    state = reduce(
        state,
        PipelineEvent::ReviewPhaseCompleted { early_exit: false },
    );
    assert_eq!(state.reviewer_pass, original_pass);

    // Transition to final validation
    state = reduce(
        state,
        PipelineEvent::CommitCreated {
            hash: "abc".to_string(),
            message: "test".to_string(),
        },
    );
    assert_eq!(state.reviewer_pass, original_pass);
}

#[test]
fn test_pipeline_aborted_from_any_phase() {
    for phase in [
        PipelinePhase::Planning,
        PipelinePhase::Development,
        PipelinePhase::Review,
        PipelinePhase::CommitMessage,
        PipelinePhase::FinalValidation,
    ] {
        let state = create_state_in_phase(phase);
        let new_state = reduce(
            state,
            PipelineEvent::PipelineAborted {
                reason: "User cancelled".to_string(),
            },
        );

        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
    }
}

#[test]
fn test_commit_skipped_transitions_preserve_phase_history() {
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        iteration: 5,
        reviewer_pass: 2,
        ..create_test_state()
    };

    let new_state = reduce(
        state.clone(),
        PipelineEvent::CommitSkipped {
            reason: "No changes".to_string(),
        },
    );

    // Should transition to FinalValidation
    assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    // Should preserve counters
    assert_eq!(new_state.iteration, state.iteration);
    assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
}

#[test]
fn test_state_preservation_through_agent_events() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 3,
        reviewer_pass: 1,
        review_issues_found: true,
        ..create_test_state()
    };

    // Agent events should not modify phase or counters
    let new_state = reduce(
        state.clone(),
        PipelineEvent::AgentInvocationStarted {
            role: crate::agents::AgentRole::Developer,
            agent: "test".to_string(),
            model: Some("test-model".to_string()),
        },
    );

    assert_eq!(new_state.phase, state.phase);
    assert_eq!(new_state.iteration, state.iteration);
    assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
    assert_eq!(new_state.review_issues_found, state.review_issues_found);
}
