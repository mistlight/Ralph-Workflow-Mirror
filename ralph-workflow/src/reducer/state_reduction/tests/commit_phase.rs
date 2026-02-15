//! Commit phase reducer tests.
//!
//! Tests for commit-related event handling in the state reduction layer.

use crate::reducer::event::*;
use crate::reducer::state::*;
use crate::reducer::state_reduction::reduce;

#[test]
fn test_diff_failed_event_is_noop_for_backward_compatibility() {
    // DiffFailed is deprecated and should not be emitted by current handler code.
    // If received (e.g., from old checkpoint), it should be a no-op to avoid termination.

    let mut state = PipelineState::initial(1, 0);
    state.phase = PipelinePhase::CommitMessage;
    state.commit_diff_prepared = true;
    state.commit_diff_empty = false;
    state.commit_diff_content_id_sha256 = Some("abc123".to_string());

    let event = PipelineEvent::commit_diff_failed("git diff failed".to_string());
    let new_state = reduce(state.clone(), event);

    // Event should be no-op: state unchanged
    assert_eq!(new_state.phase, state.phase);
    assert_eq!(new_state.commit_diff_prepared, state.commit_diff_prepared);
    assert_eq!(new_state.commit_diff_empty, state.commit_diff_empty);
    assert_eq!(
        new_state.commit_diff_content_id_sha256,
        state.commit_diff_content_id_sha256
    );

    // Should NOT transition to Interrupted
    assert_ne!(new_state.phase, PipelinePhase::Interrupted);
}

#[test]
fn test_diff_prepared_event_sets_flags() {
    let mut state = PipelineState::initial(1, 0);
    state.phase = PipelinePhase::CommitMessage;

    let event = PipelineEvent::commit_diff_prepared(false, "content_hash".to_string());
    let new_state = reduce(state, event);

    assert!(new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
    assert_eq!(
        new_state.commit_diff_content_id_sha256,
        Some("content_hash".to_string())
    );
}

#[test]
fn test_diff_prepared_empty_sets_empty_flag() {
    let mut state = PipelineState::initial(1, 0);
    state.phase = PipelinePhase::CommitMessage;

    let event = PipelineEvent::commit_diff_prepared(true, "empty_hash".to_string());
    let new_state = reduce(state, event);

    assert!(new_state.commit_diff_prepared);
    assert!(new_state.commit_diff_empty);
    assert_eq!(
        new_state.commit_diff_content_id_sha256,
        Some("empty_hash".to_string())
    );
}

#[test]
fn test_diff_invalidated_clears_flags() {
    let mut state = PipelineState::initial(1, 0);
    state.phase = PipelinePhase::CommitMessage;
    state.commit_diff_prepared = true;
    state.commit_diff_empty = false;
    state.commit_diff_content_id_sha256 = Some("old_hash".to_string());
    state.commit_prompt_prepared = true;

    let event = PipelineEvent::commit_diff_invalidated("Diff file missing".to_string());
    let new_state = reduce(state, event);

    assert!(!new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
    assert_eq!(new_state.commit_diff_content_id_sha256, None);
    assert!(!new_state.commit_prompt_prepared);
}
