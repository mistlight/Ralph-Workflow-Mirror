//! Integration tests for rebase state machine.
//!
//! Tests for checkpoint-based recovery and state management:
//! - Checkpoint save/load operations
//! - State transitions
//! - Recovery from interruptions
//! - Error recording
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (state machine transitions)
//! - Uses state machine API directly (no filesystem mocking needed)
//! - Tests are deterministic and isolated

use crate::test_timeout::with_default_timeout;

use ralph_workflow::git_helpers::{RebasePhase, RebaseStateMachine, RecoveryAction};

/// Test that creating a state machine initializes with correct initial state.
///
/// This verifies that when a new RebaseStateMachine is created,
/// it has the correct initial phase, upstream branch, zero conflicts,
/// and can recover from errors.
#[test]
fn state_machine_creates_initial_checkpoint() {
    with_default_timeout(|| {
        // Test that creating a state machine has correct initial state
        let machine = RebaseStateMachine::new("main".to_string());

        // Verify initial state
        assert_eq!(machine.phase(), &RebasePhase::NotStarted);
        assert_eq!(machine.upstream_branch(), "main");
        assert_eq!(machine.unresolved_conflict_count(), 0);
        assert!(machine.can_recover());
        assert!(!machine.should_abort());
    });
}

/// Test that the state machine transitions correctly through all phases.
///
/// This verifies that when a state machine is transitioned through
/// each phase sequentially, each transition succeeds and the phase
/// property reflects the current state.
#[test]
fn state_machine_transitions_through_phases() {
    with_default_timeout(|| {
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
    });
}

/// Test that the state machine records conflict files correctly.
///
/// This verifies that when conflict files are recorded via record_conflict,
/// the unresolved_conflict_count reflects the number of recorded conflicts.
#[test]
fn state_machine_records_conflicts() {
    with_default_timeout(|| {
        // Test that state machine records conflict files
        let mut machine = RebaseStateMachine::new("main".to_string());

        machine.record_conflict("src/main.rs".to_string());
        machine.record_conflict("src/lib.rs".to_string());

        assert_eq!(machine.unresolved_conflict_count(), 2);
    });
}

/// Test that the state machine records resolved files correctly.
///
/// This verifies that when conflict resolutions are recorded via record_resolution,
/// the unresolved_conflict_count decreases and all_conflicts_resolved returns true
/// when all conflicts are resolved.
#[test]
fn state_machine_records_resolutions() {
    with_default_timeout(|| {
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
    });
}

/// Test that the state machine correctly determines recovery state based on error count.
///
/// This verifies that when errors are recorded, can_recover returns true
/// until the error limit (3) is reached, at which point should_abort returns true.
#[test]
fn state_machine_checks_recovery_state() {
    with_default_timeout(|| {
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
    });
}

/// Test that the state machine records errors and counts them correctly.
///
/// This verifies that when errors are recorded via record_error,
/// the error count is tracked and the recovery state is updated
/// when the limit is reached.
#[test]
fn state_machine_records_and_counts_errors() {
    with_default_timeout(|| {
        // Test that state machine records errors and counts them
        let mut machine = RebaseStateMachine::new("main".to_string());

        machine.record_error("Error 1".to_string());
        machine.record_error("Error 2".to_string());
        machine.record_error("Error 3".to_string());

        // After 3 errors, should abort (default max is 3)
        assert!(!machine.can_recover());
        assert!(machine.should_abort());
    });
}

/// Test that a checkpoint can be saved and loaded correctly.
///
/// This verifies that when a state machine's checkpoint is saved
/// and then loaded into a new machine, the state is preserved correctly.
#[test]
fn state_machine_checkpoint_roundtrip() {
    with_default_timeout(|| {
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
    });
}

/// Test that a checkpoint can be cleared correctly.
///
/// This verifies that when clear_checkpoint is called on a state machine,
/// the checkpoint file is removed from the filesystem.
#[test]
fn state_machine_clears_checkpoint() {
    with_default_timeout(|| {
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
    });
}

/// Test that all valid state transitions are allowed.
///
/// This verifies that when transition_to is called with valid phase transitions,
/// each transition succeeds and returns Ok.
#[test]
fn state_machine_allows_valid_transitions() {
    with_default_timeout(|| {
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
    });
}

/// Test that the abort transition works correctly.
///
/// This verifies that when abort is called on a state machine with conflicts,
/// the phase is set to RebaseAborted and this state persists when loaded.
#[test]
fn state_machine_handles_abort() {
    with_default_timeout(|| {
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
    });
}

/// Test that all RecoveryAction enum variants can be instantiated.
///
/// This verifies that the RecoveryAction enum has all expected variants:
/// Continue, Retry, Abort, and Skip.
#[test]
fn state_machine_recovery_action_variants() {
    with_default_timeout(|| {
        // Test that all RecoveryAction variants exist
        let _ = RecoveryAction::Continue;
        let _ = RecoveryAction::Retry;
        let _ = RecoveryAction::Abort;
        let _ = RecoveryAction::Skip;
    });
}
