//! Recovery and rollback mechanisms for checkpoint state.
//!
//! This module provides functionality for recovering from checkpoint failures
//! and rolling back to previous states when validation fails.

use crate::checkpoint::file_state::{FileSystemState, ValidationError};
use crate::checkpoint::state::PipelineCheckpoint;
use std::io;

/// Recovery strategy to use when validation fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Fail fast - require user intervention
    Fail,
    /// Attempt automatic recovery where possible
    Auto,
    /// Warn but continue (not recommended)
    Force,
}

/// Result of a recovery attempt.
#[derive(Debug)]
pub enum RecoveryResult {
    /// Recovery was successful
    Success,
    /// Recovery failed with an error
    Failed(String),
    /// Recovery requires user intervention
    NeedsIntervention(Vec<String>),
}

/// Checkpoint recovery manager.
///
/// Provides methods for handling checkpoint validation failures
/// and attempting recovery or rollback operations.
pub struct RecoveryManager {
    strategy: RecoveryStrategy,
}

impl RecoveryManager {
    /// Create a new recovery manager with the given strategy.
    pub fn new(strategy: RecoveryStrategy) -> Self {
        Self { strategy }
    }

    /// Attempt to recover from validation errors.
    ///
    /// Returns a recovery result indicating the outcome.
    pub fn recover_from_errors(
        &self,
        errors: &[ValidationError],
        checkpoint: &PipelineCheckpoint,
    ) -> RecoveryResult {
        match self.strategy {
            RecoveryStrategy::Fail => {
                // Fail fast - require user intervention
                let messages: Vec<String> = errors
                    .iter()
                    .map(|e| format!("  - {} (suggestion: {})", e, e.recovery_suggestion()))
                    .collect();

                RecoveryResult::NeedsIntervention(messages)
            }
            RecoveryStrategy::Auto => {
                // Attempt automatic recovery where possible
                let mut failed = Vec::new();
                let mut recovered = 0;

                for error in errors {
                    match self.attempt_auto_recovery(error, checkpoint) {
                        Ok(()) => recovered += 1,
                        Err(e) => failed.push(format!("  - {}: {}", error, e)),
                    }
                }

                if failed.is_empty() {
                    RecoveryResult::Success
                } else {
                    RecoveryResult::Failed(format!(
                        "Recovered {}/{} errors, but failed:\n{}",
                        recovered,
                        errors.len(),
                        failed.join("\n")
                    ))
                }
            }
            RecoveryStrategy::Force => {
                // Warn but continue
                let warnings: Vec<String> = errors.iter().map(|e| format!("  - {}", e)).collect();

                eprintln!(
                    "Warning: Resuming with validation errors:\n{}",
                    warnings.join("\n")
                );

                RecoveryResult::Success
            }
        }
    }

    /// Attempt automatic recovery for a single validation error.
    fn attempt_auto_recovery(
        &self,
        error: &ValidationError,
        _checkpoint: &PipelineCheckpoint,
    ) -> Result<(), String> {
        match error {
            // For file content changes, we can't auto-recover
            // This requires user intervention to review changes
            ValidationError::FileContentChanged { .. } => {
                Err("Cannot auto-recover content changes - requires review".to_string())
            }
            // For missing files, we can't auto-recover
            ValidationError::FileMissing { .. } => {
                Err("Cannot auto-recover missing files - requires restore".to_string())
            }
            // For git head changes, we could auto-reset but it's risky
            ValidationError::GitHeadChanged { .. } => {
                Err("Cannot auto-recover git changes - risky operation".to_string())
            }
            // Other errors also require intervention
            _ => Err(format!("Cannot auto-recover: {}", error)),
        }
    }

    /// Validate a checkpoint and attempt recovery if needed.
    ///
    /// This is a convenience method that combines validation and recovery.
    pub fn validate_and_recover(&self, checkpoint: &PipelineCheckpoint) -> Result<(), String> {
        // Create file system state from checkpoint
        let fs_state = self.create_fs_state_from_checkpoint(checkpoint);

        // Validate
        let errors = fs_state.validate();

        if errors.is_empty() {
            return Ok(());
        }

        // Attempt recovery
        match self.recover_from_errors(&errors, checkpoint) {
            RecoveryResult::Success => Ok(()),
            RecoveryResult::Failed(msg) => Err(msg),
            RecoveryResult::NeedsIntervention(messages) => {
                let msg = format!(
                    "Checkpoint validation failed. Please address the following:\n{}",
                    messages.join("\n")
                );
                Err(msg)
            }
        }
    }

