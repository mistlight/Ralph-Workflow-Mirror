//! Rebase state machine integration tests.
//!
//! Tests verify that rebase operations integrate correctly with reducer state machine.
//! Tests verify actual state changes through event emission and reduce() function.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::reducer::event::{
    CheckpointTrigger, PipelineEvent, PipelinePhase, RebasePhase,
};
use ralph_workflow::reducer::state::{PipelineState, RebaseState};

fn create_initial_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
    ralph_workflow::reducer::state_reduction::reduce(state, event)
}

#[test]
fn test_rebase_started_before_planning() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::RebaseStarted {
                phase: RebasePhase::Initial,
                target_branch: "main".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
    });
}

#[test]
fn test_rebase_started_sets_original_head() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::RebaseStarted {
                phase: RebasePhase::Initial,
                target_branch: "main".to_string(),
            },
        );

        if let RebaseState::InProgress { original_head, .. } = new_state.rebase {
            assert_eq!(original_head, "HEAD");
        } else {
            panic!("Expected InProgress state");
        }
    });
}

#[test]
fn test_rebase_conflict_detected_transitions_to_conflicted() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::RebaseConflictDetected {
                files: vec!["file1.txt".into(), "file2.txt".into()],
            },
        );

        if let RebaseState::Conflicted { files, .. } = &new_state.rebase {
            assert_eq!(files.len(), 2);
            assert_eq!(files[0].as_path(), "file1.txt");
            assert_eq!(files[1].as_path(), "file2.txt");
        } else {
            panic!("Expected Conflicted state");
        }
    });
}

#[test]
fn test_rebase_conflict_resolved_returns_to_in_progress() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::Conflicted {
                original_head: "HEAD".to_string(),
                target_branch: "main".to_string(),
                files: vec!["file1.txt".into()],
                resolution_attempts: 0,
            },
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::RebaseConflictResolved {
                files: vec!["file1.txt".into()],
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
    });
}

#[test]
fn test_rebase_succeeded_transitions_to_completed() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::RebaseSucceeded {
                phase: RebasePhase::Initial,
                new_head: "def456".to_string(),
            },
        );

        if let RebaseState::Completed { new_head } = &new_state.rebase {
            assert_eq!(new_head, "def456");
        } else {
            panic!("Expected Completed state");
        }
    });
}

#[test]
fn test_rebase_succeeded_stores_new_head() {
    with_default_timeout(|| {
        let state = create_initial_state();

        let new_state = reduce(
            state,
            PipelineEvent::RebaseSucceeded {
                phase: RebasePhase::Initial,
                new_head: "def456".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::Completed { .. }));
    });
}

#[test]
fn test_rebase_failed_transitions_to_not_started() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::RebaseFailed {
                phase: RebasePhase::Initial,
                reason: "conflict".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::NotStarted));
    });
}

#[test]
fn test_rebase_skipped_transitions_to_skipped() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::RebaseSkipped {
                phase: RebasePhase::Initial,
                reason: "up to date".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::Skipped));
    });
}

#[test]
fn test_rebase_aborted_keeps_in_progress_state() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::InProgress {
                original_head: "abc123".to_string(),
                target_branch: "main".to_string(),
            },
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::RebaseAborted {
                phase: RebasePhase::Initial,
                restored_to: "abc123".to_string(),
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
    });
}

#[test]
fn test_rebase_in_conflicted_state_continues_handling() {
    with_default_timeout(|| {
        let state = PipelineState {
            rebase: RebaseState::Conflicted {
                original_head: "HEAD".to_string(),
                target_branch: "main".to_string(),
                files: vec!["file1.txt".into()],
                resolution_attempts: 0,
            },
            ..create_initial_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::CheckpointSaved {
                trigger: CheckpointTrigger::PhaseTransition,
            },
        );

        assert!(matches!(new_state.rebase, RebaseState::Conflicted { .. }));
    });
}
