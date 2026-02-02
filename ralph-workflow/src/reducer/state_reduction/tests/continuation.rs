// Continuation event handling tests.
//
// Tests for continuation triggered, succeeded, budget exhausted events,
// and continuation state management during development iterations.

use super::*;

#[test]
fn test_continuation_triggered_updates_state() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Partial,
            "Did work".to_string(),
            Some(vec!["src/main.rs".to_string()]),
            Some("Continue".to_string()),
        ),
    );

    assert!(new_state.continuation.is_continuation());
    assert_eq!(
        new_state.continuation.previous_status,
        Some(DevelopmentStatus::Partial)
    );
    assert_eq!(
        new_state.continuation.previous_summary,
        Some("Did work".to_string())
    );
    assert_eq!(
        new_state.continuation.previous_files_changed,
        Some(vec!["src/main.rs".to_string()])
    );
    assert_eq!(
        new_state.continuation.previous_next_steps,
        Some("Continue".to_string())
    );
    assert_eq!(new_state.continuation.continuation_attempt, 1);
}

#[test]
fn test_continuation_triggered_sets_iteration_from_event() {
    use crate::reducer::state::DevelopmentStatus;

    let state = PipelineState {
        iteration: 99,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            2,
            DevelopmentStatus::Partial,
            "Did work".to_string(),
            None,
            None,
        ),
    );

    assert_eq!(new_state.iteration, 2);
}

#[test]
fn test_continuation_triggered_with_failed_status() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Failed,
            "Build failed".to_string(),
            None,
            Some("Fix errors".to_string()),
        ),
    );

    assert!(new_state.continuation.is_continuation());
    assert_eq!(
        new_state.continuation.previous_status,
        Some(DevelopmentStatus::Failed)
    );
    assert_eq!(
        new_state.continuation.previous_summary,
        Some("Build failed".to_string())
    );
    assert!(new_state.continuation.previous_files_changed.is_none());
}

#[test]
fn test_continuation_succeeded_resets_state() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );
    assert!(state.continuation.is_continuation());

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_succeeded(1, 2),
    );

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.continuation.continuation_attempt, 0);
    assert!(new_state.continuation.previous_status.is_none());
}

#[test]
fn test_continuation_succeeded_sets_iteration_from_event() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 99,
        ..create_test_state()
    };
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_succeeded(1, 1),
    );

    assert_eq!(new_state.iteration, 1);
}

#[test]
fn test_iteration_started_resets_continuation() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );
    assert!(state.continuation.is_continuation());

    let new_state = reduce(state, PipelineEvent::development_iteration_started(2));

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.iteration, 2);
}

#[test]
fn test_iteration_completed_resets_continuation() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.phase = PipelinePhase::Development;
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(1, true),
    );

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_development_phase_completed_resets_continuation() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.phase = PipelinePhase::Development;
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );

    let new_state = reduce(state, PipelineEvent::development_phase_completed());

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.phase, PipelinePhase::Review);
}

#[test]
fn test_multiple_continuation_triggers_accumulate() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();

    // First continuation
    let state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Partial,
            "First attempt".to_string(),
            None,
            None,
        ),
    );
    assert_eq!(state.continuation.continuation_attempt, 1);

    // Second continuation
    let state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Partial,
            "Second attempt".to_string(),
            None,
            None,
        ),
    );
    assert_eq!(state.continuation.continuation_attempt, 2);
    assert_eq!(
        state.continuation.previous_summary,
        Some("Second attempt".to_string())
    );
}

#[test]
fn test_continuation_budget_exhausted_transitions_to_interrupted() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(0, 3, DevelopmentStatus::Partial),
    );
    assert_eq!(
        new_state.phase,
        PipelinePhase::Interrupted,
        "Should transition to Interrupted when continuation budget exhausted"
    );
}

#[test]
fn test_continuation_budget_exhausted_resets_continuation_state() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );
    assert!(state.continuation.is_continuation());

    let new_state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(0, 3, DevelopmentStatus::Partial),
    );
    assert!(
        !new_state.continuation.is_continuation(),
        "Continuation state should be reset"
    );
}

#[test]
fn test_continuation_budget_exhausted_preserves_iteration() {
    use crate::reducer::state::DevelopmentStatus;

    let mut state = create_test_state();
    state.iteration = 5;

    let new_state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(5, 3, DevelopmentStatus::Failed),
    );
    assert_eq!(
        new_state.iteration, 5,
        "Should preserve the iteration number"
    );
}
