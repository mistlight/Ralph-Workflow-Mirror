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

#[test]
fn test_pre_termination_uncommitted_changes_routes_back_to_commit_phase() {
    // When the pre-termination safety check finds uncommitted changes, the reducer must
    // route back through the commit phase (unattended-mode safety), recording the
    // phase we should resume after committing.
    let mut state = PipelineState::initial(0, 0);
    state.phase = PipelinePhase::Complete;
    state.pre_termination_commit_checked = false;
    state.termination_resume_phase = None;

    let event = PipelineEvent::pre_termination_uncommitted_changes_detected(3);
    let new_state = reduce(state, event);

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert_eq!(
        new_state.termination_resume_phase,
        Some(PipelinePhase::Complete)
    );
}

#[test]
fn test_post_commit_resumes_termination_phase_when_safety_commit_pending() {
    // If we routed into CommitMessage due to the pre-termination safety check,
    // a successful commit must resume the original termination phase and allow
    // termination to proceed.
    let mut state = PipelineState::initial(0, 0);
    state.phase = PipelinePhase::CommitMessage;
    state.termination_resume_phase = Some(PipelinePhase::Complete);
    state.pre_termination_commit_checked = false;

    let new_state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "msg".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Complete);
    assert_eq!(new_state.termination_resume_phase, None);
    assert!(
        new_state.pre_termination_commit_checked,
        "Termination should be unblocked after safety commit completes"
    );
}

#[test]
fn test_skip_does_not_unblock_termination_when_safety_commit_pending() {
    // If the pre-termination safety check detected a dirty repo and routed into CommitMessage,
    // an AI-driven "skip" must NOT unblock termination.
    //
    // The pipeline must re-run CheckUncommittedChangesBeforeTermination after any skip and only
    // proceed once the repo is actually clean.
    let mut state = PipelineState::initial(0, 0);
    state.phase = PipelinePhase::CommitMessage;
    state.termination_resume_phase = Some(PipelinePhase::Complete);
    state.pre_termination_commit_checked = false;

    let new_state = reduce(
        state,
        PipelineEvent::commit_skipped("no changes".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Complete);
    assert_eq!(new_state.termination_resume_phase, None);
    assert!(
        !new_state.pre_termination_commit_checked,
        "Skip during safety-check commit must not unblock termination"
    );
}
