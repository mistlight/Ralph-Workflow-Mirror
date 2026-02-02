// Rebase and commit state machine tests.
//
// Tests for rebase started/succeeded/conflict/resolved transitions,
// and commit generation/created state transitions.

use super::*;

#[test]
fn test_reduce_rebase_started() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::rebase_started(RebasePhase::Initial, "main".to_string()),
    );

    assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
}

#[test]
fn test_reduce_rebase_succeeded() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::rebase_succeeded(RebasePhase::Initial, "abc123".to_string()),
    );

    assert!(matches!(new_state.rebase, RebaseState::Completed { .. }));
}

#[test]
fn test_reduce_commit_generation_started() {
    let state = PipelineState {
        commit_diff_prepared: true,
        commit_diff_empty: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::commit_generation_started());

    assert!(matches!(new_state.commit, CommitState::Generating { .. }));
    assert!(new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
}

#[test]
fn test_reduce_commit_diff_failed_interrupts_pipeline() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::commit_diff_failed("diff failed".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Interrupted);
    assert!(!new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
}

#[test]
fn test_reduce_commit_created() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
    );

    assert!(matches!(new_state.commit, CommitState::Committed { .. }));
    assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
}

#[test]
fn test_reduce_rebase_full_state_machine() {
    let mut state = create_test_state();

    state = reduce(
        state,
        PipelineEvent::rebase_started(RebasePhase::Initial, "main".to_string()),
    );
    assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

    state = reduce(
        state,
        PipelineEvent::rebase_conflict_detected(vec![std::path::PathBuf::from("file1.txt")]),
    );
    assert!(matches!(state.rebase, RebaseState::Conflicted { .. }));

    state = reduce(
        state,
        PipelineEvent::rebase_conflict_resolved(vec![std::path::PathBuf::from("file1.txt")]),
    );
    assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

    state = reduce(
        state,
        PipelineEvent::rebase_succeeded(RebasePhase::Initial, "def456".to_string()),
    );
    assert!(matches!(state.rebase, RebaseState::Completed { .. }));
}

#[test]
fn test_reduce_commit_full_state_machine() {
    let mut state = create_test_state();

    state = reduce(state, PipelineEvent::commit_generation_started());
    assert!(matches!(state.commit, CommitState::Generating { .. }));

    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
    );
    assert!(matches!(state.commit, CommitState::Committed { .. }));
}
