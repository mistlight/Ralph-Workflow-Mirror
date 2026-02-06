/// Result of commit message generation.
pub struct CommitMessageResult {
    /// The generated commit message
    pub message: String,
    /// Whether the generation was successful
    pub success: bool,
    /// Path to the agent log file for debugging (currently unused)
    pub _log_path: String,
    /// Prompts that were generated during this commit generation (key -> prompt)
    pub generated_prompts: HashMap<String, String>,
}

/// Outcome from a single commit attempt.
pub struct CommitAttemptResult {
    pub had_error: bool,
    pub output_valid: bool,
    pub message: Option<String>,
    pub validation_detail: String,
    pub auth_failure: bool,
}

/// Run a single commit generation attempt with explicit agent and prompt.
///
/// This does **not** perform in-session XSD retries. If validation fails, the
/// caller should emit a MessageValidationFailed event and let the reducer decide
/// retry/fallback behavior.
///
/// **IMPORTANT:** The `model_safe_diff` parameter must be pre-truncated to the
/// effective model budget. Use the reducer's `MaterializeCommitInputs` effect
/// to truncate the diff before calling this function. The reducer writes the
/// model-safe diff to `.agent/tmp/commit_diff.model_safe.txt`.
pub fn run_commit_attempt(
    ctx: &mut PhaseContext<'_>,
    attempt: u32,
    model_safe_diff: &str,
    commit_agent: &str,
) -> anyhow::Result<CommitAttemptResult> {
    // NOTE: Truncation is now handled by materialize_commit_inputs in the reducer.
    // The diff passed here is already truncated to the effective model budget.
    // See: reducer/handler/commit.rs::materialize_commit_inputs

    let prompt_key = format!("commit_message_attempt_{attempt}");
    let (prompt, was_replayed) = build_commit_prompt(
        &prompt_key,
        ctx.template_context,
        model_safe_diff,
        ctx.workspace,
        &ctx.prompt_history,
    );

    // Enforce that the rendered prompt does not contain unresolved template placeholders.
    // This must happen before any agent invocation.
    if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
        &prompt,
        &[model_safe_diff],
    ) {
        return Err(crate::prompts::TemplateVariablesInvalidError {
            template_name: "commit_message_xml".to_string(),
            missing_variables: Vec::new(),
            unresolved_placeholders: err.unresolved_placeholders,
        }
        .into());
    }

    if !was_replayed {
        ctx.capture_prompt(&prompt_key, &prompt);
    }

    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        executor: ctx.executor,
        executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
        workspace: ctx.workspace,
    };

    let log_dir = Path::new(".agent/logs/commit_generation");
    let mut session = CommitLogSession::new(log_dir.to_str().unwrap(), ctx.workspace)
        .unwrap_or_else(|_| CommitLogSession::noop());
    let mut attempt_log = session.new_attempt(commit_agent, "single");
    attempt_log.set_prompt_size(prompt.len());
    // The diff passed here is already model-safe. However, for accurate debugging we still want
    // to record whether truncation happened upstream. We infer truncation from the marker text
    // emitted by `truncate_diff_to_model_budget`.
    let diff_was_truncated =
        model_safe_diff.contains("[Truncated:") || model_safe_diff.contains("[truncated...]");
    attempt_log.set_diff_info(model_safe_diff.len(), diff_was_truncated);

    let agent_config = ctx
        .registry
        .resolve_config(commit_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", commit_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    // Use per-run log directory with simplified naming
    let base_log_path = ctx.run_log_context.agent_log("commit", attempt, None);
    let log_attempt = crate::pipeline::logfile::next_simplified_logfile_attempt_index(
        &base_log_path,
        ctx.workspace,
    );
    let logfile = if log_attempt == 0 {
        base_log_path.to_str().unwrap().to_string()
    } else {
        ctx.run_log_context
            .agent_log("commit", attempt, Some(log_attempt))
            .to_str()
            .unwrap()
            .to_string()
    };

    // Write log file header with agent metadata
    // Use append_bytes to avoid overwriting if file exists (defense-in-depth)
    let log_header = format!(
        "# Ralph Agent Invocation Log\n\
         # Role: Commit\n\
         # Agent: {}\n\
         # Model Index: 0\n\
         # Attempt: {}\n\
         # Phase: CommitMessage\n\
         # Timestamp: {}\n\n",
        commit_agent,
        log_attempt,
        chrono::Utc::now().to_rfc3339()
    );
    if let Err(e) = ctx
        .workspace
        .append_bytes(std::path::Path::new(&logfile), log_header.as_bytes())
    {
        ctx.logger
            .warn(&format!("Failed to write agent log header: {}", e));
    }

    let log_prefix = format!("commit_{attempt}"); // For attribution only
    let model_index = 0usize; // Default model index for attribution
    let prompt_cmd = PromptCommand {
        label: commit_agent,
        display_name: commit_agent,
        cmd_str: &cmd_str,
        prompt: &prompt,
        log_prefix: &log_prefix,
        model_index: Some(model_index),
        attempt: Some(log_attempt),
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    let had_error = result.exit_code != 0;
    let auth_failure = had_error && stderr_contains_auth_error(&result.stderr);
    attempt_log.set_raw_output(&result.stderr);

    if auth_failure {
        attempt_log.set_outcome(AttemptOutcome::ExtractionFailed(
            "Authentication error detected".to_string(),
        ));
        if !session.is_noop() {
            let _ = attempt_log.write_to_workspace(session.run_dir(), ctx.workspace);
            let _ = session.write_summary(1, "AUTHENTICATION_FAILURE", ctx.workspace);
        }
        return Ok(CommitAttemptResult {
            had_error,
            output_valid: false,
            message: None,
            validation_detail: "Authentication error detected".to_string(),
            auth_failure: true,
        });
    }

    let extraction = extract_commit_message_from_file_with_workspace(ctx.workspace);
    let (outcome, detail, extraction_result) = match extraction {
        CommitExtractionOutcome::Valid(result) => (
            AttemptOutcome::Success(result.clone().into_message()),
            "Valid commit message extracted".to_string(),
            Some(result),
        ),
        CommitExtractionOutcome::InvalidXml(detail) => (
            AttemptOutcome::XsdValidationFailed(detail.clone()),
            detail,
            None,
        ),
        CommitExtractionOutcome::MissingFile(detail) => (
            AttemptOutcome::ExtractionFailed(detail.clone()),
            detail,
            None,
        ),
    };
    attempt_log.add_extraction_attempt(match &extraction_result {
        Some(_) => ExtractionAttempt::success("XML", detail.clone()),
        None => ExtractionAttempt::failure("XML", detail.clone()),
    });
    attempt_log.set_outcome(outcome.clone());

    if !session.is_noop() {
        let _ = attempt_log.write_to_workspace(session.run_dir(), ctx.workspace);
        let final_outcome = format!("{outcome}");
        let _ = session.write_summary(1, &final_outcome, ctx.workspace);
    }

    if let Some(result) = extraction_result {
        let message = result.into_message();
        return Ok(CommitAttemptResult {
            had_error,
            output_valid: true,
            message: Some(message),
            validation_detail: detail,
            auth_failure: false,
        });
    }

    Ok(CommitAttemptResult {
        had_error,
        output_valid: false,
        message: None,
        validation_detail: detail,
        auth_failure: false,
    })
}

/// Generate a commit message using a single agent attempt.
///
/// Returns an error if XML validation fails or the agent output is missing.
///
/// # Truncation Behavior (CLI vs Reducer)
///
/// **IMPORTANT:** This function uses **single-agent budget** for truncation, which
/// differs from the reducer-driven path that uses **chain-minimum budget**.
///
/// | Path | Budget Calculation | When Used |
/// |------|-------------------|-----------|
/// | CLI (`--generate-commit-msg`) | `model_budget_bytes_for_agent_name(agent)` | Single agent, no fallback chain |
/// | Reducer (`MaterializeCommitInputs`) | `effective_model_budget_bytes(&agents)` | Agent chain with potential fallbacks |
///
/// **Why this is acceptable:**
/// - CLI plumbing commands (`--generate-commit-msg`) invoke a single, explicitly-specified
///   agent with no fallback chain. There's no need to compute min budget across agents.
/// - The reducer path handles multi-agent chains where the diff must fit the smallest
///   agent's context window to ensure fallback attempts can succeed.
///
/// **Implication:** A diff that works via CLI might fail via reducer if the chain
/// includes an agent with a smaller budget. This is by design - the CLI user
/// explicitly chose the agent and accepts its budget constraints.
pub fn generate_commit_message(
    diff: &str,
    registry: &AgentRegistry,
    runtime: &mut PipelineRuntime,
    commit_agent: &str,
    template_context: &TemplateContext,
    workspace: &dyn Workspace,
    prompt_history: &HashMap<String, String>,
) -> anyhow::Result<CommitMessageResult> {
    // For CLI plumbing, we truncate to the single agent's budget.
    // This is different from the reducer path which uses min budget across the chain.
    let model_budget = model_budget_bytes_for_agent_name(commit_agent);
    let (model_safe_diff, truncated) = truncate_diff_to_model_budget(diff, model_budget);
    if truncated {
        runtime.logger.warn(&format!(
            "Diff size ({} KB) exceeds agent limit ({} KB). Truncated to {} KB.",
            diff.len() / 1024,
            model_budget / 1024,
            model_safe_diff.len() / 1024
        ));
    }

    let prompt_key = "commit_message_attempt_1";
    let (prompt, was_replayed) = build_commit_prompt(
        prompt_key,
        template_context,
        &model_safe_diff,
        workspace,
        prompt_history,
    );

    let mut generated_prompts = HashMap::new();
    if !was_replayed {
        generated_prompts.insert(prompt_key.to_string(), prompt.clone());
    }

    let agent_config = registry
        .resolve_config(commit_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", commit_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    let log_prefix = ".agent/logs/commit_generation/commit_generation";
    let model_index = 0usize;
    let attempt = 1u32;
    let agent_for_log = commit_agent.to_lowercase();
    let logfile = crate::pipeline::logfile::build_logfile_path_with_attempt(
        log_prefix,
        &agent_for_log,
        model_index,
        attempt,
    );
    let prompt_cmd = PromptCommand {
        label: commit_agent,
        display_name: commit_agent,
        cmd_str: &cmd_str,
        prompt: &prompt,
        log_prefix,
        model_index: Some(model_index),
        attempt: Some(attempt),
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, runtime)?;
    let had_error = result.exit_code != 0;
    let auth_failure = had_error && stderr_contains_auth_error(&result.stderr);
    if auth_failure {
        anyhow::bail!("Authentication error detected");
    }

    let extraction = extract_commit_message_from_file_with_workspace(workspace);
    let result = match extraction {
        CommitExtractionOutcome::Valid(result) => result,
        CommitExtractionOutcome::InvalidXml(detail)
        | CommitExtractionOutcome::MissingFile(detail) => anyhow::bail!(detail),
    };

    archive_xml_file_with_workspace(workspace, Path::new(xml_paths::COMMIT_MESSAGE_XML));

    Ok(CommitMessageResult {
        message: result.into_message(),
        success: true,
        _log_path: String::new(),
        generated_prompts,
    })
}
