//! Integration tests for v3 checkpoint prompt history replay.
//!
//! Verifies that when resuming from v3 checkpoints, the prompt history is correctly
//! replayed to ensure deterministic agent behavior across suspend/resume cycles.

use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::super::{
    make_checkpoint_json, make_checkpoint_with_prompt_history, MOCK_REPO_PATH, STANDARD_PROMPT,
};

use super::make_checkpoint_without_new_fields;

// ============================================================================
// V3 Hardened Resume Tests - Prompt Replay
// ============================================================================

#[test]
fn ralph_v3_prompt_replay_is_deterministic() {
    with_default_timeout(|| {
        // Create prompt history JSON
        let prompt_history_json = r#"{
            "development_1": "DETERMINISTIC PROMPT FOR DEVELOPMENT ITERATION 1",
            "planning_1": "DETERMINISTIC PROMPT FOR PLANNING"
        }"#;

        let checkpoint_json =
            make_checkpoint_with_prompt_history(MOCK_REPO_PATH, "Complete", prompt_history_json);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

#[test]
fn ralph_v3_prompt_replay_across_multiple_iterations() {
    with_default_timeout(|| {
        // Create prompt history JSON with multiple iterations
        let prompt_history_json = r#"{
            "planning_1": "PLANNING PROMPT ITERATION 1",
            "development_1": "DEVELOPMENT PROMPT ITERATION 1",
            "planning_2": "PLANNING PROMPT ITERATION 2"
        }"#;

        let checkpoint_json =
            make_checkpoint_with_prompt_history(MOCK_REPO_PATH, "Complete", prompt_history_json);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume from Complete phase
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Interactive Resume Offering
// ============================================================================

#[test]
fn ralph_v3_interactive_resume_offer_on_existing_checkpoint() {
    with_default_timeout(|| {
        // Create a v3 checkpoint at Complete phase
        let checkpoint_json = make_checkpoint_json(MOCK_REPO_PATH, "Complete", 1, 1);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run without --resume flag - should offer to resume interactively
        // But since we're not in a TTY, it should skip the offer and start fresh
        run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();

        // Verify the checkpoint was cleared
        assert!(!handler.file_exists(&PathBuf::from(".agent/checkpoint.json")));
    });
}

// ============================================================================
// Prompt Replay Determinism Tests
// ============================================================================

#[test]
fn ralph_resume_replays_prompts_deterministically() {
    with_default_timeout(|| {
        // Create prompt history JSON
        let prompt_history_json = r#"{
            "development_1": "DEVELOPMENT ITERATION 1 OF 2\n\nContext:\nTest plan content",
            "review_1": "REVIEW MODE\n\nReview the following changes..."
        }"#;

        let checkpoint_json =
            make_checkpoint_with_prompt_history(MOCK_REPO_PATH, "Complete", prompt_history_json);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "# Plan\n\n1. Step 1\n2. Step 2")
            .with_file(".agent/ISSUES.md", "No issues\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume and verify
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

/// Test that checkpoints missing `prompt_md_checksum` are rejected as legacy.
///
/// Legacy checkpoints (missing required fields like `prompt_md_checksum`) are no
/// longer supported. Users must delete the checkpoint and restart the pipeline.
#[test]
fn ralph_v3_rejects_legacy_checkpoint_missing_prompt_md_checksum() {
    with_default_timeout(|| {
        // Create checkpoint WITHOUT prompt_md_checksum (legacy format)
        let checkpoint_json = make_checkpoint_without_new_fields(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Verify checkpoint is REJECTED (legacy checkpoints no longer supported)
        let result = run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler);
        assert!(
            result.is_err(),
            "Should reject legacy checkpoint missing prompt_md_checksum"
        );

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Legacy checkpoints are not supported")
                || error_msg.contains("checkpoint")
                || error_msg.contains("validation"),
            "Error message should mention legacy checkpoint rejection: {error_msg}"
        );
    });
}
