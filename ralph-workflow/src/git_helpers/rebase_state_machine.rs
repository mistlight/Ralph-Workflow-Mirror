//! Rebase state machine for fault-tolerant rebase operations.
//!
//! This module provides a state machine that manages rebase operations
//! with checkpoint-based recovery. It tracks the current phase of a rebase
//! operation and can resume from interruptions.

#![deny(unsafe_code)]

use std::io;

use super::rebase_checkpoint::{
    clear_rebase_checkpoint, load_rebase_checkpoint, rebase_checkpoint_exists,
    save_rebase_checkpoint, RebaseCheckpoint, RebasePhase,
};

/// Default maximum number of recovery attempts.
#[cfg(any(test, feature = "test-utils"))]
const DEFAULT_MAX_RECOVERY_ATTEMPTS: u32 = 3;

/// State machine for fault-tolerant rebase operations.
///
/// This state machine manages rebase operations with:
/// - Checkpoint-based persistence
/// - Automatic recovery from interruptions
/// - Maximum recovery attempt limits
/// - Conflict tracking
pub struct RebaseStateMachine {
    /// Current checkpoint state
    checkpoint: RebaseCheckpoint,
    /// Maximum number of recovery attempts (only used in tests)
    #[cfg(any(test, feature = "test-utils"))]
    max_recovery_attempts: u32,
}

impl RebaseStateMachine {
    /// Create a new state machine for a rebase operation.
    ///
    /// # Arguments
    ///
    /// * `upstream_branch` - The branch to rebase onto
    pub fn new(upstream_branch: String) -> Self {
        Self {
            checkpoint: RebaseCheckpoint::new(upstream_branch),
            #[cfg(any(test, feature = "test-utils"))]
            max_recovery_attempts: DEFAULT_MAX_RECOVERY_ATTEMPTS,
        }
    }

    /// Load an existing state machine from checkpoint or create a new one.
    ///
    /// If a checkpoint exists, this will resume from that state.
    /// Otherwise, creates a new state machine.
    ///
    /// # Arguments
    ///
    /// * `upstream_branch` - The branch to rebase onto (used if no checkpoint exists)
    ///
    /// # Returns
    ///
    /// Returns `Ok(state_machine)` if successful, or an error if loading fails.
    pub fn load_or_create(upstream_branch: String) -> io::Result<Self> {
        if rebase_checkpoint_exists() {
            let checkpoint = load_rebase_checkpoint()?.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Checkpoint file exists but could not be loaded",
                )
            })?;
            Ok(Self {
                checkpoint,
                #[cfg(any(test, feature = "test-utils"))]
                max_recovery_attempts: DEFAULT_MAX_RECOVERY_ATTEMPTS,
            })
        } else {
            Ok(Self::new(upstream_branch))
        }
    }

    /// Set the maximum number of recovery attempts.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_max_recovery_attempts(mut self, max: u32) -> Self {
        self.max_recovery_attempts = max;
        self
    }

    /// Transition to a new phase and save checkpoint.
    ///
    /// # Arguments
    ///
    /// * `phase` - The new phase to transition to
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the transition succeeded, or an error if saving failed.
    pub fn transition_to(&mut self, phase: RebasePhase) -> io::Result<()> {
        self.checkpoint = self.checkpoint.clone().with_phase(phase);
        save_rebase_checkpoint(&self.checkpoint)
    }

    /// Record a conflict in a file.
    ///
    /// # Arguments
    ///
    /// * `file` - The file path that has conflicts
    pub fn record_conflict(&mut self, file: String) {
        self.checkpoint = self.checkpoint.clone().with_conflicted_file(file);
    }

    /// Record that a conflict has been resolved.
    ///
    /// # Arguments
    ///
    /// * `file` - The file path that was resolved
    pub fn record_resolution(&mut self, file: String) {
        self.checkpoint = self.checkpoint.clone().with_resolved_file(file);
    }

    /// Record an error that occurred.
    ///
    /// # Arguments
    ///
    /// * `error` - The error message to record
    pub fn record_error(&mut self, error: String) {
        self.checkpoint = self.checkpoint.clone().with_error(error);
    }

    /// Check if recovery is possible.
    ///
    /// Returns `true` if the error count is below the maximum recovery attempts.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn can_recover(&self) -> bool {
        self.checkpoint.error_count < self.max_recovery_attempts
    }

    /// Check if the rebase should be aborted.
    ///
    /// Returns `true` if the error count has exceeded the maximum recovery attempts.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn should_abort(&self) -> bool {
        self.checkpoint.error_count >= self.max_recovery_attempts
    }

    /// Check if all conflicts have been resolved.
    ///
    /// Returns `true` if all conflicted files have been marked as resolved.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn all_conflicts_resolved(&self) -> bool {
        self.checkpoint.all_conflicts_resolved()
    }

    /// Get the current checkpoint.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn checkpoint(&self) -> &RebaseCheckpoint {
        &self.checkpoint
    }

    /// Get the current phase.
    pub fn phase(&self) -> &RebasePhase {
        &self.checkpoint.phase
    }

    /// Get the upstream branch.
    pub fn upstream_branch(&self) -> &str {
        &self.checkpoint.upstream_branch
    }

    /// Get the number of unresolved conflicts.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn unresolved_conflict_count(&self) -> usize {
        self.checkpoint.unresolved_conflict_count()
    }

    /// Clear the checkpoint (typically on successful completion).
    pub fn clear_checkpoint(self) -> io::Result<()> {
        clear_rebase_checkpoint()
    }

    /// Force abort and save the aborted state.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn abort(mut self) -> io::Result<()> {
        self.checkpoint = self
            .checkpoint
            .clone()
            .with_phase(RebasePhase::RebaseAborted);
        save_rebase_checkpoint(&self.checkpoint)
    }
}

