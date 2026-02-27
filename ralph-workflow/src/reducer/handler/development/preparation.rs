//! Development prompt preparation.
//!
//! Generates prompts based on mode (Normal, XSD Retry, Same-Agent Retry, Continuation).
//! Handles template variable validation, prompt replay from history, and prompt file writes.
//!
//! ## Prompt Modes
//!
//! - **Normal** - First attempt for iteration, uses `developer_iteration_xml` template
//! - **XSD Retry** - Invalid XML output, includes `last_output.xml` and XSD error context
//! - **Same-Agent Retry** - Agent failed (non-XML issues), prepends retry preamble
//! - **Continuation** - Partial progress, includes continuation context from previous attempt
//!
//! ## Prompt Replay
//!
//! Normal and Continuation mode prompts are replayed from history if available (same `prompt_key`).
//! This ensures deterministic prompt generation across resume operations.

use super::super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::phases::PhaseContext;
use crate::prompts::content_builder::PromptContentReferences;
use crate::prompts::content_reference::{
    PlanContentReference, PromptContentReference, MAX_INLINE_CONTENT_SIZE,
};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{ErrorEvent, PipelineEvent, WorkspaceIoErrorKind};
use crate::reducer::prompt_inputs::sha256_hex_str;
use crate::reducer::state::PromptMode;
use crate::reducer::state::{
    MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason,
};
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
    /// Prepare development prompt based on prompt mode.
    ///
    /// Generates the appropriate prompt for the developer agent based on the current mode:
    ///
    /// - **Normal** - First attempt for iteration, uses `developer_iteration_xml` template
    /// - **XSD Retry** - Invalid XML output, includes `last_output.xml` and XSD error context
    /// - **Same-Agent Retry** - Agent failed (non-XML issues), prepends retry preamble
    /// - **Continuation** - Partial progress, includes continuation context from previous attempt
    ///
    /// The prompt is validated for unresolved template variables (except for explicitly ignored
    /// inline content) and written to `.agent/tmp/development_prompt.txt` for debugging and
    /// same-agent retry fallback.
    ///
    /// # Prompt Replay
    ///
    /// Normal and Continuation mode prompts are replayed from history if available (same `prompt_key`).
    /// This ensures deterministic prompt generation across resume operations.
    ///
    /// # Template Variables
    ///
    /// If template variable validation fails, an `AgentTemplateVariablesInvalid` event is emitted
    /// and the agent is not invoked. This prevents sending malformed prompts to agents.
    ///
    /// # Non-Fatal Writes
    ///
    /// Per acceptance criteria #5, prompt file write failures log warnings but do not terminate
    /// the pipeline. Loop recovery will handle convergence if needed.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Phase context with workspace, template context, and prompt history
    /// * `iteration` - Current development iteration number
    /// * `prompt_mode` - Prompt generation mode (Normal, XSD Retry, Same-Agent Retry, Continuation)
    ///
    /// # Returns
    ///
    /// `EffectResult` with `DevelopmentPromptPrepared` event, plus optional
    /// `XsdRetryLastOutputMaterialized` and `PromptInputOversizeDetected` events for XSD retry mode.
    pub(in crate::reducer::handler) fn prepare_development_prompt(
        &self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_developer_iteration_continuation_xml,
            prompt_developer_iteration_continuation_xml_with_log,
            prompt_developer_iteration_xml_with_references,
            prompt_developer_iteration_xsd_retry_with_context_files_and_log,
        };

        let continuation_state = &self.state.continuation;
        let mut additional_events: Vec<PipelineEvent> = Vec::new();

        let (dev_prompt, template_name, prompt_key, was_replayed, _should_validate, rendered_log) =
            match prompt_mode {
                PromptMode::Continuation => {
                    let prompt_key = format!(
                        "development_{}_continuation_{}",
                        iteration, continuation_state.continuation_attempt
                    );
                    let (prompt, was_replayed) =
                        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                            prompt_developer_iteration_continuation_xml(
                                ctx.template_context,
                                continuation_state,
                                ctx.workspace,
                            )
                        });
                    let rendered_log = if was_replayed {
                        None
                    } else {
                        let rendered = prompt_developer_iteration_continuation_xml_with_log(
                            ctx.template_context,
                            continuation_state,
                            ctx.workspace,
                            "developer_iteration_continuation_xml",
                        );
                        if !rendered.log.is_complete() {
                            let missing = rendered.log.unsubstituted.clone();
                            let result = EffectResult::event(PipelineEvent::template_rendered(
                                crate::reducer::event::PipelinePhase::Development,
                                "developer_iteration_continuation_xml".to_string(),
                                rendered.log,
                            ))
                            .with_additional_event(
                                PipelineEvent::agent_template_variables_invalid(
                                    AgentRole::Developer,
                                    "developer_iteration_continuation_xml".to_string(),
                                    missing,
                                    Vec::new(),
                                ),
                            );
                            return Ok(result);
                        }
                        Some(rendered.log)
                    };
                    (
                        prompt,
                        "developer_iteration_continuation_xml",
                        Some(prompt_key),
                        was_replayed,
                        !was_replayed,
                        rendered_log,
                    )
                }
                PromptMode::XsdRetry => {
                    let last_output = ctx
                        .workspace
                        .read(Path::new(xml_paths::DEVELOPMENT_RESULT_XML))
                        .or_else(|err| {
                            if err.kind() == std::io::ErrorKind::NotFound {
                                // Try reading from the archived .processed file as a fallback
                                let processed_path =
                                    Path::new(".agent/tmp/development_result.xml.processed");
                                ctx.workspace.read(processed_path).inspect(|output| {
                                    ctx.logger.info(
                                        "XSD retry: using archived .processed file as last output",
                                    );
                                    let _ = output;
                                })
                            } else {
                                Err(err)
                            }
                        })
                        .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                            path: xml_paths::DEVELOPMENT_RESULT_XML.to_string(),
                            kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                        })?;

                    let content_id_sha256 = sha256_hex_str(&last_output);
                    let consumer_signature_sha256 =
                        self.state.agent_chain.consumer_signature_sha256();
                    let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
                    let last_output_bytes = last_output.len() as u64;

                    let already_materialized = self
                        .state
                        .prompt_inputs
                        .xsd_retry_last_output
                        .as_ref()
                        .is_some_and(|m| {
                            m.phase == crate::reducer::event::PipelinePhase::Development
                                && m.scope_id == iteration
                                && m.last_output.content_id_sha256 == content_id_sha256
                                && m.last_output.consumer_signature_sha256
                                    == consumer_signature_sha256
                        });

                    if !already_materialized {
                        let tmp_dir = Path::new(".agent/tmp");
                        if !ctx.workspace.exists(tmp_dir) {
                            ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                                ErrorEvent::WorkspaceCreateDirAllFailed {
                                    path: tmp_dir.display().to_string(),
                                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                                }
                            })?;
                        }
                        let last_output_path = Path::new(".agent/tmp/last_output.xml");
                        ctx.workspace
                            .write_atomic(last_output_path, &last_output)
                            .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                                path: last_output_path.display().to_string(),
                                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                            })?;

                        let input = MaterializedPromptInput {
                            kind: PromptInputKind::LastOutput,
                            content_id_sha256: content_id_sha256.clone(),
                            consumer_signature_sha256,
                            original_bytes: last_output_bytes,
                            final_bytes: last_output_bytes,
                            model_budget_bytes: None,
                            inline_budget_bytes: Some(inline_budget_bytes),
                            representation: PromptInputRepresentation::FileReference {
                                path: last_output_path.to_path_buf(),
                            },
                            reason: PromptMaterializationReason::PolicyForcedReference,
                        };
                        additional_events.push(PipelineEvent::xsd_retry_last_output_materialized(
                            crate::reducer::event::PipelinePhase::Development,
                            iteration,
                            input,
                        ));
                        if last_output_bytes > inline_budget_bytes {
                            additional_events.push(PipelineEvent::prompt_input_oversize_detected(
                                crate::reducer::event::PipelinePhase::Development,
                                PromptInputKind::LastOutput,
                                content_id_sha256,
                                last_output_bytes,
                                inline_budget_bytes,
                                "xsd-retry-context".to_string(),
                            ));
                        }
                    }
                    let rendered = prompt_developer_iteration_xsd_retry_with_context_files_and_log(
                        ctx.template_context,
                        "XML output failed validation. Provide valid XML output.",
                        ctx.workspace,
                        "developer_iteration_xsd_retry",
                    );

                    if !rendered.log.is_complete() {
                        let missing = rendered.log.unsubstituted.clone();
                        let result = EffectResult::event(PipelineEvent::template_rendered(
                            crate::reducer::event::PipelinePhase::Development,
                            "developer_iteration_xsd_retry".to_string(),
                            rendered.log,
                        ))
                        .with_additional_event(
                            PipelineEvent::agent_template_variables_invalid(
                                AgentRole::Developer,
                                "developer_iteration_xsd_retry".to_string(),
                                missing,
                                Vec::new(),
                            ),
                        );
                        return Ok(result);
                    }

                    (
                        rendered.content,
                        "developer_iteration_xsd_retry",
                        None,
                        false,
                        true,
                        Some(rendered.log),
                    )
                }
                PromptMode::SameAgentRetry => {
                    // Same-agent retry: prepend retry guidance to the last prepared prompt for this
                    // phase (preserves XSD retry / continuation context if present).
                    let retry_preamble =
                        super::super::retry_guidance::same_agent_retry_preamble(continuation_state);
                    let inputs = self
                        .state
                        .prompt_inputs
                        .development
                        .as_ref()
                        .filter(|p| p.iteration == iteration)
                        .ok_or(ErrorEvent::DevelopmentInputsNotMaterialized { iteration })?;

                    let prompt_ref = match &inputs.prompt.representation {
                        PromptInputRepresentation::Inline => {
                            let prompt_md =
                                ctx.workspace.read(Path::new("PROMPT.md")).map_err(|err| {
                                    ErrorEvent::WorkspaceReadFailed {
                                        path: "PROMPT.md".to_string(),
                                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                                    }
                                })?;
                            PromptContentReference::inline(prompt_md)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            PromptContentReference::file_path(
                                path.clone(),
                                "Original user requirements from PROMPT.md",
                            )
                        }
                    };

                    let plan_ref = match &inputs.plan.representation {
                        PromptInputRepresentation::Inline => {
                            let plan_md =
                                ctx.workspace
                                    .read(Path::new(".agent/PLAN.md"))
                                    .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                                        path: ".agent/PLAN.md".to_string(),
                                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                                    })?;
                            PlanContentReference::Inline(plan_md)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            PlanContentReference::ReadFromFile {
                                primary_path: path.clone(),
                                fallback_path: Some(Path::new(".agent/tmp/plan.xml").to_path_buf()),
                                description: format!(
                                    "Plan is {} bytes (exceeds {} limit)",
                                    inputs.plan.final_bytes, MAX_INLINE_CONTENT_SIZE
                                ),
                            }
                        }
                    };

                    let refs = PromptContentReferences {
                        prompt: Some(prompt_ref),
                        plan: Some(plan_ref),
                        diff: None,
                    };

                    let (base_prompt, should_validate) = match ctx
                        .workspace
                        .read(Path::new(".agent/tmp/development_prompt.txt"))
                    {
                        Ok(previous_prompt) => (
                            super::super::retry_guidance::strip_existing_same_agent_retry_preamble(
                                &previous_prompt,
                            )
                            .to_string(),
                            false,
                        ),
                        Err(_) => (
                            prompt_developer_iteration_xml_with_references(
                                ctx.template_context,
                                &refs,
                                ctx.workspace,
                            ),
                            true,
                        ),
                    };
                    let prompt = format!("{retry_preamble}\n{base_prompt}");
                    let prompt_key = format!(
                        "development_{}_same_agent_retry_{}",
                        iteration, continuation_state.same_agent_retry_count
                    );
                    let rendered_log = if should_validate {
                        let rendered =
                            crate::prompts::prompt_developer_iteration_xml_with_references_and_log(
                                ctx.template_context,
                                &refs,
                                ctx.workspace,
                                "developer_iteration_xml",
                            );
                        if !rendered.log.is_complete() {
                            let missing = rendered.log.unsubstituted.clone();
                            let result = EffectResult::event(PipelineEvent::template_rendered(
                                crate::reducer::event::PipelinePhase::Development,
                                "developer_iteration_xml".to_string(),
                                rendered.log,
                            ))
                            .with_additional_event(
                                PipelineEvent::agent_template_variables_invalid(
                                    AgentRole::Developer,
                                    "developer_iteration_xml".to_string(),
                                    missing,
                                    Vec::new(),
                                ),
                            );
                            return Ok(result);
                        }
                        Some(rendered.log)
                    } else {
                        None
                    };
                    (
                        prompt,
                        "developer_iteration_xml",
                        Some(prompt_key),
                        false,
                        should_validate,
                        rendered_log,
                    )
                }
                PromptMode::Normal => {
                    let inputs = self
                        .state
                        .prompt_inputs
                        .development
                        .as_ref()
                        .filter(|p| p.iteration == iteration)
                        .ok_or(ErrorEvent::DevelopmentInputsNotMaterialized { iteration })?;

                    let prompt_md = match &inputs.prompt.representation {
                        PromptInputRepresentation::Inline => {
                            let prompt_md =
                                ctx.workspace.read(Path::new("PROMPT.md")).map_err(|err| {
                                    ErrorEvent::WorkspaceReadFailed {
                                        path: "PROMPT.md".to_string(),
                                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                                    }
                                })?;
                            Some(prompt_md)
                        }
                        PromptInputRepresentation::FileReference { .. } => None,
                    };
                    let plan_md = match &inputs.plan.representation {
                        PromptInputRepresentation::Inline => {
                            let plan_md =
                                ctx.workspace
                                    .read(Path::new(".agent/PLAN.md"))
                                    .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                                        path: ".agent/PLAN.md".to_string(),
                                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                                    })?;
                            Some(plan_md)
                        }
                        PromptInputRepresentation::FileReference { .. } => None,
                    };

                    let prompt_key = format!("development_{iteration}");
                    let prompt_ref = match &inputs.prompt.representation {
                        PromptInputRepresentation::Inline => {
                            let prompt_md =
                                prompt_md.ok_or(ErrorEvent::DevelopmentInputsNotMaterialized {
                                    iteration,
                                })?;
                            PromptContentReference::inline(prompt_md)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            PromptContentReference::file_path(
                                path.clone(),
                                "Original user requirements from PROMPT.md",
                            )
                        }
                    };
                    let plan_ref = match &inputs.plan.representation {
                        PromptInputRepresentation::Inline => {
                            let plan_md =
                                plan_md.ok_or(ErrorEvent::DevelopmentInputsNotMaterialized {
                                    iteration,
                                })?;
                            PlanContentReference::Inline(plan_md)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            PlanContentReference::ReadFromFile {
                                primary_path: path.clone(),
                                fallback_path: Some(Path::new(".agent/tmp/plan.xml").to_path_buf()),
                                description: format!(
                                    "Plan is {} bytes (exceeds {} limit)",
                                    inputs.plan.final_bytes, MAX_INLINE_CONTENT_SIZE
                                ),
                            }
                        }
                    };
                    let (prompt, was_replayed) = get_stored_or_generate_prompt(
                        &prompt_key,
                        &ctx.prompt_history,
                        || {
                            let prompt_ref = prompt_ref.clone();
                            let plan_ref = plan_ref.clone();
                            let refs = PromptContentReferences {
                                prompt: Some(prompt_ref),
                                plan: Some(plan_ref),
                                diff: None,
                            };
                            // Use log-based rendering
                            let rendered = crate::prompts::prompt_developer_iteration_xml_with_references_and_log(
                                ctx.template_context,
                                &refs,
                                ctx.workspace,
                                "developer_iteration_xml",
                            );
                            rendered.content
                        },
                    );

                    // Validate freshly generated prompts (not replayed ones)
                    let rendered_log = if was_replayed {
                        None
                    } else {
                        let refs = PromptContentReferences {
                            prompt: Some(prompt_ref),
                            plan: Some(plan_ref),
                            diff: None,
                        };
                        let rendered =
                            crate::prompts::prompt_developer_iteration_xml_with_references_and_log(
                                ctx.template_context,
                                &refs,
                                ctx.workspace,
                                "developer_iteration_xml",
                            );

                        if !rendered.log.is_complete() {
                            let missing = rendered.log.unsubstituted.clone();
                            let result = EffectResult::event(PipelineEvent::template_rendered(
                                crate::reducer::event::PipelinePhase::Development,
                                "developer_iteration_xml".to_string(),
                                rendered.log,
                            ))
                            .with_additional_event(
                                PipelineEvent::agent_template_variables_invalid(
                                    AgentRole::Developer,
                                    "developer_iteration_xml".to_string(),
                                    missing,
                                    Vec::new(),
                                ),
                            );
                            return Ok(result);
                        }
                        Some(rendered.log)
                    };

                    (
                        prompt,
                        "developer_iteration_xml",
                        Some(prompt_key),
                        was_replayed,
                        true,
                        rendered_log,
                    )
                }
            };

        if let Some(prompt_key) = prompt_key {
            if !was_replayed {
                ctx.capture_prompt(&prompt_key, &dev_prompt);
            }
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
            .write(Path::new(".agent/tmp/development_prompt.txt"), &dev_prompt)
        {
            ctx.logger.warn(&format!(
                "Failed to write development prompt file: {err}. Pipeline will continue (loop recovery will handle convergence)."
            ));
        }

        // Build events: DevelopmentPromptPrepared is primary, with additional_events and TemplateRendered as additional
        let mut result = EffectResult::event(PipelineEvent::development_prompt_prepared(iteration));

        // Add any additional events from XSD retry materialization, etc.
        for ev in additional_events {
            result = result.with_additional_event(ev);
        }

        // Add TemplateRendered if we have a log
        if let Some(log) = rendered_log {
            result = result.with_additional_event(PipelineEvent::template_rendered(
                crate::reducer::event::PipelinePhase::Development,
                template_name.to_string(),
                log,
            ));
        }

        Ok(result)
    }
}
