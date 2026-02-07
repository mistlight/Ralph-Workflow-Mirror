use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::files::llm_output_extraction::{archive_xml_file_with_workspace, validate_plan_xml};
use crate::phases::development::format_plan_as_markdown;
use crate::phases::PhaseContext;
use crate::prompts::content_reference::{PromptContentReference, MAX_INLINE_CONTENT_SIZE};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_planning_xml_with_references,
    prompt_planning_xsd_retry_with_context_files,
};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{
    AgentEvent, ErrorEvent, PipelineEvent, PipelinePhase, WorkspaceIoErrorKind,
};
use crate::reducer::prompt_inputs::sha256_hex_str;
use crate::reducer::state::PromptMode;
use crate::reducer::state::{
    MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason,
};
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use anyhow::Result;
use std::path::Path;

const PLANNING_PROMPT_PATH: &str = ".agent/tmp/planning_prompt.txt";

impl MainEffectHandler {
    pub(super) fn materialize_planning_inputs(
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

        let content_id_sha256 = sha256_hex_str(&prompt_md);
        let original_bytes = prompt_md.len() as u64;
        let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
        let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();

        let prompt_backup_path = Path::new(".agent/PROMPT.md.backup");
        let (representation, reason) = if original_bytes > inline_budget_bytes {
            crate::files::create_prompt_backup_with_workspace(ctx.workspace).map_err(|err| {
                ErrorEvent::WorkspaceWriteFailed {
                    path: prompt_backup_path.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
            })?;
            ctx.logger.warn(&format!(
                "PROMPT size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                original_bytes / 1024,
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

        let input = MaterializedPromptInput {
            kind: PromptInputKind::Prompt,
            content_id_sha256: content_id_sha256.clone(),
            consumer_signature_sha256: consumer_signature_sha256.clone(),
            original_bytes,
            final_bytes: original_bytes,
            model_budget_bytes: None,
            inline_budget_bytes: Some(inline_budget_bytes),
            representation,
            reason,
        };

        let mut result = EffectResult::event(PipelineEvent::planning_inputs_materialized(
            iteration, input,
        ));
        if original_bytes > inline_budget_bytes {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Oversize PROMPT: {} KB > {} KB; using file reference",
                    original_bytes / 1024,
                    inline_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                PipelinePhase::Planning,
                PromptInputKind::Prompt,
                content_id_sha256,
                original_bytes,
                inline_budget_bytes,
                "inline-embedding".to_string(),
            ));
        }
        Ok(result)
    }

    pub(super) fn prepare_planning_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        let mut additional_events: Vec<PipelineEvent> = Vec::new();
        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                ErrorEvent::WorkspaceCreateDirAllFailed {
                    path: tmp_dir.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
            })?;
        }

        let mut ignore_sources_owned: Vec<String> = Vec::new();
        let continuation_state = &self.state.continuation;

