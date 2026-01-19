//! Rebase state machine for fault-tolerant rebase operations.
//!
//! This module provides a state machine that manages rebase operations
//! with checkpoint-based recovery. It tracks the current phase of a rebase
//! operation and can resume from interruptions.

#![deny(unsafe_code)]

use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;

use super::rebase_checkpoint::{
    clear_rebase_checkpoint, load_rebase_checkpoint, rebase_checkpoint_exists,
    save_rebase_checkpoint, RebaseCheckpoint, RebasePhase,
};

/// Default maximum number of recovery attempts.
const DEFAULT_MAX_RECOVERY_ATTEMPTS: u32 = 3;

/// Rebase lock file name.
const REBASE_LOCK_FILE: &str = "rebase.lock";

/// Default lock timeout in seconds (30 minutes).
const DEFAULT_LOCK_TIMEOUT_SECONDS: u64 = 1800;

/// Get the rebase lock file path.
///
/// The lock is stored in `.agent/rebase.lock`
/// relative to the current working directory.
fn rebase_lock_path() -> String {
    format!(".agent/{REBASE_LOCK_FILE}")
}

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
    /// Maximum number of recovery attempts
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
                max_recovery_attempts: DEFAULT_MAX_RECOVERY_ATTEMPTS,
            })
        } else {
            Ok(Self::new(upstream_branch))
        }
    }

    /// Set the maximum number of recovery attempts.
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

/// RAII-style guard for rebase lock.
///
/// Automatically releases the lock when dropped.
#[cfg(any(test, feature = "test-utils"))]
pub struct RebaseLock {
    /// Whether we own the lock
    owns_lock: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl Drop for RebaseLock {
    fn drop(&mut self) {
        if self.owns_lock {
            let _ = release_rebase_lock();
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl RebaseLock {
    /// Create a new lock guard that owns the lock.
    fn new() -> io::Result<Self> {
        acquire_rebase_lock()?;
        Ok(Self { owns_lock: true })
    }

    /// Relinquish ownership of the lock without releasing it.
    ///
    /// This is useful when transferring ownership.
    #[must_use]
    pub fn leak(mut self) -> bool {
        let owned = self.owns_lock;
        self.owns_lock = false;
        owned
    }
}

/// Acquire the rebase lock.
///
/// Creates a lock file with the current process ID and timestamp.
/// Returns an error if the lock is held by another process.
///
/// # Errors
///
/// Returns an error if:
/// - The lock file exists and is not stale
/// - The lock file cannot be created
#[cfg(any(test, feature = "test-utils"))]
pub fn acquire_rebase_lock() -> io::Result<()> {
    let lock_path = rebase_lock_path();
    let path = Path::new(&lock_path);

    // Ensure .agent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if lock already exists
    if path.exists() {
        if !is_lock_stale()? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Rebase is already in progress. If you believe this is incorrect, \
                 wait 30 minutes for the lock to expire or manually remove `.agent/rebase.lock`.",
            ));
        }
        // Lock is stale, remove it
        fs::remove_file(&path)?;
    }

    // Create lock file with PID and timestamp
    let pid = std::process::id();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let lock_content = format!("pid={pid}\ntimestamp={timestamp}\n");

    let mut file = fs::File::create(&path)?;
    file.write_all(lock_content.as_bytes())?;
    file.sync_all()?;

    Ok(())
}

/// Release the rebase lock.
///
/// Removes the lock file. Does nothing if no lock exists.
///
/// # Errors
///
/// Returns an error if the lock file exists but cannot be removed.
#[cfg(any(test, feature = "test-utils"))]
pub fn release_rebase_lock() -> io::Result<()> {
    let lock_path = rebase_lock_path();
    let path = Path::new(&lock_path);

    if path.exists() {
        fs::remove_file(&path)?;
    }

    Ok(())
}

/// Check if the lock file is stale.
///
/// A lock is considered stale if it's older than the timeout period.
///
/// # Returns
///
/// Returns `true` if the lock is stale, `false` otherwise.
#[cfg(any(test, feature = "test-utils"))]
fn is_lock_stale() -> io::Result<bool> {
    let lock_path = rebase_lock_path();
    let path = Path::new(&lock_path);

    if !path.exists() {
        return Ok(false);
    }

    // Read lock file to get timestamp
    let content = fs::read_to_string(&path)?;

    // Parse timestamp from lock file
    let timestamp_line = content
        .lines()
        .find(|line| line.starts_with("timestamp="))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Lock file missing timestamp"))?;

    let timestamp_str = timestamp_line.strip_prefix("timestamp=").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid timestamp format in lock file",
        )
    })?;

    let lock_time = chrono::DateTime::parse_from_rfc3339(timestamp_str).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid timestamp format in lock file",
        )
    })?;

    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(lock_time);

    Ok(elapsed.num_seconds() > DEFAULT_LOCK_TIMEOUT_SECONDS as i64)
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

    #[test]
    fn test_acquire_and_release_rebase_lock() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Acquire lock
            acquire_rebase_lock().unwrap();

            // Verify lock file exists
            let lock_path = rebase_lock_path();
            assert!(Path::new(&lock_path).exists());

            // Release lock
            release_rebase_lock().unwrap();

            // Verify lock file is gone
            assert!(!Path::new(&lock_path).exists());
        });
    }

    #[test]
    fn test_rebase_lock_prevents_duplicate() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Acquire first lock
            acquire_rebase_lock().unwrap();

            // Trying to acquire again should fail
            let result = acquire_rebase_lock();
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("already in progress"));
        });
    }

    #[test]
    fn test_rebase_lock_guard_auto_releases() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            {
                // Create lock guard
                let _lock = RebaseLock::new().unwrap();
                let lock_path = rebase_lock_path();
                assert!(Path::new(&lock_path).exists());
            }
            // Lock should be released when guard goes out of scope

            let lock_path = rebase_lock_path();
            assert!(!Path::new(&lock_path).exists());
        });
    }

    #[test]
    fn test_rebase_lock_guard_leak() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            {
                let lock = RebaseLock::new().unwrap();
                let lock_path = rebase_lock_path();
                assert!(Path::new(&lock_path).exists());

                // Leak the lock - it won't be released
                lock.leak();
            }

            // Lock should still exist after guard is dropped
            let lock_path = rebase_lock_path();
            assert!(Path::new(&lock_path).exists());

            // Clean up
            let _ = release_rebase_lock();
        });
    }

    #[test]
    fn test_stale_lock_is_replaced() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Create a lock file with an old timestamp
            let lock_path = rebase_lock_path();
            let old_timestamp = chrono::Utc::now()
                - chrono::Duration::seconds(DEFAULT_LOCK_TIMEOUT_SECONDS as i64 + 60);
            let lock_content = format!("pid=12345\ntimestamp={}\n", old_timestamp.to_rfc3339());

            fs::create_dir_all(".agent").unwrap();
            fs::write(&lock_path, lock_content).unwrap();

            // Acquire lock should succeed since old lock is stale
            acquire_rebase_lock().unwrap();

            // Verify new lock file exists
            assert!(Path::new(&lock_path).exists());

            // Clean up
            release_rebase_lock().unwrap();
        });
    }
}
