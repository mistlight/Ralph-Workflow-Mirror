// Review phase prompt generation.
//
// This module handles building prompts for the reviewer agent across different invocation modes:
// Normal, XsdRetry, and SameAgentRetry. It manages materialized input embedding (inline vs
// file references), template validation, and prompt history capture.
//
// ## Responsibilities
//
// - Building prompts for 3 modes: Normal, XsdRetry, SameAgentRetry
// - Reading materialized inputs and deciding inline vs file-reference embedding
// - For XsdRetry: materializing last_output.xml and emitting events
// - For SameAgentRetry: prepending retry guidance to previous prompt
// - For Normal: using prompt template with content references
// - Building `PromptContentReferences` with `PlanContentReference` and `DiffContentReference`
// - Validating templates for unresolved placeholders
// - Capturing prompts to history
// - Writing `.agent/tmp/review_prompt.txt`
//
// ## Prompt Modes
//
// - **Normal**: First invocation or after successful validation - uses full template
// - **XsdRetry**: After XML validation failure - includes XSD error and last output
// - **SameAgentRetry**: After agent invocation failure - prepends retry guidance
//
// ## See Also
//
// - `input_materialization.rs` - PLAN and DIFF preparation
// - `validation.rs` - XML validation that triggers XSD retry

impl MainEffectHandler {
    pub(in crate::reducer::handler) fn prepare_review_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
        prompt_mode: PromptMode,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_review_xml_with_references,
            prompt_review_xsd_retry_with_context_files,
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
        let mut additional_events: Vec<PipelineEvent> = Vec::new();

        let materialized_inputs = self
            .state
            .prompt_inputs
            .review
            .as_ref()
            .filter(|p| p.pass == pass);

