//! App-level effects for pre-pipeline operations.
//!
//! This module defines effects that represent side effects in the CLI layer
//! before the pipeline reducer takes over. Effects are data describing what
//! should happen, not the execution itself.
//!
//! # Architecture
//!
//! Effects follow the functional core / imperative shell pattern:
//! - Pure functions produce [`AppEffect`] values describing desired operations
//! - An [`AppEffectHandler`] executes the effects, performing actual I/O
//! - This separation enables testing without real filesystem or git operations
//!
//! # Example
//!
//! ```ignore
//! // Pure function returns effects (testable)
//! fn setup_workspace() -> Vec<AppEffect> {
//!     vec![
//!         AppEffect::CreateDir { path: PathBuf::from(".agent") },
//!         AppEffect::WriteFile {
//!             path: PathBuf::from(".agent/config.toml"),
//!             content: "key = value".to_string(),
//!         },
//!     ]
//! }
//!
//! // Handler executes effects (I/O boundary)
//! for effect in setup_workspace() {
//!     handler.execute(effect);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result of a git commit operation.
///
/// Indicates whether a commit was successfully created or if there were
/// no changes to commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitResult {
    /// Commit succeeded with the given OID (object identifier).
    Success(String),
    /// No changes were staged to commit.
    NoChanges,
}

/// Result of a rebase operation.
///
/// Captures the various outcomes possible when rebasing a branch onto
/// an upstream branch, including success, conflicts, and failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebaseResult {
    /// Rebase completed successfully with no conflicts.
    Success,
    /// Rebase resulted in conflicts that need resolution.
    ///
    /// Contains the list of conflicted file paths.
    Conflicts(Vec<String>),
    /// No rebase was needed (already up-to-date or same commit).
    NoOp {
        /// Human-readable explanation of why no rebase was needed.
        reason: String,
    },
    /// Rebase failed with an error.
    Failed(String),
}

/// App-level effects for CLI operations.
///
/// Each variant represents a side effect that can occur during CLI
/// operations. Effects are data structures that describe what should
/// happen without actually performing the operation.
///
/// # Categories
///
/// Effects are organized into logical categories:
/// - **Working Directory**: Process working directory management
/// - **Filesystem**: File and directory operations
/// - **Git**: Version control operations
/// - **Environment**: Environment variable access
/// - **Logging**: User-facing output
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppEffect {
    // =========================================================================
    // Working Directory Effects
    // =========================================================================
    /// Set the current working directory for the process.
    SetCurrentDir {
        /// The path to set as the current directory.
        path: PathBuf,
    },

    // =========================================================================
    // Filesystem Effects
    // =========================================================================
    /// Write content to a file, creating it if it doesn't exist.
    WriteFile {
        /// Path to the file to write.
        path: PathBuf,
        /// Content to write to the file.
        content: String,
    },

    /// Read the contents of a file.
    ReadFile {
        /// Path to the file to read.
        path: PathBuf,
    },

    /// Delete a file.
    DeleteFile {
        /// Path to the file to delete.
        path: PathBuf,
    },

    /// Create a directory and all parent directories as needed.
    CreateDir {
        /// Path to the directory to create.
        path: PathBuf,
    },

    /// Check if a path exists.
    PathExists {
        /// Path to check for existence.
        path: PathBuf,
    },

    /// Set or clear the read-only flag on a file.
    SetReadOnly {
        /// Path to the file to modify.
        path: PathBuf,
        /// Whether to make the file read-only.
        readonly: bool,
    },

    // =========================================================================
    // Git Effects
    // =========================================================================
    /// Verify that we're in a git repository.
    GitRequireRepo,

    /// Get the root directory of the git repository.
    GitGetRepoRoot,

    /// Get the OID (object identifier) of HEAD.
    GitGetHeadOid,

    /// Get the diff of uncommitted changes.
    GitDiff,

    /// Get the diff from a specific commit OID to HEAD.
    GitDiffFrom {
        /// The starting commit OID.
        start_oid: String,
    },

    /// Get the diff from the saved start commit to HEAD.
    GitDiffFromStart,

    /// Create a snapshot of the current state (stash-like operation).
    GitSnapshot,

    /// Stage all changes for commit.
    GitAddAll,

    /// Create a commit with the given message.
    GitCommit {
        /// The commit message.
        message: String,
        /// Optional user name for the commit author.
        user_name: Option<String>,
        /// Optional user email for the commit author.
        user_email: Option<String>,
    },

    /// Save the current HEAD as the start commit reference.
    GitSaveStartCommit,

    /// Reset the start commit reference to the merge-base.
    GitResetStartCommit,

    /// Rebase the current branch onto an upstream branch.
    GitRebaseOnto {
        /// The upstream branch to rebase onto.
        upstream_branch: String,
    },

    /// Get the list of files with merge conflicts.
    GitGetConflictedFiles,

    /// Continue an in-progress rebase after conflicts are resolved.
    GitContinueRebase,

    /// Abort an in-progress rebase.
    GitAbortRebase,

    /// Get the default branch name (main or master).
    GitGetDefaultBranch,

    /// Check if the current branch is main or master.
    GitIsMainBranch,

    // =========================================================================
    // Environment Effects
    // =========================================================================
    /// Get the value of an environment variable.
    GetEnvVar {
        /// Name of the environment variable.
        name: String,
    },

    /// Set an environment variable.
    SetEnvVar {
        /// Name of the environment variable.
        name: String,
        /// Value to set.
        value: String,
    },

    // =========================================================================
    // Logging Effects
    // =========================================================================
    /// Log an informational message.
    LogInfo {
        /// The message to log.
        message: String,
    },

    /// Log a success message.
    LogSuccess {
        /// The message to log.
        message: String,
    },

    /// Log a warning message.
    LogWarn {
        /// The message to log.
        message: String,
    },

    /// Log an error message.
    LogError {
        /// The message to log.
        message: String,
    },
}

/// Result of executing an [`AppEffect`].
///
/// Each effect execution produces a result that either indicates success
/// (with optional return data) or an error. The variant used depends on
/// what data the effect produces.
#[derive(Debug, Clone)]
pub enum AppEffectResult {
    /// Operation completed with no return value.
    Ok,
    /// Operation returned a string value.
    String(String),
    /// Operation returned a path value.
    Path(PathBuf),
    /// Operation returned a boolean value.
    Bool(bool),
    /// Commit operation result.
    Commit(CommitResult),
    /// Rebase operation result.
    Rebase(RebaseResult),
    /// Operation returned a list of strings.
    StringList(Vec<String>),
    /// Operation failed with an error message.
    Error(String),
}

/// Trait for executing app-level effects.
///
/// Implementors of this trait perform the actual I/O operations described
/// by [`AppEffect`] values. This separation enables:
/// - **Testing**: Mock handlers can record effects without performing I/O
/// - **Batching**: Handlers can optimize by batching similar operations
/// - **Logging**: Handlers can log all operations for debugging
///
/// # Example
///
/// ```ignore
/// struct MockHandler {
///     effects: Vec<AppEffect>,
/// }
///
/// impl AppEffectHandler for MockHandler {
///     fn execute(&mut self, effect: AppEffect) -> AppEffectResult {
///         self.effects.push(effect.clone());
///         AppEffectResult::Ok
///     }
/// }
/// ```
pub trait AppEffectHandler {
    /// Execute an effect and return the result.
    ///
    /// Implementations should:
    /// - Perform the actual I/O operation described by the effect
    /// - Return the appropriate result variant
    /// - Return `AppEffectResult::Error` if the operation fails
    fn execute(&mut self, effect: AppEffect) -> AppEffectResult;
}
