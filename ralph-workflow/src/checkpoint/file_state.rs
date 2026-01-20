//! File system state capture and validation for checkpoints.
//!
//! This module provides functionality for capturing and validating the state
//! of key files in the repository to enable idempotent recovery.

use crate::checkpoint::execution_history::FileSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// File system state snapshot for key files.
///
/// Captures the state of important files that affect pipeline execution.
/// This enables validation on resume to detect unexpected changes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileSystemState {
    /// Snapshots of tracked files
    pub files: HashMap<String, FileSnapshot>,
    /// Git HEAD commit OID (if available)
    pub git_head_oid: Option<String>,
    /// Git branch name (if available)
    pub git_branch: Option<String>,
    /// Git status output (porcelain format) for tracking staged/unstaged changes
    pub git_status: Option<String>,
    /// List of modified files from git diff
    pub git_modified_files: Option<Vec<String>>,
}

impl FileSystemState {
    /// Create a new file system state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture the current state of key files.
    ///
    /// This includes files that are critical for pipeline execution:
    /// - PROMPT.md: The primary task description
    /// - .agent/PLAN.md: The implementation plan (if exists)
    /// - .agent/ISSUES.md: Review findings (if exists)
    /// - .agent/config.toml: Agent configuration (if exists)
    /// - .agent/start_commit: Baseline commit reference (if exists)
    /// - .agent/NOTES.md: Development notes (if exists)
    /// - .agent/status: Pipeline status file (if exists)
    pub fn capture_current() -> Self {
        let mut state = Self::new();

        // Always capture PROMPT.md
        state.capture_file("PROMPT.md");

        // Capture .agent/PLAN.md if it exists (moved to .agent directory)
        if Path::new(".agent/PLAN.md").exists() {
            state.capture_file(".agent/PLAN.md");
        }

        // Capture .agent/ISSUES.md if it exists (moved to .agent directory)
        if Path::new(".agent/ISSUES.md").exists() {
            state.capture_file(".agent/ISSUES.md");
        }

        // Capture .agent/config.toml if it exists
        if Path::new(".agent/config.toml").exists() {
            state.capture_file(".agent/config.toml");
        }

        // Capture .agent/start_commit if it exists
        if Path::new(".agent/start_commit").exists() {
            state.capture_file(".agent/start_commit");
        }

        // Capture .agent/NOTES.md if it exists
        if Path::new(".agent/NOTES.md").exists() {
            state.capture_file(".agent/NOTES.md");
        }

        // Capture .agent/status if it exists
        if Path::new(".agent/status").exists() {
            state.capture_file(".agent/status");
        }

        // Try to capture git state
        state.capture_git_state();

        state
    }

    /// Capture a single file's state.
    pub fn capture_file(&mut self, path: &str) {
        let path_obj = Path::new(path);
        let snapshot = if path_obj.exists() {
            if let Some(checksum) = crate::checkpoint::state::calculate_file_checksum(path_obj) {
                let metadata = std::fs::metadata(path_obj);
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                FileSnapshot::new(path, checksum, size, true)
            } else {
                FileSnapshot::not_found(path)
            }
        } else {
            FileSnapshot::not_found(path)
        };

        self.files.insert(path.to_string(), snapshot);
    }

