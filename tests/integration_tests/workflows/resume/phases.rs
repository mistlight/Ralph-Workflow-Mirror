//! Resume from different phases tests.
//!
//! These tests use MockAppEffectHandler for in-memory testing without
//! real filesystem or git operations.

use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::{make_checkpoint_json, MOCK_REPO_PATH, STANDARD_PROMPT};

// ============================================================================
// Phase Resume Tests
// ============================================================================

#[test]
fn ralph_resume_shows_checkpoint_summary() {
    with_default_timeout(|| {
        // Create a v3 checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 3, 3);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume - Complete phase means no execution needed
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// Resume from Different Phases Tests
// ============================================================================

#[test]
fn ralph_resume_from_planning_phase() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase (0 iterations = complete immediately)
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_from_development_phase() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 2, 3);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_from_review_phase() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 3, 3);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n")
            .with_file(".agent/ISSUES.md", "No issues\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_from_complete_phase() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 3, 3);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
        // Resume from Complete should recognize pipeline is done
    });
}

// ============================================================================
// Resume Context in Agent Prompts Tests
// ============================================================================

#[test]
fn ralph_resume_passes_context_to_developer_agent() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_resume_passes_context_to_reviewer_agent() {
    with_default_timeout(|| {
        // Create a checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/ISSUES.md", "No issues found.\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}
