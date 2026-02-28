//! Integration tests for oversize prompt handling.
//!
//! Tests verify that when PROMPT, PLAN, or DIFF content exceeds the size limit,
//! the system correctly references backup files instead of embedding content.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests follow the integration test style guide defined in
//! **[`INTEGRATION_TESTS.md`](../../INTEGRATION_TESTS.md)**.
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
/// When PROMPT.md content exceeds `MAX_INLINE_CONTENT_SIZE`, the generated
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
/// When diff content exceeds `MAX_INLINE_CONTENT_SIZE`, the generated
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
/// When PLAN.md content exceeds `MAX_INLINE_CONTENT_SIZE`, the generated
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
/// Content below `MAX_INLINE_CONTENT_SIZE` should be embedded directly
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

/// Test that exactly `MAX_INLINE_CONTENT_SIZE` is embedded inline.
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

/// Test that one byte over `MAX_INLINE_CONTENT_SIZE` triggers file reference.
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

/// Test that `DiffContentReference` handles empty start commit.
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

/// Test that `PlanContentReference` without XML fallback works correctly.
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

/// Test that `PromptContentBuilder` correctly reports oversize state.
///
/// The `has_oversize_content()` method should return true if ANY content
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
/// When PLAN.md content exceeds `MAX_INLINE_CONTENT_SIZE` and PLAN.md doesn't exist
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

/// Test that `PromptContentBuilder` handles all three oversized content types.
///
/// When PROMPT, PLAN, and DIFF all exceed `MAX_INLINE_CONTENT_SIZE`, the builder
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
/// When using `prompt_developer_iteration_xml_with_references` with oversized content,
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

        let workspace = ralph_workflow::workspace::MemoryWorkspace::new_test();
        let prompt = prompt_developer_iteration_xml_with_references(&context, &refs, &workspace);

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
// Tests for commit diff materialization stability across XSD retries
// =============================================================================

/// Test that commit diff materialization is stable across XSD retries.
///
/// When commit XML validation fails and we retry with an `XsdRetry` prompt, the diff
/// should NOT be re-truncated. The materialized input from the first attempt should
/// be reused because the `content_id` and `consumer_signature` match.
///
/// This is a regression test for the bug where truncation warnings repeated with
/// each retry attempt, even though the diff content hadn't changed.
#[test]
fn commit_diff_materialization_stable_across_xsd_retries() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::phases::commit::{
        effective_model_budget_bytes, truncate_diff_to_model_budget,
    };
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::prompt_inputs::sha256_hex_str;
    use ralph_workflow::reducer::state::{
        CommitState, MaterializedCommitInputs, MaterializedPromptInput, PipelineState,
        PromptInputKind, PromptInputRepresentation, PromptInputsState, PromptMaterializationReason,
        PromptPermissionsState,
    };

    with_default_timeout(|| {
        // Create a large diff that exceeds the model budget for qwen (100KB)
        let large_diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(150_000));
        let content_id = sha256_hex_str(&large_diff);

        // Calculate what the truncated diff should look like
        let model_budget = effective_model_budget_bytes(&["qwen".to_string()]);
        let (model_safe_diff, truncated) = truncate_diff_to_model_budget(&large_diff, model_budget);
        assert!(truncated, "diff should be truncated for this test");

        // Build a PipelineState with materialized inputs already present
        let agent_chain = PipelineState::initial(1, 0).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        );
        let consumer_signature = agent_chain.consumer_signature_sha256();

        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: 3,
            },
            commit_diff_prepared: true,
            commit_diff_empty: false,
            commit_diff_content_id_sha256: Some(content_id.clone()),
            commit_prompt_prepared: false,
            agent_chain,
            prompt_inputs: PromptInputsState {
                commit: Some(MaterializedCommitInputs {
                    attempt: 1,
                    diff: MaterializedPromptInput {
                        kind: PromptInputKind::Diff,
                        content_id_sha256: content_id,
                        consumer_signature_sha256: consumer_signature,
                        original_bytes: large_diff.len() as u64,
                        final_bytes: model_safe_diff.len() as u64,
                        model_budget_bytes: Some(model_budget),
                        inline_budget_bytes: Some(MAX_INLINE_CONTENT_SIZE as u64),
                        representation: PromptInputRepresentation::Inline,
                        reason: PromptMaterializationReason::ModelBudgetExceeded,
                    },
                }),
                ..Default::default()
            },
            prompt_permissions: PromptPermissionsState {
                locked: true,
                restore_needed: true,
                ..Default::default()
            },
            ..PipelineState::initial(1, 0)
        };

        // The production orchestrator should skip materialization and go to PrepareCommitPrompt
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::PrepareCommitPrompt { .. }),
            "Materialized inputs should be reused across XSD retries: got {effect:?}"
        );

        // Verify the materialized diff content is stable (truncation is deterministic)
        let second_truncation = truncate_diff_to_model_budget(&large_diff, model_budget);
        assert_eq!(
            model_safe_diff, second_truncation.0,
            "Truncation should be deterministic for the same input and budget"
        );
    });
}

