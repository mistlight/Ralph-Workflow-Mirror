//! Rebase checkpoint system for fault tolerance.
//!
//! This module provides types for persisting and restoring rebase state,
//! allowing recovery from interrupted or failed rebase operations.
//!
//! NOTE: The full checkpoint save/load functionality will be implemented
//! as part of the rebase state machine. This module currently only provides
//! the data types needed for checkpoint management.

#![deny(unsafe_code)]

use std::io;

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
}
