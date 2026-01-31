use super::util::read_commit_message_xml;
use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::phases::{commit, PhaseContext};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentErrorKind, PipelineEvent, PipelinePhase};
use crate::reducer::ui_event::{UIEvent, XmlOutputType};
use anyhow::Result;

impl MainEffectHandler {
    pub(super) fn generate_commit_message(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let attempt = match &self.state.commit {
            crate::reducer::state::CommitState::Generating { attempt, .. } => *attempt,
            _ => 1,
        };

        let diff = crate::git_helpers::git_diff().unwrap_or_default();
        if diff.trim().is_empty() {
            ctx.logger
                .info("No changes to commit (empty diff), skipping commit");
            return Ok(EffectResult::event(PipelineEvent::commit_skipped(
                "No changes to commit (empty diff)".to_string(),
            )));
        }

        let commit_agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .expect("commit agent should be initialized via InitializeAgentChain effect");

        let attempt_result = match commit::run_commit_attempt(ctx, attempt, &diff, &commit_agent) {
            Ok(r) => r,
            Err(err) => {
                if let Some(tpl_err) =
                    err.downcast_ref::<crate::prompts::TemplateVariablesInvalidError>()
                {
                    return Ok(EffectResult::event(
                        PipelineEvent::agent_template_variables_invalid(
                            AgentRole::Commit,
                            tpl_err.template_name.clone(),
                            tpl_err.missing_variables.clone(),
                            tpl_err.unresolved_placeholders.clone(),
                        ),
                    ));
                }

                return Ok(EffectResult::event(
                    PipelineEvent::commit_generation_failed(err.to_string()),
                ));
            }
        };

        if attempt_result.auth_failure {
            return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                AgentRole::Commit,
                commit_agent,
                1,
                AgentErrorKind::Authentication,
                false,
            )));
        }

        if attempt_result.had_error {
            return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                AgentRole::Commit,
                commit_agent,
                1,
                AgentErrorKind::InternalError,
                false,
            )));
        }

        if !attempt_result.output_valid {
            return Ok(EffectResult::event(
                PipelineEvent::commit_message_validation_failed(
                    attempt_result.validation_detail,
                    attempt,
                ),
            ));
        }

        let message = attempt_result
            .message
            .expect("commit attempt reported output_valid=true but message was missing");

        let event = PipelineEvent::commit_message_generated(message.clone(), attempt);

        let mut ui_events = vec![self.phase_transition_ui(PipelinePhase::CommitMessage)];
        if let Some(xml_content) = read_commit_message_xml(ctx.workspace) {
            ui_events.push(UIEvent::XmlOutput {
                xml_type: XmlOutputType::CommitMessage,
                content: xml_content,
                context: None,
            });
        }

        Ok(EffectResult::with_ui(event, ui_events))
    }

    pub(super) fn create_commit(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        message: String,
    ) -> Result<EffectResult> {
        use crate::git_helpers::{git_add_all, git_commit};

        git_add_all()?;

        match git_commit(&message, None, None, Some(_ctx.executor)) {
            Ok(Some(hash)) => Ok(EffectResult::event(PipelineEvent::commit_created(
                hash.to_string(),
                message,
            ))),
            Ok(None) => Ok(EffectResult::event(PipelineEvent::commit_skipped(
                "No changes to commit".to_string(),
            ))),
            Err(e) => Ok(EffectResult::event(
                PipelineEvent::commit_generation_failed(e.to_string()),
            )),
        }
    }

    pub(super) fn skip_commit(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        reason: String,
    ) -> Result<EffectResult> {
        Ok(EffectResult::event(PipelineEvent::commit_skipped(reason)))
    }
}
