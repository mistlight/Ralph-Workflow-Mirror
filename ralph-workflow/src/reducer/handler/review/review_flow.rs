impl MainEffectHandler {
    /// Sentinel content for missing PLAN during review phase.
    ///
    /// This is used when `.agent/PLAN.md` is missing, which can happen in isolation mode
    /// (developer_iters=0, reviewer_reviews>0) where no planning occurred.
    fn sentinel_plan_content(isolation_mode: bool) -> String {
        if isolation_mode {
            "No PLAN provided (normal in isolation mode)".to_string()
        } else {
            "No PLAN provided".to_string()
        }
    }

    /// Fallback diff instructions when `.agent/DIFF.backup` is missing.
    ///
    /// These instructions tell the reviewer how to obtain the diff via git commands.
    fn fallback_diff_instructions(baseline_oid: &str) -> String {
        if !baseline_oid.is_empty() {
            format!(
                "[DIFF NOT AVAILABLE - Use git to obtain changes]\n\n\
                 1) Committed changes since baseline:\n\
                    git diff {baseline_oid}..HEAD\n\n\
                 2) Include staged + unstaged working tree changes vs baseline:\n\
                    git diff {baseline_oid}\n\n\
                 3) Staged-only changes vs baseline:\n\
                    git diff --cached {baseline_oid}\n\n\
                 4) Untracked files (not shown by git diff):\n\
                    git ls-files --others --exclude-standard\n\n\
                 Review the full change set (committed + working tree + untracked).",
                baseline_oid = baseline_oid
            )
        } else {
            "[DIFF NOT AVAILABLE - Use git to obtain changes]\n\n\
             Run: git diff HEAD~1..HEAD  # Changes in last commit\n\
             Or:  git diff --staged      # Staged changes\n\
             Or:  git diff               # Unstaged changes\n\
             And: git ls-files --others --exclude-standard  # Untracked files\n\n\
             Review the diff and identify any issues."
                .to_string()
        }
    }

    pub(super) fn materialize_review_inputs(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        // PLAN is optional for review phase (e.g., isolation mode without planning).
        // Use sentinel content when missing and write it to PLAN.md.
        let plan_content = match ctx.workspace.read(Path::new(".agent/PLAN.md")) {
            Ok(plan_content) => plan_content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                ctx.logger
                    .warn("Missing .agent/PLAN.md; using sentinel PLAN content for review");
                let sentinel = Self::sentinel_plan_content(ctx.config.isolation_mode);
                // Write sentinel content to PLAN.md so FileReference representation works
                ctx.workspace
                    .write(Path::new(".agent/PLAN.md"), &sentinel)
                    .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                        path: ".agent/PLAN.md".to_string(),
                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                    })?;
                sentinel
            }
            Err(err) => {
                return Err(ErrorEvent::WorkspaceReadFailed {
                    path: ".agent/PLAN.md".to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
                .into());
            }
        };

        // DIFF is optional for review phase. Use fallback git instructions when missing.
        let baseline_oid = match ctx.workspace.read(Path::new(Self::DIFF_BASELINE_PATH)) {
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

        let diff_content = match ctx.workspace.read(Path::new(".agent/DIFF.backup")) {
            Ok(diff_content) => diff_content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                ctx.logger
                    .warn("Missing .agent/DIFF.backup; providing git diff fallback instructions");
                Self::fallback_diff_instructions(&baseline_oid)
            }
            Err(err) => {
                return Err(ErrorEvent::WorkspaceReadFailed {
                    path: ".agent/DIFF.backup".to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                }
                .into());
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
                plan_path.display()
            ));
            (
                PromptInputRepresentation::FileReference {
                    path: plan_path.to_path_buf(),
                },
                PromptMaterializationReason::InlineBudgetExceeded,
            )
        } else {
            (
                PromptInputRepresentation::Inline,
                PromptMaterializationReason::WithinBudgets,
            )
        };

        let diff_path = Path::new(".agent/tmp/diff.txt");
        let (diff_representation, diff_reason) = if diff_content.len() as u64 > inline_budget_bytes
        {
            let tmp_dir = Path::new(".agent/tmp");
            if !ctx.workspace.exists(tmp_dir) {
                ctx.workspace.create_dir_all(tmp_dir).map_err(|err| {
                    ErrorEvent::WorkspaceCreateDirAllFailed {
                        path: tmp_dir.display().to_string(),
                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                    }
                })?;
            }
            ctx.workspace
                .write_atomic(Path::new(".agent/tmp/diff.txt"), &diff_content)
                .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                    path: ".agent/tmp/diff.txt".to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                })?;
            ctx.logger.warn(&format!(
                "DIFF size ({} KB) exceeds inline limit ({} KB). Referencing: {}",
                (diff_content.len() as u64) / 1024,
                inline_budget_bytes / 1024,
                diff_path.display()
            ));
            (
                PromptInputRepresentation::FileReference {
                    path: diff_path.to_path_buf(),
                },
                PromptMaterializationReason::InlineBudgetExceeded,
            )
        } else {
            (
                PromptInputRepresentation::Inline,
                PromptMaterializationReason::WithinBudgets,
            )
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

        let mut result = EffectResult::event(PipelineEvent::review_inputs_materialized(
            pass,
            plan_input.clone(),
            diff_input.clone(),
        ));
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

        match create_prompt_backup_with_workspace(ctx.workspace) {
            Ok(Some(warning)) => {
                ctx.logger
                    .warn(&format!("PROMPT.md backup created with warning: {warning}"));
            }
            Ok(None) => {}
            Err(err) => {
                ctx.logger
                    .warn(&format!("Failed to create PROMPT.md backup: {err}"));
            }
        }

        let (diff, baseline_oid) =
            match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
                Ok((diff, baseline_oid)) => (diff, baseline_oid),
                Err(err) => {
                    ctx.logger
                        .warn(&format!("Failed to compute review diff: {err}"));
                    (String::new(), String::new())
                }
            };
        if let Err(err) = write_diff_backup_with_workspace(ctx.workspace, &diff) {
            ctx.logger
                .warn(&format!("Failed to write .agent/DIFF.backup: {err}"));
        }

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
            let last_output = ctx
                .workspace
                .read(Path::new(xml_paths::ISSUES_XML))
                .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                    path: xml_paths::ISSUES_XML.to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                })?;

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
        let (prompt_key, review_prompt_xml, was_replayed, template_name, should_validate) =
            match prompt_mode {
                PromptMode::XsdRetry => {
                    let prompt_key = format!(
                        "review_{pass}_xsd_retry_{}",
                        continuation_state.invalid_output_attempts
                    );
                    let prompt = prompt_review_xsd_retry_with_context_files(
                        ctx.template_context,
                        "XML output failed validation. Provide valid XML output.",
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
                                prompt_review_xml_with_references(ctx.template_context, &refs),
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
                            prompt_review_xml_with_references(ctx.template_context, &refs)
                        });
                    (prompt_key, prompt, was_replayed, "review_xml", true)
                }
                PromptMode::Continuation => {
                    return Err(ErrorEvent::ReviewContinuationNotSupported.into());
                }
            };
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

        ctx.workspace
            .write(
                Path::new(".agent/tmp/review_prompt.txt"),
                &review_prompt_xml,
            )
            .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                path: ".agent/tmp/review_prompt.txt".to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;

        let mut result = EffectResult::event(PipelineEvent::review_prompt_prepared(pass));
        for ev in additional_events {
            result = result.with_additional_event(ev);
        }
        Ok(result)
    }

    pub(super) fn invoke_review_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use std::path::Path;

        let prompt = ctx
            .workspace
            .read(Path::new(".agent/tmp/review_prompt.txt"))
            .map_err(|_| ErrorEvent::ReviewPromptMissing { pass })?;

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

        let outcome = self
            .state
            .review_validated_outcome
            .as_ref()
            .filter(|outcome| outcome.pass == pass)
            .ok_or(ErrorEvent::ValidatedReviewOutcomeMissing { pass })?;

        let elements = crate::files::llm_output_extraction::IssuesElements {
            issues: outcome.issues.clone(),
            no_issues_found: outcome.no_issues_found.clone(),
        };
        let markdown = render_issues_markdown(&elements);
        ctx.workspace
            .write(Path::new(".agent/ISSUES.md"), &markdown)
            .map_err(|err| ErrorEvent::WorkspaceWriteFailed {
                path: ".agent/ISSUES.md".to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;

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

        let outcome = self
            .state
            .review_validated_outcome
            .as_ref()
            .filter(|outcome| outcome.pass == pass)
            .ok_or(ErrorEvent::ValidatedReviewOutcomeMissing { pass })?;

        let issues_xml = ctx
            .workspace
            .read(Path::new(xml_paths::ISSUES_XML))
            .unwrap_or_default();

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