/// Test that `OversizeDetected` events are emitted once per `content_id`, not per effect invocation.
///
/// This verifies the reducer-driven materialization system correctly deduplicates
/// oversize handling. When the same diff is processed multiple times (e.g., during
/// XSD retry loops), the truncation/oversize event should only be emitted once.
///
/// Regression test for oscillating budget warnings like:
/// "Diff size (625 KB) exceeds agent limit (292 KB)" repeated multiple times.
#[test]
fn no_repeated_oversize_warnings_in_event_loop() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::phases::commit::{
        effective_model_budget_bytes, truncate_diff_to_model_budget,
    };
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::prompt_inputs::sha256_hex_str;
    use ralph_workflow::reducer::state::{
        CommitState, MaterializedCommitInputs, MaterializedPromptInput, PipelineState,
        PromptInputKind, PromptInputRepresentation, PromptInputsState, PromptMaterializationReason,
        PromptPermissionsState,
    };

    with_default_timeout(|| {
        // Create diff that exceeds model budget
        let large_diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(200_000));

        // Simulate multi-agent chain with different budgets
        let agents = vec![
            "claude-opus".to_string(), // 300KB
            "qwen".to_string(),        // 100KB (smallest)
            "gpt-4".to_string(),       // 200KB
        ];

        // The effective budget should be the minimum across the chain
        let effective_budget = effective_model_budget_bytes(&agents);
        assert_eq!(
            effective_budget, 100_000,
            "effective budget should be min across agent chain (qwen's 100KB)"
        );

        let content_id = sha256_hex_str(&large_diff);

        // First materialization: should truncate and record
        let (model_safe_diff, truncated) =
            truncate_diff_to_model_budget(&large_diff, effective_budget);
        assert!(truncated, "diff should be truncated");

        // Build state with materialized inputs already present
        let agent_chain = PipelineState::initial(1, 0).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        );
        let consumer_signature = agent_chain.consumer_signature_sha256();

        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: 3,
            },
            commit_diff_prepared: true,
            commit_diff_empty: false,
            commit_diff_content_id_sha256: Some(content_id.clone()),
            commit_prompt_prepared: false,
            agent_chain,
            prompt_inputs: PromptInputsState {
                commit: Some(MaterializedCommitInputs {
                    attempt: 1,
                    diff: MaterializedPromptInput {
                        kind: PromptInputKind::Diff,
                        content_id_sha256: content_id,
                        consumer_signature_sha256: consumer_signature,
                        original_bytes: large_diff.len() as u64,
                        final_bytes: model_safe_diff.len() as u64,
                        model_budget_bytes: Some(effective_budget),
                        inline_budget_bytes: Some(MAX_INLINE_CONTENT_SIZE as u64),
                        representation: PromptInputRepresentation::Inline,
                        reason: PromptMaterializationReason::ModelBudgetExceeded,
                    },
                }),
                ..Default::default()
            },
            prompt_permissions: PromptPermissionsState {
                locked: true,
                restore_needed: true,
                ..Default::default()
            },
            ..PipelineState::initial(1, 0)
        };

        // The orchestrator should skip materialization (PrepareCommitPrompt, not MaterializeCommitInputs)
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::PrepareCommitPrompt { .. }),
            "XSD retry should NOT trigger re-materialization: got {effect:?}"
        );

        // Verify budget is stable regardless of which agent is "current"
        // The effective budget is always the min, not per-agent
        for agent in &agents {
            let per_agent_budget =
                ralph_workflow::phases::commit::model_budget_bytes_for_agent_name(agent);
            assert!(
                per_agent_budget >= effective_budget,
                "individual agent budget should be >= effective chain budget"
            );
        }
    });
}
