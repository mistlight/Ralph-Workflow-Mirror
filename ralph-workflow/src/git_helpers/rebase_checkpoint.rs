//! Rebase checkpoint system for fault tolerance.
//!
//! This module provides types and persistence for rebase state,
//! allowing recovery from interrupted or failed rebase operations.

#![deny(unsafe_code)]

use std::fs;
use std::io;
use std::path::Path;

/// Default directory for Ralph's internal files.
const AGENT_DIR: &str = ".agent";

/// Rebase checkpoint file name.
const REBASE_CHECKPOINT_FILE: &str = "rebase_checkpoint.json";

/// Get the rebase checkpoint file path.
///
/// The checkpoint is stored in `.agent/rebase_checkpoint.json`
/// relative to the current working directory.
pub fn rebase_checkpoint_path() -> String {
    format!("{AGENT_DIR}/{REBASE_CHECKPOINT_FILE}")
}

/// Get the rebase checkpoint backup file path.
///
/// The backup is stored in `.agent/rebase_checkpoint.json.bak`
/// and is used for corruption recovery.
pub fn rebase_checkpoint_backup_path() -> String {
    format!("{AGENT_DIR}/{REBASE_CHECKPOINT_FILE}.bak")
}

/// Phase of a rebase operation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RebasePhase {
    /// Rebase has not started.
    NotStarted,
    /// Pre-rebase validation in progress.
    PreRebaseCheck,
    /// Rebase operation is in progress.
    RebaseInProgress,
    /// Conflicts detected, awaiting resolution.
    ConflictDetected,
    /// Conflict resolution in progress.
    ConflictResolutionInProgress,
    /// Completing rebase after conflict resolution.
    CompletingRebase,
    /// Rebase completed successfully.
    RebaseComplete,
    /// Rebase was aborted.
    RebaseAborted,
}

impl RebasePhase {
    /// Get the maximum number of recovery attempts allowed for this phase.
    ///
    /// Different phases have different recovery limits:
    /// - ConflictResolutionInProgress: Higher limit (5) - conflicts may need multiple AI attempts
    /// - ConflictDetected: Medium limit (3) - waiting for AI to process
    /// - RebaseInProgress: Lower limit (2) - transient Git issues
    /// - CompletingRebase: Lower limit (2) - final stages should be quick
    /// - PreRebaseCheck: Low limit (1) - validation should pass immediately
    /// - Other phases: Default limit (3)
    ///
    /// # Returns
    ///
    /// The maximum number of recovery attempts for this phase.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn max_recovery_attempts(&self) -> u32 {
        match self {
            RebasePhase::ConflictResolutionInProgress => 5,
            RebasePhase::ConflictDetected => 3,
            RebasePhase::RebaseInProgress => 2,
            RebasePhase::CompletingRebase => 2,
            RebasePhase::PreRebaseCheck => 1,
            _ => 3,
        }
    }
}

/// Checkpoint data for rebase operations.
///
/// This structure contains all the information needed to resume
/// a rebase operation after an interruption.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RebaseCheckpoint {
    /// Current phase of the rebase.
    pub phase: RebasePhase,
    /// Upstream branch being rebased onto.
    pub upstream_branch: String,
    /// Files with conflicts.
    pub conflicted_files: Vec<String>,
    /// Files that have been resolved.
    pub resolved_files: Vec<String>,
    /// Number of errors encountered.
    pub error_count: u32,
    /// Last error message.
    pub last_error: Option<String>,
    /// Timestamp of checkpoint.
    pub timestamp: String,
    /// Number of errors encountered in the current phase.
    #[serde(default)]
    pub phase_error_count: u32,
}

impl Default for RebaseCheckpoint {
    fn default() -> Self {
        Self {
            phase: RebasePhase::NotStarted,
            upstream_branch: String::new(),
            conflicted_files: Vec::new(),
            resolved_files: Vec::new(),
            error_count: 0,
            last_error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            phase_error_count: 0,
        }
    }
}

