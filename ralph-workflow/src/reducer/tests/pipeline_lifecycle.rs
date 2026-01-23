//! Tests for pipeline lifecycle events (start, resume, complete, abort).

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
