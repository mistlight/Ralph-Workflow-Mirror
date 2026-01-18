//! Mock implementation for Git operations trait.
//!
//! This module provides a mock implementation of the `GitOps` trait that allows
//! mocking external git side effects in tests. Only external side effects (git2
//! library calls, file system operations) are mocked - internal code logic is
//! never mocked.

#![cfg(any(test, feature = "test-utils"))]

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;

use super::ops::{CommitResult, GitOps, RebaseResult};

/// Clonable representation of an io::Result.
///
/// Since io::Error doesn't implement Clone, we store error info as strings
/// and reconstruct the error on demand.
#[derive(Debug, Clone)]
enum MockResult<T: Clone> {
    Ok(T),
    Err {
        kind: io::ErrorKind,
        message: String,
    },
}

impl<T: Clone> MockResult<T> {
    fn to_io_result(&self) -> io::Result<T> {
        match self {
            MockResult::Ok(v) => Ok(v.clone()),
            MockResult::Err { kind, message } => Err(io::Error::new(*kind, message.clone())),
        }
    }

    fn from_io_result(result: io::Result<T>) -> Self {
        match result {
            Ok(v) => MockResult::Ok(v),
            Err(e) => MockResult::Err {
                kind: e.kind(),
                message: e.to_string(),
            },
        }
    }
}

impl<T: Clone + Default> Default for MockResult<T> {
    fn default() -> Self {
        MockResult::Ok(T::default())
    }
}

/// Mock Git implementation that captures calls for assertion.
///
/// This implementation allows tests to verify that specific git operations
/// were called and to control their outcomes.
#[derive(Debug)]
pub struct MockGit {
    /// Captured calls to repo_root.
    repo_root_calls: RefCell<Vec<()>>,
    /// Mock return value for repo_root.
    repo_root_result: RefCell<MockResult<PathBuf>>,

    /// Captured calls to diff.
    diff_calls: RefCell<Vec<()>>,
    /// Mock return value for diff.
    diff_result: RefCell<MockResult<String>>,

    /// Captured calls to diff_from with the start OID.
    diff_from_calls: RefCell<Vec<String>>,
    /// Mock return value for diff_from.
    diff_from_result: RefCell<MockResult<String>>,

    /// Captured calls to snapshot.
    snapshot_calls: RefCell<Vec<()>>,
    /// Mock return value for snapshot.
    snapshot_result: RefCell<MockResult<String>>,

    /// Captured calls to add_all.
    add_all_calls: RefCell<Vec<()>>,
    /// Mock return value for add_all.
    add_all_result: RefCell<MockResult<bool>>,

    /// Captured calls to commit with the message.
    commit_calls: RefCell<Vec<String>>,
    /// Mock return value for commit.
    commit_result: RefCell<MockResult<CommitResult>>,

    /// Captured calls to head_oid.
    head_oid_calls: RefCell<Vec<()>>,
    /// Mock return value for head_oid.
    head_oid_result: RefCell<MockResult<String>>,

    /// Captured calls to rebase_onto with the upstream branch.
    rebase_onto_calls: RefCell<Vec<String>>,
    /// Mock return value for rebase_onto.
    rebase_onto_result: RefCell<MockResult<RebaseResult>>,

    /// Captured calls to conflicted_files.
    conflicted_files_calls: RefCell<Vec<()>>,
    /// Mock return value for conflicted_files.
    conflicted_files_result: RefCell<MockResult<Vec<String>>>,

    /// Captured calls to require_repo.
    require_repo_calls: RefCell<Vec<()>>,
    /// Mock return value for require_repo.
    require_repo_result: RefCell<MockResult<()>>,

    /// Captured calls to diff_from_start.
    diff_from_start_calls: RefCell<Vec<()>>,
    /// Mock return value for diff_from_start.
    diff_from_start_result: RefCell<MockResult<String>>,
}

