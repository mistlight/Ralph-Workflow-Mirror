//! Development prompt preparation and input materialization.
//!
//! This module handles:
//! - Reading PROMPT.md and PLAN.md from workspace
//! - Deciding whether to inline or reference based on size budgets
//! - Generating prompts based on mode (Normal, XSD Retry, Same-Agent Retry, Continuation)
//! - Template variable validation
//! - Prompt replay from history

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
use crate::reducer::ui_event::UIEvent;
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
    /// Materialize development inputs (PROMPT.md and PLAN.md).
    ///
    /// Reads PROMPT.md and PLAN.md from the workspace, determines whether to inline
    /// or reference each based on the 16KB inline budget, and emits a
    /// DevelopmentInputsMaterialized event.
    ///
    /// If either file exceeds the inline budget, a backup file is created and referenced
    /// by path instead of embedding the content. This prevents token budget exhaustion
    /// while preserving full context access for the agent.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Phase context with workspace access
    /// * `iteration` - Current development iteration number
    ///
    /// # Returns
    ///
    /// EffectResult with DevelopmentInputsMaterialized event, plus optional oversize
    /// detection events if either input exceeds the inline budget.
    pub(in crate::reducer::handler) fn materialize_development_inputs(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let prompt_md = ctx.workspace.read(Path::new("PROMPT.md")).map_err(|err| {
            ErrorEvent::WorkspaceReadFailed {
                path: "PROMPT.md".to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            }
        })?;

        let plan_md = ctx
            .workspace
            .read(Path::new(".agent/PLAN.md"))
            .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                path: ".agent/PLAN.md".to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;

        let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
        let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();

        let prompt_backup_path = Path::new(".agent/PROMPT.md.backup");
        let (prompt_representation, prompt_reason) = if prompt_md.len() as u64 > inline_budget_bytes
        {
            crate::files::create_prompt_backup_with_workspace(ctx.workspace).map_err(|err| {
                ErrorEvent::WorkspaceWriteFailed {
                    path: prompt_backup_path.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
            })?;
            ctx.logger.warn(&format!(
                "PROMPT size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                (prompt_md.len() as u64) / 1024,
                inline_budget_bytes / 1024,
                prompt_backup_path.display()
            ));
            (
                PromptInputRepresentation::FileReference {
                    path: prompt_backup_path.to_path_buf(),
                },
                PromptMaterializationReason::InlineBudgetExceeded,
            )
        } else {
            (
                PromptInputRepresentation::Inline,
                PromptMaterializationReason::WithinBudgets,
            )
        };

        let plan_path = Path::new(".agent/PLAN.md");
        let (plan_representation, plan_reason) = if plan_md.len() as u64 > inline_budget_bytes {
            ctx.logger.warn(&format!(
                "PLAN size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                (plan_md.len() as u64) / 1024,
                inline_budget_bytes / 1024,
                plan_path.display()
            ));
            (
                PromptInputRepresentation::FileReference {
                    path: plan_path.to_path_buf(),
                },
                PromptMaterializationReason::InlineBudgetExceeded,
            )
        } else {
            (
                PromptInputRepresentation::Inline,
                PromptMaterializationReason::WithinBudgets,
            )
        };

        let prompt_input = MaterializedPromptInput {
            kind: PromptInputKind::Prompt,
            content_id_sha256: sha256_hex_str(&prompt_md),
            consumer_signature_sha256: consumer_signature_sha256.clone(),
            original_bytes: prompt_md.len() as u64,
            final_bytes: prompt_md.len() as u64,
            model_budget_bytes: None,
            inline_budget_bytes: Some(inline_budget_bytes),
            representation: prompt_representation,
            reason: prompt_reason,
        };
        let plan_input = MaterializedPromptInput {
            kind: PromptInputKind::Plan,
            content_id_sha256: sha256_hex_str(&plan_md),
            consumer_signature_sha256: consumer_signature_sha256.clone(),
            original_bytes: plan_md.len() as u64,
            final_bytes: plan_md.len() as u64,
            model_budget_bytes: None,
            inline_budget_bytes: Some(inline_budget_bytes),
            representation: plan_representation,
            reason: plan_reason,
        };

        let mut result = EffectResult::event(PipelineEvent::development_inputs_materialized(
            iteration,
            prompt_input.clone(),
            plan_input.clone(),
        ));

        if prompt_input.original_bytes > inline_budget_bytes {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Oversize PROMPT: {} KB > {} KB; using file reference",
                    prompt_input.original_bytes / 1024,
                    inline_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                crate::reducer::event::PipelinePhase::Development,
                PromptInputKind::Prompt,
                prompt_input.content_id_sha256.clone(),
                prompt_input.original_bytes,
                inline_budget_bytes,
                "inline-embedding".to_string(),
            ));
        }
        if plan_input.original_bytes > inline_budget_bytes {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Oversize PLAN: {} KB > {} KB; using file reference",
                    plan_input.original_bytes / 1024,
                    inline_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                crate::reducer::event::PipelinePhase::Development,
                PromptInputKind::Plan,
                plan_input.content_id_sha256.clone(),
                plan_input.original_bytes,
                inline_budget_bytes,
                "inline-embedding".to_string(),
            ));
        }

        Ok(result)
    }

    /// Prepare development prompt based on prompt mode.
    ///
    /// Generates the appropriate prompt for the developer agent based on the current mode:
    ///
    /// - **Normal** - First attempt for iteration, uses developer_iteration_xml template
    /// - **XSD Retry** - Invalid XML output, includes last_output.xml and XSD error context
    /// - **Same-Agent Retry** - Agent failed (non-XML issues), prepends retry preamble
    /// - **Continuation** - Partial progress, includes continuation context from previous attempt
    ///
    /// The prompt is validated for unresolved template variables (except for explicitly ignored
    /// inline content) and written to `.agent/tmp/development_prompt.txt` for debugging and
    /// same-agent retry fallback.
    ///
    /// # Prompt Replay
    ///
    /// Normal and Continuation mode prompts are replayed from history if available (same prompt_key).
    /// This ensures deterministic prompt generation across resume operations.
    ///
    /// # Template Variables
    ///
    /// If template variable validation fails, an AgentTemplateVariablesInvalid event is emitted
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
    /// EffectResult with DevelopmentPromptPrepared event, plus optional
    /// XsdRetryLastOutputMaterialized and PromptInputOversizeDetected events for XSD retry mode.
    pub(in crate::reducer::handler) fn prepare_development_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_developer_iteration_continuation_xml,
            prompt_developer_iteration_xml_with_references,
            prompt_developer_iteration_xsd_retry_with_context_files,
        };

        let continuation_state = &self.state.continuation;
        let mut ignore_sources_owned: Vec<String> = Vec::new();
        let mut additional_events: Vec<PipelineEvent> = Vec::new();

        let (dev_prompt, template_name, prompt_key, was_replayed, should_validate) =
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
                    (
                        prompt,
                        "developer_iteration_continuation_xml",
                        Some(prompt_key),
                        was_replayed,
                        true,
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
                    (
                        prompt_developer_iteration_xsd_retry_with_context_files(
                            ctx.template_context,
                            "XML output failed validation. Provide valid XML output.",
                            ctx.workspace,
                        ),
                        "developer_iteration_xsd_retry",
                        None,
                        false,
                        true,
                    )
                }
                PromptMode::SameAgentRetry => {
                    // Same-agent retry: prepend retry guidance to the last prepared prompt for this
                    // phase (preserves XSD retry / continuation context if present).
                    let retry_preamble =
                        super::super::retry_guidance::same_agent_retry_preamble(continuation_state);
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
                        Err(_) => {
                            let inputs = self
                                .state
                                .prompt_inputs
                                .development
                                .as_ref()
                                .filter(|p| p.iteration == iteration)
                                .ok_or(ErrorEvent::DevelopmentInputsNotMaterialized {
                                    iteration,
                                })?;

                            let prompt_ref = match &inputs.prompt.representation {
                                PromptInputRepresentation::Inline => {
                                    let prompt_md = ctx
                                        .workspace
                                        .read(Path::new("PROMPT.md"))
                                        .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                                            path: "PROMPT.md".to_string(),
                                            kind: WorkspaceIoErrorKind::from_io_error_kind(
                                                err.kind(),
                                            ),
                                        })?;
                                    ignore_sources_owned.push(prompt_md.clone());
                                    PromptContentReference::inline(prompt_md)
                                }
                                PromptInputRepresentation::FileReference { path } => {
                                    PromptContentReference::file_path(
                                        path.to_path_buf(),
                                        "Original user requirements from PROMPT.md",
                                    )
                                }
                            };

                            let plan_ref = match &inputs.plan.representation {
                                PromptInputRepresentation::Inline => {
                                    let plan_md =
                                        ctx.workspace.read(Path::new(".agent/PLAN.md")).map_err(
                                            |err| ErrorEvent::WorkspaceReadFailed {
                                                path: ".agent/PLAN.md".to_string(),
                                                kind: WorkspaceIoErrorKind::from_io_error_kind(
                                                    err.kind(),
                                                ),
                                            },
                                        )?;
                                    ignore_sources_owned.push(plan_md.clone());
                                    PlanContentReference::Inline(plan_md)
                                }
                                PromptInputRepresentation::FileReference { path } => {
                                    PlanContentReference::ReadFromFile {
                                        primary_path: path.to_path_buf(),
                                        fallback_path: Some(
                                            Path::new(".agent/tmp/plan.xml").to_path_buf(),
                                        ),
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
                            (
                                prompt_developer_iteration_xml_with_references(
                                    ctx.template_context,
                                    &refs,
                                    ctx.workspace,
                                ),
                                true,
                            )
                        }
                    };
                    let prompt = format!("{retry_preamble}\n{base_prompt}");
                    let prompt_key = format!(
                        "development_{}_same_agent_retry_{}",
                        iteration, continuation_state.same_agent_retry_count
                    );
                    (
                        prompt,
                        "developer_iteration_xml",
                        Some(prompt_key),
                        false,
                        should_validate,
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
                            ignore_sources_owned.push(prompt_md.clone());
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
                            ignore_sources_owned.push(plan_md.clone());
                            Some(plan_md)
                        }
                        PromptInputRepresentation::FileReference { .. } => None,
                    };

                    let prompt_key = format!("development_{}", iteration);
                    let prompt_ref = match &inputs.prompt.representation {
                        PromptInputRepresentation::Inline => {
                            let prompt_md = prompt_md.clone().ok_or(
                                ErrorEvent::DevelopmentInputsNotMaterialized { iteration },
                            )?;
                            PromptContentReference::inline(prompt_md)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            PromptContentReference::file_path(
                                path.to_path_buf(),
                                "Original user requirements from PROMPT.md",
                            )
                        }
                    };
                    let plan_ref = match &inputs.plan.representation {
                        PromptInputRepresentation::Inline => {
                            let plan_md = plan_md.clone().ok_or(
                                ErrorEvent::DevelopmentInputsNotMaterialized { iteration },
                            )?;
                            PlanContentReference::Inline(plan_md)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            PlanContentReference::ReadFromFile {
                                primary_path: path.to_path_buf(),
                                fallback_path: Some(Path::new(".agent/tmp/plan.xml").to_path_buf()),
                                description: format!(
                                    "Plan is {} bytes (exceeds {} limit)",
                                    inputs.plan.final_bytes, MAX_INLINE_CONTENT_SIZE
                                ),
                            }
                        }
                    };
                    let (prompt, was_replayed) =
                        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                            let prompt_ref = prompt_ref.clone();
                            let plan_ref = plan_ref.clone();
                            let refs = PromptContentReferences {
                                prompt: Some(prompt_ref),
                                plan: Some(plan_ref),
                                diff: None,
                            };
                            prompt_developer_iteration_xml_with_references(
                                ctx.template_context,
                                &refs,
                                ctx.workspace,
                            )
                        });
                    (
                        prompt,
                        "developer_iteration_xml",
                        Some(prompt_key),
                        was_replayed,
                        true,
                    )
                }
            };
        let ignore_sources: Vec<&str> = ignore_sources_owned.iter().map(|s| s.as_str()).collect();
        if should_validate {
            if let Err(err) =
                crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
                    &dev_prompt,
                    &ignore_sources,
                )
            {
                return Ok(EffectResult::event(
                    PipelineEvent::agent_template_variables_invalid(
                        AgentRole::Developer,
                        template_name.to_string(),
                        Vec::new(),
                        err.unresolved_placeholders,
                    ),
                ));
            }
        }

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
                "Failed to write development prompt file: {}. Pipeline will continue (loop recovery will handle convergence).",
                err
            ));
        }

        let mut result = EffectResult::event(PipelineEvent::development_prompt_prepared(iteration));
        for ev in additional_events {
            result = result.with_additional_event(ev);
        }
        Ok(result)
    }
}
