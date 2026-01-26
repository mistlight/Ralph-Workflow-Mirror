//! Integration tests for test trait exports.
//!
//! These tests verify that test traits like MockGit and MemoryWorkspace
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
use ralph_workflow::git_helpers::{CommitResult, GitOps, MockGit, OpsRebaseResult};
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
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
// MemoryWorkspace tests
// ============================================================================

/// Test that MemoryWorkspace can be created and used via Workspace trait.
///
/// This verifies that when a MemoryWorkspace is created, it can be used
/// through the Workspace trait to perform filesystem operations.
#[test]
fn test_memory_workspace_creation() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();
        assert!(!workspace.exists(Path::new("nonexistent.txt")));
    });
}

/// Test that MemoryWorkspace builder pattern works for virtual file system.
///
/// This verifies that when the builder is used to add files, they exist
/// in the virtual filesystem and can be read via Workspace trait methods.
#[test]
fn test_memory_workspace_builder() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/PLAN.md", "# Plan\n\nStep 1: Do something")
            .with_file("PROMPT.md", "# Feature Request\n\nAdd a button");

        // Check file existence
        assert!(workspace.exists(Path::new(".agent/PLAN.md")));
        assert!(workspace.exists(Path::new("PROMPT.md")));
        assert!(!workspace.exists(Path::new(".agent/ISSUES.md")));

        // Read file contents
        let plan = workspace.read(Path::new(".agent/PLAN.md")).unwrap();
        assert!(plan.contains("# Plan"));
        assert!(plan.contains("Step 1"));
    });
}

/// Test that MemoryWorkspace captures write operations.
///
/// This verifies that when files are written via Workspace trait,
/// the operations are tracked and written content can be retrieved.
#[test]
fn test_memory_workspace_captures_writes() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        // Write a commit message
        workspace
            .write(Path::new(".agent/commit-message.txt"), "feat: add button")
            .unwrap();

        // Write an issues file
        workspace
            .write(Path::new(".agent/ISSUES.md"), "- Issue 1\n- Issue 2")
            .unwrap();

        // Verify writes were captured
        assert!(workspace.was_written(".agent/commit-message.txt"));
        assert!(workspace.was_written(".agent/ISSUES.md"));
        assert!(!workspace.was_written(".agent/PLAN.md"));

        // Verify written content
        let commit_msg = workspace.get_file(".agent/commit-message.txt").unwrap();
        assert_eq!(commit_msg, "feat: add button");
    });
}

/// Test that MemoryWorkspace implements Workspace trait with full roundtrip.
///
/// This verifies that when Workspace trait methods are used on MemoryWorkspace,
/// they correctly implement read, write, exists, and remove operations.
#[test]
fn test_memory_workspace_implements_workspace_trait() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        // Write via Workspace trait
        Workspace::write(&workspace, Path::new("test.txt"), "content").unwrap();

        // Read via Workspace trait
        let content = Workspace::read(&workspace, Path::new("test.txt")).unwrap();
        assert_eq!(content, "content");

        // Check existence via Workspace trait
        assert!(Workspace::exists(&workspace, Path::new("test.txt")));
        assert!(Workspace::is_file(&workspace, Path::new("test.txt")));

        // Remove via Workspace trait
        Workspace::remove(&workspace, Path::new("test.txt")).unwrap();
        assert!(!Workspace::exists(&workspace, Path::new("test.txt")));
    });
}

/// Test that MemoryWorkspace tracks written files.
///
/// This verifies that when filesystem operations are performed,
/// the written files can be inspected for verification.
#[test]
fn test_memory_workspace_written_files_tracking() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file("existing.txt", "content");

        let _ = workspace.read(Path::new("existing.txt"));
        let _ = workspace.exists(Path::new("other.txt"));
        let _ = workspace.write(Path::new("new.txt"), "new content");

        // Verify written files tracking
        let written = workspace.written_files();
        assert!(written.contains_key(&PathBuf::from("new.txt")));
        // Note: existing.txt was pre-populated, not written during the test
    });
}

/// Test that MemoryWorkspace read errors work correctly.
///
/// This verifies that when reading non-existent files,
/// appropriate errors are returned.
#[test]
fn test_memory_workspace_read_errors() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();
        let result = workspace.read(Path::new("nonexistent.txt"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    });
}

/// Test that MemoryWorkspace can simulate agent file operations.
///
/// This verifies that when a typical agent workflow is simulated,
/// all file operations are correctly tracked and can be verified.
#[test]
fn test_memory_workspace_agent_workflow_scenario() {
    with_default_timeout(|| {
        // Simulate a workflow where:
        // 1. Check if PLAN.md exists
        // 2. Read PROMPT.md
        // 3. Write PLAN.md
        // 4. Write commit-message.txt
        // 5. Delete PLAN.md after integration

        let workspace =
            MemoryWorkspace::new_test().with_file("PROMPT.md", "# Feature: Add login button");

        // Phase 1: Planning
        assert!(!workspace.exists(Path::new(".agent/PLAN.md")));
        let prompt = workspace.read(Path::new("PROMPT.md")).unwrap();
        assert!(prompt.contains("login button"));

        // Phase 2: Agent produces a plan
        workspace
            .write(
                Path::new(".agent/PLAN.md"),
                "# Plan\n\n1. Create button component",
            )
            .unwrap();
        assert!(workspace.exists(Path::new(".agent/PLAN.md")));

        // Verify PLAN.md was written (before removal)
        assert!(workspace.was_written(".agent/PLAN.md"));

        // Phase 3: Commit generation
        workspace
            .write(
                Path::new(".agent/commit-message.txt"),
                "feat(ui): add login button",
            )
            .unwrap();

        // Phase 4: Cleanup
        workspace.remove(Path::new(".agent/PLAN.md")).unwrap();
        assert!(!workspace.exists(Path::new(".agent/PLAN.md")));

        // Verify commit message was written (still exists after removal of PLAN.md)
        assert!(workspace.was_written(".agent/commit-message.txt"));

        // Verify final commit message content
        let commit_msg = workspace.get_file(".agent/commit-message.txt").unwrap();
        assert!(commit_msg.contains("login button"));
    });
}
