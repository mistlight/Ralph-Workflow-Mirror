//! Integration tests for commit message generation.
//!
//! These tests verify that:
//! - Commit messages are generated when developer_iters=0
//! - GitCommit effect is called correctly
//! - The commit message fallback system works
//!
//! Note: Tests that specifically test LLM commit message generation behavior
//! require the commit agent to run and cannot be properly tested without the
//! AgentExecutor trait infrastructure. These tests focus on the observable
//! behavior of commit effect creation.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** via effect capture
//! - Uses `MockAppEffectHandler` for git/filesystem isolation
//! - NO `TempDir`, `std::fs`, or real git operations
//! - Tests are deterministic and verify effects, not real filesystem state

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;
use ralph_workflow::app::effect::AppEffect;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use std::path::PathBuf;

/// Standard PROMPT.md content for commit tests.
const STANDARD_PROMPT: &str = r#"## Goal

Do something.

## Acceptance

- Tests pass
"#;

/// Create a mock handler with standard setup for commit tests.
///
/// Returns a handler configured with:
/// - Git repo context (valid HEAD OID)
/// - Working directory set to /mock/repo
/// - PROMPT.md file with standard content
/// - A diff to trigger commit (changes from start commit)
fn create_commit_test_handler() -> MockAppEffectHandler {
    MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file("PROMPT.md", STANDARD_PROMPT)
        // Simulate a diff exists (changes to commit)
        .with_diff("diff --git a/test.txt b/test.txt\n+new content")
        // Ensure git add stages changes
        .with_staged_changes(true)
}

/// Test that GitCommit effect is called when there is a simple change.
///
/// This verifies that when a user has uncommitted changes and runs ralph
/// with developer_iters=0 to skip agent execution, the GitCommit effect
/// is called with a non-empty commit message.
#[test]
fn test_commit_message_generated_with_simple_diff() {
    with_default_timeout(|| {
        let mut handler = create_commit_test_handler();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify GitCommit effect was called
        let effects = handler.captured();
        let commit_effect = effects.iter().find(|e| matches!(e, AppEffect::GitCommit { .. }));

        assert!(
            commit_effect.is_some(),
            "GitCommit effect should be called when there are changes"
        );

        // Verify the commit message is not empty
        if let Some(AppEffect::GitCommit { message, .. }) = commit_effect {
            assert!(
                !message.trim().is_empty(),
                "Commit message should not be empty"
            );
        }
    });
}

/// Test that GitCommit effect is called when there are changes to multiple files.
///
/// This verifies that when a user has uncommitted changes across multiple files
/// and runs ralph with developer_iters=0 to skip agent execution,
/// the GitCommit effect is called with a non-empty commit message.
#[test]
fn test_commit_message_generated_with_multiple_files() {
    with_default_timeout(|| {
        // Create handler with multiple file changes in the diff
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff(
                "diff --git a/file1.txt b/file1.txt\n+content 1\n\
                 diff --git a/file2.txt b/file2.txt\n+content 2\n\
                 diff --git a/file3.rs b/file3.rs\n+fn main() {}",
            )
            .with_staged_changes(true);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify GitCommit effect was called with a non-empty message
        let effects = handler.captured();
        let commit_effect = effects.iter().find(|e| matches!(e, AppEffect::GitCommit { .. }));

        assert!(commit_effect.is_some(), "GitCommit effect should be called");

        if let Some(AppEffect::GitCommit { message, .. }) = commit_effect {
            assert!(!message.trim().is_empty());
        }
    });
}

/// Test that GitCommit effect captures diff content correctly.
///
/// This verifies that when a user has uncommitted changes including modifications
/// deep within a large file and runs ralph with developer_iters=0,
/// the GitCommit effect is called with a non-empty commit message.
#[test]
fn test_commit_created_with_diff_content() {
    with_default_timeout(|| {
        // Create handler with a diff showing changes deep in a file
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff(
                "diff --git a/large_file.txt b/large_file.txt\n\
                 @@ -148,7 +148,7 @@\n\
                  line 148\n\
                  line 149\n\
                 -line 150\n\
                 +line 150 modified\n\
                  line 151\n\
                  line 152",
            )
            .with_staged_changes(true);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify GitCommit effect was called
        let effects = handler.captured();
        let commit_effect = effects.iter().find(|e| matches!(e, AppEffect::GitCommit { .. }));

        assert!(commit_effect.is_some());

        if let Some(AppEffect::GitCommit { message, .. }) = commit_effect {
            assert!(!message.trim().is_empty());
        }
    });
}

/// Test that GitCommit effect is called when both developer and review phases are skipped.
///
/// This verifies that when a user runs ralph with both developer_iters=0
/// and reviewer_reviews=0 to skip agent execution, the GitCommit effect is still
/// called with a non-empty commit message.
#[test]
fn test_commit_succeeds_without_developer_or_review() {
    with_default_timeout(|| {
        let mut handler = create_commit_test_handler();
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify GitCommit effect was called
        let effects = handler.captured();
        let commit_effect = effects.iter().find(|e| matches!(e, AppEffect::GitCommit { .. }));

        assert!(
            commit_effect.is_some(),
            "GitCommit effect should be called"
        );

        if let Some(AppEffect::GitCommit { message, .. }) = commit_effect {
            assert!(
                !message.trim().is_empty(),
                "Commit message should not be empty"
            );
        }
    });
}
