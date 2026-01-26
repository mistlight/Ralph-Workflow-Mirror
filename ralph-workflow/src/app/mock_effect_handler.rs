//! Mock implementation of AppEffectHandler for testing.
//!
//! This module provides a mock handler that captures all executed effects
//! for later inspection, maintains an in-memory filesystem state, and provides
//! builder methods for test configuration.
//!
//! # Example
//!
//! ```ignore
//! use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
//! use ralph_workflow::app::effect::{AppEffect, AppEffectHandler};
//!
//! let mut handler = MockAppEffectHandler::new()
//!     .with_file("config.toml", "key = value")
//!     .with_head_oid("abc1234");
//!
//! handler.execute(AppEffect::ReadFile {
//!     path: PathBuf::from("config.toml"),
//! });
//!
//! assert!(handler.was_executed(&AppEffect::ReadFile {
//!     path: PathBuf::from("config.toml"),
//! }));
//! ```

#![cfg(any(test, feature = "test-utils"))]

use super::effect::{AppEffect, AppEffectHandler, AppEffectResult, CommitResult, RebaseResult};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

/// Mock implementation of [`AppEffectHandler`] for testing.
///
/// This handler captures all executed effects for later inspection while
/// simulating the effects in an in-memory environment. It provides:
///
/// - **Effect capture**: All executed effects are stored and can be inspected
/// - **In-memory filesystem**: Files are stored in a `HashMap` for fast access
/// - **Configurable state**: Builder methods allow pre-populating files and git state
/// - **Test assertions**: Helper methods simplify test assertions
///
/// # Interior Mutability
///
/// Uses `RefCell` for interior mutability so that tests can inspect state
/// even when the handler is borrowed mutably during execution.
#[derive(Debug, Default)]
pub struct MockAppEffectHandler {
    /// All effects that have been executed, in order.
    captured_effects: RefCell<Vec<AppEffect>>,
    /// In-memory filesystem: path -> content.
    files: RefCell<HashMap<PathBuf, String>>,
    /// Current working directory (simulated).
    cwd: RefCell<PathBuf>,
    /// Whether a git repository exists.
    repo_exists: RefCell<bool>,
    /// The simulated HEAD OID.
    head_oid: RefCell<String>,
    /// The simulated default branch name.
    default_branch: RefCell<String>,
    /// Whether the current branch is main/master.
    is_main_branch: RefCell<bool>,
    /// Environment variables.
    env_vars: RefCell<HashMap<String, String>>,
    /// Log messages captured from logging effects.
    log_messages: RefCell<Vec<(String, String)>>,
    /// Simulated diff output.
    diff_output: RefCell<String>,
    /// Simulated snapshot output.
    snapshot_output: RefCell<String>,
    /// Whether git add staged anything.
    staged_changes: RefCell<bool>,
    /// List of conflicted files (for rebase simulation).
    conflicted_files: RefCell<Vec<String>>,
    /// Simulated rebase result.
    rebase_result: RefCell<Option<RebaseResult>>,
}

impl MockAppEffectHandler {
    /// Create a new mock handler with default state.
    ///
    /// Default state includes:
    /// - Empty filesystem
    /// - Current directory is "/"
    /// - Git repository exists
    /// - HEAD OID is "0000000"
    /// - Default branch is "main"
    /// - Not on main branch
    pub fn new() -> Self {
        Self {
            captured_effects: RefCell::new(Vec::new()),
            files: RefCell::new(HashMap::new()),
            cwd: RefCell::new(PathBuf::from("/")),
            repo_exists: RefCell::new(true),
            head_oid: RefCell::new("0000000".to_string()),
            default_branch: RefCell::new("main".to_string()),
            is_main_branch: RefCell::new(false),
            env_vars: RefCell::new(HashMap::new()),
            log_messages: RefCell::new(Vec::new()),
            diff_output: RefCell::new(String::new()),
            snapshot_output: RefCell::new(String::new()),
            staged_changes: RefCell::new(true),
            conflicted_files: RefCell::new(Vec::new()),
            rebase_result: RefCell::new(None),
        }
    }

