//! Integration tests for oversize prompt handling.
//!
//! Tests verify that when PROMPT, PLAN, or DIFF content exceeds the size limit,
//! the system correctly references backup files instead of embedding content.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests follow the integration test style guide defined in
//! **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Tests verify observable behavior:
//! - Generated prompts contain file references for large content
//! - Generated prompts embed small content inline
//! - Content size is correctly measured in bytes

use ralph_workflow::prompts::content_builder::PromptContentBuilder;
use ralph_workflow::prompts::content_reference::{
    DiffContentReference, PlanContentReference, PromptContentReference, MAX_INLINE_CONTENT_SIZE,
};
use ralph_workflow::workspace::MemoryWorkspace;
use std::path::Path;

use crate::test_timeout::with_default_timeout;

/// Test that oversize PROMPT uses backup file reference.
///
/// When PROMPT.md content exceeds MAX_INLINE_CONTENT_SIZE, the generated
/// prompt should tell the agent to read from .agent/PROMPT.md.backup.
#[test]
fn oversize_prompt_uses_backup_reference() {
    with_default_timeout(|| {
        let large_prompt = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1000);
        let workspace = MemoryWorkspace::new_test()
            .with_file("PROMPT.md", &large_prompt)
            .with_file(".agent/PROMPT.md.backup", &large_prompt);

        let builder = PromptContentBuilder::new(&workspace).with_prompt(large_prompt);

        assert!(
            builder.has_oversize_content(),
            "Builder should detect oversize content"
        );

        let refs = builder.build();
        let rendered = refs.prompt_for_template();

        assert!(
            rendered.contains("PROMPT.md.backup"),
            "Should reference backup file: {}",
            &rendered[..rendered.len().min(200)]
        );
        assert!(
            !rendered.contains(&"x".repeat(1000)),
            "Should not embed large content"
        );
    });
}

/// Test that oversize DIFF is written to `.agent/tmp/diff.txt` and referenced.
///
/// When diff content exceeds MAX_INLINE_CONTENT_SIZE, the generated
/// prompt should tell the agent to read from `.agent/tmp/diff.txt`.
#[test]
fn oversize_diff_writes_tmp_file_and_references_it() {
    with_default_timeout(|| {
        let large_diff = format!("+{}", "added_line\n".repeat(MAX_INLINE_CONTENT_SIZE / 10));
        let workspace = MemoryWorkspace::new_test();

        let builder = PromptContentBuilder::new(&workspace).with_diff(large_diff, "abc123def");

        assert!(
            builder.has_oversize_content(),
            "Builder should detect oversize content"
        );

        let refs = builder.build();
        let rendered = refs.diff_for_template();

        assert!(
            rendered.contains(".agent/tmp/diff.txt"),
            "Should reference .agent/tmp/diff.txt: {}",
            &rendered[..rendered.len().min(200)]
        );

        assert!(
            workspace.was_written(".agent/tmp/diff.txt"),
            "Should write oversize diff to .agent/tmp/diff.txt"
        );
    });
}

/// Test that oversize PLAN uses file reference with XML fallback.
///
/// When PLAN.md content exceeds MAX_INLINE_CONTENT_SIZE, the generated
/// prompt should tell the agent to read from .agent/PLAN.md with
/// plan.xml as fallback.
#[test]
fn oversize_plan_uses_file_reference() {
    with_default_timeout(|| {
        // Use direct size calculation for clarity - exceeds limit by 1 byte
        let large_plan = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let workspace = MemoryWorkspace::new_test().with_file(".agent/PLAN.md", &large_plan);

        let builder = PromptContentBuilder::new(&workspace).with_plan(large_plan);

        assert!(
            builder.has_oversize_content(),
            "Builder should detect oversize content"
        );

        let refs = builder.build();
        let rendered = refs.plan_for_template();

        assert!(
            rendered.contains(".agent/PLAN.md"),
            "Should reference PLAN.md file: {}",
            &rendered[..rendered.len().min(200)]
        );
        assert!(
            rendered.contains("plan.xml"),
            "Should mention XML fallback: {}",
            &rendered[..rendered.len().min(300)]
        );
    });
}

/// Test that small content is embedded inline.
///
/// Content below MAX_INLINE_CONTENT_SIZE should be embedded directly
/// in the prompt without file references.
#[test]
fn small_content_is_embedded_inline() {
    with_default_timeout(|| {
        let small_prompt = "## Goal\n\nDo something simple";
        let small_plan = "1. First step\n2. Second step";
        let small_diff = "+added line\n-removed line";

        let workspace = MemoryWorkspace::new_test().with_file("PROMPT.md", small_prompt);

        let builder = PromptContentBuilder::new(&workspace)
            .with_prompt(small_prompt.to_string())
            .with_plan(small_plan.to_string())
            .with_diff(small_diff.to_string(), "abc123");

        assert!(
            !builder.has_oversize_content(),
            "Builder should not detect oversize content"
        );

        let refs = builder.build();
        assert_eq!(
            refs.prompt_for_template(),
            small_prompt,
            "Should embed prompt inline"
        );
        assert_eq!(
            refs.plan_for_template(),
            small_plan,
            "Should embed plan inline"
        );
        assert_eq!(
            refs.diff_for_template(),
            small_diff,
            "Should embed diff inline"
        );
    });
}