        let (prompt, template_name, prompt_key, was_replayed, should_validate) = match prompt_mode {
            PromptMode::XsdRetry => {
                // Materialize last invalid output to a stable path so the retry prompt can
                // reference it without inlining content into the prompt itself.
                let last_output = ctx
                    .workspace
                    .read(Path::new(xml_paths::PLAN_XML))
                    .or_else(|err| {
                        if err.kind() == std::io::ErrorKind::NotFound {
                            // Try reading from the archived .processed file as a fallback
                            let processed_path = Path::new(".agent/tmp/plan.xml.processed");
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
                        path: xml_paths::PLAN_XML.to_string(),
                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                    })?;

                let content_id_sha256 = sha256_hex_str(&last_output);
                let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();
                let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
                let last_output_bytes = last_output.len() as u64;

                let already_materialized = self
                    .state
                    .prompt_inputs
                    .xsd_retry_last_output
                    .as_ref()
                    .is_some_and(|m| {
                        m.phase == PipelinePhase::Planning
                            && m.scope_id == iteration
                            && m.last_output.content_id_sha256 == content_id_sha256
                            && m.last_output.consumer_signature_sha256 == consumer_signature_sha256
                    });

                if !already_materialized {
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
                        PipelinePhase::Planning,
                        iteration,
                        input,
                    ));
                    if last_output_bytes > inline_budget_bytes {
                        additional_events.push(PipelineEvent::prompt_input_oversize_detected(
                            PipelinePhase::Planning,
                            PromptInputKind::LastOutput,
                            content_id_sha256,
                            last_output_bytes,
                            inline_budget_bytes,
                            "xsd-retry-context".to_string(),
                        ));
                    }
                }
                (
                    prompt_planning_xsd_retry_with_context_files(
                        ctx.template_context,
                        "Previous XML output failed XSD validation. Please provide valid XML conforming to the schema.",
                        ctx.workspace,
                    ),
                    "planning_xsd_retry",
                    None,
                    false,
                    true,
                )
            }
            PromptMode::SameAgentRetry => {
                // Same-agent retry: prepend retry guidance to the last prepared prompt for this
                // phase (preserves XSD retry / continuation context if present).
                let retry_preamble =
                    super::retry_guidance::same_agent_retry_preamble(continuation_state);
                let (base_prompt, should_validate) = match ctx
                    .workspace
                    .read(Path::new(PLANNING_PROMPT_PATH))
                {
                    Ok(previous_prompt) => (
                        super::retry_guidance::strip_existing_same_agent_retry_preamble(
                            &previous_prompt,
                        )
                        .to_string(),
                        false,
                    ),
                    Err(_) => {
                        let inputs = self
                            .state
                            .prompt_inputs
                            .planning
                            .as_ref()
                            .filter(|p| p.iteration == iteration)
                            .ok_or(ErrorEvent::PlanningInputsNotMaterialized { iteration })?;

                        let prompt_ref = match &inputs.prompt.representation {
                            PromptInputRepresentation::Inline => {
                                let prompt_md = ctx
                                    .workspace
                                    .read(Path::new("PROMPT.md"))
                                    .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                                        path: "PROMPT.md".to_string(),
                                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
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
                        (
                            prompt_planning_xml_with_references(
                                ctx.template_context,
                                &prompt_ref,
                                ctx.workspace,
                            ),
                            true,
                        )
                    }
                };
                let prompt = format!("{retry_preamble}\n{base_prompt}");
                let prompt_key = format!(
                    "planning_{iteration}_same_agent_retry_{}",
                    continuation_state.same_agent_retry_count
                );
                // If we reused a previously prepared prompt, it was already validated at the time
                // it was prepared. Re-validating can introduce false positives (e.g., XSD retry
                // prompts include last output, which may contain literal placeholders).
                (
                    prompt,
                    "planning_xml",
                    Some(prompt_key),
                    false,
                    should_validate,
                )
            }
            PromptMode::Normal => {
                let inputs = self
                    .state
                    .prompt_inputs
                    .planning
                    .as_ref()
                    .filter(|p| p.iteration == iteration)
                    .ok_or(ErrorEvent::PlanningInputsNotMaterialized { iteration })?;

                let prompt_ref = match &inputs.prompt.representation {
                    PromptInputRepresentation::Inline => {
                        let prompt_md =
                            ctx.workspace.read(Path::new("PROMPT.md")).map_err(|err| {
                                ErrorEvent::WorkspaceReadFailed {
                                    path: "PROMPT.md".to_string(),
                                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                                }
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

                let prompt_key = format!("planning_{iteration}");
                let prompt_ref_for_template = prompt_ref.clone();
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_planning_xml_with_references(
                            ctx.template_context,
                            &prompt_ref_for_template,
                            ctx.workspace,
                        )
                    });
                (prompt, "planning_xml", Some(prompt_key), was_replayed, true)
            }
            PromptMode::Continuation => {
                return Err(ErrorEvent::PlanningContinuationNotSupported.into());
            }
        };
        let ignore_sources: Vec<&str> = ignore_sources_owned.iter().map(|s| s.as_str()).collect();
        if should_validate {
            if let Err(err) =
                crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
                    &prompt,
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
                ctx.capture_prompt(&prompt_key, &prompt);
            }
        }

        // Write prompt file (non-fatal: if write fails, log warning and continue)
        // Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
        // If the prompt file write fails, we continue with orchestration - loop recovery will
        // handle convergence if needed.
        if let Err(err) = ctx
            .workspace
            .write(Path::new(PLANNING_PROMPT_PATH), &prompt)
        {
            ctx.logger.warn(&format!(
                "Failed to write planning prompt file: {}. Pipeline will continue (loop recovery will handle convergence).",
                err
            ));
        }

        let mut result = EffectResult::event(PipelineEvent::planning_prompt_prepared(iteration));
        for ev in additional_events {
            result = result.with_additional_event(ev);
        }
        Ok(result)
    }

    pub(super) fn invoke_planning_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        // Normalize agent chain state before invocation for determinism
        self.normalize_agent_chain_for_invocation(ctx, AgentRole::Developer);

        let prompt = match ctx.workspace.read(Path::new(PLANNING_PROMPT_PATH)) {
            Ok(s) => s,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ErrorEvent::PlanningPromptMissing { iteration }.into());
            }
            Err(err) => {
                return Err(ErrorEvent::WorkspaceReadFailed {
                    path: PLANNING_PROMPT_PATH.to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
                .into());
            }
        };

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.developer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Developer, agent, None, prompt)?;
        if result.additional_events.iter().any(|e| {
            matches!(
                e,
                PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
            )
        }) {
            result = result.with_additional_event(PipelineEvent::planning_agent_invoked(iteration));
        }
        Ok(result)
    }

    pub(super) fn cleanup_planning_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let plan_xml = Path::new(xml_paths::PLAN_XML);
        let _ = ctx.workspace.remove_if_exists(plan_xml);
        Ok(EffectResult::event(PipelineEvent::planning_xml_cleaned(
            iteration,
        )))
    }

    pub(super) fn extract_planning_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let plan_xml = Path::new(xml_paths::PLAN_XML);
        let content = ctx.workspace.read(plan_xml);

        match content {
            Ok(_) => Ok(EffectResult::event(PipelineEvent::planning_xml_extracted(
                iteration,
            ))),
            Err(_) => Ok(EffectResult::event(PipelineEvent::planning_xml_missing(
                iteration,
                self.state.continuation.invalid_output_attempts,
            ))),
        }
    }

    pub(super) fn validate_planning_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let plan_xml = match ctx.workspace.read(Path::new(xml_paths::PLAN_XML)) {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::planning_output_validation_failed(
                        iteration,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        match validate_plan_xml(&plan_xml) {
            Ok(elements) => {
                let markdown = format_plan_as_markdown(&elements);
                Ok(EffectResult::with_ui(
                    PipelineEvent::planning_xml_validated(iteration, true, Some(markdown)),
                    vec![UIEvent::XmlOutput {
                        xml_type: XmlOutputType::DevelopmentPlan,
                        content: plan_xml,
                        context: Some(XmlOutputContext {
                            iteration: Some(iteration),
                            pass: None,
                            snippets: Vec::new(),
                        }),
                    }],
                ))
            }
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::planning_output_validation_failed(
                    iteration,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn write_planning_markdown(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let markdown = self
            .state
            .planning_validated_outcome
            .as_ref()
            .filter(|outcome| outcome.iteration == iteration)
            .and_then(|outcome| outcome.markdown.clone())
            .ok_or(ErrorEvent::ValidatedPlanningMarkdownMissing { iteration })?;

        ctx.workspace
            .write(Path::new(".agent/PLAN.md"), &markdown)
            .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                path: ".agent/PLAN.md".to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;

        Ok(EffectResult::event(
            PipelineEvent::planning_markdown_written(iteration),
        ))
    }

    pub(super) fn archive_planning_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::PLAN_XML));
        Ok(EffectResult::event(PipelineEvent::planning_xml_archived(
            iteration,
        )))
    }

    pub(super) fn apply_planning_outcome(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        iteration: u32,
        valid: bool,
    ) -> Result<EffectResult> {
        let mut ui_events = Vec::new();
        if valid {
            ui_events.push(self.phase_transition_ui(PipelinePhase::Development));
        }
        Ok(EffectResult::with_ui(
            PipelineEvent::plan_generation_completed(iteration, valid),
            ui_events,
        ))
    }
}