/// Actions that can be taken during recovery.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Continue with the rebase operation.
    Continue,
    /// Retry the current operation.
    Retry,
    /// Abort the rebase.
    Abort,
    /// Skip the current step and proceed.
    Skip,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_new() {
        let machine = RebaseStateMachine::new("main".to_string());
        assert_eq!(machine.phase(), &RebasePhase::NotStarted);
        assert_eq!(machine.upstream_branch(), "main");
        assert!(machine.can_recover());
        assert!(!machine.should_abort());
    }

    #[test]
    fn test_state_machine_transition() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine = RebaseStateMachine::new("main".to_string());
            machine
                .transition_to(RebasePhase::RebaseInProgress)
                .unwrap();
            assert_eq!(machine.phase(), &RebasePhase::RebaseInProgress);
        });
    }

    #[test]
    fn test_state_machine_record_conflict() {
        let mut machine = RebaseStateMachine::new("main".to_string());
        machine.record_conflict("file1.rs".to_string());
        machine.record_conflict("file2.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 2);
    }

    #[test]
    fn test_state_machine_record_resolution() {
        let mut machine = RebaseStateMachine::new("main".to_string());
        machine.record_conflict("file1.rs".to_string());
        machine.record_conflict("file2.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 2);

        machine.record_resolution("file1.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 1);
        assert!(!machine.all_conflicts_resolved());

        machine.record_resolution("file2.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 0);
        assert!(machine.all_conflicts_resolved());
    }

    #[test]
    fn test_state_machine_record_error() {
        let mut machine = RebaseStateMachine::new("main".to_string());
        assert!(machine.can_recover());
        assert!(!machine.should_abort());

        machine.record_error("First error".to_string());
        assert!(machine.can_recover());

        machine.record_error("Second error".to_string());
        assert!(machine.can_recover());

        machine.record_error("Third error".to_string());
        assert!(!machine.can_recover());
        assert!(machine.should_abort());
    }

    #[test]
    fn test_state_machine_custom_max_attempts() {
        let machine = RebaseStateMachine::new("main".to_string()).with_max_recovery_attempts(1);

        assert!(machine.can_recover());
    }

    #[test]
    fn test_state_machine_save_load() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine1 = RebaseStateMachine::new("feature-branch".to_string());
            machine1
                .transition_to(RebasePhase::ConflictDetected)
                .unwrap();

            // Note: record_conflict only updates in-memory state, need to save checkpoint
            // For the test, let's create a checkpoint with conflicts and save it
            use super::super::rebase_checkpoint::{
                save_rebase_checkpoint, RebaseCheckpoint, RebasePhase,
            };
            let checkpoint = RebaseCheckpoint::new("feature-branch".to_string())
                .with_phase(RebasePhase::ConflictDetected)
                .with_conflicted_file("test.rs".to_string());
            save_rebase_checkpoint(&checkpoint).unwrap();

            // Load a new machine from the checkpoint
            let machine2 = RebaseStateMachine::load_or_create("main".to_string()).unwrap();
            assert_eq!(machine2.phase(), &RebasePhase::ConflictDetected);
            assert_eq!(machine2.upstream_branch(), "feature-branch");
            assert_eq!(machine2.unresolved_conflict_count(), 1);
        });
    }

    #[test]
    fn test_state_machine_clear_checkpoint() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine = RebaseStateMachine::new("main".to_string());
            machine
                .transition_to(RebasePhase::RebaseInProgress)
                .unwrap();
            assert!(rebase_checkpoint_exists());

            machine.clear_checkpoint().unwrap();
            assert!(!rebase_checkpoint_exists());
        });
    }

    #[test]
    fn test_state_machine_abort() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine = RebaseStateMachine::new("main".to_string());
            machine
                .transition_to(RebasePhase::ConflictDetected)
                .unwrap();
            machine.abort().unwrap();

            let loaded = RebaseStateMachine::load_or_create("main".to_string()).unwrap();
            assert_eq!(loaded.phase(), &RebasePhase::RebaseAborted);
        });
    }

    #[test]
    fn test_recovery_action_variants_exist() {
        let _ = RecoveryAction::Continue;
        let _ = RecoveryAction::Retry;
        let _ = RecoveryAction::Abort;
        let _ = RecoveryAction::Skip;
    }
}