    /// Create a FileSystemState from a checkpoint for validation.
    fn create_fs_state_from_checkpoint(&self, checkpoint: &PipelineCheckpoint) -> FileSystemState {
        let mut state = FileSystemState::new();

        // Capture PROMPT.md checksum if available
        if let Some(checksum) = &checkpoint.prompt_md_checksum {
            if let Ok(metadata) = std::fs::metadata("PROMPT.md") {
                let snapshot = crate::checkpoint::execution_history::FileSnapshot::new(
                    "PROMPT.md",
                    checksum.clone(),
                    metadata.len(),
                    true,
                );
                state.files.insert("PROMPT.md".to_string(), snapshot);
            }
        }

        state
    }
}

impl Default for RecoveryManager {
    fn default() -> Self {
        Self::new(RecoveryStrategy::Fail)
    }
}

/// Rollback options for checkpoint state.
#[derive(Debug, Clone, Default)]
pub struct RollbackOptions {
    /// Whether to restore file system state
    pub restore_files: bool,
    /// Whether to restore git state
    pub restore_git: bool,
    /// Whether to create a backup before rollback
    pub create_backup: bool,
}

/// Checkpoint rollback manager.
///
/// Provides methods for rolling back checkpoint state to a previous point.
pub struct RollbackManager {
    checkpoint_dir: String,
}

impl RollbackManager {
    /// Create a new rollback manager.
    pub fn new() -> io::Result<Self> {
        let checkpoint_dir = ".agent".to_string();
        std::fs::create_dir_all(&checkpoint_dir)?;

        Ok(Self { checkpoint_dir })
    }

    /// Rollback to a previous checkpoint.
    ///
    /// This is for future use when we support multiple checkpoint snapshots.
    pub fn rollback_to_checkpoint(
        &self,
        _target_checkpoint: &PipelineCheckpoint,
        _options: &RollbackOptions,
    ) -> io::Result<()> {
        // For now, we only support a single checkpoint file
        // In the future, we could support multiple checkpoints with timestamps
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Multiple checkpoint rollback not yet implemented",
        ))
    }

    /// Create a backup of the current checkpoint.
    ///
    /// This is useful before making destructive changes.
    pub fn backup_checkpoint(&self, checkpoint: &PipelineCheckpoint) -> io::Result<String> {
        // Use timestamp plus a unique suffix for uniqueness
        let unique_suffix = uuid::Uuid::new_v4().simple().to_string();
        let backup_path = format!(
            "{}/checkpoint.backup.{}.{}.json",
            self.checkpoint_dir,
            chrono::Local::now().format("%Y%m%d_%H%M%S"),
            &unique_suffix[..8] // Use first 8 chars of UUID
        );

        let json = serde_json::to_string_pretty(checkpoint).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize checkpoint: {e}"),
            )
        })?;

        std::fs::write(&backup_path, json)?;

        Ok(backup_path)
    }

    /// List all available checkpoint backups.
    pub fn list_backups(&self) -> io::Result<Vec<String>> {
        let mut backups = Vec::new();

        let entries = std::fs::read_dir(&self.checkpoint_dir)?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("checkpoint.backup.") && name_str.ends_with(".json") {
                backups.push(name_str.to_string());
            }
        }

        backups.sort();
        backups.reverse(); // Most recent first

        Ok(backups)
    }

    /// Delete a checkpoint backup.
    pub fn delete_backup(&self, backup_name: &str) -> io::Result<()> {
        let backup_path = format!("{}/{}", self.checkpoint_dir, backup_name);
        std::fs::remove_file(backup_path)
    }

    /// Clean up old backups, keeping only the most recent N.
    pub fn cleanup_old_backups(&self, keep: usize) -> io::Result<usize> {
        let mut backups = self.list_backups()?;

        if backups.len() <= keep {
            return Ok(0);
        }

        let to_delete = backups.split_off(keep);
        let deleted = to_delete.len();

        for backup in to_delete {
            self.delete_backup(&backup)?;
        }

        Ok(deleted)
    }
}

