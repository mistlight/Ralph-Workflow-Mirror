use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::files::llm_output_extraction::try_extract_xml_commit_with_trace;
use crate::phases::commit::check_and_pre_truncate_diff;
use crate::phases::PhaseContext;
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_generate_commit_message_with_diff_with_context,
};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::AgentEvent;
use crate::reducer::event::PipelineEvent;
use crate::reducer::state::CommitState;
use crate::reducer::ui_event::{UIEvent, XmlOutputType};
use anyhow::Result;
use std::path::Path;

fn current_commit_attempt(commit: &CommitState) -> u32 {
    match commit {
        CommitState::Generating { attempt, .. } => *attempt,
        _ => 1,
    }
}

impl MainEffectHandler {
    pub(super) fn prepare_commit_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let diff = match ctx.workspace.read(Path::new(".agent/tmp/commit_diff.txt")) {
            Ok(diff) => diff,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing commit diff at .agent/tmp/commit_diff.txt".to_string(),
                )));
            }
        };
        self.prepare_commit_prompt_with_diff(ctx, &diff)
    }

    pub(super) fn check_commit_diff(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        let diff = crate::git_helpers::git_diff().unwrap_or_default();
        self.check_commit_diff_with_content(ctx, &diff)
    }

    pub(super) fn check_commit_diff_with_content(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff: &str,
    ) -> Result<EffectResult> {
        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir)?;
        }
        ctx.workspace
            .write(Path::new(".agent/tmp/commit_diff.txt"), diff)?;

        Ok(EffectResult::event(PipelineEvent::commit_diff_prepared(
            diff.trim().is_empty(),
        )))
    }

    pub(super) fn prepare_commit_prompt_with_diff(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff: &str,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);

        let commit_agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .expect("commit agent should be initialized via InitializeAgentChain effect");

        let (working_diff, _diff_truncated) =
            check_and_pre_truncate_diff(diff, &commit_agent, ctx.logger);

        let prompt_key = format!("commit_message_attempt_{attempt}");
        let (prompt, was_replayed) =
            get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                prompt_generate_commit_message_with_diff_with_context(
                    ctx.template_context,
                    &working_diff,
                    ctx.workspace,
                )
            });

        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
            &prompt,
            &[diff],
        ) {
            return Ok(EffectResult::event(
                PipelineEvent::agent_template_variables_invalid(
                    AgentRole::Commit,
                    "commit_message_xml".to_string(),
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
            ctx.workspace.create_dir_all(tmp_dir)?;
        }

        ctx.workspace
            .write(Path::new(".agent/tmp/commit_prompt.txt"), &prompt)?;

        Ok(
            EffectResult::event(PipelineEvent::commit_prompt_prepared(attempt)).with_ui_event(
                self.phase_transition_ui(crate::reducer::event::PipelinePhase::CommitMessage),
            ),
        )
    }

    pub(super) fn invoke_commit_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);
        let prompt = match ctx
            .workspace
            .read(Path::new(".agent/tmp/commit_prompt.txt"))
        {
            Ok(prompt) => prompt,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing commit prompt at .agent/tmp/commit_prompt.txt".to_string(),
                )));
            }
        };

        let commit_xml = Path::new(xml_paths::COMMIT_MESSAGE_XML);
        if ctx.workspace.exists(commit_xml) {
            let _ = ctx.workspace.remove(commit_xml);
        }

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .expect("commit agent should be initialized via InitializeAgentChain effect");

        let mut result = self.invoke_agent(ctx, AgentRole::Commit, agent, None, prompt)?;
        if matches!(
            result.event,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        ) {
            result = result.with_additional_event(PipelineEvent::commit_agent_invoked(attempt));
        }
        Ok(result)
    }

    pub(super) fn extract_commit_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);
        let commit_xml = Path::new(xml_paths::COMMIT_MESSAGE_XML);

        match ctx.workspace.read(commit_xml) {
            Ok(_) => Ok(EffectResult::event(PipelineEvent::commit_xml_extracted(
                attempt,
            ))),
            Err(_) => Ok(EffectResult::event(PipelineEvent::commit_xml_missing(
                attempt,
            ))),
        }
    }

    pub(super) fn validate_commit_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);
        let commit_xml = Path::new(xml_paths::COMMIT_MESSAGE_XML);

        let xml_content = match ctx.workspace.read(commit_xml) {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::commit_xml_validation_failed(
                        "XML output missing or invalid; agent must write .agent/tmp/commit_message.xml"
                            .to_string(),
                        attempt,
                    ),
                ));
            }
        };

        let (message, detail) = try_extract_xml_commit_with_trace(&xml_content);
        let event = match message {
            Some(msg) => PipelineEvent::commit_xml_validated(msg, attempt),
            None => PipelineEvent::commit_xml_validation_failed(detail, attempt),
        };

        Ok(EffectResult::with_ui(
            event,
            vec![UIEvent::XmlOutput {
                xml_type: XmlOutputType::CommitMessage,
                content: xml_content,
                context: None,
            }],
        ))
    }

    pub(super) fn apply_commit_message_outcome(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let outcome =
            self.state.commit_validated_outcome.as_ref().expect(
                "validated commit outcome should exist before applying commit message outcome",
            );

        let event = match (&outcome.message, &outcome.reason) {
            (Some(message), _) => {
                PipelineEvent::commit_message_generated(message.clone(), outcome.attempt)
            }
            (None, Some(reason)) => {
                PipelineEvent::commit_message_validation_failed(reason.clone(), outcome.attempt)
            }
            _ => PipelineEvent::commit_generation_failed(
                "Commit validation outcome missing message and reason".to_string(),
            ),
        };

        Ok(EffectResult::event(event))
    }

    pub(super) fn archive_commit_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);
        archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::COMMIT_MESSAGE_XML));
        Ok(EffectResult::event(PipelineEvent::commit_xml_archived(
            attempt,
        )))
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
