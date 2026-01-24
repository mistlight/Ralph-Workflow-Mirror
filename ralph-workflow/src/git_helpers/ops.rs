//! Git operations trait abstraction.
//!
//! This module provides a trait-based abstraction for git operations that allows
//! injecting different implementations for production (RealGit) and testing (MockGit).
//! This enables integration tests to mock git operations without spawning real
//! git processes or modifying the file system.
//!
//! The trait and implementations are gated behind `test-utils` or `test` cfg
//! as they're primarily used for integration testing.

#![cfg(any(test, feature = "test-utils"))]

use std::io;
use std::path::PathBuf;

/// Result of a git commit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitResult {
    /// Commit succeeded with the given OID.
    Success(String),
    /// No changes to commit.
    NoChanges,
}

/// Result of a rebase operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseResult {
    /// Rebase completed successfully.
    Success,
    /// Rebase had conflicts.
    Conflicts(Vec<String>),
    /// No rebase was needed with a reason.
    NoOp { reason: String },
    /// Rebase failed with an error message.
    Failed(String),
}

/// Trait for Git operations.
///
/// This trait abstracts git operations to allow dependency injection.
/// Production code uses `RealGit` which calls the actual git2 library.
/// Test code can use `MockGit` to control git behavior without file system access.
///
/// Only external side effects are abstracted: git2 library calls and file system
/// operations. Internal code logic is never mocked.
pub trait GitOps {
    /// Get the repository root directory.
    fn repo_root(&self) -> io::Result<PathBuf>;

    /// Get a git diff of all changes.
    fn diff(&self) -> io::Result<String>;

    /// Get a git diff from a specific starting commit OID.
    fn diff_from(&self, start_oid: &str) -> io::Result<String>;

    /// Get the current git status snapshot.
    fn snapshot(&self) -> io::Result<String>;

    /// Stage all changes.
    fn add_all(&self) -> io::Result<bool>;

    /// Create a commit with the given message.
    ///
    /// # Arguments
    ///
    /// * `message` - The commit message
    /// * `git_user_name` - Optional git user name override
    /// * `git_user_email` - Optional git user email override
    ///
    /// # Returns
    ///
    /// - `CommitResult::Success(oid)` - Commit was created with the given OID
    /// - `CommitResult::NoChanges` - No staged changes to commit (working tree clean or only ignored changes)
    ///
    /// # Semantics
    ///
    /// This method returns `CommitResult::NoChanges` when there are no staged changes.
    /// Callers that need to distinguish between "nothing to commit" and an actual error
    /// should check for this variant explicitly.
    fn commit(
        &self,
        message: &str,
        git_user_name: Option<&str>,
        git_user_email: Option<&str>,
    ) -> io::Result<CommitResult>;

    /// Get the current HEAD commit OID.
    fn head_oid(&self) -> io::Result<String>;

    /// Perform a rebase onto the specified upstream branch.
    fn rebase_onto(&self, upstream_branch: &str) -> io::Result<RebaseResult>;

    /// Get a list of conflicted files.
    fn conflicted_files(&self) -> io::Result<Vec<String>>;

    /// Check if we're in a git repository.
    fn require_repo(&self) -> io::Result<()>;

    /// Get a diff from the saved start commit.
    fn diff_from_start(&self) -> io::Result<String>;
}

/// Real Git implementation that uses the git2 library.
///
/// This is the production implementation that delegates to the existing
/// git helper functions.
#[derive(Debug, Clone)]
pub struct RealGit {
    executor: std::sync::Arc<dyn crate::executor::ProcessExecutor>,
}

impl RealGit {
    /// Create a new RealGit instance.
    pub fn new() -> Self {
        Self {
            executor: std::sync::Arc::new(crate::executor::RealProcessExecutor::new()),
        }
    }
}

impl Default for RealGit {
    fn default() -> Self {
        Self::new()
    }
}

impl GitOps for RealGit {
    fn repo_root(&self) -> io::Result<PathBuf> {
        super::repo::get_repo_root()
    }

    fn diff(&self) -> io::Result<String> {
        super::repo::git_diff()
    }

    fn diff_from(&self, start_oid: &str) -> io::Result<String> {
        super::repo::git_diff_from(start_oid)
    }

    fn snapshot(&self) -> io::Result<String> {
        super::repo::git_snapshot()
    }

    fn add_all(&self) -> io::Result<bool> {
        super::repo::git_add_all()
    }

    fn commit(
        &self,
        message: &str,
        git_user_name: Option<&str>,
        git_user_email: Option<&str>,
    ) -> io::Result<CommitResult> {
        super::repo::git_commit(message, git_user_name, git_user_email).map(|oid_opt| match oid_opt
        {
            Some(oid) => CommitResult::Success(oid.to_string()),
            None => CommitResult::NoChanges,
        })
    }

    fn head_oid(&self) -> io::Result<String> {
        super::start_commit::get_current_head_oid()
    }

    fn rebase_onto(&self, upstream_branch: &str) -> io::Result<RebaseResult> {
        match super::rebase::rebase_onto(upstream_branch, &*self.executor) {
            Ok(super::rebase::RebaseResult::Success) => Ok(RebaseResult::Success),
            Ok(super::rebase::RebaseResult::Conflicts(files)) => Ok(RebaseResult::Conflicts(files)),
            Ok(super::rebase::RebaseResult::NoOp { reason }) => Ok(RebaseResult::NoOp { reason }),
            Ok(super::rebase::RebaseResult::Failed(err)) => {
                Ok(RebaseResult::Failed(err.description()))
            }
            Err(e) => Ok(RebaseResult::Failed(e.to_string())),
        }
    }

    fn conflicted_files(&self) -> io::Result<Vec<String>> {
        super::rebase::get_conflicted_files()
    }

    fn require_repo(&self) -> io::Result<()> {
        super::repo::require_git_repo()
    }

    fn diff_from_start(&self) -> io::Result<String> {
        super::repo::get_git_diff_from_start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_git_can_be_created() {
        let git = RealGit::new();
        // Should be able to call methods (results depend on git repo state)
        let _ = git.require_repo();
    }

    #[test]
    fn test_commit_result_variants() {
        let success = CommitResult::Success("abc123".to_string());
        let no_changes = CommitResult::NoChanges;

        assert_eq!(success, CommitResult::Success("abc123".to_string()));
        assert_eq!(no_changes, CommitResult::NoChanges);
    }

    #[test]
    fn test_rebase_result_variants() {
        let success = RebaseResult::Success;
        let conflicts = RebaseResult::Conflicts(vec!["file.txt".to_string()]);
        let no_op = RebaseResult::NoOp {
            reason: "test".to_string(),
        };
        let failed = RebaseResult::Failed("error".to_string());

        assert_eq!(success, RebaseResult::Success);
        assert_eq!(
            conflicts,
            RebaseResult::Conflicts(vec!["file.txt".to_string()])
        );
        assert_eq!(
            no_op,
            RebaseResult::NoOp {
                reason: "test".to_string()
            }
        );
        assert_eq!(failed, RebaseResult::Failed("error".to_string()));
    }
}