impl RebaseCheckpoint {
    /// Create a new rebase checkpoint.
    pub fn new(upstream_branch: String) -> Self {
        Self {
            phase: RebasePhase::NotStarted,
            upstream_branch,
            conflicted_files: Vec::new(),
            resolved_files: Vec::new(),
            error_count: 0,
            last_error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            phase_error_count: 0,
        }
    }

    /// Set the phase of the rebase.
    ///
    /// Resets the phase error count when transitioning to a new phase.
    pub fn with_phase(mut self, phase: RebasePhase) -> Self {
        // Reset phase error count when transitioning to a new phase
        if self.phase != phase {
            self.phase_error_count = 0;
        }
        self.phase = phase;
        self.timestamp = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Add a conflicted file.
    pub fn with_conflicted_file(mut self, file: String) -> Self {
        if !self.conflicted_files.contains(&file) {
            self.conflicted_files.push(file);
        }
        self
    }

    /// Add a resolved file.
    pub fn with_resolved_file(mut self, file: String) -> Self {
        if !self.resolved_files.contains(&file) {
            self.resolved_files.push(file);
        }
        self
    }

    /// Add an error.
    ///
    /// Increments both the global error count and the phase-specific error count.
    pub fn with_error(mut self, error: String) -> Self {
        self.error_count += 1;
        self.phase_error_count += 1;
        self.last_error = Some(error);
        self.timestamp = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Check if all conflicts are resolved.
    pub fn all_conflicts_resolved(&self) -> bool {
        self.conflicted_files
            .iter()
            .all(|f| self.resolved_files.contains(f))
    }

    /// Get the number of unresolved conflicts.
    pub fn unresolved_conflict_count(&self) -> usize {
        self.conflicted_files
            .iter()
            .filter(|f| !self.resolved_files.contains(f))
            .count()
    }
}

/// Save a rebase checkpoint to disk.
///
/// Writes the checkpoint atomically by writing to a temp file first,
/// then renaming to the final path. This prevents corruption if the
/// process is interrupted during the write.
///
/// Also creates a backup before overwriting an existing checkpoint.
///
/// # Errors
///
/// Returns an error if serialization fails or the file cannot be written.
pub fn save_rebase_checkpoint(checkpoint: &RebaseCheckpoint) -> io::Result<()> {
    let json = serde_json::to_string_pretty(checkpoint).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize rebase checkpoint: {e}"),
        )
    })?;

    // Ensure the .agent directory exists before attempting to write
    fs::create_dir_all(AGENT_DIR)?;

    // Check if a checkpoint already exists (we'll need this info after saving)
    let checkpoint_existed = Path::new(&rebase_checkpoint_path()).exists();

    // Create backup before overwriting existing checkpoint
    let _ = backup_checkpoint();

    // Write atomically by writing to temp file then renaming
    let checkpoint_path_str = rebase_checkpoint_path();
    let temp_path = format!("{checkpoint_path_str}.tmp");

    // Ensure temp file is cleaned up even if write or rename fails
    let write_result = fs::write(&temp_path, &json);
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return write_result;
    }

    let rename_result = fs::rename(&temp_path, &checkpoint_path_str);
    if rename_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return rename_result;
    }

    // If this was the first save (no existing checkpoint before),
    // create a backup now so we always have a backup for recovery
    if !checkpoint_existed {
        let _ = backup_checkpoint();
    }

    Ok(())
}

