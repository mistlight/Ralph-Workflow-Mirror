use std::path::PathBuf;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::super::{MOCK_REPO_PATH, STANDARD_PROMPT, STANDARD_PROMPT_CHECKSUM};

use super::{make_checkpoint_json_with_resume_count, make_comprehensive_v3_checkpoint};

#[test]
fn ralph_v3_shows_user_friendly_checkpoint_summary() {
    with_default_timeout(|| {
        // Create a v3 checkpoint with resume_count > 0 at Complete phase
        let checkpoint_json = make_checkpoint_json_with_resume_count(MOCK_REPO_PATH);

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/PLAN.md", "Test plan\n")
            .with_file(".agent/commit-message.txt", "feat: test\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Run with --resume - should show user-friendly summary
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();
    });
}

// ============================================================================
// V3 Hardened Resume Tests - Comprehensive End-to-End
// ============================================================================

#[test]
fn ralph_v3_comprehensive_resume_from_review_phase() {
    with_default_timeout(|| {
        // Use STANDARD_PROMPT for PROMPT.md
        let plan_content = "# Plan\n\n1. Step 1\n2. Step 2";

        // Calculate plan checksum
        use sha2::{Digest, Sha256};
        let mut plan_hasher = Sha256::new();
        plan_hasher.update(plan_content.as_bytes());
        let plan_checksum = format!("{:x}", plan_hasher.finalize());

        // Create comprehensive v3 checkpoint with matching checksums
        let checkpoint_json = make_comprehensive_v3_checkpoint(
            MOCK_REPO_PATH,
            STANDARD_PROMPT_CHECKSUM,
            &plan_checksum,
            STANDARD_PROMPT.len(),
            plan_content.len(),
        );

        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from(MOCK_REPO_PATH))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_file(".agent/PLAN.md", plan_content)
            .with_file(".agent/checkpoint.json", &checkpoint_json)
            .with_file(".agent/ISSUES.md", "No issues\n")
            .with_file(".agent/commit-message.txt", "feat: add feature X\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Resume from Complete phase
        run_ralph_cli_with_handler(&["--resume"], executor, config, &mut handler).unwrap();

        // Verify the pipeline completed successfully
        assert!(!handler.file_exists(&PathBuf::from(".agent/checkpoint.json")));
    });
}
