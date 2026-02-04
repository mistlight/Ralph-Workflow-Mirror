impl MainEffectHandler {
    pub(super) fn prepare_fix_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_fix_xml_with_context,
            prompt_fix_xsd_retry_with_context,
        };
        use std::path::Path;

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                ErrorEvent::WorkspaceCreateDirAllFailed {
                    path: tmp_dir.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
            })?;
        }

        let prompt_content = ctx
            .workspace
            .read(Path::new(".agent/PROMPT.md.backup"))
            .unwrap_or_default();
        // Use sentinel PLAN content when missing (consistent with review phase)
        let plan_content = ctx
            .workspace
            .read(Path::new(".agent/PLAN.md"))
            .unwrap_or_else(|_| Self::sentinel_plan_content(ctx.config.isolation_mode));
        let issues_content = ctx
            .workspace
            .read(Path::new(".agent/ISSUES.md"))
            .unwrap_or_default();

        let continuation_state = &self.state.continuation;
        let is_xsd_retry = matches!(prompt_mode, PromptMode::XsdRetry);
        let last_output = if is_xsd_retry {
            ctx.workspace
                .read(Path::new(xml_paths::FIX_RESULT_XML))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut ignore_sources = vec![
            prompt_content.as_str(),
            plan_content.as_str(),
            issues_content.as_str(),
        ];
        if is_xsd_retry {
            ignore_sources.push(last_output.as_str());
        }
        let (prompt_key, fix_prompt, was_replayed, template_name, should_validate) =
            match prompt_mode {
                PromptMode::XsdRetry => {
                    let prompt_key = format!(
                        "fix_{pass}_xsd_retry_{}",
                        continuation_state.invalid_output_attempts
                    );
                    let (prompt, was_replayed) =
                        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                            prompt_fix_xsd_retry_with_context(
                                ctx.template_context,
                                &issues_content,
                                "XML output failed validation. Provide valid XML output.",
                                &last_output,
                                ctx.workspace,
                            )
                        });
                    (prompt_key, prompt, was_replayed, "fix_mode_xsd_retry", true)
                }
                PromptMode::SameAgentRetry => {
                    // Same-agent retry: prepend retry guidance to the last prepared prompt for this
                    // phase (preserves XSD retry / continuation context if present).
                    let retry_preamble =
                        crate::reducer::handler::retry_guidance::same_agent_retry_preamble(
                            continuation_state,
                        );
                    let (base_prompt, should_validate) =
                    match ctx.workspace.read(Path::new(".agent/tmp/fix_prompt.txt")) {
                        Ok(previous_prompt) => (
                            crate::reducer::handler::retry_guidance::strip_existing_same_agent_retry_preamble(&previous_prompt)
                                .to_string(),
                            false,
                        ),
                        Err(_) => (
                            prompt_fix_xml_with_context(
                                ctx.template_context,
                                &prompt_content,
                                &plan_content,
                                &issues_content,
                                &[],
                            ),
                            true,
                        ),
                    };
                    let prompt = format!("{retry_preamble}\n{base_prompt}");
                    let prompt_key = format!(
                        "fix_{pass}_same_agent_retry_{}",
                        continuation_state.same_agent_retry_count
                    );
                    (prompt_key, prompt, false, "fix_mode_xml", should_validate)
                }
                PromptMode::Normal => {
                    let prompt_key = format!("fix_{pass}");
                    let (prompt, was_replayed) =
                        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                            prompt_fix_xml_with_context(
                                ctx.template_context,
                                &prompt_content,
                                &plan_content,
                                &issues_content,
                                &[],
                            )
                        });
                    (prompt_key, prompt, was_replayed, "fix_mode_xml", true)
                }
                PromptMode::Continuation => {
                    return Err(ErrorEvent::FixContinuationNotSupported.into());
                }
            };
        if should_validate {
            if let Err(err) =
                crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
                    &fix_prompt,
                    &ignore_sources,
                )
            {
                return Ok(EffectResult::event(
                    PipelineEvent::agent_template_variables_invalid(
                        AgentRole::Reviewer,
                        template_name.to_string(),
                        Vec::new(),
                        err.unresolved_placeholders,
                    ),
                ));
            }
        }

        if !was_replayed {
            ctx.capture_prompt(&prompt_key, &fix_prompt);
        }

        ctx.workspace
            .write(Path::new(".agent/tmp/fix_prompt.txt"), &fix_prompt)
            .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                path: ".agent/tmp/fix_prompt.txt".to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;

        Ok(EffectResult::event(PipelineEvent::fix_prompt_prepared(
            pass,
        )))
    }

    pub(super) fn invoke_fix_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use std::path::Path;

        let prompt = ctx
            .workspace
            .read(Path::new(".agent/tmp/fix_prompt.txt"))
            .map_err(|_| ErrorEvent::FixPromptMissing)?;

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.reviewer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Reviewer, agent, None, prompt)?;
        if result.additional_events.iter().any(|e| {
            matches!(
                e,
                PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
            )
        }) {
            result = result.with_additional_event(PipelineEvent::fix_agent_invoked(pass));
        }
        Ok(result)
    }

    pub(super) fn cleanup_fix_result_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        let fix_xml = Path::new(xml_paths::FIX_RESULT_XML);
        let _ = ctx.workspace.remove_if_exists(fix_xml);
        Ok(EffectResult::event(PipelineEvent::fix_result_xml_cleaned(
            pass,
        )))
    }

    pub(super) fn extract_fix_result_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        let fix_xml = Path::new(xml_paths::FIX_RESULT_XML);
        match ctx.workspace.read(fix_xml) {
            Ok(_) => Ok(EffectResult::event(
                PipelineEvent::fix_result_xml_extracted(pass),
            )),
            Err(_) => Ok(EffectResult::event(PipelineEvent::fix_result_xml_missing(
                pass,
                self.state.continuation.invalid_output_attempts,
            ))),
        }
    }

    pub(super) fn validate_fix_result_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use crate::files::llm_output_extraction::validate_fix_result_xml;
        use std::path::Path;

        let fix_xml = match ctx.workspace.read(Path::new(xml_paths::FIX_RESULT_XML)) {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::fix_output_validation_failed(
                        pass,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        match validate_fix_result_xml(&fix_xml) {
            Ok(elements) => {
                let status = crate::reducer::state::FixStatus::parse(&elements.status)
                    .unwrap_or(crate::reducer::state::FixStatus::Failed);
                Ok(EffectResult::with_ui(
                    PipelineEvent::fix_result_xml_validated(pass, status, elements.summary),
                    vec![UIEvent::XmlOutput {
                        xml_type: XmlOutputType::FixResult,
                        content: fix_xml,
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    }],
                ))
            }
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::fix_output_validation_failed(
                    pass,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn apply_fix_outcome(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        self.state
            .fix_validated_outcome
            .as_ref()
            .filter(|o| o.pass == pass)
            .ok_or(ErrorEvent::ValidatedFixOutcomeMissing { pass })?;

        Ok(EffectResult::event(PipelineEvent::fix_outcome_applied(
            pass,
        )))
    }

    pub(super) fn archive_fix_result_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::FIX_RESULT_XML));
        Ok(EffectResult::event(PipelineEvent::fix_result_xml_archived(
            pass,
        )))
    }
}
