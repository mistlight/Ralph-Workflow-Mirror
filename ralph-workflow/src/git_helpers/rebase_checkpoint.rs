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
        }
    }

    /// Set the phase of the rebase.
    pub fn with_phase(mut self, phase: RebasePhase) -> Self {
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
    pub fn with_error(mut self, error: String) -> Self {
        self.error_count += 1;
        self.last_error = Some(error);
        self.timestamp = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Check if all conflicts are resolved.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn all_conflicts_resolved(&self) -> bool {
        self.conflicted_files
            .iter()
            .all(|f| self.resolved_files.contains(f))
    }

    /// Get the number of unresolved conflicts.
    #[cfg(any(test, feature = "test-utils"))]
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

    Ok(())
}

/// Load an existing rebase checkpoint if one exists.
///
/// Returns `Ok(Some(checkpoint))` if a valid checkpoint was loaded,
/// `Ok(None)` if no checkpoint file exists, or an error if the file
/// exists but cannot be parsed.
///
/// # Errors
///
/// Returns an error if the checkpoint file exists but cannot be read
/// or contains invalid JSON.
pub fn load_rebase_checkpoint() -> io::Result<Option<RebaseCheckpoint>> {
    let checkpoint = rebase_checkpoint_path();
    let path = Path::new(&checkpoint);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let loaded_checkpoint: RebaseCheckpoint = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse rebase checkpoint: {e}"),
        )
    })?;

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
}
