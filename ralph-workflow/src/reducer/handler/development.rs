use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::phases::PhaseContext;
use crate::reducer::effect::ContinuationContextData;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::PipelineEvent;
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use crate::workspace::Workspace;
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
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
    ) -> Result<EffectResult> {
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_developer_iteration_continuation_xml,
            prompt_developer_iteration_xml_with_context,
            prompt_developer_iteration_xsd_retry_with_context,
        };

        let continuation_state = &self.state.continuation;
        let prompt_md = ctx
            .workspace
            .read(Path::new("PROMPT.md"))
            .unwrap_or_default();
        let plan_md = ctx
            .workspace
            .read(Path::new(".agent/PLAN.md"))
            .unwrap_or_default();

        let dev_prompt = if continuation_state.is_continuation() {
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

            if !was_replayed {
                ctx.capture_prompt(&prompt_key, &prompt);
            }

            prompt
        } else if continuation_state.invalid_output_attempts > 0 {
            prompt_developer_iteration_xsd_retry_with_context(
                ctx.template_context,
                &prompt_md,
                &plan_md,
                "XML output failed validation. Provide valid XML output.",
                "",
                ctx.workspace,
            )
        } else {
            let prompt_key = format!("development_{}", iteration);
            let (prompt, was_replayed) =
                get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                    prompt_developer_iteration_xml_with_context(
                        ctx.template_context,
                        &prompt_md,
                        &plan_md,
                    )
                });

            if !was_replayed {
                ctx.capture_prompt(&prompt_key, &prompt);
            }

            prompt
        };

        let template_name = if continuation_state.is_continuation() {
            "developer_iteration_continuation_xml"
        } else if continuation_state.invalid_output_attempts > 0 {
            "developer_iteration_xsd_retry"
        } else {
            "developer_iteration_xml"
        };
        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders(&dev_prompt) {
            return Ok(EffectResult::event(
                PipelineEvent::agent_template_variables_invalid(
                    AgentRole::Developer,
                    template_name.to_string(),
                    Vec::new(),
                    err.unresolved_placeholders,
                ),
            ));
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

        let result_xml = Path::new(xml_paths::DEVELOPMENT_RESULT_XML);
        let _ = ctx.workspace.remove_if_exists(result_xml);

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.developer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Developer, agent, None, prompt)?;
        result = result.with_additional_event(PipelineEvent::development_agent_invoked(iteration));
        Ok(result)
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
