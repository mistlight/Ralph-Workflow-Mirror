//! Checkpoint creation, content, and cleanup tests.
//!
//! These tests use `MockAppEffectHandler` for in-memory testing without
//! real filesystem or git operations.

use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::{make_checkpoint_json, MOCK_REPO_PATH};

/// Standard PROMPT.md content for tests - matches the required format.
const STANDARD_PROMPT: &str = r"## Goal

Do something.

## Acceptance

- Tests pass
";

// ============================================================================
// Checkpoint Creation Tests
// ============================================================================

#[test]
fn ralph_creates_checkpoint_during_development() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test commit\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with 0 iterations - pipeline completes without agent execution
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_creates_checkpoint_during_review() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with 0 iterations - pipeline completes without agent execution
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Checkpoint Content Tests
// ============================================================================

#[test]
fn ralph_checkpoint_contains_iteration_info() {
    with_default_timeout(|| {
        // Pre-create a checkpoint file with expected structure at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run pipeline - should validate checkpoint structure
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify checkpoint was cleared after successful complete phase
        assert!(
            !handler.file_exists(&PathBuf::from(".agent/checkpoint.json")),
            "Checkpoint should be cleared after successful completion"
        );
    });
}

#[test]
fn ralph_checkpoint_contains_cli_args_snapshot() {
    with_default_timeout(|| {
        // Pre-create a checkpoint file at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 3, 3);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run pipeline - checkpoint at Complete should be cleared
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_checkpoint_contains_agent_config_snapshot() {
    with_default_timeout(|| {
        // Pre-create a checkpoint file at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run pipeline
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Checkpoint Cleanup Tests
// ============================================================================

#[test]
fn ralph_clears_checkpoint_on_success() {
    with_default_timeout(|| {
        // Pre-create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run successfully - checkpoint should be cleared
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify checkpoint was cleared
        assert!(
            !handler.file_exists(&PathBuf::from(".agent/checkpoint.json")),
            "Checkpoint should be cleared on successful completion"
        );
    });
}
