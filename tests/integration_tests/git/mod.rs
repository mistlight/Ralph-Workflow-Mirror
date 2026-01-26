//! Integration tests for git workflow with per-iteration commits.
//!
//! These tests verify that:
//! - start_commit file tracking works
//! - The --reset-start-commit flag works
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (effect execution, state changes)
//! - Uses `MockAppEffectHandler` for git/filesystem isolation
//! - Tests are deterministic and verify effect sequences

use ralph_workflow::app::effect::AppEffect;
use ralph_workflow::app::effectful::{
    get_head_oid, handle_reset_start_commit, is_on_main_branch, require_repo, save_start_commit,
};
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use std::path::PathBuf;

use crate::test_timeout::with_default_timeout;

/// Test that the `--reset-start-commit` command updates `.agent/start_commit` on main branch.
///
/// This verifies that when a user invokes the reset-start-commit handler
/// on the main/master branch, the `.agent/start_commit` file is updated to the current HEAD
/// (since there's no merge-base with itself).
#[test]
fn ralph_reset_start_commit_on_main_uses_head() {
    with_default_timeout(|| {
        // Configure mock to simulate being on main branch
        let expected_oid = "a".repeat(40);
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid(&expected_oid)
            .on_main_branch();

        // Execute the command
        let result = handle_reset_start_commit(&mut handler, None);

        // Verify success
        assert!(result.is_ok(), "reset_start_commit should succeed");

        // Verify correct effects were emitted
        let effects = handler.captured();

        // Should validate git repo
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::GitRequireRepo)),
            "should validate git repo"
        );

        // Should get repo root
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::GitGetRepoRoot)),
            "should get repo root"
        );

        // Should reset start commit
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::GitResetStartCommit)),
            "should reset start commit"
        );

        // Verify the start_commit file was "written" in mock filesystem
        let start_commit_path = PathBuf::from(".agent/start_commit");
        assert!(
            handler.file_exists(&start_commit_path),
            ".agent/start_commit should be created"
        );

        let content = handler.get_file(&start_commit_path).unwrap();
        assert_eq!(
            content, expected_oid,
            "start_commit should contain HEAD OID when on main branch"
        );
    });
}

/// Test that the `--reset-start-commit` command uses merge-base on feature branches.
///
/// This verifies that when a user invokes the reset-start-commit handler
/// on a feature branch, the `.agent/start_commit` file is updated appropriately.
/// (In the mock, GitResetStartCommit returns the HEAD OID, but in production
/// it calculates merge-base).
#[test]
fn ralph_reset_start_commit_on_feature_branch_uses_merge_base() {
    with_default_timeout(|| {
        // Configure mock to simulate being on a feature branch (not main)
        let feature_head_oid = "b".repeat(40);
        let mut handler = MockAppEffectHandler::new().with_head_oid(&feature_head_oid);
        // Note: NOT calling .on_main_branch() means is_main_branch returns false

        // Execute the command
        let result = handle_reset_start_commit(&mut handler, None);

        // Verify success
        assert!(result.is_ok(), "reset_start_commit should succeed");

        // Verify effects were emitted
        let effects = handler.captured();

        // Verify the start_commit file was written
        let start_commit_path = PathBuf::from(".agent/start_commit");
        assert!(
            handler.file_exists(&start_commit_path),
            ".agent/start_commit should be created on feature branch"
        );

        // Verify correct effects sequence
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::GitRequireRepo)),
            "should validate git repo"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::GitResetStartCommit)),
            "should reset start commit"
        );
    });
}

