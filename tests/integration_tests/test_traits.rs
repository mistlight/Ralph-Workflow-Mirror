//! Integration tests for test trait exports.
//!
//! These tests verify that test traits like `MockAppEffectHandler` and `MemoryWorkspace`
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
use ralph_workflow::app::effect::{AppEffect, AppEffectHandler, AppEffectResult};
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use std::path::{Path, PathBuf};

// ============================================================================
// MockAppEffectHandler tests (replaces MockGit tests)
// ============================================================================

/// Test that `MockAppEffectHandler` can be created and used.
///
/// This verifies that when a `MockAppEffectHandler` instance is created, it can be used
/// to execute effects and capture them for assertion.
#[test]
fn test_mock_app_effect_handler_creation() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new();
        let result = handler.execute(AppEffect::GitRequireRepo);
        assert!(matches!(result, AppEffectResult::Ok));
    });
}

/// Test that `MockAppEffectHandler` builder pattern works.
///
/// This verifies that when the builder methods are chained, they configure
/// the mock handler with the specified return values for effects.
#[test]
fn test_mock_app_effect_handler_builder() {
    with_default_timeout(|| {
        let expected_oid = "abc123def456".repeat(4)[..40].to_string();
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid(&expected_oid)
            .with_file(PathBuf::from("test.txt"), "content");

        // Test GitGetHeadOid
        let result = handler.execute(AppEffect::GitGetHeadOid);
        assert!(matches!(result, AppEffectResult::String(ref s) if s == &expected_oid));

        // Test file exists
        let result = handler.execute(AppEffect::PathExists {
            path: PathBuf::from("test.txt"),
        });
        assert!(matches!(result, AppEffectResult::Bool(true)));
    });
}

/// Test that `MockAppEffectHandler` captures effects.
///
/// This verifies that when effects are executed on `MockAppEffectHandler`,
/// they are captured and can be inspected for testing assertions.
#[test]
fn test_mock_app_effect_handler_captures_effects() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new();

        handler.execute(AppEffect::GitRequireRepo);
        handler.execute(AppEffect::GitGetRepoRoot);
        handler.execute(AppEffect::GitDiff);

        let captured = handler.captured();
        assert!(captured
            .iter()
            .any(|e| matches!(e, AppEffect::GitRequireRepo)));
        assert!(captured
            .iter()
            .any(|e| matches!(e, AppEffect::GitGetRepoRoot)));
        assert!(captured.iter().any(|e| matches!(e, AppEffect::GitDiff)));
    });
}

/// Test that `MockAppEffectHandler` `was_executed` works.
///
/// This verifies that the `was_executed` method correctly identifies
/// whether a specific effect was captured.
#[test]
fn test_mock_app_effect_handler_was_executed() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new();

        handler.execute(AppEffect::GitRequireRepo);
        handler.execute(AppEffect::GitDiff);

        assert!(handler.was_executed(&AppEffect::GitRequireRepo));
        assert!(handler.was_executed(&AppEffect::GitDiff));
        assert!(!handler.was_executed(&AppEffect::GitSnapshot));
    });
}

/// Test that `MockAppEffectHandler` `without_repo` returns error.
///
/// This verifies that when the mock is configured without a repo,
/// `GitRequireRepo` returns an error.
#[test]
fn test_mock_app_effect_handler_without_repo() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new().without_repo();
        let result = handler.execute(AppEffect::GitRequireRepo);
        assert!(matches!(result, AppEffectResult::Error(_)));
    });
}

/// Test that `MockAppEffectHandler` filesystem operations work.
///
/// This verifies that the mock filesystem correctly handles
/// write, read, and exists operations.
#[test]
fn test_mock_app_effect_handler_filesystem() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new();

        // Write a file
        let result = handler.execute(AppEffect::WriteFile {
            path: PathBuf::from(".agent/test.txt"),
            content: "test content".to_string(),
        });
        assert!(matches!(result, AppEffectResult::Ok));

        // Read it back
        let result = handler.execute(AppEffect::ReadFile {
            path: PathBuf::from(".agent/test.txt"),
        });
        assert!(matches!(result, AppEffectResult::String(ref s) if s == "test content"));

        // Check existence
        assert!(handler.file_exists(&PathBuf::from(".agent/test.txt")));
        assert!(!handler.file_exists(&PathBuf::from(".agent/other.txt")));
    });
}

/// Test that `MockAppEffectHandler` `GitSaveStartCommit` writes to mock filesystem.
///
/// This verifies that when `GitSaveStartCommit` is executed, it writes
/// the HEAD OID to .`agent/start_commit` in the mock filesystem.
#[test]
fn test_mock_app_effect_handler_save_start_commit() {
    with_default_timeout(|| {
        let expected_oid = "a".repeat(40);
        let mut handler = MockAppEffectHandler::new().with_head_oid(&expected_oid);

        let result = handler.execute(AppEffect::GitSaveStartCommit);
        assert!(matches!(result, AppEffectResult::String(ref s) if s == &expected_oid));

        // Verify file was written
        let start_commit_path = PathBuf::from(".agent/start_commit");
        assert!(handler.file_exists(&start_commit_path));
        assert_eq!(handler.get_file(&start_commit_path).unwrap(), expected_oid);
    });
}

// ============================================================================
// MemoryWorkspace tests
// ============================================================================

/// Test that `MemoryWorkspace` can be created and used via Workspace trait.
///
/// This verifies that when a `MemoryWorkspace` is created, it can be used
/// through the Workspace trait to perform filesystem operations.
#[test]
fn test_memory_workspace_creation() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();
        assert!(!workspace.exists(Path::new("nonexistent.txt")));
    });
}

/// Test that `MemoryWorkspace` builder pattern works for virtual file system.
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

/// Test that `MemoryWorkspace` captures write operations.
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

/// Test that `MemoryWorkspace` implements Workspace trait with full roundtrip.
///
/// This verifies that when Workspace trait methods are used on `MemoryWorkspace`,
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

/// Test that `MemoryWorkspace` tracks written files.
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

/// Test that `MemoryWorkspace` read errors work correctly.
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

/// Test that `MemoryWorkspace` can simulate agent file operations.
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