        let baseline_oid_for_prompts = match ctx.workspace.read(Path::new(Self::DIFF_BASELINE_PATH))
        {
            Ok(s) => s.trim().to_string(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(ErrorEvent::WorkspaceReadFailed {
                    path: Self::DIFF_BASELINE_PATH.to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
                .into());
            }
        };

        let mut ignore_sources_owned: Vec<String> = Vec::new();
        let (plan_inline, diff_inline) = if matches!(prompt_mode, PromptMode::Normal) {
            let inputs = match materialized_inputs {
                Some(inputs) => inputs,
                None => {
                    return Err(ErrorEvent::ReviewInputsNotMaterialized { pass }.into());
                }
            };
            let plan_inline = match &inputs.plan.representation {
                PromptInputRepresentation::Inline => {
                    // Use sentinel if .agent/PLAN.md is missing.
                    let plan = match ctx.workspace.read(Path::new(".agent/PLAN.md")) {
                        Ok(plan) => plan,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            Self::sentinel_plan_content(ctx.config.isolation_mode)
                        }
                        Err(err) => {
                            return Err(ErrorEvent::WorkspaceReadFailed {
                                path: ".agent/PLAN.md".to_string(),
                                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                            }
                            .into());
                        }
                    };
                    ignore_sources_owned.push(plan.clone());
                    Some(plan)
                }
                PromptInputRepresentation::FileReference { .. } => None,
            };
            let diff_inline = match &inputs.diff.representation {
                PromptInputRepresentation::Inline => {
                    // Use fallback if .agent/DIFF.backup is missing.
                    let diff = match ctx.workspace.read(Path::new(".agent/DIFF.backup")) {
                        Ok(diff) => diff,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            Self::fallback_diff_instructions(&baseline_oid_for_prompts)
                        }
                        Err(err) => {
                            return Err(ErrorEvent::WorkspaceReadFailed {
                                path: ".agent/DIFF.backup".to_string(),
                                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                            }
                            .into());
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
        if is_xsd_retry {
            let last_output = match ctx.workspace.read(Path::new(xml_paths::ISSUES_XML)) {
                Ok(output) => output,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    // The canonical file was archived after successful validation or a previous retry.
                    // Try reading from the archived .processed file as a fallback.
                    let processed_path = Path::new(".agent/tmp/issues.xml.processed");
                    match ctx.workspace.read(processed_path) {
                        Ok(output) => {
                            ctx.logger
                                .info("XSD retry: using archived .processed file as last output");
                            output
                        }
                        Err(_) => {
                            ctx.logger.warn(
                                "Missing .agent/tmp/issues.xml and .processed fallback; using empty output for review XSD retry",
                            );
                            String::new()
                        }
                    }
                }
                Err(err) => {
                    return Err(ErrorEvent::WorkspaceReadFailed {
                        path: xml_paths::ISSUES_XML.to_string(),
                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                    }
                    .into());
                }
            };

            let content_id_sha256 = sha256_hex_str(&last_output);
            let consumer_signature_sha256 = self.state.agent_chain.consumer_signature_sha256();
            let inline_budget_bytes = MAX_INLINE_CONTENT_SIZE as u64;
            let last_output_bytes = last_output.len() as u64;

            let already_materialized = self
                .state
                .prompt_inputs
                .xsd_retry_last_output
                .as_ref()
                .is_some_and(|m| {
                    m.phase == crate::reducer::event::PipelinePhase::Review
                        && m.scope_id == pass
                        && m.last_output.content_id_sha256 == content_id_sha256
                        && m.last_output.consumer_signature_sha256 == consumer_signature_sha256
                });

            if !already_materialized {
                let last_output_path = Path::new(".agent/tmp/last_output.xml");
                ctx.workspace
                    .write_atomic(last_output_path, &last_output)
                    .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                        path: last_output_path.display().to_string(),
                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                    })?;

                let input = MaterializedPromptInput {
                    kind: PromptInputKind::LastOutput,
                    content_id_sha256: content_id_sha256.clone(),
                    consumer_signature_sha256,
                    original_bytes: last_output_bytes,
                    final_bytes: last_output_bytes,
                    model_budget_bytes: None,
                    inline_budget_bytes: Some(inline_budget_bytes),
                    representation: PromptInputRepresentation::FileReference {
                        path: last_output_path.to_path_buf(),
                    },
                    reason: PromptMaterializationReason::PolicyForcedReference,
                };
                additional_events.push(PipelineEvent::xsd_retry_last_output_materialized(
                    crate::reducer::event::PipelinePhase::Review,
                    pass,
                    input,
                ));
                if last_output_bytes > inline_budget_bytes {
                    additional_events.push(PipelineEvent::prompt_input_oversize_detected(
                        crate::reducer::event::PipelinePhase::Review,
                        PromptInputKind::LastOutput,
                        content_id_sha256,
                        last_output_bytes,
                        inline_budget_bytes,
                        "xsd-retry-context".to_string(),
                    ));
                }
            }
        }
        let mut xsd_error_for_validation: Option<String> = None;
        let (prompt_key, review_prompt_xml, was_replayed, template_name, should_validate) =
            match prompt_mode {
                PromptMode::XsdRetry => {
                    let prompt_key = format!(
                        "review_{pass}_xsd_retry_{}",
                        continuation_state.invalid_output_attempts
                    );
                    // Use the actual XSD error from state, or fall back to generic message
                    let xsd_error = continuation_state
                        .last_review_xsd_error
                        .as_deref()
                        .unwrap_or("XML output failed validation. Provide valid XML output.");
                    xsd_error_for_validation = Some(xsd_error.to_string());
                    let prompt = prompt_review_xsd_retry_with_context_files(
                        ctx.template_context,
                        xsd_error,
                        ctx.workspace,
                    );
                    // XSD retry prompts must not replay potentially stale prompt history content.
                    (prompt_key, prompt, false, "review_xsd_retry", true)
                }
                PromptMode::SameAgentRetry => {
                    // Same-agent retry: prepend retry guidance to the last prepared prompt for this
                    // phase (preserves XSD retry / normal context if present).
                    let retry_preamble =
                        crate::reducer::handler::retry_guidance::same_agent_retry_preamble(
                            continuation_state,
                        );
                    let (base_prompt, should_validate) =
                    match ctx.workspace.read(Path::new(".agent/tmp/review_prompt.txt")) {
                        Ok(previous_prompt) => (
                            crate::reducer::handler::retry_guidance::strip_existing_same_agent_retry_preamble(&previous_prompt)
                                .to_string(),
                            false,
                        ),
                        Err(_) => {
                            let inputs = match materialized_inputs {
                                Some(inputs) => inputs,
                                None => {
                                    return Err(ErrorEvent::ReviewInputsNotMaterialized { pass }.into());
                                }
                            };
                            let plan_ref = match &inputs.plan.representation {
                                PromptInputRepresentation::Inline => {
                                    let plan_inline = plan_inline.clone().unwrap_or_else(||
                                        Self::sentinel_plan_content(ctx.config.isolation_mode)
                                    );
                                    PlanContentReference::Inline(plan_inline)
                                }
                                PromptInputRepresentation::FileReference { path } => {
                                    PlanContentReference::ReadFromFile {
                                        primary_path: path.to_path_buf(),
                                        fallback_path: Some(Path::new(".agent/tmp/plan.xml").to_path_buf()),
                                        description: format!(
                                            "Plan is {} bytes (exceeds {} limit)",
                                            inputs.plan.final_bytes, MAX_INLINE_CONTENT_SIZE
                                        ),
                                    }
                                }
                            };
                            let diff_ref = match &inputs.diff.representation {
                                PromptInputRepresentation::Inline => {
                                    let diff_inline = diff_inline.clone().unwrap_or_else(||
                                        Self::fallback_diff_instructions(&baseline_oid_for_prompts)
                                    );
                                    DiffContentReference::Inline(diff_inline)
                                }
                                PromptInputRepresentation::FileReference { path } => {
                                    DiffContentReference::ReadFromFile {
                                        path: path.to_path_buf(),
                                        start_commit: baseline_oid_for_prompts.clone(),
                                        description: format!(
                                            "Diff is {} bytes (exceeds {} limit)",
                                            inputs.diff.final_bytes, MAX_INLINE_CONTENT_SIZE
                                        ),
                                    }
                                }
                            };

                            let refs = PromptContentReferences {
                                prompt: None,
                                plan: Some(plan_ref),
                                diff: Some(diff_ref),
                            };
                            (
                                prompt_review_xml_with_references(ctx.template_context, &refs, ctx.workspace),
                                true,
                            )
                        }
                    };
                    let prompt = format!("{retry_preamble}\n{base_prompt}");
                    let prompt_key = format!(
                        "review_{pass}_same_agent_retry_{}",
                        continuation_state.same_agent_retry_count
                    );
                    (prompt_key, prompt, false, "review_xml", should_validate)
                }
                PromptMode::Normal => {
                    let inputs = match materialized_inputs {
                        Some(inputs) => inputs,
                        None => {
                            return Err(ErrorEvent::ReviewInputsNotMaterialized { pass }.into());
                        }
                    };
                    let prompt_key = format!("review_{pass}");
                    let plan_ref = match &inputs.plan.representation {
                        PromptInputRepresentation::Inline => {
                            let plan_inline = plan_inline.clone().unwrap_or_else(|| {
                                Self::sentinel_plan_content(ctx.config.isolation_mode)
                            });
                            PlanContentReference::Inline(plan_inline)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            PlanContentReference::ReadFromFile {
                                primary_path: path.to_path_buf(),
                                fallback_path: Some(Path::new(".agent/tmp/plan.xml").to_path_buf()),
                                description: format!(
                                    "Plan is {} bytes (exceeds {} limit)",
                                    inputs.plan.final_bytes, MAX_INLINE_CONTENT_SIZE
                                ),
                            }
                        }
                    };
                    let diff_ref = match &inputs.diff.representation {
                        PromptInputRepresentation::Inline => {
                            let diff_inline = diff_inline.clone().unwrap_or_else(|| {
                                Self::fallback_diff_instructions(&baseline_oid_for_prompts)
                            });
                            DiffContentReference::Inline(diff_inline)
                        }
                        PromptInputRepresentation::FileReference { path } => {
                            DiffContentReference::ReadFromFile {
                                path: path.to_path_buf(),
                                start_commit: baseline_oid_for_prompts.clone(),
                                description: format!(
                                    "Diff is {} bytes (exceeds {} limit)",
                                    inputs.diff.final_bytes, MAX_INLINE_CONTENT_SIZE
                                ),
                            }
                        }
                    };
                    let (prompt, was_replayed) =
                        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                            let plan_ref = plan_ref.clone();
                            let diff_ref = diff_ref.clone();

                            let refs = PromptContentReferences {
                                prompt: None,
                                plan: Some(plan_ref),
                                diff: Some(diff_ref),
                            };
                            prompt_review_xml_with_references(
                                ctx.template_context,
                                &refs,
                                ctx.workspace,
                            )
                        });
                    (prompt_key, prompt, was_replayed, "review_xml", true)
                }
                PromptMode::Continuation => {
                    return Err(ErrorEvent::ReviewContinuationNotSupported.into());
                }
            };
        if let Some(xsd_error) = xsd_error_for_validation {
            ignore_sources_owned.push(xsd_error);
        }
        let ignore_sources: Vec<&str> = ignore_sources_owned.iter().map(|s| s.as_str()).collect();
        if should_validate {
            if let Err(err) =
                crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
                    &review_prompt_xml,
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
            ctx.capture_prompt(&prompt_key, &review_prompt_xml);
        }

        // Write prompt file (non-fatal: if write fails, log warning and continue)
        // Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
        // If the prompt file write fails, we continue with orchestration - loop recovery will
        // handle convergence if needed.
        if let Err(err) = ctx.workspace.write(
            Path::new(".agent/tmp/review_prompt.txt"),
            &review_prompt_xml,
        ) {
            ctx.logger.warn(&format!(
                "Failed to write review prompt file: {}. Pipeline will continue (loop recovery will handle convergence).",
                err
            ));
        }

        let mut result = EffectResult::event(PipelineEvent::review_prompt_prepared(pass));
        for ev in additional_events {
            result = result.with_additional_event(ev);
        }
        Ok(result)
    }

    pub(in crate::reducer::handler) fn invoke_review_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use std::path::Path;

        // Normalize agent chain state before invocation for determinism
        self.normalize_agent_chain_for_invocation(ctx, AgentRole::Reviewer);

        let prompt = match ctx
            .workspace
            .read(Path::new(".agent/tmp/review_prompt.txt"))
        {
            Ok(s) => s,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ErrorEvent::ReviewPromptMissing { pass }.into());
            }
            Err(err) => {
                return Err(ErrorEvent::WorkspaceReadFailed {
                    path: ".agent/tmp/review_prompt.txt".to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
                .into());
            }
        };

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
            result = result.with_additional_event(PipelineEvent::review_agent_invoked(pass));
        }
        Ok(result)
    }
}