    // =========================================================================
    // Builder Methods
    // =========================================================================

    /// Add a file to the in-memory filesystem.
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file
    /// * `content` - The content of the file
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_file("config.toml", "[section]\nkey = value")
    ///     .with_file(".agent/start_commit", "abc1234");
    /// ```
    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        self.files.borrow_mut().insert(path.into(), content.into());
        self
    }

    /// Set the current branch as main/master.
    ///
    /// This affects the result of `GitIsMainBranch` effects.
    pub fn on_main_branch(self) -> Self {
        *self.is_main_branch.borrow_mut() = true;
        self
    }

    /// Set the simulated HEAD OID.
    ///
    /// This affects the result of `GitGetHeadOid` effects.
    pub fn with_head_oid(self, oid: impl Into<String>) -> Self {
        *self.head_oid.borrow_mut() = oid.into();
        self
    }

    /// Configure the handler to simulate no git repository.
    ///
    /// This will cause `GitRequireRepo` effects to return an error.
    pub fn without_repo(self) -> Self {
        *self.repo_exists.borrow_mut() = false;
        self
    }

    /// Set the simulated default branch name.
    ///
    /// This affects the result of `GitGetDefaultBranch` effects.
    pub fn with_default_branch(self, branch: impl Into<String>) -> Self {
        *self.default_branch.borrow_mut() = branch.into();
        self
    }

    /// Set an environment variable.
    ///
    /// This affects the result of `GetEnvVar` effects.
    pub fn with_env_var(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.borrow_mut().insert(name.into(), value.into());
        self
    }

    /// Set the simulated diff output.
    ///
    /// This affects the result of `GitDiff` and related effects.
    pub fn with_diff(self, diff: impl Into<String>) -> Self {
        *self.diff_output.borrow_mut() = diff.into();
        self
    }

    /// Set whether git add will stage changes.
    ///
    /// This affects the result of `GitAddAll` effects.
    pub fn with_staged_changes(self, staged: bool) -> Self {
        *self.staged_changes.borrow_mut() = staged;
        self
    }

    /// Set the simulated rebase result.
    ///
    /// This affects the result of `GitRebaseOnto` effects.
    pub fn with_rebase_result(self, result: RebaseResult) -> Self {
        *self.rebase_result.borrow_mut() = Some(result);
        self
    }

    /// Set the simulated conflicted files.
    ///
    /// This affects the result of `GitGetConflictedFiles` effects.
    pub fn with_conflicted_files(self, files: Vec<String>) -> Self {
        *self.conflicted_files.borrow_mut() = files;
        self
    }

    /// Set the current working directory.
    ///
    /// This affects the initial CWD state.
    pub fn with_cwd(self, cwd: impl Into<PathBuf>) -> Self {
        *self.cwd.borrow_mut() = cwd.into();
        self
    }

    // =========================================================================
    // Inspection Methods
    // =========================================================================

    /// Get all captured effects in execution order.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let effects = handler.captured();
    /// assert_eq!(effects.len(), 3);
    /// ```
    pub fn captured(&self) -> Vec<AppEffect> {
        self.captured_effects.borrow().clone()
    }

    /// Check if a specific effect was executed.
    ///
    /// Uses [`PartialEq`] comparison to match effects.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert!(handler.was_executed(&AppEffect::GitRequireRepo));
    /// ```
    pub fn was_executed(&self, effect: &AppEffect) -> bool {
        self.captured_effects.borrow().contains(effect)
    }

    /// Get the content of a file from the in-memory filesystem.
    ///
    /// Returns `None` if the file does not exist.
    pub fn get_file(&self, path: &PathBuf) -> Option<String> {
        self.files.borrow().get(path).cloned()
    }

    /// Check if a file exists in the in-memory filesystem.
    pub fn file_exists(&self, path: &PathBuf) -> bool {
        self.files.borrow().contains_key(path)
    }

    /// Get the current simulated working directory.
    pub fn get_cwd(&self) -> PathBuf {
        self.cwd.borrow().clone()
    }

    /// Get all captured log messages.
    ///
    /// Returns tuples of (level, message) where level is one of:
    /// "info", "success", "warn", "error".
    pub fn get_log_messages(&self) -> Vec<(String, String)> {
        self.log_messages.borrow().clone()
    }

    /// Get the number of captured effects.
    pub fn effect_count(&self) -> usize {
        self.captured_effects.borrow().len()
    }

    /// Clear all captured effects.
    ///
    /// Useful for testing multiple phases where you want to
    /// verify effects from a specific phase only.
    pub fn clear_captured(&self) {
        self.captured_effects.borrow_mut().clear();
    }
}

