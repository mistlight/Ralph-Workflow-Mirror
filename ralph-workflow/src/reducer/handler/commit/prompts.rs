//! Commit prompt preparation.
//!
//! This module handles generating prompts for commit message generation:
//! - Loading materialized diff inputs
//! - Generating prompts from templates
//! - Handling XSD retry prompts
//! - Handling same-agent retry prompts
//! - Validating template variables
//!
//! ## Prompt Modes
//!
//! - **Normal** - Standard commit message generation prompt
//! - **XsdRetry** - Retry prompt after XML validation failure
//! - **SameAgentRetry** - Retry prompt with retry guidance preamble
//! - **Continuation** - Not supported for commit phase (returns error)
//!
//! ## Template Validation
//!
//! All prompts are validated for unresolved template placeholders.
//! If validation fails, emits `agent_template_variables_invalid` event.
//!
//! ## Prompt Storage
//!
//! Prompts are written to `.agent/tmp/commit_prompt.txt` for:
//! - Agent invocation
//! - Same-agent retry context preservation
//! - Debugging and observability

use super::super::MainEffectHandler;
use super::current_commit_attempt;
use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use crate::prompts::content_reference::{DiffContentReference, MAX_INLINE_CONTENT_SIZE};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_commit_xsd_retry_with_context,
    prompt_generate_commit_message_with_diff_with_context,
};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::ErrorEvent;
use crate::reducer::event::PipelineEvent;
use crate::reducer::event::WorkspaceIoErrorKind;
use crate::reducer::state::{PromptInputRepresentation, PromptMode};
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
    /// Prepare commit prompt based on prompt mode.
    ///
    /// This is the main entry point for commit prompt preparation.
    /// It handles XSD retry mode specially, then delegates to
    /// `prepare_commit_prompt_with_diff_and_mode` for normal/retry modes.
    ///
    /// # Events Emitted
    ///
    /// - `commit_prompt_prepared` - Prompt successfully generated
    /// - `commit_diff_invalidated` - Materialized diff missing
    /// - `agent_template_variables_invalid` - Template validation failed
    ///
    /// # Errors
    ///
    /// - `CommitContinuationNotSupported` - Continuation mode not supported for commit
    /// - `CommitInputsNotMaterialized` - Inputs not materialized for this attempt
    pub(in crate::reducer::handler) fn prepare_commit_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        if matches!(prompt_mode, PromptMode::Continuation) {
            return Err(ErrorEvent::CommitContinuationNotSupported.into());
        }
        let attempt = current_commit_attempt(&self.state.commit);

        if matches!(prompt_mode, PromptMode::XsdRetry) {
            let xsd_error = self
                .state
                .continuation
                .last_xsd_error
                .clone()
                .unwrap_or_else(|| {
                    "XML output failed validation. Provide valid XML output.".to_string()
                });

            let prompt_key = format!(
                "commit_message_attempt_{attempt}_xsd_retry_{}",
                self.state.continuation.xsd_retry_count
            );
            let (prompt, was_replayed) =
                get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                    prompt_commit_xsd_retry_with_context(
                        ctx.template_context,
                        &xsd_error,
                        ctx.workspace,
                    )
                });

            if let Err(err) =
                crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
                    &prompt,
                    &[],
                )
            {
                return Ok(EffectResult::event(
                    PipelineEvent::agent_template_variables_invalid(
                        AgentRole::Commit,
                        "commit_xsd_retry".to_string(),
                        Vec::new(),
                        err.unresolved_placeholders,
                    ),
                ));
            }

            if !was_replayed {
                ctx.capture_prompt(&prompt_key, &prompt);
            }

            let tmp_dir = Path::new(".agent/tmp");
            if !ctx.workspace.exists(tmp_dir) {
                ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                    ErrorEvent::WorkspaceCreateDirAllFailed {
                        path: tmp_dir.display().to_string(),
                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                    }
                })?;
            }

            // Write prompt file (non-fatal: if write fails, log warning and continue)
            // Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
            // If the prompt file write fails, we continue with orchestration - loop recovery will
            // handle convergence if needed.
            if let Err(err) = ctx
                .workspace
                .write(Path::new(".agent/tmp/commit_prompt.txt"), &prompt)
            {
                ctx.logger.warn(&format!(
                    "Failed to write commit prompt file: {}. Pipeline will continue (loop recovery will handle convergence).",
                    err
                ));
            }

            return Ok(
                EffectResult::event(PipelineEvent::commit_prompt_prepared(attempt)).with_ui_event(
                    self.phase_transition_ui(crate::reducer::event::PipelinePhase::CommitMessage),
                ),
            );
        }

        let inputs = self
            .state
            .prompt_inputs
            .commit
            .as_ref()
            .filter(|c| c.attempt == attempt)
            .ok_or(ErrorEvent::CommitInputsNotMaterialized { attempt })?;

        let model_safe_path = Path::new(".agent/tmp/commit_diff.model_safe.txt");
        let diff_for_prompt = match &inputs.diff.representation {
            PromptInputRepresentation::Inline => match ctx.workspace.read(model_safe_path) {
                Ok(diff) => diff,
                Err(err) => {
                    ctx.logger.warn(&format!(
                        "Missing/unreadable materialized commit diff at {} ({err}); invalidating commit inputs to rematerialize",
                        model_safe_path.display()
                    ));
                    // Recoverability: tmp artifacts may be cleaned between checkpoints.
                    // Force rerunning CheckCommitDiff to recreate the diff and its materialization.
                    return Ok(EffectResult::event(PipelineEvent::commit_diff_invalidated(
                        "Missing/unreadable .agent/tmp/commit_diff.model_safe.txt".to_string(),
                    )));
                }
            },
            PromptInputRepresentation::FileReference { path } => {
                if !ctx.workspace.exists(path) {
                    ctx.logger.warn(&format!(
                        "Missing materialized commit diff reference at {}; invalidating commit inputs to rematerialize",
                        path.display()
                    ));
                    // Recoverability: tmp artifacts may be cleaned between checkpoints.
                    // Force rerunning CheckCommitDiff to recreate the diff and its materialization.
                    return Ok(EffectResult::event(PipelineEvent::commit_diff_invalidated(
                        "Missing materialized commit diff reference".to_string(),
                    )));
                }
                DiffContentReference::ReadFromFile {
                    path: path.to_path_buf(),
                    start_commit: String::new(),
                    description: format!(
                        "Diff is {} bytes (exceeds {} limit)",
                        inputs.diff.final_bytes, MAX_INLINE_CONTENT_SIZE
                    ),
                }
                .render_for_template()
            }
        };
        self.prepare_commit_prompt_with_diff_and_mode(ctx, &diff_for_prompt, prompt_mode)
    }

    /// Prepare commit prompt with pre-loaded diff content and mode.
    ///
    /// This handles Normal and SameAgentRetry modes. XsdRetry mode is handled
    /// in `prepare_commit_prompt` which returns early.
    ///
    /// # Prompt Modes
    ///
    /// - **Normal** - Generate fresh prompt from template
    /// - **SameAgentRetry** - Prepend retry guidance to last prompt
    ///
    /// # Template Validation
    ///
    /// Validates that all template placeholders are resolved. If validation fails,
    /// emits `agent_template_variables_invalid` event.
    pub(in crate::reducer::handler) fn prepare_commit_prompt_with_diff_and_mode(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff_for_prompt: &str,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);

        let continuation_state = &self.state.continuation;
        let (prompt_key, prompt, was_replayed, should_validate) = match prompt_mode {
            PromptMode::SameAgentRetry => {
                // Same-agent retry: prepend retry guidance to the last prepared prompt for this
                // phase (preserves XSD retry context if present).
                let retry_preamble =
                    super::super::retry_guidance::same_agent_retry_preamble(continuation_state);
                let (base_prompt, should_validate) = match ctx
                    .workspace
                    .read(Path::new(".agent/tmp/commit_prompt.txt"))
                {
                    Ok(previous_prompt) => (
                        super::super::retry_guidance::strip_existing_same_agent_retry_preamble(
                            &previous_prompt,
                        )
                        .to_string(),
                        false,
                    ),
                    Err(_) => (
                        prompt_generate_commit_message_with_diff_with_context(
                            ctx.template_context,
                            diff_for_prompt,
                            ctx.workspace,
                        ),
                        true,
                    ),
                };
                let prompt = format!("{retry_preamble}\n{base_prompt}");
                let prompt_key = format!(
                    "commit_message_attempt_{attempt}_same_agent_retry_{}",
                    continuation_state.same_agent_retry_count
                );
                (prompt_key, prompt, false, should_validate)
            }
            PromptMode::Normal => {
                let prompt_key = format!("commit_message_attempt_{attempt}");
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_generate_commit_message_with_diff_with_context(
                            ctx.template_context,
                            diff_for_prompt,
                            ctx.workspace,
                        )
                    });
                (prompt_key, prompt, was_replayed, true)
            }
            PromptMode::XsdRetry => {
                // XsdRetry is handled in prepare_commit_prompt() which returns early.
                // This branch is unreachable but required for exhaustiveness.
                unreachable!(
                    "XsdRetry mode should be handled by prepare_commit_prompt() before calling this function"
                )
            }
            PromptMode::Continuation => {
                return Err(ErrorEvent::CommitContinuationNotSupported.into());
            }
        };

        if should_validate {
            if let Err(err) =
                crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
                    &prompt,
                    &[diff_for_prompt],
                )
            {
                return Ok(EffectResult::event(
                    PipelineEvent::agent_template_variables_invalid(
                        AgentRole::Commit,
                        "commit_message_xml".to_string(),
                        Vec::new(),
                        err.unresolved_placeholders,
                    ),
                ));
            }
        }

        if !was_replayed {
            ctx.capture_prompt(&prompt_key, &prompt);
        }

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                ErrorEvent::WorkspaceCreateDirAllFailed {
                    path: tmp_dir.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
            })?;
        }

        // Write prompt file (non-fatal: if write fails, log warning and continue)
        // Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
        // If the prompt file write fails, we continue with orchestration - loop recovery will
        // handle convergence if needed.
        if let Err(err) = ctx
            .workspace
            .write(Path::new(".agent/tmp/commit_prompt.txt"), &prompt)
        {
            ctx.logger.warn(&format!(
                "Failed to write commit prompt file: {}. Pipeline will continue (loop recovery will handle convergence).",
                err
            ));
        }

        Ok(
            EffectResult::event(PipelineEvent::commit_prompt_prepared(attempt)).with_ui_event(
                self.phase_transition_ui(crate::reducer::event::PipelinePhase::CommitMessage),
            ),
        )
    }
}