/// Test that save_start_commit correctly saves the starting commit.
///
/// This verifies that when the pipeline starts, save_start_commit
/// creates the `.agent/start_commit` file with a valid OID.
#[test]
fn ralph_start_commit_created_during_pipeline() {
    with_default_timeout(|| {
        let expected_oid = "c".repeat(40);
        let mut handler = MockAppEffectHandler::new().with_head_oid(&expected_oid);

        // Execute save_start_commit (what happens at pipeline start)
        let result = save_start_commit(&mut handler);

        // Verify success and OID returned
        assert!(result.is_ok(), "save_start_commit should succeed");
        assert_eq!(result.unwrap(), expected_oid, "should return the HEAD OID");

        // Verify the GitSaveStartCommit effect was emitted
        let effects = handler.captured();
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::GitSaveStartCommit)),
            "should emit GitSaveStartCommit effect"
        );

        // Verify the start_commit file exists in mock filesystem
        let start_commit_path = PathBuf::from(".agent/start_commit");
        assert!(
            handler.file_exists(&start_commit_path),
            ".agent/start_commit should be created"
        );
    });
}

/// Test that require_repo fails gracefully on an empty repository.
///
/// This verifies that when a user invokes commands in a non-git directory,
/// the command fails with an appropriate error.
#[test]
fn ralph_save_start_commit_handles_empty_repo() {
    with_default_timeout(|| {
        // Configure mock to simulate no git repository
        let mut handler = MockAppEffectHandler::new().without_repo();

        // Try to require repo - should fail
        let result = require_repo(&mut handler);
        assert!(result.is_err(), "should fail without a git repository");
        assert!(
            result.unwrap_err().contains("git repository"),
            "error message should mention git repository"
        );

        // Now test with a valid repo
        let mut handler_with_repo = MockAppEffectHandler::new().with_head_oid("d".repeat(40));

        // require_repo should succeed
        let result = require_repo(&mut handler_with_repo);
        assert!(result.is_ok(), "should succeed with a git repository");

        // save_start_commit should also succeed
        let result = save_start_commit(&mut handler_with_repo);
        assert!(result.is_ok(), "save_start_commit should succeed");

        // Verify the start_commit file was created
        let start_commit_path = PathBuf::from(".agent/start_commit");
        assert!(
            handler_with_repo.file_exists(&start_commit_path),
            ".agent/start_commit should be created"
        );
    });
}

/// Test that is_on_main_branch correctly reports branch status.
#[test]
fn ralph_is_on_main_branch_detection() {
    with_default_timeout(|| {
        // Test on main branch
        let mut main_handler = MockAppEffectHandler::new().on_main_branch();
        let result = is_on_main_branch(&mut main_handler);
        assert!(result.is_ok());
        assert!(result.unwrap(), "should report true when on main branch");

        // Test on feature branch
        let mut feature_handler = MockAppEffectHandler::new(); // default is not on main
        let result = is_on_main_branch(&mut feature_handler);
        assert!(result.is_ok());
        assert!(
            !result.unwrap(),
            "should report false when on feature branch"
        );
    });
}

/// Test that get_head_oid returns the correct OID.
#[test]
fn ralph_get_head_oid_returns_correct_value() {
    with_default_timeout(|| {
        let expected_oid = "1234567890abcdef1234567890abcdef12345678";
        let mut handler = MockAppEffectHandler::new().with_head_oid(expected_oid);

        let result = get_head_oid(&mut handler);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_oid);

        // Verify the effect was captured
        let effects = handler.captured();
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::GitGetHeadOid)),
            "should emit GitGetHeadOid effect"
        );
    });
}

/// Test that reset_start_commit works with a working directory override.
#[test]
fn ralph_reset_start_commit_with_working_dir_override() {
    with_default_timeout(|| {
        let expected_oid = "e".repeat(40);
        let override_dir = PathBuf::from("/custom/workspace");
        let mut handler = MockAppEffectHandler::new().with_head_oid(&expected_oid);

        let result = handle_reset_start_commit(&mut handler, Some(&override_dir));

        assert!(result.is_ok(), "reset_start_commit should succeed");

        // Verify the SetCurrentDir effect was emitted with the override path
        let effects = handler.captured();
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::SetCurrentDir { path } if path == &override_dir)),
            "should set current dir to override path"
        );

        // Verify start_commit was written
        let start_commit_path = PathBuf::from(".agent/start_commit");
        assert!(
            handler.file_exists(&start_commit_path),
            ".agent/start_commit should be created"
        );
    });
}
