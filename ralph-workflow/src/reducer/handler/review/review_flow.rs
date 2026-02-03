impl MainEffectHandler {
    pub(super) fn materialize_review_inputs(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        let plan_content = match ctx.workspace.read(Path::new(".agent/PLAN.md")) {
            Ok(plan_content) => plan_content,
            Err(err) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(format!(
                    "Failed to read required .agent/PLAN.md: {err}",
                ))));
            }
        };
        let diff_content = match ctx.workspace.read(Path::new(".agent/DIFF.backup")) {
            Ok(diff_content) => diff_content,
            Err(err) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(format!(
                    "Failed to read required .agent/DIFF.backup: {err}",
                ))));
            }
        };

        let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
        let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();

        let plan_path = Path::new(".agent/PLAN.md");
        let (plan_representation, plan_reason) = if plan_content.len() as u64 > inline_budget_bytes
        {
            ctx.logger.warn(&format!(
                "PLAN size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                (plan_content.len() as u64) / 1024,
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
            (PromptInputRepresentation::Inline, PromptMaterializationReason::WithinBudgets)
        };

        let diff_path = Path::new(".agent/tmp/diff.txt");
        let (diff_representation, diff_reason) = if diff_content.len() as u64 > inline_budget_bytes {
            let tmp_dir = Path::new(".agent/tmp");
            if !ctx.workspace.exists(tmp_dir) {
                if let Err(err) = ctx.workspace.create_dir_all(tmp_dir) {
                    return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(format!(
                        "Failed to create directory {}: {err}",
                        tmp_dir.display()
                    ))));
                }
            }
            if let Err(err) = ctx.workspace.write(Path::new(".agent/tmp/diff.txt"), &diff_content) {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(format!(
                    "Failed to write materialized diff to .agent/tmp/diff.txt: {err}",
                ))));
            }
            ctx.logger.warn(&format!(
                "DIFF size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                (diff_content.len() as u64) / 1024,
                inline_budget_bytes / 1024,
                ctx.workspace.absolute(diff_path).display()
            ));
            (
                PromptInputRepresentation::FileReference {
                    path: diff_path.to_path_buf(),
                },
                PromptMaterializationReason::InlineBudgetExceeded,
            )
        } else {
            (PromptInputRepresentation::Inline, PromptMaterializationReason::WithinBudgets)
        };

        let plan_input = MaterializedPromptInput {
            kind: PromptInputKind::Plan,
            content_id_sha256: sha256_hex_str(&plan_content),
            consumer_signature_sha256: consumer_signature_sha256.clone(),
            original_bytes: plan_content.len() as u64,
            final_bytes: plan_content.len() as u64,
            model_budget_bytes: None,
            inline_budget_bytes: Some(inline_budget_bytes),
            representation: plan_representation,
            reason: plan_reason,
        };
        let diff_input = MaterializedPromptInput {
            kind: PromptInputKind::Diff,
            content_id_sha256: sha256_hex_str(&diff_content),
            consumer_signature_sha256: consumer_signature_sha256.clone(),
            original_bytes: diff_content.len() as u64,
            final_bytes: diff_content.len() as u64,
            model_budget_bytes: None,
            inline_budget_bytes: Some(inline_budget_bytes),
            representation: diff_representation,
            reason: diff_reason,
        };

        let mut result =
            EffectResult::event(PipelineEvent::review_inputs_materialized(pass, plan_input.clone(), diff_input.clone()));
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
                crate::reducer::event::PipelinePhase::Review,
                PromptInputKind::Plan,
                plan_input.content_id_sha256.clone(),
                plan_input.original_bytes,
                inline_budget_bytes,
                "inline-embedding".to_string(),
            ));
        }
        if diff_input.original_bytes > inline_budget_bytes {
            result = result.with_ui_event(UIEvent::AgentActivity {
                agent: "pipeline".to_string(),
                message: format!(
                    "Oversize DIFF: {} KB > {} KB; using file reference",
                    diff_input.original_bytes / 1024,
                    inline_budget_bytes / 1024
                ),
            });
            result = result.with_additional_event(PipelineEvent::prompt_input_oversize_detected(
                crate::reducer::event::PipelinePhase::Review,
                PromptInputKind::Diff,
                diff_input.content_id_sha256.clone(),
                diff_input.original_bytes,
                inline_budget_bytes,
                "inline-embedding".to_string(),
            ));
        }
        Ok(result)
    }

    pub(super) fn prepare_review_context(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::{create_prompt_backup_with_workspace, write_diff_backup_with_workspace};

        let _ = create_prompt_backup_with_workspace(ctx.workspace);

        let (diff, baseline_oid) =
            match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
                Ok((diff, baseline_oid)) => (diff, baseline_oid),
                Err(err) => {
                    ctx.logger
                        .warn(&format!("Failed to compute review diff: {err}"));
                    (String::new(), String::new())
                }
            };
        let _ = write_diff_backup_with_workspace(ctx.workspace, &diff);

        let baseline_path = Path::new(Self::DIFF_BASELINE_PATH);
        if baseline_oid.trim().is_empty() {
            let _ = ctx.workspace.remove_if_exists(baseline_path);
        } else if let Err(err) = ctx.workspace.write(baseline_path, &baseline_oid) {
            ctx.logger
                .warn(&format!("Failed to write review diff baseline: {err}"));
        }

        Ok(EffectResult::with_ui(
            PipelineEvent::review_context_prepared(pass),
            vec![UIEvent::ReviewProgress {
                pass,
                total: self.state.total_reviewer_passes,
            }],
        ))
    }

    pub(super) fn prepare_review_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_review_xml_with_references,
            prompt_review_xsd_retry_with_context,
        };
        use std::path::Path;

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir)?;
        }

        let materialized_inputs = self
            .state
            .prompt_inputs
            .review
            .as_ref()
            .filter(|p| p.pass == pass);

        let baseline_oid_for_prompts = ctx
            .workspace
            .read(Path::new(Self::DIFF_BASELINE_PATH))
            .unwrap_or_default()
            .trim()
            .to_string();

        let mut ignore_sources_owned: Vec<String> = Vec::new();
        let (plan_inline, diff_inline) = if matches!(prompt_mode, PromptMode::Normal) {
            let inputs =
                materialized_inputs.expect("review inputs must be materialized before preparing prompt");
            let plan_inline = match &inputs.plan.representation {
                PromptInputRepresentation::Inline => {
                    let plan = match ctx.workspace.read(Path::new(".agent/PLAN.md")) {
                        Ok(plan) => plan,
                        Err(err) => {
                            return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                                format!("Failed to read required .agent/PLAN.md: {err}"),
                            )));
                        }
                    };
                    ignore_sources_owned.push(plan.clone());
                    Some(plan)
                }
                PromptInputRepresentation::FileReference { .. } => None,
            };
            let diff_inline = match &inputs.diff.representation {
                PromptInputRepresentation::Inline => {
                    let diff = match ctx.workspace.read(Path::new(".agent/DIFF.backup")) {
                        Ok(diff) => diff,
                        Err(err) => {
                            return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                                format!("Failed to read required .agent/DIFF.backup: {err}"),
                            )));
                        }
                    };
                    ignore_sources_owned.push(diff.clone());
                    Some(diff)
                }
                PromptInputRepresentation::FileReference { .. } => None,
            };
            (plan_inline, diff_inline)
        } else {
            (None, None)
        };
        let continuation_state = &self.state.continuation;
        let is_xsd_retry = matches!(prompt_mode, PromptMode::XsdRetry);
        let last_output = if is_xsd_retry {
            match ctx.workspace.read(Path::new(xml_paths::ISSUES_XML)) {
                Ok(output) => output,
                Err(err) => {
                    return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(format!(
                        "Failed to read last review output at {}: {err}",
                        xml_paths::ISSUES_XML
                    ))));
                }
            }
        } else {
            String::new()
        };
        if is_xsd_retry {
            ignore_sources_owned.push(last_output.clone());
        }
        let ignore_sources: Vec<&str> = ignore_sources_owned.iter().map(|s| s.as_str()).collect();
        let (prompt_key, review_prompt_xml, was_replayed, template_name) = match prompt_mode {
            PromptMode::XsdRetry => {
                let prompt_key = format!(
                    "review_{pass}_xsd_retry_{}",
                    continuation_state.invalid_output_attempts
                );
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_review_xsd_retry_with_context(
                            ctx.template_context,
                            "",
                            "",
                            "",
                            "XML output failed validation. Provide valid XML output.",
                            &last_output,
                            ctx.workspace,
                        )
                    });
                (prompt_key, prompt, was_replayed, "review_xsd_retry")
            }
            PromptMode::Normal => {
                let inputs =
                    materialized_inputs.expect("review inputs must be materialized before preparing prompt");
                let prompt_key = format!("review_{pass}");
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        let plan_ref = match &inputs.plan.representation {
                            PromptInputRepresentation::Inline => {
                                PlanContentReference::Inline(
                                    plan_inline
                                        .clone()
                                        .expect("plan content must be loaded for inline"),
                                )
                            }
                            PromptInputRepresentation::FileReference { path } => {
                                PlanContentReference::ReadFromFile {
                                    primary_path: ctx.workspace.absolute(path),
                                    fallback_path: Some(
                                        ctx.workspace.absolute(Path::new(".agent/tmp/plan.xml")),
                                    ),
                                    description: format!(
                                        "Plan is {} bytes (exceeds {} limit)",
                                        inputs.plan.final_bytes,
                                        MAX_INLINE_CONTENT_SIZE
                                    ),
                                }
                            }
                        };

                        let diff_ref = match &inputs.diff.representation {
                            PromptInputRepresentation::Inline => {
                                DiffContentReference::Inline(
                                    diff_inline
                                        .clone()
                                        .expect("diff content must be loaded for inline"),
                                )
                            }
                            PromptInputRepresentation::FileReference { path } => {
                                DiffContentReference::ReadFromFile {
                                    path: ctx.workspace.absolute(path),
                                    start_commit: baseline_oid_for_prompts.clone(),
                                    description: format!(
                                        "Diff is {} bytes (exceeds {} limit)",
                                        inputs.diff.final_bytes,
                                        MAX_INLINE_CONTENT_SIZE
                                    ),
                                }
                            }
                        };

                        let refs = PromptContentReferences {
                            prompt: None,
                            plan: Some(plan_ref),
                            diff: Some(diff_ref),
                        };
                        prompt_review_xml_with_references(ctx.template_context, &refs)
                    });
                (prompt_key, prompt, was_replayed, "review_xml")
            }
            PromptMode::Continuation => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Review does not support continuation prompts".to_string(),
                )));
            }
        };
        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
            &review_prompt_xml,
            &ignore_sources,
        ) {
            return Ok(EffectResult::event(
                PipelineEvent::agent_template_variables_invalid(
                    AgentRole::Reviewer,
                    template_name.to_string(),
                    Vec::new(),
                    err.unresolved_placeholders,
                ),
            ));
        }

        if !was_replayed {
            ctx.capture_prompt(&prompt_key, &review_prompt_xml);
        }

        ctx.workspace.write(
            Path::new(".agent/tmp/review_prompt.txt"),
            &review_prompt_xml,
        )?;

        Ok(EffectResult::event(PipelineEvent::review_prompt_prepared(
            pass,
        )))
    }

    pub(super) fn invoke_review_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use std::path::Path;

        let prompt = match ctx
            .workspace
            .read(Path::new(".agent/tmp/review_prompt.txt"))
        {
            Ok(prompt) => prompt,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing review prompt at .agent/tmp/review_prompt.txt".to_string(),
                )));
            }
        };

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.reviewer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Reviewer, agent, None, prompt)?;
        if matches!(
            result.event,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        ) {
            result = result.with_additional_event(PipelineEvent::review_agent_invoked(pass));
        }
        Ok(result)
    }

    pub(super) fn cleanup_review_issues_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        let issues_xml = Path::new(xml_paths::ISSUES_XML);
        let _ = ctx.workspace.remove_if_exists(issues_xml);
        Ok(EffectResult::event(
            PipelineEvent::review_issues_xml_cleaned(pass),
        ))
    }

    pub(super) fn extract_review_issues_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        // Only the canonical path is considered input. Archived `.processed` files
        // are debug artifacts and must not be used as fallback inputs.
        let issues_xml = Path::new(xml_paths::ISSUES_XML);
        let content = ctx.workspace.read(issues_xml);

        match content {
            Ok(_) => Ok(EffectResult::event(
                PipelineEvent::review_issues_xml_extracted(pass),
            )),
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::review_issues_xml_missing(
                    pass,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn validate_review_issues_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use crate::files::llm_output_extraction::validate_issues_xml;
        use std::path::Path;

        let issues_xml = ctx.workspace.read(Path::new(xml_paths::ISSUES_XML));
        let issues_xml = match issues_xml {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::review_output_validation_failed(
                        pass,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        match validate_issues_xml(&issues_xml) {
            Ok(elements) => {
                let issues_found = !elements.issues.is_empty();
                let clean_no_issues =
                    elements.no_issues_found.is_some() && elements.issues.is_empty();
                Ok(EffectResult::with_ui(
                    PipelineEvent::review_issues_xml_validated(
                        pass,
                        issues_found,
                        clean_no_issues,
                        elements.issues,
                        elements.no_issues_found,
                    ),
                    vec![UIEvent::XmlOutput {
                        xml_type: XmlOutputType::ReviewIssues,
                        content: issues_xml,
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    }],
                ))
            }
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::review_output_validation_failed(
                    pass,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn write_issues_markdown(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use std::path::Path;

        let outcome = match self
            .state
            .review_validated_outcome
            .as_ref()
            .filter(|outcome| outcome.pass == pass)
        {
            Some(outcome) => outcome,
            None => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing validated review outcome".to_string(),
                )));
            }
        };
        let elements = crate::files::llm_output_extraction::IssuesElements {
            issues: outcome.issues.clone(),
            no_issues_found: outcome.no_issues_found.clone(),
        };
        let markdown = render_issues_markdown(&elements);
        ctx.workspace
            .write(Path::new(".agent/ISSUES.md"), &markdown)?;

        Ok(EffectResult::event(
            PipelineEvent::review_issues_markdown_written(pass),
        ))
    }

    pub(super) fn extract_review_issue_snippets(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        let outcome = match self
            .state
            .review_validated_outcome
            .as_ref()
            .filter(|outcome| outcome.pass == pass)
        {
            Some(outcome) => outcome,
            None => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing validated review outcome".to_string(),
                )));
            }
        };

        let issues_xml = match ctx.workspace.read(Path::new(xml_paths::ISSUES_XML)) {
            Ok(content) => content,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing review issues XML for snippet extraction".to_string(),
                )));
            }
        };

        let snippets = extract_issue_snippets(&outcome.issues, ctx.workspace);
        Ok(EffectResult::with_ui(
            PipelineEvent::review_issue_snippets_extracted(pass),
            vec![UIEvent::XmlOutput {
                xml_type: XmlOutputType::ReviewIssues,
                content: issues_xml,
                context: Some(XmlOutputContext {
                    iteration: None,
                    pass: Some(pass),
                    snippets,
                }),
            }],
        ))
    }

    pub(super) fn archive_review_issues_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML));
        Ok(EffectResult::event(
            PipelineEvent::review_issues_xml_archived(pass),
        ))
    }

    pub(super) fn apply_review_outcome(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        pass: u32,
        issues_found: bool,
        clean_no_issues: bool,
    ) -> Result<EffectResult> {
        if clean_no_issues {
            return Ok(EffectResult::event(
                PipelineEvent::review_pass_completed_clean(pass),
            ));
        }
        Ok(EffectResult::event(PipelineEvent::review_completed(
            pass,
            issues_found,
        )))
    }
}
