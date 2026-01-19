//! Integration tests for rebase state machine.
//!
//! Tests for checkpoint-based recovery and state management:
//! - Checkpoint save/load operations
//! - State transitions
//! - Recovery from interruptions
//! - Error recording

use ralph_workflow::git_helpers::{RebasePhase, RebaseStateMachine, RecoveryAction};

#[test]
fn state_machine_creates_initial_checkpoint() {
    // Test that creating a state machine has correct initial state
    let machine = RebaseStateMachine::new("main".to_string());

    // Verify initial state
    assert_eq!(machine.phase(), &RebasePhase::NotStarted);
    assert_eq!(machine.upstream_branch(), "main");
    assert_eq!(machine.unresolved_conflict_count(), 0);
    assert!(machine.can_recover());
    assert!(!machine.should_abort());
}

#[test]
fn state_machine_transitions_through_phases() {
    // Test that state machine transitions correctly through phases
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|_dir| {
        let mut machine = RebaseStateMachine::new("main".to_string());

        // Pre-rebase check
        machine.transition_to(RebasePhase::PreRebaseCheck).unwrap();
        assert_eq!(machine.phase(), &RebasePhase::PreRebaseCheck);

        // Rebase in progress
        machine
            .transition_to(RebasePhase::RebaseInProgress)
            .unwrap();
        assert_eq!(machine.phase(), &RebasePhase::RebaseInProgress);

        // Conflict detected
        machine
            .transition_to(RebasePhase::ConflictDetected)
            .unwrap();
        assert_eq!(machine.phase(), &RebasePhase::ConflictDetected);

        // Resolution in progress
        machine
            .transition_to(RebasePhase::ConflictResolutionInProgress)
            .unwrap();
        assert_eq!(machine.phase(), &RebasePhase::ConflictResolutionInProgress);

        // Complete
        machine.transition_to(RebasePhase::RebaseComplete).unwrap();
        assert_eq!(machine.phase(), &RebasePhase::RebaseComplete);
    });
}

#[test]
fn state_machine_records_conflicts() {
    // Test that state machine records conflict files
    let mut machine = RebaseStateMachine::new("main".to_string());

    machine.record_conflict("src/main.rs".to_string());
    machine.record_conflict("src/lib.rs".to_string());

    assert_eq!(machine.unresolved_conflict_count(), 2);
}

#[test]
fn state_machine_records_resolutions() {
    // Test that state machine records resolved files
    let mut machine = RebaseStateMachine::new("main".to_string());

    // Record conflicts first
    machine.record_conflict("src/main.rs".to_string());
    machine.record_conflict("src/lib.rs".to_string());
    assert_eq!(machine.unresolved_conflict_count(), 2);

    // Record resolutions
    machine.record_resolution("src/main.rs".to_string());
    assert_eq!(machine.unresolved_conflict_count(), 1);

    machine.record_resolution("src/lib.rs".to_string());
    assert_eq!(machine.unresolved_conflict_count(), 0);
    assert!(machine.all_conflicts_resolved());
}

#[test]
fn state_machine_checks_recovery_state() {
    // Test that state machine correctly determines recovery state
    let mut machine = RebaseStateMachine::new("main".to_string());

    // Initially can recover
    assert!(machine.can_recover());
    assert!(!machine.should_abort());

    // Record errors up to limit
    machine.record_error("Error 1".to_string());
    assert!(machine.can_recover());

    machine.record_error("Error 2".to_string());
    assert!(machine.can_recover());

    machine.record_error("Error 3".to_string());
    assert!(!machine.can_recover());
    assert!(machine.should_abort());
}

#[test]
fn state_machine_records_and_counts_errors() {
    // Test that state machine records errors and counts them
    let mut machine = RebaseStateMachine::new("main".to_string());

    machine.record_error("Error 1".to_string());
    machine.record_error("Error 2".to_string());
    machine.record_error("Error 3".to_string());

    // After 3 errors, should abort (default max is 3)
    assert!(!machine.can_recover());
    assert!(machine.should_abort());
}

#[test]
fn state_machine_checkpoint_roundtrip() {
    // Test that checkpoint can be saved and loaded
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|_dir| {
        let mut machine = RebaseStateMachine::new("main".to_string());
        machine
            .transition_to(RebasePhase::ConflictDetected)
            .unwrap();
        machine.record_error("Test error".to_string());

        // Load into new machine
        let loaded = RebaseStateMachine::load_or_create("main".to_string()).unwrap();

        assert_eq!(loaded.phase(), machine.phase());
        assert_eq!(loaded.upstream_branch(), machine.upstream_branch());
        // Note: record_conflict doesn't save to checkpoint, only record_error saves
        // So we don't check unresolved_conflict_count equality here
        assert_eq!(loaded.can_recover(), machine.can_recover());
    });
}

#[test]
fn state_machine_clears_checkpoint() {
    // Test that checkpoint can be cleared
    use ralph_workflow::git_helpers::rebase_checkpoint::rebase_checkpoint_exists;
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|_dir| {
        let mut machine = RebaseStateMachine::new("main".to_string());
        machine.transition_to(RebasePhase::RebaseComplete).unwrap();

        // Check checkpoint exists
        assert!(rebase_checkpoint_exists());

        // Clear checkpoint
        machine.clear_checkpoint().unwrap();

        // Verify checkpoint is gone
        assert!(!rebase_checkpoint_exists());
    });
}

#[test]
fn state_machine_allows_valid_transitions() {
    // Test that valid state transitions are allowed
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|_dir| {
        let mut machine = RebaseStateMachine::new("main".to_string());

        // Valid transitions
        assert!(machine.transition_to(RebasePhase::PreRebaseCheck).is_ok());
        assert!(machine.transition_to(RebasePhase::RebaseInProgress).is_ok());
        assert!(machine.transition_to(RebasePhase::ConflictDetected).is_ok());
        assert!(machine
            .transition_to(RebasePhase::ConflictResolutionInProgress)
            .is_ok());
        assert!(machine.transition_to(RebasePhase::CompletingRebase).is_ok());
        assert!(machine.transition_to(RebasePhase::RebaseComplete).is_ok());
    });
}

#[test]
fn state_machine_handles_abort() {
    // Test that abort transition works correctly
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|_dir| {
        let mut machine = RebaseStateMachine::new("main".to_string());

        machine
            .transition_to(RebasePhase::ConflictDetected)
            .unwrap();
        machine.record_conflict("test.rs".to_string());
        machine.record_error("Conflict resolution failed".to_string());

        // Abort the rebase
        machine.abort().unwrap();

        // Load and verify aborted state
        let loaded = RebaseStateMachine::load_or_create("main".to_string()).unwrap();
        assert_eq!(loaded.phase(), &RebasePhase::RebaseAborted);
    });
}

#[test]
fn state_machine_recovery_action_variants() {
    // Test that all RecoveryAction variants exist
    let _ = RecoveryAction::Continue;
    let _ = RecoveryAction::Retry;
    let _ = RecoveryAction::Abort;
    let _ = RecoveryAction::Skip;
}