impl Default for MockGit {
    fn default() -> Self {
        Self {
            repo_root_calls: RefCell::new(Vec::new()),
            repo_root_result: RefCell::new(MockResult::Ok(PathBuf::from("/mock/repo"))),

            diff_calls: RefCell::new(Vec::new()),
            diff_result: RefCell::new(MockResult::Ok(String::new())),

            diff_from_calls: RefCell::new(Vec::new()),
            diff_from_result: RefCell::new(MockResult::Ok(String::new())),

            snapshot_calls: RefCell::new(Vec::new()),
            snapshot_result: RefCell::new(MockResult::Ok(String::new())),

            add_all_calls: RefCell::new(Vec::new()),
            add_all_result: RefCell::new(MockResult::Ok(true)),

            commit_calls: RefCell::new(Vec::new()),
            commit_result: RefCell::new(MockResult::Ok(CommitResult::Success(
                "mock_oid".to_string(),
            ))),

            head_oid_calls: RefCell::new(Vec::new()),
            head_oid_result: RefCell::new(MockResult::Ok("mock_head_oid".to_string())),

            rebase_onto_calls: RefCell::new(Vec::new()),
            rebase_onto_result: RefCell::new(MockResult::Ok(RebaseResult::Success)),

            conflicted_files_calls: RefCell::new(Vec::new()),
            conflicted_files_result: RefCell::new(MockResult::Ok(Vec::new())),

            require_repo_calls: RefCell::new(Vec::new()),
            require_repo_result: RefCell::new(MockResult::Ok(())),

            diff_from_start_calls: RefCell::new(Vec::new()),
            diff_from_start_result: RefCell::new(MockResult::Ok(String::new())),
        }
    }
}

impl MockGit {
    /// Create a new MockGit with default successful responses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new MockGit that returns errors for all operations.
    pub fn new_error() -> Self {
        fn err_result<T: Clone>(msg: &str) -> MockResult<T> {
            MockResult::Err {
                kind: io::ErrorKind::Other,
                message: msg.to_string(),
            }
        }

        Self {
            repo_root_calls: RefCell::new(Vec::new()),
            repo_root_result: RefCell::new(err_result("mock git error")),

            diff_calls: RefCell::new(Vec::new()),
            diff_result: RefCell::new(err_result("mock git error")),

            diff_from_calls: RefCell::new(Vec::new()),
            diff_from_result: RefCell::new(err_result("mock git error")),

            snapshot_calls: RefCell::new(Vec::new()),
            snapshot_result: RefCell::new(err_result("mock git error")),

            add_all_calls: RefCell::new(Vec::new()),
            add_all_result: RefCell::new(err_result("mock git error")),

            commit_calls: RefCell::new(Vec::new()),
            commit_result: RefCell::new(err_result("mock git error")),

            head_oid_calls: RefCell::new(Vec::new()),
            head_oid_result: RefCell::new(err_result("mock git error")),

            rebase_onto_calls: RefCell::new(Vec::new()),
            rebase_onto_result: RefCell::new(err_result("mock git error")),

            conflicted_files_calls: RefCell::new(Vec::new()),
            conflicted_files_result: RefCell::new(err_result("mock git error")),

            require_repo_calls: RefCell::new(Vec::new()),
            require_repo_result: RefCell::new(err_result("mock git error")),

            diff_from_start_calls: RefCell::new(Vec::new()),
            diff_from_start_result: RefCell::new(err_result("mock git error")),
        }
    }

    // Builder methods for setting mock return values