    /// Capture git HEAD state and working tree status.
    fn capture_git_state(&mut self) {
        // Try to get HEAD OID
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
        {
            if output.status.success() {
                let oid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                self.git_head_oid = Some(oid);
            }
        }

        // Try to get branch name
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
        {
            if output.status.success() {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !branch.is_empty() && branch != "HEAD" {
                    self.git_branch = Some(branch);
                }
            }
        }

        // Capture git status --porcelain for tracking staged/unstaged changes
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
        {
            if output.status.success() {
                let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !status.is_empty() {
                    self.git_status = Some(status);
                }
            }
        }

        // Capture list of modified files from git diff
        if let Ok(output) = std::process::Command::new("git")
            .args(["diff", "--name-only"])
            .output()
        {
            if output.status.success() {
                let diff_output = String::from_utf8_lossy(&output.stdout);
                let modified_files: Vec<String> = diff_output
                    .lines()
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty())
                    .collect();
                if !modified_files.is_empty() {
                    self.git_modified_files = Some(modified_files);
                }
            }
        }
    }

    /// Validate the current file system state against this snapshot.
    ///
    /// Returns a list of validation errors. Empty list means all checks passed.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Validate each tracked file
        for (path, snapshot) in &self.files {
            if let Err(e) = self.validate_file(path, snapshot) {
                errors.push(e);
            }
        }

        // Validate git state if we captured it
        if let Err(e) = self.validate_git_state() {
            errors.push(e);
        }

        errors
    }

    /// Validate a single file against its snapshot.
    fn validate_file(&self, path: &str, snapshot: &FileSnapshot) -> Result<(), ValidationError> {
        let path_obj = Path::new(path);

        // Check existence
        if snapshot.exists && !path_obj.exists() {
            return Err(ValidationError::FileMissing {
                path: path.to_string(),
            });
        }

        if !snapshot.exists && path_obj.exists() {
            return Err(ValidationError::FileUnexpectedlyExists {
                path: path.to_string(),
            });
        }

        // Verify checksum for existing files
        if snapshot.exists && !snapshot.verify() {
            return Err(ValidationError::FileContentChanged {
                path: path.to_string(),
            });
        }

        Ok(())
    }

    /// Validate git state against the snapshot.
    fn validate_git_state(&self) -> Result<(), ValidationError> {
        // Validate HEAD OID if we captured it
        if let Some(expected_oid) = &self.git_head_oid {
            if let Ok(output) = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
            {
                if output.status.success() {
                    let current_oid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if current_oid != *expected_oid {
                        return Err(ValidationError::GitHeadChanged {
                            expected: expected_oid.clone(),
                            actual: current_oid,
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// Validation errors for file system state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidationError {
    /// A file that should exist is missing
    FileMissing { path: String },

    /// A file that shouldn't exist unexpectedly exists
    FileUnexpectedlyExists { path: String },

    /// A file's content has changed
    FileContentChanged { path: String },

    /// Git HEAD has changed
    GitHeadChanged { expected: String, actual: String },

    /// Git working tree has changes (files modified, staged, etc.)
    GitWorkingTreeChanged { changes: String },

    /// Git state is invalid
    GitStateInvalid { reason: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileMissing { path } => {
                write!(f, "File missing: {}", path)
            }
            Self::FileUnexpectedlyExists { path } => {
                write!(f, "File unexpectedly exists: {}", path)
            }
            Self::FileContentChanged { path } => {
                write!(f, "File content changed: {}", path)
            }
            Self::GitHeadChanged { expected, actual } => {
                write!(f, "Git HEAD changed: expected {}, got {}", expected, actual)
            }
            Self::GitWorkingTreeChanged { changes } => {
                write!(f, "Git working tree changed: {}", changes)
            }
            Self::GitStateInvalid { reason } => {
                write!(f, "Git state invalid: {}", reason)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Recovery suggestion for a validation error.
impl ValidationError {
    /// Get a suggested recovery action for this error.
    pub fn recovery_suggestion(&self) -> String {
        match self {
            Self::FileMissing { path } => {
                if path.contains("PROMPT.md")
                    || path.contains("PLAN.md")
                    || path.contains("ISSUES.md")
                {
                    format!(
                        "Restore {} from .agent/checkpoint.json or recreate from requirements",
                        path
                    )
                } else if path.contains(".agent/") {
                    format!(
                        "Restore {} from checkpoint or delete checkpoint to start fresh",
                        path
                    )
                } else {
                    format!("Restore {} from backup or recreate it", path)
                }
            }
            Self::FileUnexpectedlyExists { path } => {
                format!("Remove {} with: rm {}", path, path)
            }
            Self::FileContentChanged { path } => {
                if path.contains("PROMPT.md") {
                    format!(
                        "Review {} changes - ensure requirements are still captured",
                        path
                    )
                } else {
                    format!("Restore {} to its previous state or review changes", path)
                }
            }
            Self::GitHeadChanged { expected, .. } => {
                format!(
                    "Run: git reset {} (or review changes with git diff)",
                    expected
                )
            }
            Self::GitStateInvalid { reason } => {
                if reason.contains("detached") {
                    "Run: git checkout <branch-name> to attach to a branch".to_string()
                } else if reason.contains("merge") || reason.contains("rebase") {
                    "Run: git status to check for conflicts, then resolve or abort".to_string()
                } else {
                    format!("Review git state: {} - run git status", reason)
                }
            }
            Self::GitWorkingTreeChanged { .. } => {
                "Run: git status to review changes, or git stash to save changes".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use test_helpers::with_temp_cwd;

    #[test]
    fn test_file_system_state_new() {
        let state = FileSystemState::new();
        assert!(state.files.is_empty());
        assert!(state.git_head_oid.is_none());
        assert!(state.git_branch.is_none());
    }

    #[test]
    fn test_file_system_state_capture_file() {
        with_temp_cwd(|_dir| {
            fs::write("test.txt", "content").unwrap();

            let mut state = FileSystemState::new();
            state.capture_file("test.txt");

            assert!(state.files.contains_key("test.txt"));
            let snapshot = &state.files["test.txt"];
            assert!(snapshot.exists);
            assert_eq!(snapshot.size, 7);
        });
    }

    #[test]
    fn test_file_system_state_capture_nonexistent() {
        let mut state = FileSystemState::new();
        state.capture_file("nonexistent.txt");

        assert!(state.files.contains_key("nonexistent.txt"));
        let snapshot = &state.files["nonexistent.txt"];
        assert!(!snapshot.exists);
        assert_eq!(snapshot.size, 0);
    }

    #[test]
    fn test_file_system_state_validate_success() {
        with_temp_cwd(|_dir| {
            fs::write("test.txt", "content").unwrap();

            let mut state = FileSystemState::new();
            state.capture_file("test.txt");

            let errors = state.validate();
            assert!(errors.is_empty());
        });
    }

    #[test]
    fn test_file_system_state_validate_file_missing() {
        with_temp_cwd(|_dir| {
            // Create a file and capture its state
            fs::write("test.txt", "content").unwrap();
            let mut state = FileSystemState::new();
            state.capture_file("test.txt");

            // Now delete the file
            fs::remove_file("test.txt").unwrap();

            // Validation should fail because file is missing
            let errors = state.validate();
            assert!(!errors.is_empty());
            assert!(matches!(errors[0], ValidationError::FileMissing { .. }));
        });
    }

    #[test]
    fn test_file_system_state_validate_file_changed() {
        with_temp_cwd(|_dir| {
            fs::write("test.txt", "content").unwrap();

            let mut state = FileSystemState::new();
            state.capture_file("test.txt");

            // Modify the file
            fs::write("test.txt", "modified").unwrap();

            let errors = state.validate();
            assert!(!errors.is_empty());
            assert!(matches!(
                errors[0],
                ValidationError::FileContentChanged { .. }
            ));
        });
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::FileMissing {
            path: "test.txt".to_string(),
        };
        assert_eq!(err.to_string(), "File missing: test.txt");

        let err = ValidationError::FileContentChanged {
            path: "test.txt".to_string(),
        };
        assert_eq!(err.to_string(), "File content changed: test.txt");
    }

    #[test]
    fn test_validation_error_recovery_suggestion() {
        let err = ValidationError::FileMissing {
            path: "test.txt".to_string(),
        };
        assert!(err.recovery_suggestion().contains("test.txt"));

        let err = ValidationError::GitHeadChanged {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        assert!(err.recovery_suggestion().contains("abc123"));
    }
}
