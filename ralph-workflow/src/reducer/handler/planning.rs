use super::util::read_xml_and_archive_if_present;
use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::phases::{development, PhaseContext};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentErrorKind, PipelineEvent, PipelinePhase};
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
    pub(super) fn generate_plan(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        // Planning must honor the reducer-selected agent chain.
        // We achieve this by running the planning phase with a temporary PhaseContext
        // whose `developer_agent` is set to the current agent in `state.agent_chain`.
        let effective_agent = self
            .state
            .agent_chain
            .current_agent()
            .map(|s| s.as_str())
            .unwrap_or(ctx.developer_agent);

        // Pass continuation state so planning can use XSD retry prompt when appropriate
        let continuation_state = self.state.continuation.clone();
        match with_overridden_developer_agent(ctx, effective_agent, |inner_ctx| {
            development::run_planning_step(inner_ctx, iteration, &continuation_state)
        }) {
            Ok(_) => {
                // Validate plan was created
                let plan_path = Path::new(".agent/PLAN.md");
                let plan_exists = ctx.workspace.exists(plan_path);
                let plan_content = if plan_exists {
                    ctx.workspace.read(plan_path).ok().unwrap_or_default()
                } else {
                    String::new()
                };

                let is_valid = plan_exists && !plan_content.trim().is_empty();

                let event = if is_valid {
                    PipelineEvent::plan_generation_completed(iteration, true)
                } else {
                    // Planning invalid output attempt count is tracked in reducer state.
                    PipelineEvent::planning_output_validation_failed(
                        iteration,
                        self.state.continuation.invalid_output_attempts,
                    )
                };

                // Build UI events
                let mut ui_events = vec![];

                // Emit phase transition UI event when plan is valid
                if is_valid {
                    ui_events.push(self.phase_transition_ui(PipelinePhase::Development));

                    // Try to read plan XML for semantic rendering via helper.
                    if let Some(xml_content) = read_xml_and_archive_if_present(
                        ctx.workspace,
                        Path::new(xml_paths::PLAN_XML),
                    ) {
                        ui_events.push(UIEvent::XmlOutput {
                            xml_type: XmlOutputType::DevelopmentPlan,
                            content: xml_content,
                            context: Some(XmlOutputContext {
                                iteration: Some(iteration),
                                pass: None,
                                snippets: Vec::new(),
                            }),
                        });
                    }
                }

                Ok(EffectResult::with_ui(event, ui_events))
            }
            Err(err) => {
                if let Some(tpl_err) =
                    err.downcast_ref::<crate::prompts::TemplateVariablesInvalidError>()
                {
                    return Ok(EffectResult::event(
                        PipelineEvent::agent_template_variables_invalid(
                            AgentRole::Developer,
                            tpl_err.template_name.clone(),
                            tpl_err.missing_variables.clone(),
                            tpl_err.unresolved_placeholders.clone(),
                        ),
                    ));
                }

                if Self::is_auth_failure(&err) {
                    let current_agent = self
                        .state
                        .agent_chain
                        .current_agent()
                        .cloned()
                        .unwrap_or_else(|| ctx.developer_agent.to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Developer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                Ok(EffectResult::event(
                    PipelineEvent::planning_output_validation_failed(
                        iteration,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ))
            }
        }
    }
}

fn with_overridden_developer_agent<R>(
    ctx: &mut PhaseContext<'_>,
    developer_agent: &str,
    run: impl for<'a> FnOnce(&mut PhaseContext<'a>) -> R,
) -> R {
    // PhaseContext owns some state (run_context/execution_history/prompt_history).
    // To override `developer_agent` without leaking lifetimes, we temporarily move
    // those owned values into a new PhaseContext with a shorter lifetime.
    let run_context = std::mem::take(&mut ctx.run_context);
    let execution_history = std::mem::take(&mut ctx.execution_history);
    let prompt_history = std::mem::take(&mut ctx.prompt_history);

    let (result, run_context, execution_history, prompt_history) = {
        let mut inner_ctx = PhaseContext {
            config: ctx.config,
            registry: ctx.registry,
            logger: ctx.logger,
            colors: ctx.colors,
            timer: &mut *ctx.timer,
            stats: &mut *ctx.stats,
            developer_agent,
            reviewer_agent: ctx.reviewer_agent,
            review_guidelines: ctx.review_guidelines,
            template_context: ctx.template_context,
            run_context,
            execution_history,
            prompt_history,
            executor: ctx.executor,
            executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
            repo_root: ctx.repo_root,
            workspace: ctx.workspace,
        };

        let result = run(&mut inner_ctx);
        (
            result,
            inner_ctx.run_context,
            inner_ctx.execution_history,
            inner_ctx.prompt_history,
        )
    };

    ctx.run_context = run_context;
    ctx.execution_history = execution_history;
    ctx.prompt_history = prompt_history;

    result
}
