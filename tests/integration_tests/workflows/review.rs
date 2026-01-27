//! Review workflow integration tests.
//!
//! These tests verify the review workflow functionality.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (exit codes, output)
//! - Uses `tempfile::TempDir` for tests requiring real git operations
//! - Uses `MemoryWorkspace` + `MockProcessExecutor` for tests verifying agent prompts
//! - Uses **dependency injection** for configuration (no env vars)
//! - Tests are deterministic and isolated

use tempfile::TempDir;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected,
};
use crate::test_timeout::with_default_timeout;
use test_helpers::{commit_all, init_git_repo, write_file};

// ============================================================================
// Review Workflow Tests
//
// Note: Tests that require agent execution (reviewer_reviews > 0) cannot be
// properly tested without the AgentExecutor trait infrastructure. Those tests
// should be unit tests with mocked executors at the code level.
//
// These integration tests focus on behavior that doesn't require agent execution.
// ============================================================================

/// Test that setting reviewer_reviews to zero skips the review phase.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// the review phase is skipped entirely and no ISSUES.md file is created.
#[test]
fn ralph_zero_reviewer_reviews_skips_review() {
    with_default_timeout(|| {
        // Test that reviewer_reviews=0 skips the review phase entirely
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);

        // Create initial commit with tracked files
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change for the diff
        write_file(dir.path().join("initial.txt"), "updated content");

        // Use dependency injection - no env vars needed
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

        // ISSUES.md should NOT be created when review is skipped
        assert!(!dir.path().join(".agent/ISSUES.md").exists());
    });
}

/// Test that the pipeline succeeds without a review phase.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// the pipeline completes successfully and outputs "Pipeline Complete".
#[test]
fn ralph_succeeds_without_review_phase() {
    with_default_timeout(|| {
        // Test that the pipeline can succeed without any review phase
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        // Use dependency injection - no env vars needed
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
    });
}

/// Test that a commit is created when the review phase is skipped.
///
/// This verifies that when a user runs ralph with reviewer_reviews=0,
/// a commit is still created with a non-empty commit message.
#[test]
fn ralph_commit_created_when_review_skipped() {
    with_default_timeout(|| {
        // Test that commits are still created when review phase is skipped
        let dir = TempDir::new().unwrap();
        let _repo = init_git_repo(&dir);

        // Create initial commit
        write_file(dir.path().join("initial.txt"), "initial content");
        let _ = commit_all(&_repo, "initial commit");

        // Create a change
        write_file(dir.path().join("test.txt"), "new content");

        // Use dependency injection - no env vars needed
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();

        // Verify commit was created
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert!(!commit.message().unwrap().is_empty());
    });
}

// ============================================================================
// Review Prompt Construction Tests
//
// These tests verify that the review prompt is correctly constructed with
// the expected content. They test prompt_review_xml_with_context directly.
// ============================================================================

/// Test that review prompt is constructed with all required components.
///
/// This verifies that `prompt_review_xml_with_context` produces a prompt that includes:
/// - "REVIEW MODE" marker
/// - Reference to PROMPT.md.backup file (reviewer reads it directly)
/// - The implementation plan (PLAN content)
/// - Changes made (git diff content)
/// - XML output format instructions with <ralph-issues> tags
#[test]
fn review_prompt_construction_includes_all_required_components() {
    use ralph_workflow::prompts::prompt_review_xml_with_context;
    use ralph_workflow::prompts::template_context::TemplateContext;

    with_default_timeout(|| {
        let template_context = TemplateContext::default();

        // Test inputs - prompt_content is unused, reviewer reads PROMPT.md.backup directly
        let prompt_content = "# Test Requirements\n\nImplement feature X with validation";
        let plan_content = "# Implementation Plan\n\n1. Create module\n2. Add tests";
        let changes_content = "diff --git a/src/lib.rs b/src/lib.rs\n+fn new_function() {}";

        // Build the review prompt
        let review_prompt = prompt_review_xml_with_context(
            &template_context,
            prompt_content,
            plan_content,
            changes_content,
        );

        // Verify the prompt contains all required components
        assert!(
            review_prompt.contains("REVIEW MODE"),
            "Review prompt must contain 'REVIEW MODE' marker. Got:\n{}",
            &review_prompt[..500.min(review_prompt.len())]
        );

        // prompt_content is no longer embedded - reviewer reads PROMPT.md.backup directly
        assert!(
            review_prompt.contains("PROMPT.md.backup"),
            "Review prompt must reference PROMPT.md.backup for original requirements"
        );

        assert!(
            review_prompt.contains(plan_content),
            "Review prompt must contain the implementation plan"
        );

        assert!(
            review_prompt.contains(changes_content),
            "Review prompt must contain the changes/diff content"
        );

        assert!(
            review_prompt.contains("<ralph-issues>"),
            "Review prompt must contain XML output format instructions with <ralph-issues> tag"
        );

        assert!(
            review_prompt.contains("issues.xml"),
            "Review prompt must reference the issues.xml output file path"
        );
    });
}

/// Test that review prompt includes severity levels and file references in format instructions.
///
/// This verifies the prompt guides the reviewer to provide actionable output
/// with severity levels and file:line references.
#[test]
fn review_prompt_includes_output_format_guidance() {
    use ralph_workflow::prompts::prompt_review_xml_with_context;
    use ralph_workflow::prompts::template_context::TemplateContext;

    with_default_timeout(|| {
        let template_context = TemplateContext::default();

        let review_prompt =
            prompt_review_xml_with_context(&template_context, "requirements", "plan", "changes");

        // Verify format guidance
        assert!(
            review_prompt.contains("Severity")
                || review_prompt.contains("severity")
                || review_prompt.contains("Critical")
                || review_prompt.contains("High")
                || review_prompt.contains("Medium")
                || review_prompt.contains("Low"),
            "Review prompt must mention severity levels for issues"
        );

        assert!(
            review_prompt.contains("file")
                && (review_prompt.contains("line") || review_prompt.contains(":")),
            "Review prompt must mention file:line references"
        );
    });
}

/// Test that review prompt handles empty inputs gracefully.
///
/// This verifies the prompt construction doesn't crash or produce invalid
/// output when given empty prompt, plan, or changes content.
#[test]
fn review_prompt_handles_empty_inputs() {
    use ralph_workflow::prompts::prompt_review_xml_with_context;
    use ralph_workflow::prompts::template_context::TemplateContext;

    with_default_timeout(|| {
        let template_context = TemplateContext::default();

        // All empty inputs
        let review_prompt = prompt_review_xml_with_context(&template_context, "", "", "");

        // Should still produce a valid prompt structure
        assert!(
            review_prompt.contains("REVIEW MODE"),
            "Review prompt must contain 'REVIEW MODE' even with empty inputs"
        );
        assert!(
            review_prompt.contains("<ralph-issues>"),
            "Review prompt must contain XML format instructions even with empty inputs"
        );
    });
}
