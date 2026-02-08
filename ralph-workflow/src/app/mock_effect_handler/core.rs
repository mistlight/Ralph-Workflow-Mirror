//! Core `MockAppEffectHandler` implementation and `AppEffectHandler` trait.
//!
//! This module defines the main mock handler struct and implements the
//! `AppEffectHandler` trait to simulate app-layer effects in tests.
//!
//! # Struct Fields
//!
//! The `MockAppEffectHandler` uses `RefCell` for interior mutability to allow
//! inspection of state even when borrowed mutably during effect execution.
//!
//! # AppEffectHandler Implementation
//!
//! The `execute` method handles all `AppEffect` variants by:
//! 1. Capturing the effect in the `captured_effects` vec
//! 2. Simulating the effect using in-memory state
//! 3. Returning the appropriate `AppEffectResult`
//!
//! ## Effect Categories
//!
//! - **Working Directory**: SetCurrentDir
//! - **Filesystem**: WriteFile, ReadFile, DeleteFile, CreateDir, PathExists, SetReadOnly
//! - **Git**: GitRequireRepo, GitGetRepoRoot, GitGetHeadOid, GitDiff*, GitSnapshot,
//!   GitAddAll, GitCommit, GitSave/ResetStartCommit, GitRebase*, GitGetConflictedFiles,
//!   GitContinueRebase, GitAbortRebase, GitGetDefaultBranch, GitIsMainBranch
//! - **Environment**: GetEnvVar, SetEnvVar
//! - **Logging**: LogInfo, LogSuccess, LogWarn, LogError

use super::super::effect::{
    AppEffect, AppEffectHandler, AppEffectResult, CommitResult, RebaseResult,
};
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
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
/// use ralph_workflow::app::effect::{AppEffect, AppEffectHandler};
///
/// let mut handler = MockAppEffectHandler::new()
///     .with_file("config.toml", "key = value")
///     .with_head_oid("abc1234");
///
/// handler.execute(AppEffect::ReadFile {
///     path: PathBuf::from("config.toml"),
/// });
///
/// assert!(handler.was_executed(&AppEffect::ReadFile {
///     path: PathBuf::from("config.toml"),
/// }));
/// ```
#[derive(Debug, Default)]
pub struct MockAppEffectHandler {
    /// All effects that have been executed, in order.
    pub(super) captured_effects: RefCell<Vec<AppEffect>>,
    /// In-memory filesystem: path -> content.
    pub(super) files: RefCell<HashMap<PathBuf, String>>,
    /// Current working directory (simulated).
    pub(super) cwd: RefCell<PathBuf>,
    /// Whether a git repository exists.
    pub(super) repo_exists: RefCell<bool>,
    /// The simulated HEAD OID.
    pub(super) head_oid: RefCell<String>,
    /// The simulated default branch name.
    pub(super) default_branch: RefCell<String>,
    /// Whether the current branch is main/master.
    pub(super) is_main_branch: RefCell<bool>,
    /// Environment variables.
    pub(super) env_vars: RefCell<HashMap<String, String>>,
    /// Log messages captured from logging effects.
    pub(super) log_messages: RefCell<Vec<(String, String)>>,
    /// Simulated diff output.
    pub(super) diff_output: RefCell<String>,
    /// Simulated snapshot output.
    pub(super) snapshot_output: RefCell<String>,
    /// Whether git add staged anything.
    pub(super) staged_changes: RefCell<bool>,
    /// List of conflicted files (for rebase simulation).
    pub(super) conflicted_files: RefCell<Vec<String>>,
    /// Simulated rebase result.
    pub(super) rebase_result: RefCell<Option<RebaseResult>>,
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
                    .insert(PathBuf::from(".agent/start_commit"), oid.clone());
                AppEffectResult::String(oid)
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