impl Default for RollbackManager {
    fn default() -> Self {
        Self::new().expect("Failed to create RollbackManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::state::{
        AgentConfigSnapshot, CheckpointParams, CliArgsSnapshot, PipelinePhase, RebaseState,
    };

    fn make_test_checkpoint() -> PipelineCheckpoint {
        let cli_args = CliArgsSnapshot::new(5, 2, "test".to_string(), None, false);
        let dev_config =
            AgentConfigSnapshot::new("claude".into(), "cmd".into(), "-o".into(), None, true);
        let rev_config =
            AgentConfigSnapshot::new("codex".into(), "cmd".into(), "-o".into(), None, true);
        let run_id = uuid::Uuid::new_v4().to_string();

        PipelineCheckpoint::from_params(CheckpointParams {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args,
            developer_agent_config: dev_config,
            reviewer_agent_config: rev_config,
            rebase_state: RebaseState::default(),
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: 2,
            actual_reviewer_runs: 0,
        })
    }

    #[test]
    fn test_recovery_manager_new() {
        let manager = RecoveryManager::new(RecoveryStrategy::Fail);
        assert_eq!(manager.strategy, RecoveryStrategy::Fail);
    }

    #[test]
    fn test_recovery_manager_default() {
        let manager = RecoveryManager::default();
        assert_eq!(manager.strategy, RecoveryStrategy::Fail);
    }

    #[test]
    fn test_recovery_strategy_fail() {
        let manager = RecoveryManager::new(RecoveryStrategy::Fail);
        let checkpoint = make_test_checkpoint();

        // Create validation errors
        let errors = vec![ValidationError::FileMissing {
            path: "test.txt".to_string(),
        }];

        match manager.recover_from_errors(&errors, &checkpoint) {
            RecoveryResult::NeedsIntervention(msgs) => {
                assert_eq!(msgs.len(), 1);
                assert!(msgs[0].contains("test.txt"));
            }
            _ => panic!("Expected NeedsIntervention"),
        }
    }

    #[test]
    fn test_recovery_strategy_auto() {
        let manager = RecoveryManager::new(RecoveryStrategy::Auto);
        let checkpoint = make_test_checkpoint();

        // Auto recovery typically fails for most errors
        let errors = vec![ValidationError::FileContentChanged {
            path: "test.txt".to_string(),
        }];

        match manager.recover_from_errors(&errors, &checkpoint) {
            RecoveryResult::Failed(_) => {
                // Expected - auto recovery fails for content changes
            }
            _ => panic!("Expected Failed for auto recovery of content changes"),
        }
    }

    #[test]
    fn test_rollback_manager_new() {
        let manager = RollbackManager::new().unwrap();
        assert_eq!(manager.checkpoint_dir, ".agent");
    }

    #[test]
    fn test_rollback_manager_default() {
        let manager = RollbackManager::default();
        assert_eq!(manager.checkpoint_dir, ".agent");
    }

    #[test]
    fn test_rollback_manager_backup() {
        test_helpers::with_temp_cwd(|_dir| {
            let manager = RollbackManager::new().unwrap();
            let checkpoint = make_test_checkpoint();

            let backup_path = manager.backup_checkpoint(&checkpoint).unwrap();

            assert!(backup_path.contains("checkpoint.backup."));
            assert!(std::path::Path::new(&backup_path).exists());

            // Clean up
            std::fs::remove_file(&backup_path).unwrap();
        });
    }

    #[test]
    fn test_rollback_manager_list_backups() {
        test_helpers::with_temp_cwd(|_dir| {
            let manager = RollbackManager::new().unwrap();
            let checkpoint = make_test_checkpoint();

            // Create a few backups
            manager.backup_checkpoint(&checkpoint).unwrap();
            manager.backup_checkpoint(&checkpoint).unwrap();

            let backups = manager.list_backups().unwrap();
            assert_eq!(backups.len(), 2);

            // Clean up
            for backup in backups {
                manager.delete_backup(&backup).unwrap();
            }
        });
    }

    #[test]
    fn test_rollback_manager_cleanup_old_backups() {
        test_helpers::with_temp_cwd(|_dir| {
            let manager = RollbackManager::new().unwrap();
            let checkpoint = make_test_checkpoint();

            // Create multiple backups
            for _ in 0..5 {
                manager.backup_checkpoint(&checkpoint).unwrap();
            }

            // Keep only 2
            let deleted = manager.cleanup_old_backups(2).unwrap();
            assert_eq!(deleted, 3);

            let backups = manager.list_backups().unwrap();
            assert_eq!(backups.len(), 2);
        });
    }

    #[test]
    fn test_rollback_manager_delete_backup() {
        test_helpers::with_temp_cwd(|_dir| {
            let manager = RollbackManager::new().unwrap();
            let checkpoint = make_test_checkpoint();

            manager.backup_checkpoint(&checkpoint).unwrap();
            let backups = manager.list_backups().unwrap();
            assert_eq!(backups.len(), 1);

            manager.delete_backup(&backups[0]).unwrap();
            let backups = manager.list_backups().unwrap();
            assert_eq!(backups.len(), 0);
        });
    }

    #[test]
    fn test_recovery_result_variants() {
        // Test that all RecoveryResult variants work
        let success = RecoveryResult::Success;
        assert!(matches!(success, RecoveryResult::Success));

        let failed = RecoveryResult::Failed("error".to_string());
        assert!(matches!(failed, RecoveryResult::Failed(_)));

        let needs_intervention = RecoveryResult::NeedsIntervention(vec![]);
        assert!(matches!(
            needs_intervention,
            RecoveryResult::NeedsIntervention(_)
        ));
    }

    #[test]
    fn test_rollback_options_default() {
        let options = RollbackOptions::default();
        assert!(!options.restore_files);
        assert!(!options.restore_git);
        assert!(!options.create_backup);
    }
}
