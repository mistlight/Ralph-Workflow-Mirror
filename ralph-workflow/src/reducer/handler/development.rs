use super::util::read_xml_and_archive_if_present;
use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::phases::{development, PhaseContext};
use crate::prompts::ContextLevel;
use crate::reducer::effect::ContinuationContextData;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentErrorKind, PipelineEvent};
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use crate::workspace::Workspace;
use anyhow::Result;
use std::path::Path;

pub(super) fn is_auth_failure(err: &anyhow::Error) -> bool {
    if err.chain().any(|cause| {
        cause
            .downcast_ref::<development::AuthFailureError>()
            .is_some()
    }) {
        return true;
    }

    let msg = err.to_string().to_lowercase();
    msg.contains("authentication error")
        || msg.contains("auth/credential")
        || msg.contains("unauthorized")
        || msg.contains("credential")
        || msg.contains("api key")
}

impl MainEffectHandler {
    pub(super) fn run_development_iteration(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        use crate::checkpoint::restore::ResumeContext;

        let developer_context = ContextLevel::from(ctx.config.developer_context);

        // Get current agent from agent chain
        let dev_agent = self.state.agent_chain.current_agent().cloned();

        // Get continuation state from reducer state
        let continuation_state = &self.state.continuation;

        // Config semantics: max_dev_continuations counts *continuation attempts* (fresh sessions)
        // allowed after the initial attempt. Total valid attempts per iteration is
        // `1 + max_dev_continuations`.
        let max_continuations = ctx.config.max_dev_continuations.unwrap_or(2);

        // Defensive guard: if checkpoint state already exceeds the configured limit,
        // emit a domain event and let the reducer/orchestration decide the policy.
        if continuation_state.continuation_attempt > max_continuations {
            let last_status = continuation_state
                .previous_status
                .clone()
                .unwrap_or(crate::reducer::state::DevelopmentStatus::Failed);
            let reason = development_continuation_budget_exhausted_abort_reason(
                iteration,
                continuation_state.continuation_attempt,
                max_continuations,
                last_status.clone(),
            );
            ctx.logger.warn(&reason);

            return Ok(EffectResult::with_ui(
                PipelineEvent::development_continuation_budget_exhausted(
                    iteration,
                    continuation_state.continuation_attempt,
                    last_status,
                ),
                vec![UIEvent::IterationProgress {
                    current: iteration,
                    total: self.state.total_iterations,
                }],
            ));
        }

        // Run a single development attempt (one session) with validation.
        let attempt = development::run_development_attempt(
            ctx,
            iteration,
            developer_context,
            false,
            None::<&ResumeContext>,
            dev_agent.as_deref(),
            continuation_state,
        );

        let attempt = match attempt {
            Ok(a) => a,
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
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    format!("Development attempt failed: {err}"),
                )));
            }
        };

        // Check for auth failure - trigger agent fallback immediately
        if attempt.auth_failure {
            let current_agent = dev_agent.clone().unwrap_or_else(|| "unknown".to_string());
            return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                current_agent,
                attempt.exit_code,
                AgentErrorKind::Authentication,
                false,
            )));
        }

        if attempt.had_error {
            let current_agent = dev_agent.clone().unwrap_or_else(|| "unknown".to_string());
            return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                current_agent,
                attempt.exit_code,
                AgentErrorKind::InternalError,
                false,
            )));
        }

        // Check if output is invalid (XSD/XML parsing failed) - emit event, let reducer decide
        if !attempt.output_valid {
            let mut ui_events = vec![UIEvent::IterationProgress {
                current: iteration,
                total: self.state.total_iterations,
            }];

            // Try to read development result XML for semantic rendering via helper.
            if let Some(xml_content) = read_xml_and_archive_if_present(
                ctx.workspace,
                Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
            ) {
                ui_events.push(UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentResult,
                    content: xml_content,
                    context: Some(XmlOutputContext {
                        iteration: Some(iteration),
                        pass: None,
                        snippets: Vec::new(),
                    }),
                });
            }

            // Emit OutputValidationFailed - reducer decides whether to retry or switch agents
            return Ok(EffectResult::with_ui(
                PipelineEvent::development_output_validation_failed(
                    iteration,
                    continuation_state.invalid_output_attempts,
                ),
                ui_events,
            ));
        }

        // If we reached completed, the iteration can transition to commit.
        if attempt.output_valid
            && matches!(
                attempt.status,
                crate::reducer::state::DevelopmentStatus::Completed
            )
        {
            let event = if continuation_state.is_continuation() {
                PipelineEvent::development_iteration_continuation_succeeded(
                    iteration,
                    continuation_state.continuation_attempt,
                )
            } else {
                PipelineEvent::development_iteration_completed(iteration, true)
            };

            let mut ui_events = vec![UIEvent::IterationProgress {
                current: iteration,
                total: self.state.total_iterations,
            }];

            if let Some(xml_content) = read_xml_and_archive_if_present(
                ctx.workspace,
                Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
            ) {
                ui_events.push(UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentResult,
                    content: xml_content,
                    context: Some(XmlOutputContext {
                        iteration: Some(iteration),
                        pass: None,
                        snippets: Vec::new(),
                    }),
                });
            }

            return Ok(EffectResult::with_ui(event, ui_events));
        }

        // Not completed (valid output): partial/failed status triggers a continuation attempt.
        // Check if continuation budget is exhausted - emit event, let reducer decide
        let next_attempt = continuation_state.continuation_attempt + 1;
        if next_attempt > max_continuations {
            let reason = development_continuation_budget_exhausted_abort_reason(
                iteration,
                continuation_state.continuation_attempt,
                max_continuations,
                attempt.status.clone(),
            );
            ctx.logger.warn(&reason);

            let event = PipelineEvent::development_continuation_budget_exhausted(
                iteration,
                continuation_state.continuation_attempt,
                attempt.status.clone(),
            );

            let mut ui_events = vec![UIEvent::IterationProgress {
                current: iteration,
                total: self.state.total_iterations,
            }];

            if let Some(xml_content) = read_xml_and_archive_if_present(
                ctx.workspace,
                Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
            ) {
                ui_events.push(UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentResult,
                    content: xml_content,
                    context: Some(XmlOutputContext {
                        iteration: Some(iteration),
                        pass: None,
                        snippets: Vec::new(),
                    }),
                });
            }

            return Ok(EffectResult::with_ui(event, ui_events));
        }

        ctx.logger.info(&format!(
            "Triggering development continuation attempt {}/{} (previous status={})",
            next_attempt, max_continuations, attempt.status
        ));

        let status = attempt.status.clone();
        let summary = attempt.summary.clone();
        let files_changed = attempt.files_changed.clone();
        let next_steps = attempt.next_steps.clone();

        let event = PipelineEvent::development_iteration_continuation_triggered(
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        );

        let mut ui_events = vec![UIEvent::IterationProgress {
            current: iteration,
            total: self.state.total_iterations,
        }];

        if let Some(xml_content) = read_xml_and_archive_if_present(
            ctx.workspace,
            Path::new(xml_paths::DEVELOPMENT_RESULT_XML),
        ) {
            ui_events.push(UIEvent::XmlOutput {
                xml_type: XmlOutputType::DevelopmentResult,
                content: xml_content,
                context: Some(XmlOutputContext {
                    iteration: Some(iteration),
                    pass: None,
                    snippets: Vec::new(),
                }),
            });
        }

        Ok(EffectResult::with_ui(event, ui_events))
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

fn development_continuation_budget_exhausted_abort_reason(
    iteration: u32,
    total_attempts: u32,
    max_continuations: u32,
    last_status: crate::reducer::state::DevelopmentStatus,
) -> String {
    format!(
        "Development continuation attempts exhausted (iteration={iteration}, total_attempts={total_attempts}, max_continuations={max_continuations}, last_status={last_status})"
    )
}
