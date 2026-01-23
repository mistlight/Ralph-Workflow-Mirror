//! Tests for rebase events (start, conflicts, resolution).

use super::*;
use crate::reducer::event::RebasePhase;
use std::path::PathBuf;

#[test]
fn test_rebase_started_sets_in_progress() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::RebaseStarted {
            phase: RebasePhase::Initial,
            target_branch: "main".to_string(),
        },
    );

    assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
}

#[test]
fn test_rebase_started_stores_target_branch() {
    let state = create_test_state();
    let target = "develop".to_string();
    let new_state = reduce(
        state,
        PipelineEvent::RebaseStarted {
            phase: RebasePhase::Initial,
            target_branch: target.clone(),
        },
    );

    if let RebaseState::InProgress {
        target_branch,
        original_head: _,
    } = new_state.rebase
    {
        assert_eq!(target_branch, target);
    } else {
        panic!("Expected RebaseState::InProgress");
    }
}

#[test]
fn test_rebase_conflict_detected_transitions_to_conflicted() {
    let state = PipelineState {
        rebase: RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseConflictDetected {
            files: vec![PathBuf::from("file.rs")],
        },
    );

    assert!(matches!(new_state.rebase, RebaseState::Conflicted { .. }));
}

#[test]
fn test_rebase_conflict_detected_stores_files() {
    let state = PipelineState {
        rebase: RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        },
        ..create_test_state()
    };
    let files = vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")];
    let new_state = reduce(
        state,
        PipelineEvent::RebaseConflictDetected {
            files: files.clone(),
        },
    );

    if let RebaseState::Conflicted {
        target_branch: _,
        original_head: _,
        files: stored_files,
        resolution_attempts: _,
    } = new_state.rebase
    {
        assert_eq!(stored_files, files);
    } else {
        panic!("Expected RebaseState::Conflicted");
    }
}

#[test]
fn test_rebase_conflict_resolved_transitions_to_in_progress() {
    let state = PipelineState {
        rebase: RebaseState::Conflicted {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
            files: vec![PathBuf::from("file.rs")],
            resolution_attempts: 0,
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseConflictResolved {
            files: vec![PathBuf::from("file.rs")],
        },
    );

    // After resolving conflict, should transition back to InProgress
    if let RebaseState::InProgress {
        target_branch,
        original_head,
    } = new_state.rebase
    {
        assert_eq!(target_branch, "main");
        assert_eq!(original_head, "abc123");
    } else {
        panic!(
            "Expected RebaseState::InProgress, got {:?}",
            new_state.rebase
        );
    }
}

#[test]
fn test_rebase_succeeded_transitions_to_completed() {
    let state = PipelineState {
        rebase: RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        },
        ..create_test_state()
    };
    let new_head_hash = "def456".to_string();
    let new_state = reduce(
        state,
        PipelineEvent::RebaseSucceeded {
            phase: RebasePhase::Initial,
            new_head: new_head_hash.clone(),
        },
    );

    if let RebaseState::Completed { new_head } = new_state.rebase {
        assert_eq!(new_head, new_head_hash);
    } else {
        panic!(
            "Expected RebaseState::Completed, got {:?}",
            new_state.rebase
        );
    }
}

#[test]
fn test_rebase_failed_resets_to_not_started() {
    let state = PipelineState {
        rebase: RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseFailed {
            phase: RebasePhase::Initial,
            reason: "Merge conflict".to_string(),
        },
    );

    assert!(matches!(new_state.rebase, RebaseState::NotStarted));
}

#[test]
fn test_rebase_aborted_is_noop() {
    let state = PipelineState {
        rebase: RebaseState::Conflicted {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
            files: vec![PathBuf::from("file.rs")],
            resolution_attempts: 2,
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state.clone(),
        PipelineEvent::RebaseAborted {
            phase: RebasePhase::Initial,
            restored_to: "abc123".to_string(),
        },
    );

    // RebaseAborted is currently a no-op - state is preserved
    assert!(matches!(new_state.rebase, RebaseState::Conflicted { .. }));
}

#[test]
fn test_rebase_skipped_transitions_to_skipped() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::RebaseSkipped {
            phase: RebasePhase::Initial,
            reason: "Not needed".to_string(),
        },
    );

    assert!(matches!(new_state.rebase, RebaseState::Skipped));
}

#[test]
fn test_rebase_conflict_detected_initializes_resolution_attempts_to_zero() {
    let state = PipelineState {
        rebase: RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseConflictDetected {
            files: vec![PathBuf::from("file.rs")],
        },
    );

    if let RebaseState::Conflicted {
        resolution_attempts,
        ..
    } = new_state.rebase
    {
        assert_eq!(resolution_attempts, 0);
    } else {
        panic!("Expected RebaseState::Conflicted");
    }
}

#[test]
fn test_rebase_conflict_detected_preserves_branch_info() {
    let state = PipelineState {
        rebase: RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "develop".to_string(),
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseConflictDetected {
            files: vec![PathBuf::from("file.rs")],
        },
    );

    if let RebaseState::Conflicted {
        original_head,
        target_branch,
        ..
    } = new_state.rebase
    {
        assert_eq!(original_head, "abc123");
        assert_eq!(target_branch, "develop");
    } else {
        panic!("Expected RebaseState::Conflicted");
    }
}

#[test]
fn test_rebase_conflict_resolved_preserves_branch_info() {
    let state = PipelineState {
        rebase: RebaseState::Conflicted {
            original_head: "abc123".to_string(),
            target_branch: "develop".to_string(),
            files: vec![PathBuf::from("file.rs")],
            resolution_attempts: 5,
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseConflictResolved {
            files: vec![PathBuf::from("file.rs")],
        },
    );

    if let RebaseState::InProgress {
        original_head,
        target_branch,
    } = new_state.rebase
    {
        assert_eq!(original_head, "abc123");
        assert_eq!(target_branch, "develop");
    } else {
        panic!("Expected RebaseState::InProgress");
    }
}

#[test]
fn test_rebase_succeeded_from_conflicted_state() {
    let state = PipelineState {
        rebase: RebaseState::Conflicted {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
            files: vec![PathBuf::from("file.rs")],
            resolution_attempts: 3,
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseSucceeded {
            phase: RebasePhase::PostReview,
            new_head: "def456".to_string(),
        },
    );

    if let RebaseState::Completed { new_head } = new_state.rebase {
        assert_eq!(new_head, "def456");
    } else {
        panic!("Expected RebaseState::Completed");
    }
}

#[test]
fn test_rebase_failed_from_conflicted_state() {
    let state = PipelineState {
        rebase: RebaseState::Conflicted {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
            files: vec![PathBuf::from("file.rs")],
            resolution_attempts: 10,
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::RebaseFailed {
            phase: RebasePhase::PostReview,
            reason: "Too many conflicts".to_string(),
        },
    );

    assert!(matches!(new_state.rebase, RebaseState::NotStarted));
}