/// Test that exactly MAX_INLINE_CONTENT_SIZE is embedded inline.
///
/// Content exactly at the limit should be embedded, not referenced.
#[test]
fn exactly_max_size_is_embedded_inline() {
    with_default_timeout(|| {
        let exact_size_content = "x".repeat(MAX_INLINE_CONTENT_SIZE);

        let ref_result = PromptContentReference::from_content(
            exact_size_content.clone(),
            Path::new("/backup"),
            "test",
        );

        assert!(ref_result.is_inline(), "Exactly max size should be inline");
        assert_eq!(
            ref_result.render_for_template(),
            exact_size_content,
            "Should return content directly"
        );
    });
}

/// Test that one byte over MAX_INLINE_CONTENT_SIZE triggers file reference.
///
/// Content one byte over the limit should be referenced by file path.
#[test]
fn one_byte_over_max_triggers_file_reference() {
    with_default_timeout(|| {
        let over_size_content = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);

        let ref_result = PromptContentReference::from_content(
            over_size_content,
            Path::new("/backup/path.md"),
            "test content",
        );

        assert!(
            !ref_result.is_inline(),
            "One byte over max should trigger file reference"
        );
        assert!(
            ref_result.render_for_template().contains("/backup/path.md"),
            "Should reference backup path"
        );
    });
}

/// Test that DiffContentReference handles empty start commit.
///
/// Even with an empty start commit, the git diff command should be generated.
#[test]
fn diff_with_empty_start_commit() {
    with_default_timeout(|| {
        let large_diff = "d".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let ref_result =
            DiffContentReference::from_diff(large_diff, "", Path::new(".agent/tmp/diff.txt"));

        assert!(!ref_result.is_inline(), "Large diff should not be inline");

        let rendered = ref_result.render_for_template();
        assert!(
            rendered.contains("git diff ..HEAD"),
            "Should handle empty start commit: {}",
            &rendered[..rendered.len().min(200)]
        );
    });
}

/// Test that PlanContentReference without XML fallback works correctly.
///
/// When no XML fallback is provided, the rendered output should only
/// reference the primary PLAN.md file.
#[test]
fn plan_without_xml_fallback() {
    with_default_timeout(|| {
        let large_plan = "p".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let ref_result = PlanContentReference::from_plan(
            large_plan,
            Path::new(".agent/PLAN.md"),
            None, // No fallback
        );

        assert!(!ref_result.is_inline(), "Large plan should not be inline");

        let rendered = ref_result.render_for_template();
        assert!(
            rendered.contains(".agent/PLAN.md"),
            "Should reference PLAN.md"
        );
        assert!(
            !rendered.contains("plan.xml"),
            "Should not mention XML fallback when not provided"
        );
    });
}

/// Test that unicode content size is correctly measured in bytes.
///
/// UTF-8 characters take multiple bytes. Size should be measured in bytes,
/// not characters, to match CLI argument limits.
#[test]
fn unicode_content_size_in_bytes() {
    with_default_timeout(|| {
        // 🎉 emoji is 4 bytes in UTF-8
        // MAX_INLINE_CONTENT_SIZE / 4 emojis would be just under limit in chars
        // but over limit in bytes
        let emoji_count = MAX_INLINE_CONTENT_SIZE / 4 + 1;
        let emoji_content = "🎉".repeat(emoji_count);

        // Verify our test setup is correct
        assert!(
            emoji_content.len() > MAX_INLINE_CONTENT_SIZE,
            "Emoji content should exceed byte limit: {} bytes vs {} limit",
            emoji_content.len(),
            MAX_INLINE_CONTENT_SIZE
        );

        let ref_result =
            PromptContentReference::from_content(emoji_content, Path::new("/backup"), "test");

        assert!(
            !ref_result.is_inline(),
            "Unicode content exceeding byte limit should not be inline"
        );
    });
}

/// Test that PromptContentBuilder correctly reports oversize state.
///
/// The has_oversize_content() method should return true if ANY content
/// exceeds the limit.
#[test]
fn builder_reports_any_oversize() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        // Only prompt is oversize
        let builder1 = PromptContentBuilder::new(&workspace)
            .with_prompt("x".repeat(MAX_INLINE_CONTENT_SIZE + 1))
            .with_plan("small".to_string())
            .with_diff("small".to_string(), "abc");
        assert!(
            builder1.has_oversize_content(),
            "Should detect oversize prompt"
        );

        // Only plan is oversize
        let builder2 = PromptContentBuilder::new(&workspace)
            .with_prompt("small".to_string())
            .with_plan("x".repeat(MAX_INLINE_CONTENT_SIZE + 1))
            .with_diff("small".to_string(), "abc");
        assert!(
            builder2.has_oversize_content(),
            "Should detect oversize plan"
        );

        // Only diff is oversize
        let builder3 = PromptContentBuilder::new(&workspace)
            .with_prompt("small".to_string())
            .with_plan("small".to_string())
            .with_diff("x".repeat(MAX_INLINE_CONTENT_SIZE + 1), "abc");
        assert!(
            builder3.has_oversize_content(),
            "Should detect oversize diff"
        );

        // None oversize
        let builder4 = PromptContentBuilder::new(&workspace)
            .with_prompt("small".to_string())
            .with_plan("small".to_string())
            .with_diff("small".to_string(), "abc");
        assert!(
            !builder4.has_oversize_content(),
            "Should not detect oversize when all small"
        );
    });
}

