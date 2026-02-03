use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::files::llm_output_extraction::{archive_xml_file_with_workspace, validate_plan_xml};
use crate::phases::development::format_plan_as_markdown;
use crate::phases::PhaseContext;
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_planning_xml_with_context,
    prompt_planning_xsd_retry_with_context,
};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentEvent, PipelineEvent, PipelinePhase};
use crate::reducer::state::PromptMode;
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use anyhow::Result;
use std::path::Path;

const PLANNING_PROMPT_PATH: &str = ".agent/tmp/planning_prompt.txt";

impl MainEffectHandler {
    pub(super) fn prepare_planning_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir)?;
        }

        let prompt_md = ctx
            .workspace
            .read(Path::new("PROMPT.md"))
            .unwrap_or_default();
        let prompt_md_str = prompt_md.as_str();

        let is_xsd_retry = matches!(prompt_mode, PromptMode::XsdRetry);
        let last_output = if is_xsd_retry {
            ctx.workspace
                .read(Path::new(xml_paths::PLAN_XML))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut ignore_sources = vec![prompt_md_str];
        if is_xsd_retry {
            ignore_sources.push(last_output.as_str());
        }
        let continuation_state = &self.state.continuation;
        let (prompt, template_name, prompt_key, was_replayed, should_validate) = match prompt_mode {
            PromptMode::XsdRetry => {
                (
                    prompt_planning_xsd_retry_with_context(
                        ctx.template_context,
                        prompt_md_str,
                        "Previous XML output failed XSD validation. Please provide valid XML conforming to the schema.",
                        &last_output,
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
                let (base_prompt, should_validate) =
                    match ctx.workspace.read(Path::new(PLANNING_PROMPT_PATH)) {
                        Ok(previous_prompt) => (previous_prompt, false),
                        Err(_) => (
                            prompt_planning_xml_with_context(
                        ctx.template_context,
                        Some(prompt_md_str),
                        ctx.workspace,
                    ),
                            true,
                        ),
                    };
                let prompt = format!("{retry_preamble}\n{base_prompt}");
                let prompt_key = format!(
                    "planning_{iteration}_same_agent_retry_{}",
                    continuation_state.same_agent_retry_count
                );
                // If we reused a previously prepared prompt, it was already validated at the time
                // it was prepared. Re-validating can introduce false positives (e.g., XSD retry
                // prompts include last output, which may contain literal placeholders).
                (prompt, "planning_xml", Some(prompt_key), false, should_validate)
            }
            PromptMode::Normal => {
                let prompt_key = format!("planning_{iteration}");
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_planning_xml_with_context(
                            ctx.template_context,
                            Some(prompt_md_str),
                            ctx.workspace,
                        )
                    });
                (prompt, "planning_xml", Some(prompt_key), was_replayed, true)
            }
            PromptMode::Continuation => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Planning does not support continuation prompts".to_string(),
                )));
            }
        };
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

        ctx.workspace
            .write(Path::new(PLANNING_PROMPT_PATH), &prompt)?;

        Ok(EffectResult::event(
            PipelineEvent::planning_prompt_prepared(iteration),
        ))
    }

    pub(super) fn invoke_planning_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        let prompt = match ctx.workspace.read(Path::new(PLANNING_PROMPT_PATH)) {
            Ok(prompt) => prompt,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    format!("Missing planning prompt at {PLANNING_PROMPT_PATH}"),
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
        let markdown = match self
            .state
            .planning_validated_outcome
            .as_ref()
            .filter(|outcome| outcome.iteration == iteration)
            .and_then(|outcome| outcome.markdown.clone())
        {
            Some(markdown) => markdown,
            None => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing validated planning markdown".to_string(),
                )));
            }
        };
        ctx.workspace
            .write(Path::new(".agent/PLAN.md"), &markdown)?;

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
