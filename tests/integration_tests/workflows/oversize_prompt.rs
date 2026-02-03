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
            rendered.contains("Unstaged changes: git diff")
                && rendered.contains("Staged changes:   git diff --cached"),
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

/// Rebuilding references with the same content should be deterministic.
#[test]
fn prompt_content_builder_is_deterministic_across_repeated_builds() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");

        let large_diff = format!(
            "diff --git a/a b/a\n+{}\n",
            "x".repeat(MAX_INLINE_CONTENT_SIZE + 10)
        );

        let refs1 = PromptContentBuilder::new(&workspace)
            .with_diff(large_diff.clone(), "abc123")
            .build();
        let first = refs1.diff_for_template();

        let refs2 = PromptContentBuilder::new(&workspace)
            .with_diff(large_diff.clone(), "abc123")
            .build();
        let second = refs2.diff_for_template();

        assert_eq!(
            first, second,
            "diff reference rendering should be deterministic"
        );
        assert_eq!(
            workspace
                .get_file(".agent/tmp/diff.txt")
                .expect("diff file should be written"),
            large_diff,
            "diff file contents should remain stable across repeated builds"
        );
    });
}

// =============================================================================
// Reducer-driven materialization stability tests
// =============================================================================

/// Test that MaterializedPromptInput records the correct sizes and reasons.
///
/// This verifies that the reducer state accurately reflects what happened
/// during materialization, enabling deduplication and observability.
#[test]
fn materialized_prompt_input_records_sizes_correctly() {
    use ralph_workflow::reducer::state::{
        MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
        PromptMaterializationReason,
    };

    with_default_timeout(|| {
        let original_bytes = (MAX_INLINE_CONTENT_SIZE + 1000) as u64;
        let final_bytes = MAX_INLINE_CONTENT_SIZE as u64;

        let input = MaterializedPromptInput {
            kind: PromptInputKind::Diff,
            content_id_sha256: "abc123".to_string(),
            consumer_signature_sha256: "def456".to_string(),
            original_bytes,
            final_bytes,
            model_budget_bytes: Some(200_000),
            inline_budget_bytes: Some(MAX_INLINE_CONTENT_SIZE as u64),
            representation: PromptInputRepresentation::FileReference {
                path: std::path::PathBuf::from(".agent/tmp/diff.txt"),
            },
            reason: PromptMaterializationReason::InlineBudgetExceeded,
        };

        assert_eq!(input.original_bytes, original_bytes);
        assert_eq!(input.final_bytes, final_bytes);
        assert!(
            matches!(
                input.reason,
                PromptMaterializationReason::InlineBudgetExceeded
            ),
            "should record correct reason"
        );
        assert!(
            matches!(
                input.representation,
                PromptInputRepresentation::FileReference { .. }
            ),
            "should use file reference for oversize content"
        );
    });
}

/// Test that content_id_sha256 is consistent for identical content.
///
/// This ensures deduplication works correctly - same content should
/// produce the same content ID regardless of when it's computed.
#[test]
fn content_id_sha256_is_deterministic_for_same_content() {
    use ralph_workflow::reducer::prompt_inputs::sha256_hex_str;

    with_default_timeout(|| {
        let content = "x".repeat(MAX_INLINE_CONTENT_SIZE + 100);

        let id1 = sha256_hex_str(&content);
        let id2 = sha256_hex_str(&content);
        let id3 = sha256_hex_str(&content);

        assert_eq!(id1, id2, "SHA-256 should be deterministic");
        assert_eq!(id2, id3, "SHA-256 should be deterministic");
        assert_eq!(id1.len(), 64, "SHA-256 should be 64 hex characters");
    });
}

/// Test that different content produces different content_id_sha256.
///
/// This ensures content IDs can distinguish between different inputs.
#[test]
fn content_id_sha256_differs_for_different_content() {
    use ralph_workflow::reducer::prompt_inputs::sha256_hex_str;

    with_default_timeout(|| {
        let content1 = "content version 1";
        let content2 = "content version 2";

        let id1 = sha256_hex_str(content1);
        let id2 = sha256_hex_str(content2);

        assert_ne!(id1, id2, "different content should produce different IDs");
    });
}

/// Test that PromptInputRepresentation correctly distinguishes inline from file reference.
#[test]
fn prompt_input_representation_inline_vs_file_reference() {
    use ralph_workflow::reducer::state::PromptInputRepresentation;

    with_default_timeout(|| {
        let inline = PromptInputRepresentation::Inline;
        let file_ref = PromptInputRepresentation::FileReference {
            path: std::path::PathBuf::from(".agent/tmp/test.txt"),
        };

        assert!(
            matches!(inline, PromptInputRepresentation::Inline),
            "inline should match Inline"
        );
        assert!(
            matches!(file_ref, PromptInputRepresentation::FileReference { .. }),
            "file ref should match FileReference"
        );
    });
}

/// Test that PromptMaterializationReason covers all expected cases.
#[test]
fn prompt_materialization_reason_covers_all_cases() {
    use ralph_workflow::reducer::state::PromptMaterializationReason;

    with_default_timeout(|| {
        // Verify all expected enum variants exist and can be matched
        let reasons = vec![
            PromptMaterializationReason::WithinBudgets,
            PromptMaterializationReason::InlineBudgetExceeded,
            PromptMaterializationReason::ModelBudgetExceeded,
            PromptMaterializationReason::PolicyForcedReference,
        ];

        for reason in reasons {
            match reason {
                PromptMaterializationReason::WithinBudgets => {}
                PromptMaterializationReason::InlineBudgetExceeded => {}
                PromptMaterializationReason::ModelBudgetExceeded => {}
                PromptMaterializationReason::PolicyForcedReference => {}
            }
        }
    });
}