/// Test that oversize PLAN falls back to XML file when PLAN.md is unavailable.
///
/// When PLAN.md content exceeds MAX_INLINE_CONTENT_SIZE and PLAN.md doesn't exist
/// or is empty, the generated prompt should tell the agent to read from
/// .agent/tmp/plan.xml as fallback.
#[test]
fn oversize_plan_falls_back_to_xml_when_md_unavailable() {
    with_default_timeout(|| {
        let large_plan = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        // Create workspace with only plan.xml, no PLAN.md
        let _workspace = MemoryWorkspace::new_test().with_file(".agent/tmp/plan.xml", &large_plan);

        let plan_ref = PlanContentReference::from_plan(
            large_plan,
            Path::new(".agent/PLAN.md"),
            Some(Path::new(".agent/tmp/plan.xml")),
        );

        assert!(!plan_ref.is_inline(), "Large plan should not be inline");

        let rendered = plan_ref.render_for_template();
        assert!(
            rendered.contains(".agent/PLAN.md"),
            "Should reference primary PLAN.md"
        );
        assert!(
            rendered.contains("plan.xml"),
            "Should mention XML fallback: {}",
            &rendered[..rendered.len().min(300)]
        );
    });
}

/// Test that PromptContentBuilder handles all three oversized content types.
///
/// When PROMPT, PLAN, and DIFF all exceed MAX_INLINE_CONTENT_SIZE, the builder
/// should correctly reference all backup locations and the rendered output
/// should contain appropriate instructions for each.
#[test]
fn builder_handles_all_three_oversized_content_types() {
    with_default_timeout(|| {
        let large_content = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/PROMPT.md.backup", &large_content)
            .with_file(".agent/PLAN.md", &large_content);

        let builder = PromptContentBuilder::new(&workspace)
            .with_prompt(large_content.clone())
            .with_plan(large_content.clone())
            .with_diff(large_content, "abc123def");

        assert!(
            builder.has_oversize_content(),
            "Builder should detect all oversize content"
        );

        let refs = builder.build();

        // Verify PROMPT references backup
        let prompt_rendered = refs.prompt_for_template();
        assert!(
            prompt_rendered.contains("PROMPT.md.backup"),
            "PROMPT should reference backup: {}",
            &prompt_rendered[..prompt_rendered.len().min(200)]
        );

        // Verify PLAN references file with XML fallback
        let plan_rendered = refs.plan_for_template();
        assert!(
            plan_rendered.contains(".agent/PLAN.md"),
            "PLAN should reference PLAN.md"
        );
        assert!(
            plan_rendered.contains("plan.xml"),
            "PLAN should mention XML fallback"
        );

        // Verify DIFF references git command
        let diff_rendered = refs.diff_for_template();
        assert!(
            diff_rendered.contains(".agent/tmp/diff.txt"),
            "DIFF should reference .agent/tmp/diff.txt: {}",
            &diff_rendered[..diff_rendered.len().min(200)]
        );

        assert!(workspace.was_written(".agent/tmp/diff.txt"));

        // Verify none are inline
        assert!(!refs.prompt_is_inline());
        assert!(!refs.plan_is_inline());
        assert!(!refs.diff_is_inline());
    });
}

/// Test that developer iteration prompt correctly uses oversized content references.
///
/// When using prompt_developer_iteration_xml_with_references with oversized content,
/// the generated prompt should include file path instructions, not embedded content.
#[test]
fn developer_iteration_prompt_uses_oversize_references() {
    with_default_timeout(|| {
        use ralph_workflow::prompts::prompt_developer_iteration_xml_with_references;
        use ralph_workflow::prompts::template_context::TemplateContext;

        let large_content = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/PROMPT.md.backup", &large_content);

        let context = TemplateContext::default();
        let refs = PromptContentBuilder::new(&workspace)
            .with_prompt(large_content.clone())
            .with_plan(large_content)
            .build();

        let prompt = prompt_developer_iteration_xml_with_references(&context, &refs);

        // Should contain file reference instructions, not embedded content
        assert!(
            prompt.contains("PROMPT.md.backup") || prompt.contains("Read from"),
            "Prompt should reference backup file: {}",
            &prompt[..prompt.len().min(500)]
        );
        assert!(
            prompt.contains(".agent/PLAN.md") || prompt.contains("plan.xml"),
            "Prompt should reference plan file"
        );
        // Should NOT contain the repeated 'x' pattern from large content
        assert!(
            !prompt.contains(&"x".repeat(1000)),
            "Prompt should not embed large content"
        );
    });
}
