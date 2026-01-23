//! Tests for development phase events (iterations, plan generation).

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

#[test]
fn test_development_iteration_completed_with_large_iteration_number() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 999,
        total_iterations: 1000,
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::DevelopmentIterationCompleted {
            iteration: 999,
            output_valid: true,
        },
    );

    // Should increment to 1000 and transition to Review
    assert_eq!(new_state.iteration, 1000);
    assert_eq!(new_state.phase, PipelinePhase::Review);
}

#[test]
fn test_development_iteration_started_with_max_u32() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::DevelopmentIterationStarted {
            iteration: u32::MAX,
        },
    );

    assert_eq!(new_state.iteration, u32::MAX);
}
