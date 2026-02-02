// Basic pipeline transition tests.
//
// Tests for fundamental state transitions: pipeline started/completed,
// development iteration completed, plan generation, phase transitions.

use super::*;

#[test]
fn test_reduce_pipeline_started() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::pipeline_started());
    assert_eq!(new_state.phase, PipelinePhase::Planning);
}

#[test]
fn test_reduce_pipeline_completed() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::pipeline_completed());
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
        PipelineEvent::development_iteration_completed(2, true),
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
        PipelineEvent::development_iteration_completed(5, true),
    );
    // Iteration stays at 5 (incremented by CommitCreated later)
    assert_eq!(new_state.iteration, 5);
    // Goes to CommitMessage phase first
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_plan_generation_completed_invalid_does_not_transition_to_development() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::plan_generation_completed(1, false));

    assert_eq!(
        new_state.phase,
        PipelinePhase::Planning,
        "Invalid plan should keep pipeline in Planning phase"
    );
}

#[test]
fn test_reduce_phase_transitions() {
    let mut state = create_test_state();

    state = reduce(state, PipelineEvent::planning_phase_completed());
    assert_eq!(state.phase, PipelinePhase::Development);

    state = reduce(state, PipelineEvent::development_phase_started());
    assert_eq!(state.phase, PipelinePhase::Development);

    state = reduce(state, PipelineEvent::development_phase_completed());
    assert_eq!(state.phase, PipelinePhase::Review);

    state = reduce(state, PipelineEvent::review_phase_started());
    assert_eq!(state.phase, PipelinePhase::Review);

    state = reduce(state, PipelineEvent::review_phase_completed(false));
    assert_eq!(state.phase, PipelinePhase::CommitMessage);
}