    /// Set the mock return value for repo_root.
    pub fn with_repo_root(self, result: io::Result<PathBuf>) -> Self {
        self.repo_root_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for diff.
    pub fn with_diff(self, result: io::Result<String>) -> Self {
        self.diff_result.replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for diff_from.
    pub fn with_diff_from(self, result: io::Result<String>) -> Self {
        self.diff_from_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for snapshot.
    pub fn with_snapshot(self, result: io::Result<String>) -> Self {
        self.snapshot_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for add_all.
    pub fn with_add_all(self, result: io::Result<bool>) -> Self {
        self.add_all_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for commit.
    pub fn with_commit(self, result: io::Result<CommitResult>) -> Self {
        self.commit_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for head_oid.
    pub fn with_head_oid(self, result: io::Result<String>) -> Self {
        self.head_oid_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for rebase_onto.
    pub fn with_rebase_onto(self, result: io::Result<RebaseResult>) -> Self {
        self.rebase_onto_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for conflicted_files.
    pub fn with_conflicted_files(self, result: io::Result<Vec<String>>) -> Self {
        self.conflicted_files_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for require_repo.
    pub fn with_require_repo(self, result: io::Result<()>) -> Self {
        self.require_repo_result
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Set the mock return value for diff_from_start.
    pub fn with_diff_from_start(self, result: io::Result<String>) -> Self {
        self.diff_from_start_result
            .replace(MockResult::from_io_result(result));
        self
    }

    // Assertion methods

    /// Get the number of times repo_root was called.
    pub fn repo_root_count(&self) -> usize {
        self.repo_root_calls.borrow().len()
    }

    /// Get the number of times diff was called.
    pub fn diff_count(&self) -> usize {
        self.diff_calls.borrow().len()
    }

    /// Get the arguments passed to diff_from calls.
    pub fn diff_from_calls(&self) -> Vec<String> {
        self.diff_from_calls.borrow().clone()
    }

    /// Get the number of times snapshot was called.
    pub fn snapshot_count(&self) -> usize {
        self.snapshot_calls.borrow().len()
    }

    /// Get the number of times add_all was called.
    pub fn add_all_count(&self) -> usize {
        self.add_all_calls.borrow().len()
    }

    /// Get the arguments passed to commit calls.
    pub fn commit_calls(&self) -> Vec<String> {
        self.commit_calls.borrow().clone()
    }

    /// Get the number of times head_oid was called.
    pub fn head_oid_count(&self) -> usize {
        self.head_oid_calls.borrow().len()
    }

    /// Get the arguments passed to rebase_onto calls.
    pub fn rebase_onto_calls(&self) -> Vec<String> {
        self.rebase_onto_calls.borrow().clone()
    }

    /// Get the number of times conflicted_files was called.
    pub fn conflicted_files_count(&self) -> usize {
        self.conflicted_files_calls.borrow().len()
    }

    /// Get the number of times require_repo was called.
    pub fn require_repo_count(&self) -> usize {
        self.require_repo_calls.borrow().len()
    }

    /// Get the number of times diff_from_start was called.
    pub fn diff_from_start_count(&self) -> usize {
        self.diff_from_start_calls.borrow().len()
    }
}

impl GitOps for MockGit {
    fn repo_root(&self) -> io::Result<PathBuf> {
        self.repo_root_calls.borrow_mut().push(());
        self.repo_root_result.borrow().to_io_result()
    }

    fn diff(&self) -> io::Result<String> {
        self.diff_calls.borrow_mut().push(());
        self.diff_result.borrow().to_io_result()
    }

    fn diff_from(&self, start_oid: &str) -> io::Result<String> {
        self.diff_from_calls
            .borrow_mut()
            .push(start_oid.to_string());
        self.diff_from_result.borrow().to_io_result()
    }

    fn snapshot(&self) -> io::Result<String> {
        self.snapshot_calls.borrow_mut().push(());
        self.snapshot_result.borrow().to_io_result()
    }

    fn add_all(&self) -> io::Result<bool> {
        self.add_all_calls.borrow_mut().push(());
        self.add_all_result.borrow().to_io_result()
    }

    fn commit(
        &self,
        message: &str,
        _git_user_name: Option<&str>,
        _git_user_email: Option<&str>,
    ) -> io::Result<CommitResult> {
        self.commit_calls.borrow_mut().push(message.to_string());
        self.commit_result.borrow().to_io_result()
    }

    fn head_oid(&self) -> io::Result<String> {
        self.head_oid_calls.borrow_mut().push(());
        self.head_oid_result.borrow().to_io_result()
    }

    fn rebase_onto(&self, upstream_branch: &str) -> io::Result<RebaseResult> {
        self.rebase_onto_calls
            .borrow_mut()
            .push(upstream_branch.to_string());
        self.rebase_onto_result.borrow().to_io_result()
    }

    fn conflicted_files(&self) -> io::Result<Vec<String>> {
        self.conflicted_files_calls.borrow_mut().push(());
        self.conflicted_files_result.borrow().to_io_result()
    }

    fn require_repo(&self) -> io::Result<()> {
        self.require_repo_calls.borrow_mut().push(());
        self.require_repo_result.borrow().to_io_result()
    }

    fn diff_from_start(&self) -> io::Result<String> {
        self.diff_from_start_calls.borrow_mut().push(());
        self.diff_from_start_result.borrow().to_io_result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_git_captures_repo_root_call() {
        let mock = MockGit::new().with_repo_root(Ok(PathBuf::from("/test/repo")));
        let _ = GitOps::repo_root(&mock);
        assert_eq!(mock.repo_root_count(), 1);
    }

    #[test]
    fn test_mock_git_captures_diff_call() {
        let mock = MockGit::new().with_diff(Ok("test diff".to_string()));
        let _ = GitOps::diff(&mock);
        assert_eq!(mock.diff_count(), 1);
    }

    #[test]
    fn test_mock_git_captures_diff_from_call() {
        let mock = MockGit::new().with_diff_from(Ok("test diff".to_string()));
        let _ = GitOps::diff_from(&mock, "abc123");
        let calls = mock.diff_from_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "abc123");
    }

    #[test]
    fn test_mock_git_captures_commit_call() {
        let mock =
            MockGit::new().with_commit(Ok(CommitResult::Success("def456".to_string())));
        let _ = GitOps::commit(&mock, "test message", None, None);
        let calls = mock.commit_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "test message");
    }

    #[test]
    fn test_mock_git_captures_rebase_onto_call() {
        let mock = MockGit::new().with_rebase_onto(Ok(RebaseResult::Success));
        let _ = GitOps::rebase_onto(&mock, "main");
        let calls = mock.rebase_onto_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "main");
    }

    #[test]
    fn test_commit_result_success() {
        let mock =
            MockGit::new().with_commit(Ok(CommitResult::Success("abc123".to_string())));
        let result = GitOps::commit(&mock, "test", None, None).unwrap();
        assert_eq!(result, CommitResult::Success("abc123".to_string()));
    }

    #[test]
    fn test_commit_result_no_changes() {
        let mock = MockGit::new().with_commit(Ok(CommitResult::NoChanges));
        let result = GitOps::commit(&mock, "test", None, None).unwrap();
        assert_eq!(result, CommitResult::NoChanges);
    }

    #[test]
    fn test_rebase_result_conflicts() {
        let conflicts = vec!["file1.txt".to_string(), "file2.txt".to_string()];
        let mock =
            MockGit::new().with_rebase_onto(Ok(RebaseResult::Conflicts(conflicts.clone())));
        let result = GitOps::rebase_onto(&mock, "main").unwrap();
        assert_eq!(result, RebaseResult::Conflicts(conflicts));
    }

    #[test]
    fn test_rebase_result_no_op() {
        let mock = MockGit::new().with_rebase_onto(Ok(RebaseResult::NoOp));
        let result = GitOps::rebase_onto(&mock, "main").unwrap();
        assert_eq!(result, RebaseResult::NoOp);
    }

    #[test]
    fn test_mock_git_error_returns_error() {
        let mock = MockGit::new_error();
        assert!(GitOps::repo_root(&mock).is_err());
        assert!(GitOps::diff(&mock).is_err());
        assert!(GitOps::commit(&mock, "test", None, None).is_err());
    }

    #[test]
    fn test_git_ops_diff_from_start() {
        let mock = MockGit::new().with_diff_from_start(Ok("diff content".to_string()));
        let result = GitOps::diff_from_start(&mock).unwrap();
        assert_eq!(result, "diff content");
        assert_eq!(mock.diff_from_start_count(), 1);
    }
}
