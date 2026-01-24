//! Integration tests for test trait exports.
//!
//! These tests verify that test traits like MockGit and MockFileOps
//! are properly exported from the ralph-workflow crate and can be used
//! in integration tests.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (trait exports and basic functionality)
//! - Uses mock traits at architectural boundaries (git, filesystem, agent execution)
//! - Tests are deterministic and isolated

use crate::test_timeout::with_default_timeout;
use ralph_workflow::files::{FileOperation, FileOps, MockFileOps};
use ralph_workflow::git_helpers::{CommitResult, GitOps, MockGit, OpsRebaseResult};
use std::path::{Path, PathBuf};

/// Test that MockGit can be created and used via GitOps trait.
///
/// This verifies that when a MockGit instance is created, it can be used
/// through the GitOps trait interface to perform git operations.
#[test]
fn test_mock_git_creation() {
    with_default_timeout(|| {
        let mock = MockGit::new();
        assert!(GitOps::require_repo(&mock).is_ok());
    });
}

/// Test that MockGit builder pattern works.
///
/// This verifies that when the builder methods are chained, they configure
/// the MockGit instance with the specified return values for git operations.
#[test]
fn test_mock_git_builder() {
    with_default_timeout(|| {
        let mock = MockGit::new()
            .with_repo_root(Ok(PathBuf::from("/test/repo")))
            .with_diff(Ok("test diff".to_string()))
            .with_snapshot(Ok("M file.txt".to_string()));

        assert_eq!(
            GitOps::repo_root(&mock).unwrap(),
            PathBuf::from("/test/repo")
        );
        assert_eq!(GitOps::diff(&mock).unwrap(), "test diff");
        assert_eq!(GitOps::snapshot(&mock).unwrap(), "M file.txt");
    });
}

/// Test that MockGit implements GitOps trait.
///
/// This verifies that when MockGit is used through the GitOps trait,
/// it correctly executes trait methods and returns configured results.
#[test]
fn test_mock_git_implements_git_ops() {
    with_default_timeout(|| {
        let mock = MockGit::new()
            .with_commit(Ok(CommitResult::Success("abc123".to_string())))
            .with_rebase_onto(Ok(OpsRebaseResult::Success));

        // Test via GitOps trait
        let commit_result = GitOps::commit(&mock, "test message", None, None, None).unwrap();
        assert_eq!(commit_result, CommitResult::Success("abc123".to_string()));

        let rebase_result = GitOps::rebase_onto(&mock, "main").unwrap();
        assert_eq!(rebase_result, OpsRebaseResult::Success);
    });
}

/// Test that MockGit call capture works.
///
/// This verifies that when git operations are called on MockGit,
/// the call counts and arguments are tracked for assertion.
#[test]
fn test_mock_git_call_capture() {
    with_default_timeout(|| {
        let mock = MockGit::new();

        let _ = GitOps::diff(&mock);
        let _ = GitOps::diff(&mock);
        let _ = GitOps::commit(&mock, "first", None, None, None);
        let _ = GitOps::commit(&mock, "second", None, None, None);

        assert_eq!(mock.diff_count(), 2);
        assert_eq!(mock.commit_calls().len(), 2);
        assert_eq!(mock.commit_calls()[0], "first");
        assert_eq!(mock.commit_calls()[1], "second");
    });
}

/// Test that mock error variants work.
///
/// This verifies that when mock instances are created in error mode,
/// they return errors for all operations to test error handling.
#[test]
fn test_mock_error_variants() {
    with_default_timeout(|| {
        let mock_git = MockGit::new_error();
        assert!(GitOps::repo_root(&mock_git).is_err());
        assert!(GitOps::diff(&mock_git).is_err());
    });
}

// ============================================================================
// MockFileOps tests
// ============================================================================

/// Test that MockFileOps can be created and used via FileOps trait.
///
/// This verifies that when a MockFileOps is created, it can be used
/// through the FileOps trait to perform filesystem operations.
#[test]
fn test_mock_file_ops_creation() {
    with_default_timeout(|| {
        let mock = MockFileOps::new();
        assert!(!mock.exists(Path::new("nonexistent.txt")));
    });
}

/// Test that MockFileOps builder pattern works for virtual file system.
///
/// This verifies that when the builder is used to add files, they exist
/// in the virtual filesystem and can be read via FileOps trait methods.
#[test]
fn test_mock_file_ops_builder() {
    with_default_timeout(|| {
        let mock = MockFileOps::new()
            .with_file(".agent/PLAN.md", "# Plan\n\nStep 1: Do something")
            .with_file("PROMPT.md", "# Feature Request\n\nAdd a button");

        // Check file existence
        assert!(mock.exists(Path::new(".agent/PLAN.md")));
        assert!(mock.exists(Path::new("PROMPT.md")));
        assert!(!mock.exists(Path::new(".agent/ISSUES.md")));

        // Read file contents
        let plan = mock.read_to_string(Path::new(".agent/PLAN.md")).unwrap();
        assert!(plan.contains("# Plan"));
        assert!(plan.contains("Step 1"));
    });
}

