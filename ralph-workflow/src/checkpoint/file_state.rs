//! File system state capture and validation for checkpoints.
//!
//! This module provides functionality for capturing and validating state
//! of key files in repository to enable idempotent recovery.

use crate::checkpoint::execution_history::FileSnapshot;
use crate::executor::{ProcessExecutor, RealProcessExecutor};
use crate::workspace::Workspace;
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

    /// Capture the current state with an optional executor.
    ///
    /// If executor is None, uses RealProcessExecutor (production default).
    ///
    /// # Note
    ///
    /// This function requires an explicit executor parameter to enable proper
    /// dependency injection for testing. For production code, pass
    /// `Some(&RealProcessExecutor::new())`.
    ///
    /// # Deprecated
    ///
    /// This function uses CWD-relative paths. Prefer `capture_with_workspace` for new code.
    pub fn capture_with_optional_executor(executor: Option<&dyn ProcessExecutor>) -> Self {
        match executor {
            Some(exec) => Self::capture_current_with_executor(exec),
            None => {
                // Create a temporary executor and capture the state
                // This is only used in code paths where no executor is available
                let real_executor = RealProcessExecutor::new();
                Self::capture_current_with_executor(&real_executor)
            }
        }
    }

    /// Capture the current state of key files using a workspace.
    ///
    /// This includes files that are critical for pipeline execution:
    /// - PROMPT.md: The primary task description
    /// - .agent/PLAN.md: The implementation plan (if exists)
    /// - .agent/ISSUES.md: Review findings (if exists)
    /// - .agent/config.toml: Agent configuration (if exists)
    /// - .agent/start_commit: Baseline commit reference (if exists)
    /// - .agent/NOTES.md: Development notes (if exists)
    /// - .agent/status: Pipeline status file (if exists)
    pub fn capture_with_workspace(
        workspace: &dyn Workspace,
        executor: &dyn ProcessExecutor,
    ) -> Self {
        let mut state = Self::new();

        // Always capture PROMPT.md
        state.capture_file_with_workspace(workspace, "PROMPT.md");

        // Capture .agent/PLAN.md if it exists
        if workspace.exists(Path::new(".agent/PLAN.md")) {
            state.capture_file_with_workspace(workspace, ".agent/PLAN.md");
        }

        // Capture .agent/ISSUES.md if it exists
        if workspace.exists(Path::new(".agent/ISSUES.md")) {
            state.capture_file_with_workspace(workspace, ".agent/ISSUES.md");
        }

        // Capture .agent/config.toml if it exists
        if workspace.exists(Path::new(".agent/config.toml")) {
            state.capture_file_with_workspace(workspace, ".agent/config.toml");
        }

        // Capture .agent/start_commit if it exists
        if workspace.exists(Path::new(".agent/start_commit")) {
            state.capture_file_with_workspace(workspace, ".agent/start_commit");
        }

        // Capture .agent/NOTES.md if it exists
        if workspace.exists(Path::new(".agent/NOTES.md")) {
            state.capture_file_with_workspace(workspace, ".agent/NOTES.md");
        }

        // Capture .agent/status if it exists
        if workspace.exists(Path::new(".agent/status")) {
            state.capture_file_with_workspace(workspace, ".agent/status");
        }

        // Try to capture git state
        state.capture_git_state(executor);

        state
    }

    /// Capture the current state of key files with a provided process executor.
    ///
    /// This includes files that are critical for pipeline execution:
    /// - PROMPT.md: The primary task description
    /// - .agent/PLAN.md: The implementation plan (if exists)
    /// - .agent/ISSUES.md: Review findings (if exists)
    /// - .agent/config.toml: Agent configuration (if exists)
    /// - .agent/start_commit: Baseline commit reference (if exists)
    /// - .agent/NOTES.md: Development notes (if exists)
    /// - .agent/status: Pipeline status file (if exists)
    ///
    /// # Deprecated
    ///
    /// This function uses CWD-relative paths. Prefer `capture_with_workspace` for new code.
    pub fn capture_current_with_executor(executor: &dyn ProcessExecutor) -> Self {
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
        state.capture_git_state(executor);

        state
    }

    /// Capture a single file's state using a workspace.
    pub fn capture_file_with_workspace(&mut self, workspace: &dyn Workspace, path: &str) {
        let path_ref = Path::new(path);
        let snapshot = if workspace.exists(path_ref) {
            if let Ok(content) = workspace.read_bytes(path_ref) {
                let checksum = crate::checkpoint::state::calculate_checksum_from_bytes(&content);
                let size = content.len() as u64;
                FileSnapshot::new(path, checksum, size, true)
            } else {
                FileSnapshot::not_found(path)
            }
        } else {
            FileSnapshot::not_found(path)
        };

        self.files.insert(path.to_string(), snapshot);
    }

    /// Capture a single file's state.
    ///
    /// # Deprecated
    ///
    /// This function uses CWD-relative paths. Prefer `capture_file_with_workspace` for new code.
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
    fn capture_git_state(&mut self, executor: &dyn ProcessExecutor) {
        // Try to get HEAD OID
        if let Ok(output) = executor.execute("git", &["rev-parse", "HEAD"], &[], None) {
            if output.status.success() {
                let oid = output.stdout.trim().to_string();
                self.git_head_oid = Some(oid);
            }
        }

        // Try to get branch name
        if let Ok(output) =
            executor.execute("git", &["rev-parse", "--abbrev-ref", "HEAD"], &[], None)
        {
            if output.status.success() {
                let branch = output.stdout.trim().to_string();
                if !branch.is_empty() && branch != "HEAD" {
                    self.git_branch = Some(branch);
                }
            }
        }

        // Capture git status --porcelain for tracking staged/unstaged changes
        if let Ok(output) = executor.execute("git", &["status", "--porcelain"], &[], None) {
            if output.status.success() {
                let status = output.stdout.trim().to_string();
                if !status.is_empty() {
                    self.git_status = Some(status);
                }
            }
        }

        // Capture list of modified files from git diff
        if let Ok(output) = executor.execute("git", &["diff", "--name-only"], &[], None) {
            if output.status.success() {
                let diff_output = &output.stdout;
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
    ///
    /// # Deprecated
    ///
    /// This function uses CWD-relative paths. Prefer `validate_with_workspace` for new code.
    pub fn validate(&self) -> Vec<ValidationError> {
        self.validate_with_executor(None)
    }

    /// Validate the current file system state against this snapshot using a workspace.
    ///
    /// Returns a list of validation errors. Empty list means all checks passed.
    pub fn validate_with_workspace(
        &self,
        workspace: &dyn Workspace,
        executor: Option<&dyn ProcessExecutor>,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Validate each tracked file
        for (path, snapshot) in &self.files {
            if let Err(e) = self.validate_file_with_workspace(workspace, path, snapshot) {
                errors.push(e);
            }
        }

        // Validate git state if we captured it and executor was provided
        if let Some(exec) = executor {
            if let Err(e) = self.validate_git_state_with_executor(exec) {
                errors.push(e);
            }
        }

        errors
    }

    /// Validate the current file system state against this snapshot with a provided executor.
    ///
    /// Returns a list of validation errors. Empty list means all checks passed.
    ///
    /// # Deprecated
    ///
    /// This function uses CWD-relative paths. Prefer `validate_with_workspace` for new code.
    pub fn validate_with_executor(
        &self,
        executor: Option<&dyn ProcessExecutor>,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Validate each tracked file
        for (path, snapshot) in &self.files {
            if let Err(e) = self.validate_file(path, snapshot) {
                errors.push(e);
            }
        }

        // Validate git state if we captured it and executor was provided
        if let Some(exec) = executor {
            if let Err(e) = self.validate_git_state_with_executor(exec) {
                errors.push(e);
            }
        }

        errors
    }

    /// Validate a single file against its snapshot using a workspace.
    fn validate_file_with_workspace(
        &self,
        workspace: &dyn Workspace,
        path: &str,
        snapshot: &FileSnapshot,
    ) -> Result<(), ValidationError> {
        let path_ref = Path::new(path);

        // Check existence
        if snapshot.exists && !workspace.exists(path_ref) {
            return Err(ValidationError::FileMissing {
                path: path.to_string(),
            });
        }

        if !snapshot.exists && workspace.exists(path_ref) {
            return Err(ValidationError::FileUnexpectedlyExists {
                path: path.to_string(),
            });
        }

        // Verify checksum for existing files
        if snapshot.exists && !snapshot.verify_with_workspace(workspace) {
            return Err(ValidationError::FileContentChanged {
                path: path.to_string(),
            });
        }

        Ok(())
    }

    /// Validate a single file against its snapshot.
    ///
    /// # Deprecated
    ///
    /// This function uses CWD-relative paths. Prefer `validate_file_with_workspace` for new code.
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

        // Verify checksum for existing files - use old verify method that reads from CWD
        // This is deprecated but kept for backward compatibility
        if snapshot.exists {
            // Read file and verify checksum manually since we don't have workspace
            let content = std::fs::read(path_obj);
            let matches = match content {
                Ok(bytes) => {
                    if bytes.len() as u64 != snapshot.size {
                        false
                    } else {
                        let checksum =
                            crate::checkpoint::state::calculate_checksum_from_bytes(&bytes);
                        checksum == snapshot.checksum
                    }
                }
                Err(_) => false,
            };
            if !matches {
                return Err(ValidationError::FileContentChanged {
                    path: path.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Validate git state against the snapshot with a provided process executor.
    fn validate_git_state_with_executor(
        &self,
        executor: &dyn ProcessExecutor,
    ) -> Result<(), ValidationError> {
        // Validate HEAD OID if we captured it
        if let Some(expected_oid) = &self.git_head_oid {
            if let Ok(output) = executor.execute("git", &["rev-parse", "HEAD"], &[], None) {
                if output.status.success() {
                    let current_oid = output.stdout.trim().to_string();
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
    /// Get a structured recovery guide with "What's wrong" and "How to fix" sections.
    ///
    /// Returns a tuple of (problem_description, recovery_commands) where:
    /// - problem_description explains what the issue is
    /// - recovery_commands is a vector of suggested commands to fix it
    pub fn recovery_commands(&self) -> (String, Vec<String>) {
        match self {
            Self::FileMissing { path } => {
                let problem = format!(
                    "The file '{}' is missing but was present when the checkpoint was created.",
                    path
                );
                let commands = if path.contains("PROMPT.md") {
                    vec![
                        format!("# Check if file exists elsewhere"),
                        format!("find . -name 'PROMPT.md' -type f 2>/dev/null"),
                        format!(""),
                        format!("# Or recreate from requirements"),
                        format!("# Restore from backup or recreate PROMPT.md"),
                        format!(""),
                        format!("# If unrecoverable, delete checkpoint to start fresh"),
                        format!("rm .agent/checkpoint.json"),
                    ]
                } else if path.contains(".agent/") {
                    vec![
                        format!("# Agent files should be restored from checkpoint if available"),
                        format!(""),
                        format!("# Or delete checkpoint to start fresh"),
                        format!("rm .agent/checkpoint.json"),
                    ]
                } else {
                    vec![
                        format!("# Restore from backup or recreate"),
                        format!("git checkout HEAD -- {}", path),
                        format!(""),
                        format!("# Or if unrecoverable, delete checkpoint"),
                        format!("rm .agent/checkpoint.json"),
                    ]
                };
                (problem, commands)
            }
            Self::FileUnexpectedlyExists { path } => {
                let problem = format!("The file '{}' should not exist but was found.", path);
                let commands = vec![
                    format!("# Review the file to see if it should be kept"),
                    format!("cat {}", path),
                    format!(""),
                    format!("# If it should be removed:"),
                    format!("rm {}", path),
                    format!(""),
                    format!("# Or if it should be kept, delete the checkpoint to start fresh"),
                    format!("rm .agent/checkpoint.json"),
                ];
                (problem, commands)
            }
            Self::FileContentChanged { path } => {
                let problem = format!(
                    "The content of '{}' has changed since the checkpoint was created.",
                    path
                );
                let commands = if path.contains("PROMPT.md") {
                    vec![
                        format!("# Review the changes to ensure requirements are still correct"),
                        format!("git diff -- {}", path),
                        format!(""),
                        format!("# If changes are incorrect, revert:"),
                        format!("git checkout HEAD -- {}", path),
                        format!(""),
                        format!("# If changes are correct and intentional, use --recovery-strategy=force"),
                    ]
                } else {
                    vec![
                        format!("# Review the changes"),
                        format!("git diff -- {}", path),
                        format!(""),
                        format!("# If changes are incorrect, revert:"),
                        format!("git checkout HEAD -- {}", path),
                        format!(""),
                        format!("# Or stash current changes and restore from checkpoint"),
                        format!("git stash"),
                    ]
                };
                (problem, commands)
            }
            Self::GitHeadChanged { expected, actual } => {
                let problem = format!("Git HEAD has changed from {} to {}. New commits may have been made or HEAD was reset.", expected, actual);
                let commands = vec![
                    format!("# View the commits that were made after checkpoint"),
                    format!("git log {}..HEAD --oneline", expected),
                    format!(""),
                    format!("# Option 1: Reset to checkpoint state"),
                    format!("git reset {}", expected),
                    format!(""),
                    format!("# Option 2: Accept new state and delete checkpoint"),
                    format!("rm .agent/checkpoint.json"),
                    format!(""),
                    format!("# Option 3: Use --recovery-strategy=force to proceed anyway (risky)"),
                ];
                (problem, commands)
            }
            Self::GitStateInvalid { reason } => {
                let problem = format!("Git state is invalid: {}", reason);
                let commands = if reason.contains("detached") {
                    vec![
                        format!("# View current branch situation"),
                        format!("git branch -a"),
                        format!(""),
                        format!("# Reattach to a branch"),
                        format!("git checkout <branch-name>"),
                        format!(""),
                        format!("# Or list recent commits to choose from"),
                        format!("git log --oneline -10"),
                    ]
                } else if reason.contains("merge") || reason.contains("rebase") {
                    vec![
                        format!("# Check current git status"),
                        format!("git status"),
                        format!(""),
                        format!("# Option 1: Continue the operation"),
                        format!("# (resolve conflicts, then git add/rm && git continue)"),
                        format!(""),
                        format!("# Option 2: Abort the operation"),
                        format!("git merge --abort  # or 'git rebase --abort'"),
                        format!(""),
                        format!("# Option 3: Delete checkpoint and start fresh"),
                        format!("rm .agent/checkpoint.json"),
                    ]
                } else {
                    vec![
                        format!("# Check current git status"),
                        format!("git status"),
                        format!(""),
                        format!("# Fix the reported issue or delete checkpoint to start fresh"),
                        format!("rm .agent/checkpoint.json"),
                    ]
                };
                (problem, commands)
            }
            Self::GitWorkingTreeChanged { changes } => {
                let problem = format!("Git working tree has uncommitted changes: {}", changes);
                let commands = vec![
                    format!("# View what changed"),
                    format!("git status"),
                    format!("git diff"),
                    format!(""),
                    format!("# Option 1: Commit the changes"),
                    format!("git add -A && git commit -m 'Save work before resume'"),
                    format!(""),
                    format!("# Option 2: Stash the changes"),
                    format!("git stash push -m 'Work saved before resume'"),
                    format!(""),
                    format!("# Option 3: Discard the changes"),
                    format!("git reset --hard HEAD"),
                    format!(""),
                    format!("# Option 4: Use --recovery-strategy=force to proceed anyway"),
                ];
                (problem, commands)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Workspace-based tests (for testability without real filesystem)
    // =========================================================================

    #[cfg(feature = "test-utils")]
    mod workspace_tests {
        use super::*;
        use crate::workspace::MemoryWorkspace;

        #[test]
        fn test_file_system_state_new() {
            let state = FileSystemState::new();
            assert!(state.files.is_empty());
            assert!(state.git_head_oid.is_none());
            assert!(state.git_branch.is_none());
        }

        #[test]
        fn test_capture_file_with_workspace() {
            let workspace = MemoryWorkspace::new_test().with_file("test.txt", "content");

            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace, "test.txt");

            assert!(state.files.contains_key("test.txt"));
            let snapshot = &state.files["test.txt"];
            assert!(snapshot.exists);
            assert_eq!(snapshot.size, 7);
        }

        #[test]
        fn test_capture_file_with_workspace_nonexistent() {
            let workspace = MemoryWorkspace::new_test();

            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace, "nonexistent.txt");

            assert!(state.files.contains_key("nonexistent.txt"));
            let snapshot = &state.files["nonexistent.txt"];
            assert!(!snapshot.exists);
            assert_eq!(snapshot.size, 0);
        }

        #[test]
        fn test_validate_with_workspace_success() {
            let workspace = MemoryWorkspace::new_test().with_file("test.txt", "content");

            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace, "test.txt");

            let errors = state.validate_with_workspace(&workspace, None);
            assert!(errors.is_empty());
        }

        #[test]
        fn test_validate_with_workspace_file_missing() {
            // Create workspace with file, capture state
            let workspace_with_file = MemoryWorkspace::new_test().with_file("test.txt", "content");
            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace_with_file, "test.txt");

            // Create new workspace without the file (simulating file deletion)
            let workspace_without_file = MemoryWorkspace::new_test();

            // Validation should fail because file is missing
            let errors = state.validate_with_workspace(&workspace_without_file, None);
            assert!(!errors.is_empty());
            assert!(matches!(errors[0], ValidationError::FileMissing { .. }));
        }

        #[test]
        fn test_validate_with_workspace_file_changed() {
            // Create workspace with original file
            let workspace_original = MemoryWorkspace::new_test().with_file("test.txt", "content");
            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace_original, "test.txt");

            // Create new workspace with modified content
            let workspace_modified = MemoryWorkspace::new_test().with_file("test.txt", "modified");

            let errors = state.validate_with_workspace(&workspace_modified, None);
            assert!(!errors.is_empty());
            assert!(matches!(
                errors[0],
                ValidationError::FileContentChanged { .. }
            ));
        }

        #[test]
        fn test_validate_with_workspace_file_unexpectedly_exists() {
            // Create state with non-existent file
            let workspace_empty = MemoryWorkspace::new_test();
            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace_empty, "test.txt");

            // Create new workspace with the file (simulating unexpected file creation)
            let workspace_with_file = MemoryWorkspace::new_test().with_file("test.txt", "content");

            let errors = state.validate_with_workspace(&workspace_with_file, None);
            assert!(!errors.is_empty());
            assert!(matches!(
                errors[0],
                ValidationError::FileUnexpectedlyExists { .. }
            ));
        }
    }

    // =========================================================================
    // Pure unit tests (no filesystem access)
    // =========================================================================

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
        let (problem, commands) = err.recovery_commands();
        assert!(problem.contains("test.txt"));
        assert!(!commands.is_empty());

        let err = ValidationError::GitHeadChanged {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let (problem, commands) = err.recovery_commands();
        assert!(problem.contains("abc123"));
        assert!(commands.iter().any(|c| c.contains("git reset")));
    }

    #[test]
    fn test_validation_error_recovery_commands_file_missing() {
        let err = ValidationError::FileMissing {
            path: "PROMPT.md".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("missing"));
        assert!(problem.contains("PROMPT.md"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("find")));
    }

    #[test]
    fn test_validation_error_recovery_commands_git_head_changed() {
        let err = ValidationError::GitHeadChanged {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("changed"));
        assert!(problem.contains("abc123"));
        assert!(problem.contains("def456"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git reset")));
        assert!(commands.iter().any(|c| c.contains("git log")));
    }

    #[test]
    fn test_validation_error_recovery_commands_working_tree_changed() {
        let err = ValidationError::GitWorkingTreeChanged {
            changes: "M file1.txt\nM file2.txt".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("uncommitted changes"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git status")));
        assert!(commands.iter().any(|c| c.contains("git stash")));
        assert!(commands.iter().any(|c| c.contains("git commit")));
    }

    #[test]
    fn test_validation_error_recovery_commands_git_state_invalid() {
        let err = ValidationError::GitStateInvalid {
            reason: "detached HEAD state".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("detached HEAD state"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git checkout")));
    }

    #[test]
    fn test_validation_error_recovery_commands_file_content_changed() {
        let err = ValidationError::FileContentChanged {
            path: "PROMPT.md".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("changed"));
        assert!(problem.contains("PROMPT.md"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git diff")));
    }
}
