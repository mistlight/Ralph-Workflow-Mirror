use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::files::llm_output_extraction::try_extract_xml_commit_with_trace;
use crate::phases::commit::check_and_pre_truncate_diff;
use crate::phases::PhaseContext;
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_commit_xsd_retry_with_context,
    prompt_generate_commit_message_with_diff_with_context,
};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::AgentEvent;
use crate::reducer::event::PipelineEvent;
use crate::reducer::state::CommitState;
use crate::reducer::state::PromptMode;
use crate::reducer::ui_event::{UIEvent, XmlOutputType};
use anyhow::Result;
use std::path::Path;

const COMMIT_XSD_ERROR_PATH: &str = ".agent/tmp/commit_xsd_error.txt";

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
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        if matches!(prompt_mode, PromptMode::Continuation) {
            return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                "Commit message generation does not support continuation prompts".to_string(),
            )));
        }
        let diff = match ctx.workspace.read(Path::new(".agent/tmp/commit_diff.txt")) {
            Ok(diff) => diff,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing commit diff at .agent/tmp/commit_diff.txt".to_string(),
                )));
            }
        };
        self.prepare_commit_prompt_with_diff_and_mode(ctx, &diff, prompt_mode)
    }

    pub(super) fn check_commit_diff(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        let diff = crate::git_helpers::git_diff().map_err(anyhow::Error::from);
        self.check_commit_diff_with_result(ctx, diff)
    }

    pub(super) fn check_commit_diff_with_result(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff: Result<String, anyhow::Error>,
    ) -> Result<EffectResult> {
        match diff {
            Ok(diff) => self.check_commit_diff_with_content(ctx, &diff),
            Err(err) => Ok(EffectResult::event(PipelineEvent::commit_diff_failed(
                err.to_string(),
            ))),
        }
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

    pub(super) fn prepare_commit_prompt_with_diff_and_mode(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff: &str,
        prompt_mode: PromptMode,
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

        let continuation_state = &self.state.continuation;
        let (prompt_key, prompt, was_replayed, should_validate) = match prompt_mode {
            PromptMode::SameAgentRetry => {
                // Same-agent retry: prepend retry guidance to the last prepared prompt for this
                // phase (preserves XSD retry context if present).
                let retry_preamble =
                    super::retry_guidance::same_agent_retry_preamble(continuation_state);
                let (base_prompt, should_validate) = match ctx
                    .workspace
                    .read(Path::new(".agent/tmp/commit_prompt.txt"))
                {
                    Ok(previous_prompt) => (previous_prompt, false),
                    Err(_) => (
                        prompt_generate_commit_message_with_diff_with_context(
                            ctx.template_context,
                            &working_diff,
                            ctx.workspace,
                        ),
                        true,
                    ),
                };
                let prompt = format!("{retry_preamble}\n{base_prompt}");
                let prompt_key = format!(
                    "commit_message_attempt_{attempt}_same_agent_retry_{}",
                    continuation_state.same_agent_retry_count
                );
                (prompt_key, prompt, false, should_validate)
            }
            PromptMode::XsdRetry => {
                // XSD retry: use XML-only retry prompt and include the last XSD error.
                // Do not use cached prompts here: the error context can change between retries.
                let xsd_error = ctx
                    .workspace
                    .read(Path::new(COMMIT_XSD_ERROR_PATH))
                    .unwrap_or_else(|_| {
                        "XSD validation failed. Provide valid XML output.".to_string()
                    });
                let prompt = prompt_commit_xsd_retry_with_context(
                    ctx.template_context,
                    &xsd_error,
                    ctx.workspace,
                );
                ("commit_xsd_retry".to_string(), prompt, false, true)
            }
            _ => {
                // Normal (or Continuation rejected earlier)
                let prompt_key = format!("commit_message_attempt_{attempt}");
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_generate_commit_message_with_diff_with_context(
                            ctx.template_context,
                            &working_diff,
                            ctx.workspace,
                        )
                    });
                (prompt_key, prompt, was_replayed, true)
            }
        };

        if should_validate {
            if let Err(err) =
                crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
                    &prompt,
                    &[diff],
                )
            {
                return Ok(EffectResult::event(
                    PipelineEvent::agent_template_variables_invalid(
                        AgentRole::Commit,
                        "commit_message_xml".to_string(),
                        Vec::new(),
                        err.unresolved_placeholders,
                    ),
                ));
            }
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

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .expect("commit agent should be initialized via InitializeAgentChain effect");

        let mut result = self.invoke_agent(ctx, AgentRole::Commit, agent, None, prompt)?;
        if result.additional_events.iter().any(|e| {
            matches!(
                e,
                PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
            )
        }) {
            result = result.with_additional_event(PipelineEvent::commit_agent_invoked(attempt));
        }
        Ok(result)
    }

    pub(super) fn cleanup_commit_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);
        let commit_xml = Path::new(xml_paths::COMMIT_MESSAGE_XML);
        let _ = ctx.workspace.remove_if_exists(commit_xml);
        Ok(EffectResult::event(PipelineEvent::commit_xml_cleaned(
            attempt,
        )))
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
        if message.is_none() {
            // Persist XSD error context for the XSD retry prompt.
            let _ = ctx
                .workspace
                .write(Path::new(COMMIT_XSD_ERROR_PATH), &detail);
        } else {
            let _ = ctx
                .workspace
                .remove_if_exists(Path::new(COMMIT_XSD_ERROR_PATH));
        }
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
