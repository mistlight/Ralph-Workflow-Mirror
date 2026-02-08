//! Commit input materialization and diff checking.
//!
//! This module handles preparing inputs for commit message generation:
//! - Reading git diff from workspace
//! - Truncating diff to model budget limits
//! - Materializing diff as inline or file reference
//! - Computing content hashes for cache invalidation
//!
//! ## Model Budget Management
//!
//! Large diffs are truncated to fit within the model's context window:
//! - Compute effective model budget from agent configuration
//! - Truncate diff if it exceeds model budget
//! - Write truncated diff to `.agent/tmp/commit_diff.model_safe.txt`
//! - Emit events tracking truncation for observability
//!
//! ## Inline vs File Reference
//!
//! Small diffs are embedded inline in prompts; large diffs are passed by reference:
//! - Inline budget: 32KB (MAX_INLINE_CONTENT_SIZE)
//! - Below inline budget → PromptInputRepresentation::Inline
//! - Above inline budget → PromptInputRepresentation::FileReference
//!
//! ## Diff Checking
//!
//! Before materializing, check if there are staged changes to commit:
//! - Run `git diff` to get staged changes
//! - Write diff to `.agent/tmp/commit_diff.txt`
//! - Emit `commit_diff_prepared` with empty flag and content hash

use super::super::MainEffectHandler;
use crate::phases::commit::{effective_model_budget_bytes, truncate_diff_to_model_budget};
use crate::phases::PhaseContext;
use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::ErrorEvent;
use crate::reducer::event::PipelineEvent;
use crate::reducer::event::WorkspaceIoErrorKind;
use crate::reducer::prompt_inputs::sha256_hex_str;
use crate::reducer::state::{
    MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason,
};
use crate::reducer::ui_event::UIEvent;
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
    /// Materialize commit inputs from staged diff.
    ///
    /// Reads `.agent/tmp/commit_diff.txt`, truncates to model budget,
    /// and materializes as inline or file reference representation.
    ///
    /// # Events Emitted
    ///
    /// - `commit_inputs_materialized` - Inputs successfully materialized
    /// - `commit_diff_invalidated` - Diff file missing; needs recomputation
    /// - `prompt_input_oversize_detected` - Diff exceeds budget (UI observability)
    pub(in crate::reducer::handler) fn materialize_commit_inputs(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        attempt: u32,
    ) -> Result<EffectResult> {
        let diff = match ctx.workspace.read(Path::new(".agent/tmp/commit_diff.txt")) {
            Ok(diff) => diff,
            Err(_) => {
                ctx.logger.warn(
                        "Missing commit diff at .agent/tmp/commit_diff.txt; invalidating diff-prepared state to recompute",
                    );
                return Ok(EffectResult::event(PipelineEvent::commit_diff_invalidated(
                    "Missing commit diff at .agent/tmp/commit_diff.txt".to_string(),
                )));
            }
        };

        let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();
        let content_id_sha256 = sha256_hex_str(&diff);
        let original_bytes = diff.len() as u64;

        let model_budget_bytes = effective_model_budget_bytes(&self.state.agent_chain.agents);
        let (model_safe_diff, truncated_for_model_budget) =
            truncate_diff_to_model_budget(&diff, model_budget_bytes);
        let final_bytes = model_safe_diff.len() as u64;

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                ErrorEvent::WorkspaceCreateDirAllFailed {
                    path: tmp_dir.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
            })?;
        }
        let model_safe_path = Path::new(".agent/tmp/commit_diff.model_safe.txt");
        ctx.workspace
            .write_atomic(model_safe_path, &model_safe_diff)
            .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                path: model_safe_path.display().to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;

        let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
        let representation = if final_bytes <= inline_budget_bytes {
            PromptInputRepresentation::Inline
        } else {
            PromptInputRepresentation::FileReference {
                path: model_safe_path.to_path_buf(),
            }
        };

        let reason = if truncated_for_model_budget {
            // Preserve the fact that we truncated for the model budget even if we ultimately
            // choose a file reference due to inline constraints.
            PromptMaterializationReason::ModelBudgetExceeded
        } else if matches!(
            representation,
            PromptInputRepresentation::FileReference { .. }
        ) {
            PromptMaterializationReason::InlineBudgetExceeded
        } else {
            PromptMaterializationReason::WithinBudgets
        };

        if truncated_for_model_budget {
            ctx.logger.warn(&format!(
                "Diff size ({} KB) exceeds model budget ({} KB). Truncated to {} KB at: {}",
                original_bytes / 1024,
                model_budget_bytes / 1024,
                final_bytes / 1024,
                model_safe_path.display()
            ));
        } else if final_bytes > inline_budget_bytes {
            ctx.logger.warn(&format!(
                "Diff size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                final_bytes / 1024,
                inline_budget_bytes / 1024,
                model_safe_path.display()
            ));
        }

        let input = MaterializedPromptInput {
            kind: PromptInputKind::Diff,
            content_id_sha256: content_id_sha256.clone(),
            consumer_signature_sha256: consumer_signature_sha256.clone(),
            original_bytes,
            final_bytes,
            model_budget_bytes: Some(model_budget_bytes),
            inline_budget_bytes: Some(inline_budget_bytes),
            representation,
            reason,
        };

        let mut result =
            EffectResult::event(PipelineEvent::commit_inputs_materialized(attempt, input));
        if truncated_for_model_budget {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Truncated DIFF for model budget: {} KB -> {} KB (budget {} KB)",
                    original_bytes / 1024,
                    final_bytes / 1024,
                    model_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                crate::reducer::event::PipelinePhase::CommitMessage,
                PromptInputKind::Diff,
                content_id_sha256.clone(),
                original_bytes,
                model_budget_bytes,
                "model-context".to_string(),
            ));
        }
        if final_bytes > inline_budget_bytes {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Oversize DIFF: {} KB > {} KB; using file reference",
                    final_bytes / 1024,
                    inline_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                crate::reducer::event::PipelinePhase::CommitMessage,
                PromptInputKind::Diff,
                content_id_sha256,
                final_bytes,
                inline_budget_bytes,
                "inline-embedding".to_string(),
            ));
        }
        Ok(result)
    }

    /// Check commit diff by running `git diff`.
    ///
    /// This is the main entry point for diff checking. It runs `git diff` and
    /// delegates to `check_commit_diff_with_result`.
    pub(in crate::reducer::handler) fn check_commit_diff(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let diff = crate::git_helpers::git_diff().map_err(anyhow::Error::from);
        self.check_commit_diff_with_result(ctx, diff)
    }

    /// Check commit diff with a pre-computed diff result.
    ///
    /// This variant allows testing with mocked diff results.
    pub(in crate::reducer::handler) fn check_commit_diff_with_result(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff: Result<String, anyhow::Error>,
    ) -> Result<EffectResult> {
        match diff {
            Ok(diff) => self.check_commit_diff_with_content(ctx, &diff),
            Err(err) => Ok(EffectResult::event(PipelineEvent::commit_diff_failed(
                err.to_string(),
            ))),
        }
    }

    /// Check commit diff with pre-computed diff content.
    ///
    /// Writes diff to `.agent/tmp/commit_diff.txt` and emits `commit_diff_prepared`.
    pub(in crate::reducer::handler) fn check_commit_diff_with_content(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff: &str,
    ) -> Result<EffectResult> {
        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                ErrorEvent::WorkspaceCreateDirAllFailed {
                    path: tmp_dir.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
            })?;
        }
        ctx.workspace
            .write(Path::new(".agent/tmp/commit_diff.txt"), diff)
            .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                path: ".agent/tmp/commit_diff.txt".to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;

        Ok(EffectResult::event(PipelineEvent::commit_diff_prepared(
            diff.trim().is_empty(),
            sha256_hex_str(diff),
        )))
    }
}
