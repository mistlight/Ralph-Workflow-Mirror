use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::phases::PhaseContext;
use crate::prompts::content_builder::PromptContentReferences;
use crate::prompts::content_reference::{
    PlanContentReference, PromptContentReference, MAX_INLINE_CONTENT_SIZE,
};
use crate::reducer::effect::ContinuationContextData;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentEvent, PipelineEvent};
use crate::reducer::prompt_inputs::sha256_hex_str;
use crate::reducer::state::PromptMode;
use crate::reducer::state::{
    MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason,
};
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use crate::workspace::Workspace;
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
    pub(super) fn materialize_development_inputs(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let prompt_md = match ctx.workspace.read(Path::new("PROMPT.md")) {
            Ok(prompt_md) => prompt_md,
            Err(err) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    format!("Failed to read required PROMPT.md: {err}"),
                )));
            }
        };
        let plan_md = match ctx.workspace.read(Path::new(".agent/PLAN.md")) {
            Ok(plan_md) => plan_md,
            Err(err) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    format!("Failed to read required .agent/PLAN.md: {err}"),
                )));
            }
        };

        let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
        let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();

        let prompt_backup_path = Path::new(".agent/PROMPT.md.backup");
        let (prompt_representation, prompt_reason) = if prompt_md.len() as u64 > inline_budget_bytes
        {
            match crate::files::create_prompt_backup_with_workspace(ctx.workspace) {
                Ok(Some(warning)) => {
                    ctx.logger
                        .warn(&format!("PROMPT backup created with warning: {warning}"));
                }
                Ok(None) => {}
                Err(err) => {
                    return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                        format!("Failed to create PROMPT backup: {err}"),
                    )));
                }
            }
            ctx.logger.warn(&format!(
                "PROMPT size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                (prompt_md.len() as u64) / 1024,
                inline_budget_bytes / 1024,
                ctx.workspace.absolute(prompt_backup_path).display()
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
                ctx.workspace.absolute(plan_path).display()
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

    pub(super) fn prepare_development_context(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let _ = crate::files::create_prompt_backup_with_workspace(ctx.workspace);
        Ok(EffectResult::event(
            PipelineEvent::development_context_prepared(iteration),
        ))
    }

    pub(super) fn prepare_development_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_developer_iteration_continuation_xml,
            prompt_developer_iteration_xml_with_references,
            prompt_developer_iteration_xsd_retry_with_context,
        };

        let continuation_state = &self.state.continuation;
        let mut ignore_sources_owned: Vec<String> = Vec::new();

        let (dev_prompt, template_name, prompt_key, was_replayed) = match prompt_mode {
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
                        )
                    });
                (
                    prompt,
                    "developer_iteration_continuation_xml",
                    Some(prompt_key),
                    was_replayed,
                )
            }
            PromptMode::XsdRetry => {
                let last_output = match ctx
                    .workspace
                    .read(Path::new(xml_paths::DEVELOPMENT_RESULT_XML))
                {
                    Ok(output) => output,
                    Err(err) => {
                        return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                            format!(
                                "Failed to read last development output at {}: {err}",
                                xml_paths::DEVELOPMENT_RESULT_XML
                            ),
                        )));
                    }
                };
                ignore_sources_owned.push(last_output.clone());
                (
                    prompt_developer_iteration_xsd_retry_with_context(
                        ctx.template_context,
                        "", // kept for API compatibility; template reads context files instead
                        "",
                        "XML output failed validation. Provide valid XML output.",
                        &last_output,
                        ctx.workspace,
                    ),
                    "developer_iteration_xsd_retry",
                    None,
                    false,
                )
            }
            PromptMode::Normal => {
                let inputs = self
                    .state
                    .prompt_inputs
                    .development
                    .as_ref()
                    .filter(|p| p.iteration == iteration)
                    .expect("development inputs must be materialized before preparing prompt");

                let prompt_md = match &inputs.prompt.representation {
                    PromptInputRepresentation::Inline => {
                        let prompt_md = match ctx.workspace.read(Path::new("PROMPT.md")) {
                            Ok(prompt_md) => prompt_md,
                            Err(err) => {
                                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                                    format!("Failed to read required PROMPT.md: {err}"),
                                )));
                            }
                        };
                        ignore_sources_owned.push(prompt_md.clone());
                        Some(prompt_md)
                    }
                    PromptInputRepresentation::FileReference { .. } => None,
                };
                let plan_md = match &inputs.plan.representation {
                    PromptInputRepresentation::Inline => {
                        let plan_md = match ctx.workspace.read(Path::new(".agent/PLAN.md")) {
                            Ok(plan_md) => plan_md,
                            Err(err) => {
                                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                                    format!("Failed to read required .agent/PLAN.md: {err}"),
                                )));
                            }
                        };
                        ignore_sources_owned.push(plan_md.clone());
                        Some(plan_md)
                    }
                    PromptInputRepresentation::FileReference { .. } => None,
                };

                let prompt_key = format!("development_{}", iteration);
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        let prompt_ref = match &inputs.prompt.representation {
                            PromptInputRepresentation::Inline => PromptContentReference::inline(
                                prompt_md
                                    .clone()
                                    .expect("prompt_md must be loaded for inline"),
                            ),
                            PromptInputRepresentation::FileReference { path } => {
                                PromptContentReference::file_path(
                                    ctx.workspace.absolute(path),
                                    "Original user requirements from PROMPT.md",
                                )
                            }
                        };
                        let plan_ref = match &inputs.plan.representation {
                            PromptInputRepresentation::Inline => PlanContentReference::Inline(
                                plan_md.clone().expect("plan_md must be loaded for inline"),
                            ),
                            PromptInputRepresentation::FileReference { path } => {
                                PlanContentReference::ReadFromFile {
                                    primary_path: ctx.workspace.absolute(path),
                                    fallback_path: Some(
                                        ctx.workspace.absolute(Path::new(".agent/tmp/plan.xml")),
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
                        prompt_developer_iteration_xml_with_references(ctx.template_context, &refs)
                    });
                (
                    prompt,
                    "developer_iteration_xml",
                    Some(prompt_key),
                    was_replayed,
                )
            }
        };
        let ignore_sources: Vec<&str> = ignore_sources_owned.iter().map(|s| s.as_str()).collect();
        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
            &dev_prompt,
            &ignore_sources,
        ) {
            return Ok(EffectResult::event(
                PipelineEvent::agent_template_variables_invalid(
                    AgentRole::Developer,
                    template_name.to_string(),
                    Vec::new(),
                    err.unresolved_placeholders,
                ),
            ));
        }

        if let Some(prompt_key) = prompt_key {
            if !was_replayed {
                ctx.capture_prompt(&prompt_key, &dev_prompt);
            }
        }

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir)?;
        }

        ctx.workspace
            .write(Path::new(".agent/tmp/development_prompt.txt"), &dev_prompt)?;

        Ok(EffectResult::event(
            PipelineEvent::development_prompt_prepared(iteration),
        ))
    }

    pub(super) fn invoke_development_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let prompt = match ctx
            .workspace
            .read(Path::new(".agent/tmp/development_prompt.txt"))
        {
            Ok(prompt) => prompt,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing development prompt at .agent/tmp/development_prompt.txt".to_string(),
                )));
            }
        };

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.developer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Developer, agent, None, prompt)?;
        if matches!(
            result.event,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        ) {
            result =
                result.with_additional_event(PipelineEvent::development_agent_invoked(iteration));
        }
        Ok(result)
    }

    pub(super) fn cleanup_development_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let result_xml = Path::new(xml_paths::DEVELOPMENT_RESULT_XML);
        let _ = ctx.workspace.remove_if_exists(result_xml);
        Ok(EffectResult::event(PipelineEvent::development_xml_cleaned(
            iteration,
        )))
    }

    pub(super) fn extract_development_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let xml_path = Path::new(xml_paths::DEVELOPMENT_RESULT_XML);
        let mut ui_events = vec![UIEvent::IterationProgress {
            current: iteration,
            total: self.state.total_iterations,
        }];

        match ctx.workspace.read(xml_path) {
            Ok(content) => {
                ui_events.push(UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentResult,
                    content,
                    context: Some(XmlOutputContext {
                        iteration: Some(iteration),
                        pass: None,
                        snippets: Vec::new(),
                    }),
                });
                Ok(EffectResult::with_ui(
                    PipelineEvent::development_xml_extracted(iteration),
                    ui_events,
                ))
            }
            Err(_) => Ok(EffectResult::with_ui(
                PipelineEvent::development_xml_missing(
                    iteration,
                    self.state.continuation.invalid_output_attempts,
                ),
                ui_events,
            )),
        }
    }

    pub(super) fn validate_development_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::validate_development_result_xml;

        let xml = match ctx
            .workspace
            .read(Path::new(xml_paths::DEVELOPMENT_RESULT_XML))
        {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::development_output_validation_failed(
                        iteration,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        match validate_development_result_xml(&xml) {
            Ok(elements) => {
                let status = if elements.is_completed() {
                    crate::reducer::state::DevelopmentStatus::Completed
                } else if elements.is_partial() {
                    crate::reducer::state::DevelopmentStatus::Partial
                } else {
                    crate::reducer::state::DevelopmentStatus::Failed
                };

                let files_changed = elements
                    .files_changed
                    .as_ref()
                    .map(|f| f.lines().map(|s| s.to_string()).collect());

                Ok(EffectResult::event(
                    PipelineEvent::development_xml_validated(
                        iteration,
                        status,
                        elements.summary.clone(),
                        files_changed,
                        elements.next_steps.clone(),
                    ),
                ))
            }
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::development_output_validation_failed(
                    iteration,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn archive_development_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::archive_xml_file_with_workspace;

        archive_xml_file_with_workspace(
            ctx.workspace,
            Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
        );
        Ok(EffectResult::event(
            PipelineEvent::development_xml_archived(iteration),
        ))
    }

    pub(super) fn apply_development_outcome(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let outcome = match self.state.development_validated_outcome.as_ref() {
            Some(outcome) if outcome.iteration == iteration => outcome,
            _ => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing validated development outcome".to_string(),
                )));
            }
        };

        let _ = outcome;
        Ok(EffectResult::event(
            PipelineEvent::development_outcome_applied(iteration),
        ))
    }
}

