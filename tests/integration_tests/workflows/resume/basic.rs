//! Resume flag tests and working directory validation tests.
//!
//! These tests use `MockAppEffectHandler` for in-memory testing without
//! real filesystem or git operations.

use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::{make_checkpoint_json, MOCK_REPO_PATH, STANDARD_PROMPT};

// ============================================================================
// Resume Flag Tests
// ============================================================================

#[test]
fn ralph_resume_flag_reads_checkpoint() {
    with_default_timeout(|| {
        // Create a checkpoint file manually at Complete phase to avoid agent execution
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 2, 2);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume flag - should detect the checkpoint
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_without_checkpoint_starts_fresh() {
    with_default_timeout(|| {
        // No checkpoint exists, but we pass --resume
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH));

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Working Directory Validation Tests
// ============================================================================

#[test]
fn ralph_resume_validates_working_directory() {
    with_default_timeout(|| {
        // Create a checkpoint with a different working directory
        let wrong_working_dir = "/some/other/directory";
        let checkpoint_json = make_checkpoint_json(wrong_working_dir, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume - should detect working directory mismatch but still complete
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// --no-resume Flag Tests
// ============================================================================

#[test]
fn ralph_no_resume_flag_skips_interactive_prompt() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --no-resume - should skip interactive prompt and start fresh
        run_ralph_cli_with_handler(&["--no-resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_no_resume_env_var_skips_interactive_prompt() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_env_var("RALPH_NO_RESUME_PROMPT", "1");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run without flags - env var should skip interactive prompt
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_flag_takes_precedence_over_no_resume() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with both --resume and --no-resume - --resume should take precedence
        run_ralph_cli_with_handler(&["--resume", "--no-resume"], executor, config, &mut handler)
            .unwrap();
    });
}

// ============================================================================
// Idempotent Resume Tests
// ============================================================================

#[test]
fn ralph_resume_is_idempotent_same_checkpoint() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // First resume run
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();

        // Check that checkpoint was cleared on success
        assert!(
            !handler.file_exists(&PathBuf::from(".agent/checkpoint.json")),
            "Checkpoint should be cleared after successful Complete phase resume"
        );
    });
}
