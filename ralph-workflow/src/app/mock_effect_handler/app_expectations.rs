//! Builder methods for configuring mock app-layer state.
//!
//! This module provides fluent builder methods to configure the `MockAppEffectHandler`
//! with pre-set state for testing. These methods allow tests to set up:
//!
//! - In-memory files before execution
//! - Git repository state (HEAD OID, branch, default branch)
//! - Environment variables
//! - Expected diff/snapshot outputs
//! - Rebase simulation results
//!
//! # Builder Pattern
//!
//! All builder methods consume `self` and return `Self`, allowing method chaining:
//!
//! ```ignore
//! let handler = MockAppEffectHandler::new()
//!     .with_file("PROMPT.md", "# Task")
//!     .with_head_oid("abc123")
//!     .on_main_branch()
//!     .with_env_var("HOME", "/home/user");
//! ```
//!
//! # Effect Simulation
//!
//! The builder methods pre-configure state that will be accessed when effects are executed:
//!
//! - `with_file()` → `ReadFile`, `PathExists` will find the file
//! - `with_head_oid()` → `GitGetHeadOid` will return the configured OID
//! - `on_main_branch()` → `GitIsMainBranch` will return true
//! - `with_env_var()` → `GetEnvVar` will return the configured value
//!
//! # See Also
//!
//! - `core` - Core struct and trait implementation
//! - `file_state` - File management and inspection methods

use super::super::effect::RebaseResult;
use super::core::MockAppEffectHandler;
use std::path::PathBuf;

impl MockAppEffectHandler {
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
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .on_main_branch();
    ///
    /// let result = handler.execute(AppEffect::GitIsMainBranch);
    /// assert!(matches!(result, AppEffectResult::Bool(true)));
    /// ```
    pub fn on_main_branch(self) -> Self {
        *self.is_main_branch.borrow_mut() = true;
        self
    }

    /// Set the simulated HEAD OID.
    ///
    /// This affects the result of `GitGetHeadOid` effects.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_head_oid("deadbeef");
    ///
    /// let result = handler.execute(AppEffect::GitGetHeadOid);
    /// assert!(matches!(result, AppEffectResult::String(ref s) if s == "deadbeef"));
    /// ```
    pub fn with_head_oid(self, oid: impl Into<String>) -> Self {
        *self.head_oid.borrow_mut() = oid.into();
        self
    }

    /// Configure the handler to simulate no git repository.
    ///
    /// This will cause `GitRequireRepo` effects to return an error.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .without_repo();
    ///
    /// let result = handler.execute(AppEffect::GitRequireRepo);
    /// assert!(matches!(result, AppEffectResult::Error(_)));
    /// ```
    pub fn without_repo(self) -> Self {
        *self.repo_exists.borrow_mut() = false;
        self
    }

    /// Set the simulated default branch name.
    ///
    /// This affects the result of `GitGetDefaultBranch` effects.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_default_branch("develop");
    ///
    /// let result = handler.execute(AppEffect::GitGetDefaultBranch);
    /// assert!(matches!(result, AppEffectResult::String(ref s) if s == "develop"));
    /// ```
    pub fn with_default_branch(self, branch: impl Into<String>) -> Self {
        *self.default_branch.borrow_mut() = branch.into();
        self
    }

    /// Set an environment variable.
    ///
    /// This affects the result of `GetEnvVar` effects.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_env_var("API_KEY", "secret123");
    ///
    /// let result = handler.execute(AppEffect::GetEnvVar {
    ///     name: "API_KEY".to_string(),
    /// });
    /// assert!(matches!(result, AppEffectResult::String(ref s) if s == "secret123"));
    /// ```
    pub fn with_env_var(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.borrow_mut().insert(name.into(), value.into());
        self
    }

    /// Set the simulated diff output.
    ///
    /// This affects the result of `GitDiff`, `GitDiffFrom`, and `GitDiffFromStart` effects.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_diff("diff --git a/file.txt...");
    ///
    /// let result = handler.execute(AppEffect::GitDiff);
    /// assert!(matches!(result, AppEffectResult::String(ref s) if s.contains("diff --git")));
    /// ```
    pub fn with_diff(self, diff: impl Into<String>) -> Self {
        *self.diff_output.borrow_mut() = diff.into();
        self
    }

    /// Set whether git add will stage changes.
    ///
    /// This affects the result of `GitAddAll` and `GitCommit` effects.
    /// If set to false, `GitCommit` will return `CommitResult::NoChanges`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_staged_changes(false);
    ///
    /// let result = handler.execute(AppEffect::GitCommit {
    ///     message: "test".to_string(),
    ///     user_name: "user".to_string(),
    ///     user_email: "user@example.com".to_string(),
    /// });
    /// assert!(matches!(result, AppEffectResult::Commit(CommitResult::NoChanges)));
    /// ```
    pub fn with_staged_changes(self, staged: bool) -> Self {
        *self.staged_changes.borrow_mut() = staged;
        self
    }

    /// Set the simulated rebase result.
    ///
    /// This affects the result of `GitRebaseOnto` effects. If not set,
    /// rebase will return `RebaseResult::Success` by default.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_rebase_result(RebaseResult::Conflict);
    ///
    /// let result = handler.execute(AppEffect::GitRebaseOnto {
    ///     upstream_branch: "main".to_string(),
    /// });
    /// assert!(matches!(result, AppEffectResult::Rebase(RebaseResult::Conflict)));
    /// ```
    pub fn with_rebase_result(self, result: RebaseResult) -> Self {
        *self.rebase_result.borrow_mut() = Some(result);
        self
    }

    /// Set the simulated conflicted files.
    ///
    /// This affects the result of `GitGetConflictedFiles` effects.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_conflicted_files(vec!["file1.rs".to_string(), "file2.rs".to_string()]);
    ///
    /// let result = handler.execute(AppEffect::GitGetConflictedFiles);
    /// assert!(matches!(result, AppEffectResult::StringList(ref files) if files.len() == 2));
    /// ```
    pub fn with_conflicted_files(self, files: Vec<String>) -> Self {
        *self.conflicted_files.borrow_mut() = files;
        self
    }

    /// Set the current working directory.
    ///
    /// This affects the initial CWD state and the result of `GitGetRepoRoot`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_cwd("/home/user/project");
    ///
    /// assert_eq!(handler.get_cwd(), PathBuf::from("/home/user/project"));
    /// ```
    pub fn with_cwd(self, cwd: impl Into<PathBuf>) -> Self {
        *self.cwd.borrow_mut() = cwd.into();
        self
    }
}