/// Load an existing rebase checkpoint if one exists.
///
/// Returns `Ok(Some(checkpoint))` if a valid checkpoint was loaded,
/// `Ok(None)` if no checkpoint file exists, or an error if the file
/// exists but cannot be parsed.
///
/// If the main checkpoint is corrupted, attempts to restore from backup.
///
/// # Errors
///
/// Returns an error if the checkpoint file exists but cannot be read
/// or contains invalid JSON, and no valid backup exists.
pub fn load_rebase_checkpoint() -> io::Result<Option<RebaseCheckpoint>> {
    let checkpoint = rebase_checkpoint_path();
    let path = Path::new(&checkpoint);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let loaded_checkpoint: RebaseCheckpoint = match serde_json::from_str(&content) {
        Ok(cp) => cp,
        Err(e) => {
            // Checkpoint is corrupted - try to restore from backup
            eprintln!("Checkpoint corrupted, attempting restore from backup: {e}");
            return restore_from_backup();
        }
    };

    // Validate the loaded checkpoint
    if let Err(e) = validate_checkpoint(&loaded_checkpoint) {
        eprintln!("Checkpoint validation failed, attempting restore from backup: {e}");
        return restore_from_backup();
    }

    Ok(Some(loaded_checkpoint))
}

/// Delete the rebase checkpoint file.
///
/// Called on successful rebase completion to clean up the checkpoint.
/// Does nothing if the checkpoint file doesn't exist.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be deleted.
pub fn clear_rebase_checkpoint() -> io::Result<()> {
    let checkpoint = rebase_checkpoint_path();
    let path = Path::new(&checkpoint);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Check if a rebase checkpoint exists.
///
/// Returns `true` if a checkpoint file exists, `false` otherwise.
pub fn rebase_checkpoint_exists() -> bool {
    Path::new(&rebase_checkpoint_path()).exists()
}

/// Validate a checkpoint's integrity.
///
/// Checks that all required fields are present and valid.
/// Returns `Ok(())` if valid, or an error describing the issue.
#[cfg(any(test, feature = "test-utils"))]
pub fn validate_checkpoint(checkpoint: &RebaseCheckpoint) -> io::Result<()> {
    validate_checkpoint_impl(checkpoint)
}

/// Validate a checkpoint's integrity.
///
/// Checks that all required fields are present and valid.
/// Returns `Ok(())` if valid, or an error describing the issue.
#[cfg(not(any(test, feature = "test-utils")))]
fn validate_checkpoint(checkpoint: &RebaseCheckpoint) -> io::Result<()> {
    validate_checkpoint_impl(checkpoint)
}

/// Implementation of checkpoint validation.
fn validate_checkpoint_impl(checkpoint: &RebaseCheckpoint) -> io::Result<()> {
    // Validate upstream branch is not empty (unless it's a new checkpoint)
    if checkpoint.phase != RebasePhase::NotStarted && checkpoint.upstream_branch.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Checkpoint has empty upstream branch",
        ));
    }

    // Validate timestamp format
    if chrono::DateTime::parse_from_rfc3339(&checkpoint.timestamp).is_err() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Checkpoint has invalid timestamp format",
        ));
    }

    // Validate resolved files are a subset of conflicted files
    for resolved in &checkpoint.resolved_files {
        if !checkpoint.conflicted_files.contains(resolved) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Resolved file '{}' not found in conflicted files list",
                    resolved
                ),
            ));
        }
    }

    Ok(())
}

/// Create a backup of the current checkpoint.
///
/// Copies the current checkpoint file to a `.bak` file.
/// Returns `Ok(())` if backup succeeded, or an error if it failed.
///
/// If the checkpoint file doesn't exist, this is not an error
/// (the backup simply doesn't exist).
fn backup_checkpoint() -> io::Result<()> {
    let checkpoint_path = rebase_checkpoint_path();
    let backup_path = rebase_checkpoint_backup_path();
    let checkpoint = Path::new(&checkpoint_path);
    let backup = Path::new(&backup_path);

    if !checkpoint.exists() {
        // No checkpoint to back up - this is fine
        return Ok(());
    }

    // Remove existing backup if it exists
    if backup.exists() {
        fs::remove_file(backup)?;
    }

    // Copy checkpoint to backup
    fs::copy(checkpoint, backup)?;
    Ok(())
}