pub(super) fn write_continuation_context_to_workspace(
    workspace: &dyn Workspace,
    logger: &crate::logger::Logger,
    data: &ContinuationContextData,
) -> Result<()> {
    let tmp_dir = Path::new(".agent/tmp");
    if !workspace.exists(tmp_dir) {
        workspace.create_dir_all(tmp_dir)?;
    }

    let mut content = String::new();
    content.push_str("# Development Continuation Context\n\n");
    content.push_str(&format!("- Iteration: {}\n", data.iteration));
    content.push_str(&format!("- Continuation attempt: {}\n", data.attempt));
    content.push_str(&format!("- Previous status: {}\n\n", data.status));

    content.push_str("## Previous summary\n\n");
    content.push_str(&data.summary);
    content.push('\n');

    if let Some(ref files) = data.files_changed {
        content.push_str("\n## Files changed\n\n");
        for file in files {
            content.push_str("- ");
            content.push_str(file);
            content.push('\n');
        }
    }

    if let Some(ref steps) = data.next_steps {
        content.push_str("\n## Recommended next steps\n\n");
        content.push_str(steps);
        content.push('\n');
    }

    content.push_str("\n## Reference files (do not modify)\n\n");
    content.push_str("- PROMPT.md\n");
    content.push_str("- .agent/PLAN.md\n");

    workspace.write(Path::new(".agent/tmp/continuation_context.md"), &content)?;

    logger.info("Continuation context written to .agent/tmp/continuation_context.md");

    Ok(())
}
