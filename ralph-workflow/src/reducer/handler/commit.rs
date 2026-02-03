use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::files::llm_output_extraction::try_extract_xml_commit_with_trace;
use crate::phases::commit::{effective_model_budget_bytes, truncate_diff_to_model_budget};
use crate::phases::PhaseContext;
use crate::prompts::content_reference::{DiffContentReference, MAX_INLINE_CONTENT_SIZE};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_generate_commit_message_with_diff_with_context,
};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::AgentEvent;
use crate::reducer::event::PipelineEvent;
use crate::reducer::prompt_inputs::sha256_hex_str;
use crate::reducer::state::{
    CommitState, MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason, PromptMode,
};
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
    pub(super) fn materialize_commit_inputs(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        attempt: u32,
    ) -> Result<EffectResult> {
        let diff = match ctx.workspace.read(Path::new(".agent/tmp/commit_diff.txt")) {
            Ok(diff) => diff,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing commit diff at .agent/tmp/commit_diff.txt".to_string(),
                )));
            }
        };

        let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();
        let content_id_sha256 = sha256_hex_str(&diff);
        let original_bytes = diff.len() as u64;

        let model_budget_bytes = effective_model_budget_bytes(&self.state.agent_chain.agents);
        let (model_safe_diff, truncated_for_model_budget) =
            truncate_diff_to_model_budget(&diff, model_budget_bytes);
        let final_bytes = model_safe_diff.len() as u64;

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir)?;
        }
        let model_safe_path = Path::new(".agent/tmp/commit_diff.model_safe.txt");
        ctx.workspace
            .write_atomic(model_safe_path, &model_safe_diff)?;

        let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
        let representation = if final_bytes <= inline_budget_bytes {
            PromptInputRepresentation::Inline
        } else {
            PromptInputRepresentation::FileReference {
                path: ctx.workspace.absolute(model_safe_path),
            }
        };

        let reason = match &representation {
            // Align reason with representation for observability/UX. When we use a file reference,
            // the inline budget is the immediate constraint even if we also truncated for the model.
            PromptInputRepresentation::FileReference { .. } => {
                PromptMaterializationReason::InlineBudgetExceeded
            }
            PromptInputRepresentation::Inline => {
                if truncated_for_model_budget {
                    PromptMaterializationReason::ModelBudgetExceeded
                } else {
                    PromptMaterializationReason::WithinBudgets
                }
            }
        };

        if truncated_for_model_budget {
            ctx.logger.warn(&format!(
                "Diff size ({} KB) exceeds model budget ({} KB). Truncated to {} KB at: {}",
                original_bytes / 1024,
                model_budget_bytes / 1024,
                final_bytes / 1024,
                ctx.workspace.absolute(model_safe_path).display()
            ));
        } else if final_bytes > inline_budget_bytes {
            ctx.logger.warn(&format!(
                "Diff size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                final_bytes / 1024,
                inline_budget_bytes / 1024,
                ctx.workspace.absolute(model_safe_path).display()
            ));
        }

        let input = MaterializedPromptInput {
            kind: PromptInputKind::Diff,
            content_id_sha256: content_id_sha256.clone(),
            consumer_signature_sha256: consumer_signature_sha256.clone(),
            original_bytes,
            final_bytes,
            model_budget_bytes: Some(model_budget_bytes),
            inline_budget_bytes: Some(inline_budget_bytes),
            representation,
            reason,
        };

        let mut result =
            EffectResult::event(PipelineEvent::commit_inputs_materialized(attempt, input));
        if truncated_for_model_budget {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Truncated DIFF for model budget: {} KB -> {} KB (budget {} KB)",
                    original_bytes / 1024,
                    final_bytes / 1024,
                    model_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                crate::reducer::event::PipelinePhase::CommitMessage,
                PromptInputKind::Diff,
                content_id_sha256.clone(),
                original_bytes,
                model_budget_bytes,
                "model-context".to_string(),
            ));
        }
        if final_bytes > inline_budget_bytes {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Oversize DIFF: {} KB > {} KB; using file reference",
                    final_bytes / 1024,
                    inline_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                crate::reducer::event::PipelinePhase::CommitMessage,
                PromptInputKind::Diff,
                content_id_sha256,
                final_bytes,
                inline_budget_bytes,
                "inline-embedding".to_string(),
            ));
        }
        Ok(result)
    }

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
        let attempt = current_commit_attempt(&self.state.commit);
        let inputs = self
            .state
            .prompt_inputs
            .commit
            .as_ref()
            .filter(|c| c.attempt == attempt)
            .expect("commit inputs must be materialized before preparing prompt");

        let model_safe_path = Path::new(".agent/tmp/commit_diff.model_safe.txt");
        let diff_for_prompt = match &inputs.diff.representation {
            PromptInputRepresentation::Inline => match ctx.workspace.read(model_safe_path) {
                Ok(diff) => diff,
                Err(err) => {
                    return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                        format!(
                            "Failed to read materialized commit diff at {}: {err}",
                            model_safe_path.display()
                        ),
                    )));
                }
            },
            PromptInputRepresentation::FileReference { path } => {
                DiffContentReference::ReadFromFile {
                    path: path.clone(),
                    start_commit: String::new(),
                    description: format!(
                        "Diff is {} bytes (exceeds {} limit)",
                        inputs.diff.final_bytes, MAX_INLINE_CONTENT_SIZE
                    ),
                }
                .render_for_template()
            }
        };
        self.prepare_commit_prompt_with_diff(ctx, &diff_for_prompt)
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

    pub(super) fn prepare_commit_prompt_with_diff(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        diff_for_prompt: &str,
    ) -> Result<EffectResult> {
        let attempt = current_commit_attempt(&self.state.commit);

        let prompt_key = format!("commit_message_attempt_{attempt}");
        let (prompt, was_replayed) =
            get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                prompt_generate_commit_message_with_diff_with_context(
                    ctx.template_context,
                    diff_for_prompt,
                    ctx.workspace,
                )
            });

        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
            &prompt,
            &[diff_for_prompt],
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