/// Restore a checkpoint from backup.
///
/// Attempts to restore from the backup file if the main checkpoint
/// is corrupted or missing. Returns `Ok(Some(checkpoint))` if restored,
/// `Ok(None)` if no backup exists, or an error if restoration failed.
fn restore_from_backup() -> io::Result<Option<RebaseCheckpoint>> {
    let backup_path = rebase_checkpoint_backup_path();
    let backup = Path::new(&backup_path);

    if !backup.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(backup)?;
    let checkpoint: RebaseCheckpoint = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse backup checkpoint: {e}"),
        )
    })?;

    // Validate the restored checkpoint
    validate_checkpoint(&checkpoint)?;

    // If valid, copy backup back to main checkpoint
    let checkpoint_path = rebase_checkpoint_path();
    fs::copy(backup, checkpoint_path)?;

    Ok(Some(checkpoint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebase_checkpoint_default() {
        let checkpoint = RebaseCheckpoint::default();
        assert_eq!(checkpoint.phase, RebasePhase::NotStarted);
        assert!(checkpoint.upstream_branch.is_empty());
        assert!(checkpoint.conflicted_files.is_empty());
        assert!(checkpoint.resolved_files.is_empty());
        assert_eq!(checkpoint.error_count, 0);
        assert!(checkpoint.last_error.is_none());
    }

    #[test]
    fn test_rebase_checkpoint_new() {
        let checkpoint = RebaseCheckpoint::new("main".to_string());
        assert_eq!(checkpoint.phase, RebasePhase::NotStarted);
        assert_eq!(checkpoint.upstream_branch, "main");
    }

    #[test]
    fn test_rebase_checkpoint_with_phase() {
        let checkpoint =
            RebaseCheckpoint::new("main".to_string()).with_phase(RebasePhase::RebaseInProgress);
        assert_eq!(checkpoint.phase, RebasePhase::RebaseInProgress);
    }

    #[test]
    fn test_rebase_checkpoint_with_conflicted_file() {
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_conflicted_file("file1.txt".to_string())
            .with_conflicted_file("file2.txt".to_string());
        assert_eq!(checkpoint.conflicted_files.len(), 2);
        // Adding duplicate should not increase count
        let checkpoint = checkpoint.with_conflicted_file("file1.txt".to_string());
        assert_eq!(checkpoint.conflicted_files.len(), 2);
    }

    #[test]
    fn test_rebase_checkpoint_with_resolved_file() {
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_conflicted_file("file1.txt".to_string())
            .with_resolved_file("file1.txt".to_string());
        assert!(checkpoint.resolved_files.contains(&"file1.txt".to_string()));
    }

    #[test]
    fn test_rebase_checkpoint_with_error() {
        let checkpoint =
            RebaseCheckpoint::new("main".to_string()).with_error("Test error".to_string());
        assert_eq!(checkpoint.error_count, 1);
        assert_eq!(checkpoint.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_rebase_checkpoint_all_conflicts_resolved() {
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_conflicted_file("file1.txt".to_string())
            .with_conflicted_file("file2.txt".to_string())
            .with_resolved_file("file1.txt".to_string())
            .with_resolved_file("file2.txt".to_string());
        assert!(checkpoint.all_conflicts_resolved());
    }

    #[test]
    fn test_rebase_checkpoint_unresolved_conflict_count() {
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_conflicted_file("file1.txt".to_string())
            .with_conflicted_file("file2.txt".to_string())
            .with_resolved_file("file1.txt".to_string());
        assert_eq!(checkpoint.unresolved_conflict_count(), 1);
    }

    #[test]
    fn test_rebase_phase_equality() {
        assert_eq!(RebasePhase::NotStarted, RebasePhase::NotStarted);
        assert_ne!(RebasePhase::NotStarted, RebasePhase::RebaseInProgress);
    }

    #[test]
    fn test_rebase_checkpoint_path() {
        let path = rebase_checkpoint_path();
        assert!(path.contains(".agent"));
        assert!(path.contains("rebase_checkpoint.json"));
    }

    #[test]
    fn test_save_load_rebase_checkpoint() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let checkpoint = RebaseCheckpoint::new("main".to_string())
                .with_phase(RebasePhase::ConflictDetected)
                .with_conflicted_file("file1.rs".to_string())
                .with_conflicted_file("file2.rs".to_string());

            save_rebase_checkpoint(&checkpoint).unwrap();
            assert!(rebase_checkpoint_exists());

            let loaded = load_rebase_checkpoint()
                .unwrap()
                .expect("checkpoint should exist after save");
            assert_eq!(loaded.phase, RebasePhase::ConflictDetected);
            assert_eq!(loaded.upstream_branch, "main");
            assert_eq!(loaded.conflicted_files.len(), 2);
        });
    }

    #[test]
    fn test_clear_rebase_checkpoint() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let checkpoint = RebaseCheckpoint::new("main".to_string());
            save_rebase_checkpoint(&checkpoint).unwrap();
            assert!(rebase_checkpoint_exists());

            clear_rebase_checkpoint().unwrap();
            assert!(!rebase_checkpoint_exists());
        });
    }

    #[test]
    fn test_load_nonexistent_rebase_checkpoint() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let result = load_rebase_checkpoint().unwrap();
            assert!(result.is_none());
            assert!(!rebase_checkpoint_exists());
        });
    }

    #[test]
    fn test_rebase_checkpoint_serialization() {
        let checkpoint = RebaseCheckpoint::new("feature-branch".to_string())
            .with_phase(RebasePhase::ConflictResolutionInProgress)
            .with_conflicted_file("src/lib.rs".to_string())
            .with_resolved_file("src/main.rs".to_string())
            .with_error("Test error".to_string());

        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains("feature-branch"));
        assert!(json.contains("src/lib.rs"));

        let deserialized: RebaseCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.phase, checkpoint.phase);
        assert_eq!(deserialized.upstream_branch, checkpoint.upstream_branch);
    }

    #[test]
    fn test_atomic_checkpoint_write() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Create a checkpoint
            let checkpoint1 =
                RebaseCheckpoint::new("main".to_string()).with_phase(RebasePhase::RebaseInProgress);

            save_rebase_checkpoint(&checkpoint1).unwrap();

            // Verify it was written
            assert!(rebase_checkpoint_exists());

            // Overwrite with a new checkpoint
            let checkpoint2 = RebaseCheckpoint::new("main".to_string())
                .with_phase(RebasePhase::RebaseComplete)
                .with_conflicted_file("test.rs".to_string());

            save_rebase_checkpoint(&checkpoint2).unwrap();

            // Load and verify the new state
            let loaded = load_rebase_checkpoint()
                .unwrap()
                .expect("checkpoint should exist");
            assert_eq!(loaded.phase, RebasePhase::RebaseComplete);
            assert_eq!(loaded.conflicted_files.len(), 1);
        });
    }

    #[test]
    fn test_validate_checkpoint_valid() {
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_phase(RebasePhase::RebaseInProgress)
            .with_conflicted_file("file1.rs".to_string())
            .with_resolved_file("file1.rs".to_string());

        assert!(validate_checkpoint(&checkpoint).is_ok());
    }

    #[test]
    fn test_validate_checkpoint_empty_upstream() {
        // NotStarted phase allows empty upstream
        let checkpoint = RebaseCheckpoint::new("".to_string()).with_phase(RebasePhase::NotStarted);
        assert!(validate_checkpoint(&checkpoint).is_ok());

        // Other phases require non-empty upstream
        let checkpoint =
            RebaseCheckpoint::new("".to_string()).with_phase(RebasePhase::RebaseInProgress);
        assert!(validate_checkpoint(&checkpoint).is_err());
    }

    #[test]
    fn test_validate_checkpoint_invalid_timestamp() {
        let mut checkpoint = RebaseCheckpoint::new("main".to_string());
        checkpoint.timestamp = "invalid-timestamp".to_string();

        assert!(validate_checkpoint(&checkpoint).is_err());
    }

    #[test]
    fn test_validate_checkpoint_resolved_without_conflicted() {
        let checkpoint =
            RebaseCheckpoint::new("main".to_string()).with_resolved_file("file1.rs".to_string());

        // Resolved file not in conflicted list should fail validation
        assert!(validate_checkpoint(&checkpoint).is_err());
    }

    #[test]
    fn test_checkpoint_backup_and_restore() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Create and save a checkpoint
            let checkpoint1 = RebaseCheckpoint::new("main".to_string())
                .with_phase(RebasePhase::ConflictDetected)
                .with_conflicted_file("file.rs".to_string());

            save_rebase_checkpoint(&checkpoint1).unwrap();

            // Verify checkpoint and backup exist
            let checkpoint_path = rebase_checkpoint_path();
            let backup_path = rebase_checkpoint_backup_path();
            assert!(Path::new(&checkpoint_path).exists());
            assert!(Path::new(&backup_path).exists());

            // Corrupt the main checkpoint
            fs::write(&checkpoint_path, "corrupted data {{{").unwrap();

            // Loading should restore from backup
            let loaded = load_rebase_checkpoint()
                .unwrap()
                .expect("should restore from backup");

            assert_eq!(loaded.phase, RebasePhase::ConflictDetected);
            assert_eq!(loaded.conflicted_files.len(), 1);
        });
    }

    #[test]
    fn test_checkpoint_save_creates_backup() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Create initial checkpoint
            let checkpoint1 =
                RebaseCheckpoint::new("main".to_string()).with_phase(RebasePhase::RebaseInProgress);
            save_rebase_checkpoint(&checkpoint1).unwrap();

            // Save another checkpoint (should create backup)
            let checkpoint2 =
                RebaseCheckpoint::new("main".to_string()).with_phase(RebasePhase::RebaseComplete);
            save_rebase_checkpoint(&checkpoint2).unwrap();

            // Backup should exist
            let backup_path = rebase_checkpoint_backup_path();
            assert!(Path::new(&backup_path).exists());

            // Verify backup has old data
            let backup_content = fs::read_to_string(&backup_path).unwrap();
            let backup_checkpoint: RebaseCheckpoint =
                serde_json::from_str(&backup_content).unwrap();
            assert_eq!(backup_checkpoint.phase, RebasePhase::RebaseInProgress);
        });
    }

    #[test]
    fn test_checkpoint_validation_failure_triggers_restore() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Create and save a valid checkpoint
            let checkpoint1 = RebaseCheckpoint::new("main".to_string())
                .with_phase(RebasePhase::RebaseInProgress)
                .with_conflicted_file("file.rs".to_string());

            save_rebase_checkpoint(&checkpoint1).unwrap();

            // Manually corrupt the checkpoint with invalid JSON but valid structure
            let checkpoint_path = rebase_checkpoint_path();
            let corrupted_json = r#"{
                "phase": "RebaseInProgress",
                "upstream_branch": "main",
                "conflicted_files": ["file.rs"],
                "resolved_files": ["not_in_conflicted.rs"],
                "error_count": 0,
                "last_error": null,
                "timestamp": "2024-01-01T00:00:00Z"
            }"#;
            fs::write(&checkpoint_path, corrupted_json).unwrap();

            // Loading should detect validation failure and restore from backup
            let loaded = load_rebase_checkpoint()
                .unwrap()
                .expect("should restore from backup");

            assert_eq!(loaded.conflicted_files.len(), 1);
            assert!(!loaded
                .resolved_files
                .contains(&"not_in_conflicted.rs".to_string()));
        });
    }
}