/// Test that MockFileOps captures write operations.
///
/// This verifies that when files are written via FileOps trait,
/// the operations are tracked and written content can be retrieved.
#[test]
fn test_mock_file_ops_captures_writes() {
    with_default_timeout(|| {
        let mock = MockFileOps::new();

        // Write a commit message
        mock.write_file(Path::new(".agent/commit-message.txt"), "feat: add button")
            .unwrap();

        // Write an issues file
        mock.write_file(Path::new(".agent/ISSUES.md"), "- Issue 1\n- Issue 2")
            .unwrap();

        // Verify writes were captured
        assert!(mock.was_written(Path::new(".agent/commit-message.txt")));
        assert!(mock.was_written(Path::new(".agent/ISSUES.md")));
        assert!(!mock.was_written(Path::new(".agent/PLAN.md")));

        // Verify written content
        let commit_msg = mock
            .get_written_content(Path::new(".agent/commit-message.txt"))
            .unwrap();
        assert_eq!(commit_msg, "feat: add button");
    });
}

/// Test that MockFileOps implements FileOps trait with full roundtrip.
///
/// This verifies that when FileOps trait methods are used on MockFileOps,
/// they correctly implement read, write, exists, and remove operations.
#[test]
fn test_mock_file_ops_implements_file_ops_trait() {
    with_default_timeout(|| {
        let mock = MockFileOps::new();

        // Write via FileOps trait
        FileOps::write_file(&mock, Path::new("test.txt"), "content").unwrap();

        // Read via FileOps trait
        let content = FileOps::read_to_string(&mock, Path::new("test.txt")).unwrap();
        assert_eq!(content, "content");

        // Check existence via FileOps trait
        assert!(FileOps::exists(&mock, Path::new("test.txt")));
        assert!(FileOps::is_file(&mock, Path::new("test.txt")));

        // Remove via FileOps trait
        FileOps::remove_file(&mock, Path::new("test.txt")).unwrap();
        assert!(!FileOps::exists(&mock, Path::new("test.txt")));
    });
}

/// Test that MockFileOps tracks all operations in order.
///
/// This verifies that when filesystem operations are performed, they
/// are recorded in order with their operation types for inspection.
#[test]
fn test_mock_file_ops_operation_tracking() {
    with_default_timeout(|| {
        let mock = MockFileOps::new().with_file("existing.txt", "content");

        let _ = mock.read_to_string(Path::new("existing.txt"));
        let _ = mock.exists(Path::new("other.txt"));
        let _ = mock.write_file(Path::new("new.txt"), "new content");

        let ops = mock.operations();
        assert_eq!(ops.len(), 3);

        // Verify operation types
        assert!(matches!(ops[0], FileOperation::Read(_)));
        assert!(matches!(ops[1], FileOperation::Exists(_)));
        assert!(matches!(ops[2], FileOperation::Write(_, _)));
    });
}

/// Test that MockFileOps error variants work.
///
/// This verifies that when MockFileOps is configured for errors,
/// it returns appropriate error results for read and write operations.
#[test]
fn test_mock_file_ops_error_variants() {
    with_default_timeout(|| {
        // Test error mode
        let mock_error = MockFileOps::new_error();
        assert!(mock_error.read_to_string(Path::new("any.txt")).is_err());
        assert!(mock_error.write_file(Path::new("any.txt"), "x").is_err());

        // Test specific path errors
        let mock_specific = MockFileOps::new()
            .with_file("readable.txt", "content")
            .with_read_error(
                "readable.txt",
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no permission"),
            );

        assert!(mock_specific
            .read_to_string(Path::new("readable.txt"))
            .is_err());
    });
}

/// Test that MockFileOps can simulate agent file operations.
///
/// This verifies that when a typical agent workflow is simulated,
/// all file operations are correctly tracked and can be verified.
#[test]
fn test_mock_file_ops_agent_workflow_scenario() {
    with_default_timeout(|| {
        // Simulate a workflow where:
        // 1. Check if PLAN.md exists
        // 2. Read PROMPT.md
        // 3. Write PLAN.md
        // 4. Write commit-message.txt
        // 5. Delete PLAN.md after integration

        let mock = MockFileOps::new().with_file("PROMPT.md", "# Feature: Add login button");

        // Phase 1: Planning
        assert!(!mock.exists(Path::new(".agent/PLAN.md")));
        let prompt = mock.read_to_string(Path::new("PROMPT.md")).unwrap();
        assert!(prompt.contains("login button"));

        // Phase 2: Agent produces a plan
        mock.write_file(
            Path::new(".agent/PLAN.md"),
            "# Plan\n\n1. Create button component",
        )
        .unwrap();
        assert!(mock.exists(Path::new(".agent/PLAN.md")));

        // Phase 3: Commit generation
        mock.write_file(
            Path::new(".agent/commit-message.txt"),
            "feat(ui): add login button",
        )
        .unwrap();

        // Phase 4: Cleanup
        mock.remove_file(Path::new(".agent/PLAN.md")).unwrap();
        assert!(!mock.exists(Path::new(".agent/PLAN.md")));

        // Verify the workflow operations
        assert!(mock.was_read(Path::new("PROMPT.md")));
        assert!(mock.was_written(Path::new(".agent/PLAN.md")));
        assert!(mock.was_written(Path::new(".agent/commit-message.txt")));
        assert!(mock.was_removed(Path::new(".agent/PLAN.md")));

        // Verify final commit message is available
        let commit_msg = mock
            .get_written_content(Path::new(".agent/commit-message.txt"))
            .unwrap();
        assert!(commit_msg.contains("login button"));
    });
}