/// Test that PromptInputsState can store all phase inputs.
#[test]
fn prompt_inputs_state_stores_all_phases() {
    use ralph_workflow::reducer::state::{
        MaterializedCommitInputs, MaterializedDevelopmentInputs, MaterializedPlanningInputs,
        MaterializedPromptInput, MaterializedReviewInputs, PromptInputKind,
        PromptInputRepresentation, PromptInputsState, PromptMaterializationReason,
    };

    with_default_timeout(|| {
        let make_input = |kind| MaterializedPromptInput {
            kind,
            content_id_sha256: "hash".to_string(),
            consumer_signature_sha256: "sig".to_string(),
            original_bytes: 100,
            final_bytes: 100,
            model_budget_bytes: None,
            inline_budget_bytes: Some(100_000),
            representation: PromptInputRepresentation::Inline,
            reason: PromptMaterializationReason::WithinBudgets,
        };

        let state = PromptInputsState {
            planning: Some(MaterializedPlanningInputs {
                iteration: 1,
                prompt: make_input(PromptInputKind::Prompt),
            }),
            development: Some(MaterializedDevelopmentInputs {
                iteration: 1,
                prompt: make_input(PromptInputKind::Prompt),
                plan: make_input(PromptInputKind::Plan),
            }),
            review: Some(MaterializedReviewInputs {
                pass: 1,
                plan: make_input(PromptInputKind::Plan),
                diff: make_input(PromptInputKind::Diff),
            }),
            commit: Some(MaterializedCommitInputs {
                attempt: 1,
                diff: make_input(PromptInputKind::Diff),
            }),
        };

        assert!(state.planning.is_some());
        assert!(state.development.is_some());
        assert!(state.review.is_some());
        assert!(state.commit.is_some());
    });
}

// =============================================================================
// Reducer-driven materialization stability tests (regression tests for bug fix)
// =============================================================================

/// Test that commit diff model budget uses minimum across agent chain.
///
/// This regression test verifies the fix for the oscillating budget bug where
/// truncation repeated with different limits as the current agent changed.
/// The effective budget should be the minimum across ALL agents in the chain.
#[test]
fn commit_model_budget_is_min_across_agent_chain() {
    use ralph_workflow::phases::commit::effective_model_budget_bytes;

    with_default_timeout(|| {
        // Mixed chain: claude (300KB), qwen (100KB), default (200KB)
        let agents = vec![
            "claude-opus".to_string(),
            "qwen-turbo".to_string(),
            "gpt-4".to_string(),
        ];

        let budget = effective_model_budget_bytes(&agents);

        // Should be qwen's 100KB (the minimum)
        assert_eq!(
            budget, 100_000,
            "budget should be min across chain (qwen's 100KB), not oscillate per-agent"
        );
    });
}

/// Test that commit inputs materialization is stable when only current_agent_index changes.
///
/// This regression test verifies that changing which agent is current (during
/// fallback or retry) does NOT change the consumer_signature, so materialized
/// inputs are reused instead of re-truncated.
#[test]
fn commit_inputs_reused_during_agent_fallback() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::state::AgentChainState;

    with_default_timeout(|| {
        // Create chain with multiple agents
        let chain1 = AgentChainState::initial().with_agents(
            vec!["claude".to_string(), "qwen".to_string()],
            vec![vec![], vec![]],
            AgentRole::Commit,
        );

        // Simulate fallback: same chain but different current agent
        let mut chain2 = chain1.clone();
        chain2.current_agent_index = 1;

        let sig1 = chain1.consumer_signature_sha256();
        let sig2 = chain2.consumer_signature_sha256();

        assert_eq!(
            sig1, sig2,
            "consumer signature should be stable when only current_agent_index changes, \
             ensuring materialized inputs are reused during fallback"
        );
    });
}

/// Test that commit inputs are re-materialized when agent chain configuration changes.
///
/// If the set of agents or their models change, the consumer_signature should
/// change, triggering re-materialization with the new effective budget.
#[test]
fn commit_inputs_rematerialized_when_agent_chain_changes() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::state::AgentChainState;

    with_default_timeout(|| {
        // Original chain with just claude (300KB budget)
        let chain1 = AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        );

        // Modified chain with qwen added (now 100KB budget)
        let chain2 = AgentChainState::initial().with_agents(
            vec!["claude".to_string(), "qwen".to_string()],
            vec![vec![], vec![]],
            AgentRole::Commit,
        );

        let sig1 = chain1.consumer_signature_sha256();
        let sig2 = chain2.consumer_signature_sha256();

        assert_ne!(
            sig1, sig2,
            "consumer signature should change when agent chain configuration changes, \
             triggering re-materialization with new effective budget"
        );
    });
}

/// Test that truncate_diff_to_model_budget is deterministic.
///
/// Given the same diff and budget, truncation should always produce the
/// same result, ensuring stable behavior across retries.
#[test]
fn truncation_is_deterministic_across_calls() {
    use ralph_workflow::phases::commit::truncate_diff_to_model_budget;

    with_default_timeout(|| {
        let large_diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(300_000));
        let budget = 100_000u64;

        let (result1, truncated1) = truncate_diff_to_model_budget(&large_diff, budget);
        let (result2, truncated2) = truncate_diff_to_model_budget(&large_diff, budget);
        let (result3, truncated3) = truncate_diff_to_model_budget(&large_diff, budget);

        assert_eq!(
            result1, result2,
            "truncation result should be deterministic"
        );
        assert_eq!(
            result2, result3,
            "truncation result should be deterministic"
        );
        assert_eq!(
            truncated1, truncated2,
            "truncation flag should be deterministic"
        );
        assert_eq!(
            truncated2, truncated3,
            "truncation flag should be deterministic"
        );
    });
}
