//! Tests for planning phase events.

use super::*;

#[test]
fn test_planning_phase_started_sets_planning_phase() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::planning_phase_started());

    assert_eq!(new_state.phase, PipelinePhase::Planning);
}

#[test]
fn test_planning_phase_completed_transitions_to_development() {
    let state = create_state_in_phase(PipelinePhase::Planning);
    let new_state = reduce(state, PipelineEvent::planning_phase_completed());

    assert_eq!(new_state.phase, PipelinePhase::Development);
}

#[test]
fn test_plan_generation_started_is_noop() {
    let state = create_test_state();
    let new_state = reduce(state.clone(), PipelineEvent::plan_generation_started(1));

    assert_eq!(new_state.phase, state.phase);
    assert_eq!(new_state.iteration, state.iteration);
}

#[test]
fn test_plan_generation_completed_transitions_to_development() {
    let state = create_state_in_phase(PipelinePhase::Planning);
    let new_state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

    assert_eq!(new_state.phase, PipelinePhase::Development);
}