impl AppEffectHandler for MockAppEffectHandler {
    fn execute(&mut self, effect: AppEffect) -> AppEffectResult {
        // 1. Always capture the effect first
        self.captured_effects.borrow_mut().push(effect.clone());

        // 2. Execute mock behavior based on effect type
        match effect {
            // =========================================================================
            // Working Directory Effects
            // =========================================================================
            AppEffect::SetCurrentDir { path } => {
                *self.cwd.borrow_mut() = path;
                AppEffectResult::Ok
            }

            // =========================================================================
            // Filesystem Effects
            // =========================================================================
            AppEffect::WriteFile { path, content } => {
                self.files.borrow_mut().insert(path, content);
                AppEffectResult::Ok
            }

            AppEffect::ReadFile { path } => match self.files.borrow().get(&path) {
                Some(content) => AppEffectResult::String(content.clone()),
                None => AppEffectResult::Error(format!("File not found: {}", path.display())),
            },

            AppEffect::DeleteFile { path } => {
                if self.files.borrow_mut().remove(&path).is_some() {
                    AppEffectResult::Ok
                } else {
                    AppEffectResult::Error(format!("File not found: {}", path.display()))
                }
            }

            AppEffect::CreateDir { path: _ } => {
                // Directories are implicit in our mock filesystem
                AppEffectResult::Ok
            }

            AppEffect::PathExists { path } => {
                AppEffectResult::Bool(self.files.borrow().contains_key(&path))
            }

            AppEffect::SetReadOnly {
                path: _,
                readonly: _,
            } => {
                // Permissions are not tracked in the mock
                AppEffectResult::Ok
            }

            // =========================================================================
            // Git Effects
            // =========================================================================
            AppEffect::GitRequireRepo => {
                if *self.repo_exists.borrow() {
                    AppEffectResult::Ok
                } else {
                    AppEffectResult::Error("Not in a git repository".to_string())
                }
            }

            AppEffect::GitGetRepoRoot => {
                if *self.repo_exists.borrow() {
                    AppEffectResult::Path(self.cwd.borrow().clone())
                } else {
                    AppEffectResult::Error("Not in a git repository".to_string())
                }
            }

            AppEffect::GitGetHeadOid => AppEffectResult::String(self.head_oid.borrow().clone()),

            AppEffect::GitDiff => AppEffectResult::String(self.diff_output.borrow().clone()),

            AppEffect::GitDiffFrom { start_oid: _ } => {
                AppEffectResult::String(self.diff_output.borrow().clone())
            }

            AppEffect::GitDiffFromStart => {
                AppEffectResult::String(self.diff_output.borrow().clone())
            }

            AppEffect::GitSnapshot => {
                AppEffectResult::String(self.snapshot_output.borrow().clone())
            }

            AppEffect::GitAddAll => AppEffectResult::Bool(*self.staged_changes.borrow()),

            AppEffect::GitCommit {
                message: _,
                user_name: _,
                user_email: _,
            } => {
                if *self.staged_changes.borrow() {
                    let oid = self.head_oid.borrow().clone();
                    AppEffectResult::Commit(CommitResult::Success(oid))
                } else {
                    AppEffectResult::Commit(CommitResult::NoChanges)
                }
            }

            AppEffect::GitSaveStartCommit => {
                // Write the current HEAD OID to .agent/start_commit
                let oid = self.head_oid.borrow().clone();
                self.files
                    .borrow_mut()
                    .insert(PathBuf::from(".agent/start_commit"), oid);
                AppEffectResult::Ok
            }

            AppEffect::GitResetStartCommit => {
                // Reset start commit to merge-base (simulated as HEAD)
                let oid = self.head_oid.borrow().clone();
                self.files
                    .borrow_mut()
                    .insert(PathBuf::from(".agent/start_commit"), oid.clone());
                AppEffectResult::String(oid)
            }

            AppEffect::GitRebaseOnto { upstream_branch: _ } => {
                match self.rebase_result.borrow().clone() {
                    Some(result) => AppEffectResult::Rebase(result),
                    None => AppEffectResult::Rebase(RebaseResult::Success),
                }
            }

            AppEffect::GitGetConflictedFiles => {
                AppEffectResult::StringList(self.conflicted_files.borrow().clone())
            }

            AppEffect::GitContinueRebase => AppEffectResult::Ok,

            AppEffect::GitAbortRebase => AppEffectResult::Ok,

            AppEffect::GitGetDefaultBranch => {
                AppEffectResult::String(self.default_branch.borrow().clone())
            }

            AppEffect::GitIsMainBranch => AppEffectResult::Bool(*self.is_main_branch.borrow()),

            // =========================================================================
            // Environment Effects
            // =========================================================================
            AppEffect::GetEnvVar { name } => match self.env_vars.borrow().get(&name) {
                Some(value) => AppEffectResult::String(value.clone()),
                None => AppEffectResult::Error(format!("Environment variable '{}' not set", name)),
            },

            AppEffect::SetEnvVar { name, value } => {
                self.env_vars.borrow_mut().insert(name, value);
                AppEffectResult::Ok
            }

            // =========================================================================
            // Logging Effects
            // =========================================================================
            AppEffect::LogInfo { message } => {
                self.log_messages
                    .borrow_mut()
                    .push(("info".to_string(), message));
                AppEffectResult::Ok
            }

            AppEffect::LogSuccess { message } => {
                self.log_messages
                    .borrow_mut()
                    .push(("success".to_string(), message));
                AppEffectResult::Ok
            }

            AppEffect::LogWarn { message } => {
                self.log_messages
                    .borrow_mut()
                    .push(("warn".to_string(), message));
                AppEffectResult::Ok
            }

            AppEffect::LogError { message } => {
                self.log_messages
                    .borrow_mut()
                    .push(("error".to_string(), message));
                AppEffectResult::Ok
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_captures_effects() {
        let mut handler = MockAppEffectHandler::new();

        handler.execute(AppEffect::GitRequireRepo);
        handler.execute(AppEffect::PathExists {
            path: PathBuf::from("test.txt"),
        });

        let captured = handler.captured();
        assert_eq!(captured.len(), 2);
        assert!(handler.was_executed(&AppEffect::GitRequireRepo));
    }

    #[test]
    fn test_mock_filesystem_write_and_read() {
        let mut handler = MockAppEffectHandler::new();

        let write_result = handler.execute(AppEffect::WriteFile {
            path: PathBuf::from("test.txt"),
            content: "hello world".to_string(),
        });
        assert!(matches!(write_result, AppEffectResult::Ok));

        let read_result = handler.execute(AppEffect::ReadFile {
            path: PathBuf::from("test.txt"),
        });
        assert!(matches!(read_result, AppEffectResult::String(ref s) if s == "hello world"));

        assert!(handler.file_exists(&PathBuf::from("test.txt")));
        assert_eq!(
            handler.get_file(&PathBuf::from("test.txt")),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn test_mock_filesystem_read_not_found() {
        let mut handler = MockAppEffectHandler::new();

        let result = handler.execute(AppEffect::ReadFile {
            path: PathBuf::from("nonexistent.txt"),
        });
        assert!(matches!(result, AppEffectResult::Error(_)));
    }

    #[test]
    fn test_builder_with_file() {
        let handler = MockAppEffectHandler::new()
            .with_file("config.toml", "key = value")
            .with_file(".agent/start_commit", "abc1234");

        assert!(handler.file_exists(&PathBuf::from("config.toml")));
        assert_eq!(
            handler.get_file(&PathBuf::from("config.toml")),
            Some("key = value".to_string())
        );
    }

    #[test]
    fn test_builder_on_main_branch() {
        let mut handler = MockAppEffectHandler::new().on_main_branch();

        let result = handler.execute(AppEffect::GitIsMainBranch);
        assert!(matches!(result, AppEffectResult::Bool(true)));
    }

    #[test]
    fn test_builder_with_head_oid() {
        let mut handler = MockAppEffectHandler::new().with_head_oid("deadbeef");

        let result = handler.execute(AppEffect::GitGetHeadOid);
        assert!(matches!(result, AppEffectResult::String(ref s) if s == "deadbeef"));
    }

    #[test]
    fn test_builder_without_repo() {
        let mut handler = MockAppEffectHandler::new().without_repo();

        let result = handler.execute(AppEffect::GitRequireRepo);
        assert!(matches!(result, AppEffectResult::Error(_)));

        let result = handler.execute(AppEffect::GitGetRepoRoot);
        assert!(matches!(result, AppEffectResult::Error(_)));
    }

    #[test]
    fn test_set_current_dir() {
        let mut handler = MockAppEffectHandler::new();

        handler.execute(AppEffect::SetCurrentDir {
            path: PathBuf::from("/new/path"),
        });

        assert_eq!(handler.get_cwd(), PathBuf::from("/new/path"));
    }

    #[test]
    fn test_git_save_start_commit() {
        let mut handler = MockAppEffectHandler::new().with_head_oid("abc1234");

        let result = handler.execute(AppEffect::GitSaveStartCommit);
        assert!(matches!(result, AppEffectResult::Ok));

        assert_eq!(
            handler.get_file(&PathBuf::from(".agent/start_commit")),
            Some("abc1234".to_string())
        );
    }

    #[test]
    fn test_git_reset_start_commit() {
        let mut handler = MockAppEffectHandler::new().with_head_oid("def5678");

        let result = handler.execute(AppEffect::GitResetStartCommit);
        assert!(matches!(result, AppEffectResult::String(ref s) if s == "def5678"));

        assert_eq!(
            handler.get_file(&PathBuf::from(".agent/start_commit")),
            Some("def5678".to_string())
        );
    }

    #[test]
    fn test_env_var_operations() {
        let mut handler = MockAppEffectHandler::new().with_env_var("PATH", "/usr/bin");

        let result = handler.execute(AppEffect::GetEnvVar {
            name: "PATH".to_string(),
        });
        assert!(matches!(result, AppEffectResult::String(ref s) if s == "/usr/bin"));

        handler.execute(AppEffect::SetEnvVar {
            name: "NEW_VAR".to_string(),
            value: "new_value".to_string(),
        });

        let result = handler.execute(AppEffect::GetEnvVar {
            name: "NEW_VAR".to_string(),
        });
        assert!(matches!(result, AppEffectResult::String(ref s) if s == "new_value"));
    }

    #[test]
    fn test_env_var_not_set() {
        let mut handler = MockAppEffectHandler::new();

        let result = handler.execute(AppEffect::GetEnvVar {
            name: "NONEXISTENT".to_string(),
        });
        assert!(matches!(result, AppEffectResult::Error(_)));
    }

    #[test]
    fn test_logging_effects() {
        let mut handler = MockAppEffectHandler::new();

        handler.execute(AppEffect::LogInfo {
            message: "info message".to_string(),
        });
        handler.execute(AppEffect::LogWarn {
            message: "warning".to_string(),
        });
        handler.execute(AppEffect::LogError {
            message: "error".to_string(),
        });

        let logs = handler.get_log_messages();
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0], ("info".to_string(), "info message".to_string()));
        assert_eq!(logs[1], ("warn".to_string(), "warning".to_string()));
        assert_eq!(logs[2], ("error".to_string(), "error".to_string()));
    }

    #[test]
    fn test_git_commit_with_changes() {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("commit123")
            .with_staged_changes(true);

        let result = handler.execute(AppEffect::GitCommit {
            message: "test commit".to_string(),
            user_name: None,
            user_email: None,
        });

        assert!(matches!(
            result,
            AppEffectResult::Commit(CommitResult::Success(ref oid)) if oid == "commit123"
        ));
    }

    #[test]
    fn test_git_commit_no_changes() {
        let mut handler = MockAppEffectHandler::new().with_staged_changes(false);

        let result = handler.execute(AppEffect::GitCommit {
            message: "test commit".to_string(),
            user_name: None,
            user_email: None,
        });

        assert!(matches!(
            result,
            AppEffectResult::Commit(CommitResult::NoChanges)
        ));
    }

    #[test]
    fn test_rebase_result() {
        let mut handler =
            MockAppEffectHandler::new().with_rebase_result(RebaseResult::Conflicts(vec![
                "file1.rs".to_string(),
                "file2.rs".to_string(),
            ]));

        let result = handler.execute(AppEffect::GitRebaseOnto {
            upstream_branch: "main".to_string(),
        });

        assert!(matches!(
            result,
            AppEffectResult::Rebase(RebaseResult::Conflicts(ref files))
                if files.len() == 2
        ));
    }

    #[test]
    fn test_delete_file() {
        let mut handler = MockAppEffectHandler::new().with_file("to_delete.txt", "content");

        assert!(handler.file_exists(&PathBuf::from("to_delete.txt")));

        let result = handler.execute(AppEffect::DeleteFile {
            path: PathBuf::from("to_delete.txt"),
        });
        assert!(matches!(result, AppEffectResult::Ok));

        assert!(!handler.file_exists(&PathBuf::from("to_delete.txt")));
    }

    #[test]
    fn test_delete_nonexistent_file() {
        let mut handler = MockAppEffectHandler::new();

        let result = handler.execute(AppEffect::DeleteFile {
            path: PathBuf::from("nonexistent.txt"),
        });
        assert!(matches!(result, AppEffectResult::Error(_)));
    }

    #[test]
    fn test_clear_captured() {
        let mut handler = MockAppEffectHandler::new();

        handler.execute(AppEffect::GitRequireRepo);
        assert_eq!(handler.effect_count(), 1);

        handler.clear_captured();
        assert_eq!(handler.effect_count(), 0);
        assert!(handler.captured().is_empty());
    }

    #[test]
    fn test_git_diff_with_configured_output() {
        let diff_content = "diff --git a/file.rs b/file.rs\n+added line";
        let mut handler = MockAppEffectHandler::new().with_diff(diff_content);

        let result = handler.execute(AppEffect::GitDiff);
        assert!(matches!(result, AppEffectResult::String(ref s) if s == diff_content));
    }

    #[test]
    fn test_default_branch() {
        let mut handler = MockAppEffectHandler::new().with_default_branch("develop");

        let result = handler.execute(AppEffect::GitGetDefaultBranch);
        assert!(matches!(result, AppEffectResult::String(ref s) if s == "develop"));
    }

    #[test]
    fn test_conflicted_files() {
        let mut handler = MockAppEffectHandler::new()
            .with_conflicted_files(vec!["conflict1.rs".to_string(), "conflict2.rs".to_string()]);

        let result = handler.execute(AppEffect::GitGetConflictedFiles);
        assert!(matches!(result, AppEffectResult::StringList(ref files) if files.len() == 2));
    }

    #[test]
    fn test_path_exists() {
        let mut handler = MockAppEffectHandler::new().with_file("exists.txt", "content");

        let result = handler.execute(AppEffect::PathExists {
            path: PathBuf::from("exists.txt"),
        });
        assert!(matches!(result, AppEffectResult::Bool(true)));

        let result = handler.execute(AppEffect::PathExists {
            path: PathBuf::from("not_exists.txt"),
        });
        assert!(matches!(result, AppEffectResult::Bool(false)));
    }
}
