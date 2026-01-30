//! Command execution helpers and fallback orchestration.

use crate::agents::{validate_model_flag, AgentConfig, AgentRegistry, AgentRole, JsonParserType};
use crate::common::split_command;
use std::path::Path;
use std::sync::Arc;

use super::fallback::try_agent_with_retries;
use super::fallback::TryAgentResult;
use super::model_flag::resolve_model_with_provider;
use super::prompt::PipelineRuntime;

/// Build the list of agents to try and log the fallback chain.
fn build_agents_to_try<'a>(fallbacks: &'a [&'a str], primary_agent: &'a str) -> Vec<&'a str> {
    let mut agents_to_try: Vec<&'a str> = vec![primary_agent];
    for fb in fallbacks {
        if *fb != primary_agent && !agents_to_try.contains(fb) {
            agents_to_try.push(fb);
        }
    }
    agents_to_try
}

/// Get CLI model/provider overrides based on role.
fn get_cli_overrides(
    role: AgentRole,
    runtime: &PipelineRuntime<'_>,
) -> (Option<String>, Option<String>) {
    match role {
        AgentRole::Developer => (
            runtime.config.developer_model.clone(),
            runtime.config.developer_provider.clone(),
        ),
        AgentRole::Reviewer => (
            runtime.config.reviewer_model.clone(),
            runtime.config.reviewer_provider.clone(),
        ),
        AgentRole::Commit => (None, None), // Commit role doesn't have CLI overrides
    }
}

/// Context for building model flags.
struct ModelFlagBuildContext<'a> {
    agent_index: usize,
    cli_model_override: Option<&'a String>,
    cli_provider_override: Option<&'a String>,
    agent_config: &'a AgentConfig,
    agent_name: &'a str,
    fallback_config: &'a crate::agents::fallback::FallbackConfig,
    display_name: &'a str,
    runtime: &'a PipelineRuntime<'a>,
}

/// Build the list of model flags to try for an agent.
fn build_model_flags_list(ctx: &ModelFlagBuildContext<'_>) -> Vec<Option<String>> {
    let mut model_flags_to_try: Vec<Option<String>> = Vec::new();

    // CLI override takes highest priority for primary agent
    // Provider override can modify the model's provider prefix
    if ctx.agent_index == 0
        && (ctx.cli_model_override.is_some() || ctx.cli_provider_override.is_some())
    {
        let resolved = resolve_model_with_provider(
            ctx.cli_provider_override.map(std::string::String::as_str),
            ctx.cli_model_override.map(std::string::String::as_str),
            ctx.agent_config.model_flag.as_deref(),
        );
        if resolved.is_some() {
            model_flags_to_try.push(resolved);
        }
    }

    // Add the agent's default model (None means use agent's configured model_flag or no model)
    if model_flags_to_try.is_empty() {
        model_flags_to_try.push(None);
    }

    // Add provider fallback models for this agent
    if ctx.fallback_config.has_provider_fallbacks(ctx.agent_name) {
        let provider_fallbacks = ctx.fallback_config.get_provider_fallbacks(ctx.agent_name);
        ctx.runtime.logger.info(&format!(
            "Agent '{}' has {} provider fallback(s) configured",
            ctx.display_name,
            provider_fallbacks.len()
        ));
        for model in provider_fallbacks {
            model_flags_to_try.push(Some(model.clone()));
        }
    }

    model_flags_to_try
}

/// Build the command string for a specific model configuration.
fn build_command_for_model(ctx: &TryModelContext<'_>, runtime: &PipelineRuntime<'_>) -> String {
    let model_ref = ctx.model_flag.map(std::string::String::as_str);
    // Enable yolo for ALL roles - this is an automated pipeline, not interactive.
    // All agents need file write access to output their XML results.
    let yolo = true;

    if ctx.agent_index == 0 && ctx.cycle == 0 && ctx.model_index == 0 {
        // For primary agent on first cycle, respect env var command overrides
        match ctx.role {
            AgentRole::Developer => runtime.config.developer_cmd.clone().unwrap_or_else(|| {
                ctx.agent_config
                    .build_cmd_with_model(true, true, true, model_ref)
            }),
            AgentRole::Reviewer => runtime.config.reviewer_cmd.clone().unwrap_or_else(|| {
                ctx.agent_config
                    .build_cmd_with_model(true, true, yolo, model_ref)
            }),
            AgentRole::Commit => runtime.config.commit_cmd.clone().unwrap_or_else(|| {
                ctx.agent_config
                    .build_cmd_with_model(true, true, yolo, model_ref)
            }),
        }
    } else {
        ctx.agent_config
            .build_cmd_with_model(true, true, yolo, model_ref)
    }
}

/// GLM-specific validation for print flag.
///
/// This validation only applies to CCS/Claude-based GLM agents that use the `-p` flag
/// for non-interactive mode. OpenCode agents are excluded because they use
/// `--auto-approve` for non-interactive mode instead.
fn validate_glm_print_flag(
    agent_name: &str,
    agent_config: &AgentConfig,
    cmd_str: &str,
    agent_index: usize,
    cycle: u32,
    model_index: usize,
    runtime: &PipelineRuntime<'_>,
) {
    // Skip validation for non-CCS/Claude GLM agents
    // is_glm_like_agent only matches CCS/Claude-based GLM agents, not OpenCode
    if !crate::agents::is_glm_like_agent(agent_name)
        || agent_index != 0
        || cycle != 0
        || model_index != 0
    {
        return;
    }

    let cmd_argv = split_command(cmd_str).ok();
    let has_print_flag = cmd_argv
        .as_ref()
        .is_some_and(|argv| argv.iter().any(|arg| arg == "-p"));
    if !has_print_flag {
        if agent_config.print_flag.is_empty() {
            runtime.logger.warn(&format!(
                "GLM agent '{agent_name}' is missing '-p' flag: print_flag is empty in configuration. \
                 Add 'print_flag = \"-p\"' to [ccs] section in ~/.config/ralph-workflow.toml"
            ));
        } else {
            runtime.logger.warn(&format!(
                "GLM agent '{agent_name}' may be missing '-p' flag in command. Check configuration."
            ));
        }
    }
}

/// Build label and logfile paths for execution.
fn build_execution_metadata(
    model_flag: Option<&String>,
    display_name: &str,
    base_label: &str,
    agent_name: &str,
    logfile_prefix: &str,
    model_index: usize,
) -> (String, String, String) {
    let model_suffix = model_flag.map(|m| format!(" [{m}]")).unwrap_or_default();
    let display_name_with_suffix = format!("{display_name}{model_suffix}");
    let label = format!("{base_label} ({display_name_with_suffix})");
    let logfile = super::logfile::build_logfile_path(logfile_prefix, agent_name, model_index);
    (label, logfile, display_name_with_suffix)
}

/// Result of trying a single agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrySingleAgentResult {
    /// Agent succeeded - return success
    Success,
    /// Unrecoverable error - abort immediately
    Unrecoverable(i32),
    /// Should fall back to next agent
    Fallback,
    /// Continue to next model (no retry)
    NoRetry,
}

/// Context for trying a single model.
struct TryModelContext<'a> {
    agent_config: &'a AgentConfig,
    agent_name: &'a str,
    display_name: &'a str,
    agent_index: usize,
    cycle: u32,
    model_index: usize,
    role: AgentRole,
    model_flag: Option<&'a String>,
    base_label: &'a str,
    prompt: &'a str,
    logfile_prefix: &'a str,
    fallback_config: &'a crate::agents::fallback::FallbackConfig,
    output_validator: Option<crate::pipeline::fallback::OutputValidator>,
    retry_timer: Arc<dyn crate::agents::RetryTimerProvider>,
    workspace: &'a dyn crate::workspace::Workspace,
}

/// Try a single model configuration for an agent.
fn try_single_model(
    ctx: &TryModelContext<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> std::io::Result<TrySingleAgentResult> {
    let mut parser_type = ctx.agent_config.json_parser;

    if ctx.role == AgentRole::Reviewer {
        if let Some(ref parser_override) = runtime.config.reviewer_json_parser {
            parser_type = JsonParserType::parse(parser_override);
            if ctx.agent_index == 0 && ctx.cycle == 0 && ctx.model_index == 0 {
                runtime.logger.info(&format!(
                    "Using JSON parser override '{parser_override}' for reviewer"
                ));
            }
        }
    }

    let cmd_str = build_command_for_model(ctx, runtime);

    validate_glm_print_flag(
        ctx.agent_name,
        ctx.agent_config,
        &cmd_str,
        ctx.agent_index,
        ctx.cycle,
        ctx.model_index,
        runtime,
    );

    let (label, logfile, display_name_with_suffix) = build_execution_metadata(
        ctx.model_flag,
        ctx.display_name,
        ctx.base_label,
        ctx.agent_name,
        ctx.logfile_prefix,
        ctx.model_index,
    );

    let attempt_config = crate::pipeline::fallback::AgentAttemptConfig {
        agent_name: ctx.agent_name,
        model_flag: ctx.model_flag.map(std::string::String::as_str),
        label: &label,
        display_name: &display_name_with_suffix,
        cmd_str: &cmd_str,
        prompt: ctx.prompt,
        logfile: &logfile,
        logfile_prefix: ctx.logfile_prefix,
        parser_type,
        env_vars: &ctx.agent_config.env_vars,
        model_index: ctx.model_index,
        agent_index: ctx.agent_index,
        cycle: ctx.cycle as usize,
        fallback_config: ctx.fallback_config,
        output_validator: ctx.output_validator,
        retry_timer: Arc::clone(&ctx.retry_timer),
        workspace: ctx.workspace,
    };
    let result = try_agent_with_retries(&attempt_config, runtime)?;

    match result {
        TryAgentResult::Success => Ok(TrySingleAgentResult::Success),
        TryAgentResult::Unrecoverable(exit_code) => {
            Ok(TrySingleAgentResult::Unrecoverable(exit_code))
        }
        TryAgentResult::Fallback => Ok(TrySingleAgentResult::Fallback),
        TryAgentResult::NoRetry => Ok(TrySingleAgentResult::NoRetry),
    }
}

/// Context for trying a single agent.
struct TryAgentContext<'a> {
    agent_name: &'a str,
    agent_index: usize,
    cycle: u32,
    role: AgentRole,
    base_label: &'a str,
    prompt: &'a str,
    logfile_prefix: &'a str,
    cli_model_override: Option<&'a String>,
    cli_provider_override: Option<&'a String>,
    output_validator: Option<crate::pipeline::fallback::OutputValidator>,
    retry_timer: Arc<dyn crate::agents::RetryTimerProvider>,
    workspace: &'a dyn crate::workspace::Workspace,
}

/// Try a single agent with all its model configurations.
fn try_single_agent(
    ctx: &TryAgentContext<'_>,
    runtime: &mut PipelineRuntime<'_>,
    registry: &AgentRegistry,
    fallback_config: &crate::agents::fallback::FallbackConfig,
) -> std::io::Result<TrySingleAgentResult> {
    let Some(agent_config) = registry.resolve_config(ctx.agent_name) else {
        runtime.logger.warn(&format!(
            "Agent '{}' not found in registry, skipping",
            ctx.agent_name
        ));
        return Ok(TrySingleAgentResult::Fallback);
    };

    let display_name = registry.display_name(ctx.agent_name);
    let model_ctx = ModelFlagBuildContext {
        agent_index: ctx.agent_index,
        cli_model_override: ctx.cli_model_override,
        cli_provider_override: ctx.cli_provider_override,
        agent_config: &agent_config,
        agent_name: ctx.agent_name,
        fallback_config,
        display_name: &display_name,
        runtime,
    };
    let model_flags_to_try = build_model_flags_list(&model_ctx);

    if ctx.agent_index == 0 && ctx.cycle == 0 {
        for model_flag in model_flags_to_try.iter().flatten() {
            for warning in validate_model_flag(model_flag) {
                runtime.logger.warn(&warning);
            }
        }
    }

    for (model_index, model_flag) in model_flags_to_try.iter().enumerate() {
        let model_ctx = TryModelContext {
            agent_config: &agent_config,
            agent_name: ctx.agent_name,
            display_name: &display_name,
            agent_index: ctx.agent_index,
            cycle: ctx.cycle,
            model_index,
            role: ctx.role,
            model_flag: model_flag.as_ref(),
            base_label: ctx.base_label,
            prompt: ctx.prompt,
            logfile_prefix: ctx.logfile_prefix,
            fallback_config,
            output_validator: ctx.output_validator,
            retry_timer: Arc::clone(&ctx.retry_timer),
            workspace: ctx.workspace,
        };
        let result = try_single_model(&model_ctx, runtime)?;

        match result {
            TrySingleAgentResult::Success => return Ok(TrySingleAgentResult::Success),
            TrySingleAgentResult::Unrecoverable(exit_code) => {
                return Ok(TrySingleAgentResult::Unrecoverable(exit_code))
            }
            TrySingleAgentResult::Fallback => return Ok(TrySingleAgentResult::Fallback),
            TrySingleAgentResult::NoRetry => {}
        }
    }

    Ok(TrySingleAgentResult::NoRetry)
}

/// Configuration for running with fallback.
pub struct FallbackConfig<'a, 'b> {
    pub role: AgentRole,
    pub base_label: &'a str,
    pub prompt: &'a str,
    pub logfile_prefix: &'a str,
    pub runtime: &'a mut PipelineRuntime<'b>,
    pub registry: &'a AgentRegistry,
    pub primary_agent: &'a str,
    pub output_validator: Option<crate::pipeline::fallback::OutputValidator>,
    pub workspace: &'a dyn crate::workspace::Workspace,
}

/// Run a command with automatic fallback to alternative agents on failure.
///
/// Includes an optional output validator callback that checks if the agent
/// produced valid output after `exit_code=0`. If validation fails, triggers fallback.
///
/// This variant takes a `FallbackConfig` directly for cases where you need
/// to specify an output validator.
pub fn run_with_fallback_and_validator(
    config: &mut FallbackConfig<'_, '_>,
) -> std::io::Result<i32> {
    run_with_fallback_internal(config)
}

/// Run a command with automatic fallback to alternative agents on failure.
///
/// Includes an optional output validator callback that checks if the agent
/// produced valid output after `exit_code=0`. If validation fails, triggers fallback.
fn run_with_fallback_internal(config: &mut FallbackConfig<'_, '_>) -> std::io::Result<i32> {
    let fallback_config = config.registry.fallback_config();
    let fallbacks = config.registry.available_fallbacks(config.role);
    if fallback_config.has_fallbacks(config.role) {
        config.runtime.logger.info(&format!(
            "Agent fallback chain for {}: {}",
            config.role,
            fallbacks.join(", ")
        ));
    } else {
        config.runtime.logger.info(&format!(
            "No configured fallbacks for {}, using primary only",
            config.role
        ));
    }

    let agents_to_try = build_agents_to_try(&fallbacks, config.primary_agent);
    let (cli_model_override, cli_provider_override) =
        get_cli_overrides(config.role, config.runtime);

    for cycle in 0..fallback_config.max_cycles {
        if cycle > 0 {
            let backoff_ms = fallback_config.calculate_backoff(cycle - 1);
            config.runtime.logger.info(&format!(
                "Cycle {}/{}: All agents exhausted, waiting {}ms before retry (exponential backoff)...",
                cycle + 1,
                fallback_config.max_cycles,
                backoff_ms
            ));
            config
                .registry
                .retry_timer()
                .sleep(std::time::Duration::from_millis(backoff_ms));
        }

        for (agent_index, agent_name) in agents_to_try.iter().enumerate() {
            let ctx = TryAgentContext {
                agent_name,
                agent_index,
                cycle,
                role: config.role,
                base_label: config.base_label,
                prompt: config.prompt,
                logfile_prefix: config.logfile_prefix,
                cli_model_override: cli_model_override.as_ref(),
                cli_provider_override: cli_provider_override.as_ref(),
                output_validator: config.output_validator,
                retry_timer: config.registry.retry_timer(),
                workspace: config.workspace,
            };
            let result = try_single_agent(&ctx, config.runtime, config.registry, fallback_config)?;

            match result {
                TrySingleAgentResult::Success => return Ok(0),
                TrySingleAgentResult::Unrecoverable(exit_code) => return Ok(exit_code),
                TrySingleAgentResult::Fallback | TrySingleAgentResult::NoRetry => {}
            }
        }
    }

    config.runtime.logger.error(&format!(
        "All agents exhausted after {} cycles with exponential backoff",
        fallback_config.max_cycles
    ));
    Ok(1)
}

// ============================================================================
// Session Continuation for XSD Retries
// ============================================================================
//
// Session continuation allows XSD validation retries to continue the same
// agent session, so the AI retains memory of its previous reasoning.
//
// DESIGN PRINCIPLE: Session continuation is an OPTIMIZATION, not a requirement.
// It must be completely fault-tolerant:
//
// 1. If session continuation produces output (regardless of exit code) -> use it
// 2. If it fails for ANY reason (segfault, crash, invalid session, I/O error,
//    timeout, or any other failure) -> silently fall back to normal behavior
//
// The fallback chain must NEVER be affected by session continuation failures.
// Even a segfaulting agent during session continuation must not break anything.
//
// IMPORTANT: Some AI agents have quirky behavior where they return non-zero exit
// codes but still produce valid XML. For example, an agent might output valid XML
// with status="partial" and then exit with code 1. We should still use that XML.
// The caller is responsible for checking if valid XML exists in the log file.

/// Result of attempting session continuation.
#[derive(Debug)]
pub enum SessionContinuationResult {
    /// Session continuation ran (agent was invoked).
    /// NOTE: This does NOT mean the agent succeeded - the caller must check
    /// the log file for valid output. Some agents produce valid XML even
    /// when returning non-zero exit codes.
    Ran { exit_code: i32 },
    /// Session continuation detected an auth/credential error.
    /// The caller should trigger agent fallback (switch to next agent).
    AuthError,
    /// Session continuation failed to run or was not attempted.
    /// The caller should fall back to normal `run_with_fallback`.
    Fallback,
}

/// Result of an XSD retry attempt.
#[derive(Debug)]
pub struct XsdRetryResult {
    /// The agent's exit code.
    pub exit_code: i32,
    /// If true, an auth/credential error was detected and agent fallback should occur.
    /// The XSD retry loop should stop and the caller should advance the agent chain.
    pub auth_error_detected: bool,
}

/// Configuration for XSD retry with optional session continuation.
pub struct XsdRetryConfig<'a, 'b> {
    /// Agent role for the retry.
    pub role: AgentRole,
    /// Label for logging (e.g., "planning #1").
    pub base_label: &'a str,
    /// The prompt to send.
    pub prompt: &'a str,
    /// Log file prefix (e.g., ".agent/logs/planning_1").
    pub logfile_prefix: &'a str,
    /// Pipeline runtime for logging and timing.
    pub runtime: &'a mut PipelineRuntime<'b>,
    /// Agent registry for resolving agent configs.
    pub registry: &'a AgentRegistry,
    /// Primary agent name.
    pub primary_agent: &'a str,
    /// Optional session info from previous run.
    /// If provided and valid, session continuation will be attempted first.
    pub session_info: Option<&'a crate::pipeline::session::SessionInfo>,
    /// Retry number (0 = first attempt, 1+ = XSD retries).
    pub retry_num: usize,
    /// Optional output validator to check if agent produced valid output.
    /// Used by review phase to validate JSON output extraction.
    pub output_validator: Option<crate::pipeline::fallback::OutputValidator>,
    /// Workspace for file operations.
    pub workspace: &'a dyn crate::workspace::Workspace,
}

/// Run an XSD retry with optional session continuation.
///
/// This function attempts session continuation first (if session info is available),
/// and falls back to normal `run_with_fallback` if:
/// - No session info is available
/// - The agent doesn't support session continuation
/// - Session continuation fails to even start (I/O error, panic, etc.)
///
/// # Important: Quirky Agent Behavior
///
/// Some AI agents return non-zero exit codes but still produce valid XML output.
/// For example, an agent might output valid XML with status="partial" and then
/// exit with code 1. This function does NOT treat non-zero exit codes as failures
/// for session continuation - it returns the exit code and lets the caller check
/// if valid XML was produced.
///
/// # Fault Tolerance
///
/// This function is designed to be completely fault-tolerant. Even if the agent
/// segfaults during session continuation, this function will catch the error and
/// fall back to normal behavior. The fallback chain is NEVER affected.
///
/// # Arguments
///
/// * `config` - XSD retry configuration
///
/// # Returns
///
/// * `Ok(XsdRetryResult)` - Contains exit code and whether auth error was detected
/// * `Err(_)` - I/O error (only from the fallback path, never from session continuation)
pub fn run_xsd_retry_with_session(
    config: &mut XsdRetryConfig<'_, '_>,
) -> std::io::Result<XsdRetryResult> {
    // Try session continuation first (if we have session info and it's a retry)
    if config.retry_num > 0 {
        if let Some(session_info) = config.session_info {
            // Log session continuation attempt
            config.runtime.logger.info(&format!(
                "  Attempting session continuation with {} (session: {}...)",
                session_info.agent_name,
                &session_info.session_id[..8.min(session_info.session_id.len())]
            ));
            match try_session_continuation(config, session_info) {
                SessionContinuationResult::Ran { exit_code } => {
                    // Session continuation ran - agent was invoked and produced a log file
                    // Return the exit code; the caller will check for valid XML
                    // Even if exit_code != 0, there might be valid XML in the log
                    config
                        .runtime
                        .logger
                        .info("  Session continuation succeeded");
                    return Ok(XsdRetryResult {
                        exit_code,
                        auth_error_detected: false,
                    });
                }
                SessionContinuationResult::AuthError => {
                    // Auth/credential error detected during session continuation
                    // Signal to caller that agent fallback should occur
                    config.runtime.logger.warn(
                        "  Session continuation detected auth/credential error, triggering agent fallback",
                    );
                    return Ok(XsdRetryResult {
                        exit_code: 1,
                        auth_error_detected: true,
                    });
                }
                SessionContinuationResult::Fallback => {
                    // Session continuation failed to start - fall through to normal behavior
                    config
                        .runtime
                        .logger
                        .warn("  Session continuation failed, falling back to new session");
                }
            }
        } else {
            config
                .runtime
                .logger
                .warn("  No session info available for retry, starting new session");
        }
    }

    // Normal fallback path (first attempt or session continuation failed to start)
    let mut fallback_config = FallbackConfig {
        role: config.role,
        base_label: config.base_label,
        prompt: config.prompt,
        logfile_prefix: config.logfile_prefix,
        runtime: config.runtime,
        registry: config.registry,
        primary_agent: config.primary_agent,
        output_validator: config.output_validator,
        workspace: config.workspace,
    };
    let exit_code = run_with_fallback_and_validator(&mut fallback_config)?;
    Ok(XsdRetryResult {
        exit_code,
        auth_error_detected: false,
    })
}

/// Attempt session continuation with full fault tolerance.
///
/// This function catches ALL errors and returns `Fallback` instead of propagating them.
/// Even segfaults are handled gracefully (they appear as non-zero exit codes or I/O errors).
///
/// # Returns
///
/// - `Ran { logfile, exit_code }` if the agent was successfully invoked (even if it crashed)
/// - `Fallback` if session continuation couldn't even start
fn try_session_continuation(
    config: &mut XsdRetryConfig<'_, '_>,
    session_info: &crate::pipeline::session::SessionInfo,
) -> SessionContinuationResult {
    // The agent name from session_info should already be the registry name
    // (e.g., "ccs/glm", "opencode/anthropic/claude-sonnet-4") when passed from
    // the calling code. For robustness, we still try to resolve it in case
    // it's a sanitized name from log file parsing.
    let registry_name = config
        .registry
        .resolve_from_logfile_name(&session_info.agent_name)
        .unwrap_or_else(|| session_info.agent_name.clone());

    // Check if the agent supports session continuation
    let agent_config = match config.registry.resolve_config(&registry_name) {
        Some(cfg) => cfg,
        None => {
            // Agent not found - fall back silently
            return SessionContinuationResult::Fallback;
        }
    };

    if !agent_config.supports_session_continuation() {
        // Agent doesn't support session continuation - fall back silently
        return SessionContinuationResult::Fallback;
    }

    // Build the command with session continuation flag
    let yolo = true;
    let cmd_str = agent_config.build_cmd_with_session(
        true, // output (JSON)
        yolo, // yolo mode
        true, // verbose
        None, // model override
        Some(&session_info.session_id),
    );

    // Build log file path - use a unique name to avoid overwriting previous logs
    // Sanitize the agent name to avoid creating subdirectories from slashes
    let sanitized_agent = super::logfile::sanitize_agent_name(&session_info.agent_name);
    let logfile = format!(
        "{}_{}_session_{}.log",
        config.logfile_prefix, sanitized_agent, config.retry_num
    );

    // Log the attempt (debug level since this is an optimization)
    if config.runtime.config.verbosity.is_debug() {
        config.runtime.logger.info(&format!(
            "  Attempting session continuation with {} (session: {})",
            session_info.agent_name, session_info.session_id
        ));
    }

    // Create the prompt command
    let cmd = crate::pipeline::PromptCommand {
        cmd_str: &cmd_str,
        prompt: config.prompt,
        label: &format!("{} (session)", config.base_label),
        display_name: &session_info.agent_name,
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    // Execute with full error handling - catch EVERYTHING
    // Use catch_unwind to handle panics, and Result to handle I/O errors
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::pipeline::run_with_prompt(&cmd, config.runtime)
    }));

    match result {
        Ok(Ok(cmd_result)) => {
            // Check for auth/credential errors.
            // IMPORTANT: Some agent CLIs (notably OpenCode) emit auth failures into stdout/logs
            // rather than stderr, and may even return exit_code=0 while printing an error event.
            // We must inspect the session log output as well as stderr.
            let log_output = config
                .workspace
                .read(Path::new(&logfile))
                .ok()
                .unwrap_or_default();
            if output_contains_auth_error(&cmd_result.stderr, &log_output) {
                return SessionContinuationResult::AuthError;
            }

            // Agent ran (even if it returned non-zero exit code)
            // The caller will check if valid XML was produced
            SessionContinuationResult::Ran {
                exit_code: cmd_result.exit_code,
            }
        }
        Ok(Err(_io_error)) => {
            // I/O error during execution (e.g., couldn't spawn process)
            // Fall back to normal behavior
            SessionContinuationResult::Fallback
        }
        Err(_panic) => {
            // Panic during execution (shouldn't happen, but handle it)
            // Fall back to normal behavior
            SessionContinuationResult::Fallback
        }
    }
}

fn output_contains_auth_error(stderr: &str, log_output: &str) -> bool {
    // Keep detection conservative to avoid false positives from informational text like
    // "Check authentication: opencode auth login".
    let combined = format!("{stderr}\n{log_output}").to_lowercase();

    // Highly specific known OpenCode credential error.
    if combined.contains("this credential is only authorized for use with claude code") {
        return true;
    }

    // Strong, unambiguous phrases.
    if combined.contains("authentication failed")
        || combined.contains("credential is invalid")
        || combined.contains("invalid credential")
        || combined.contains("invalid api key")
        || combined.contains("api key invalid")
        || combined.contains("unauthorized")
        || combined.contains("forbidden")
        || combined.contains("not authorized")
        || combined.contains("permission denied")
    {
        return true;
    }

    // Broader heuristic: auth keywords must be paired with an error-ish marker.
    let has_errorish_marker = combined.contains("error")
        || combined.contains("failed")
        || combined.contains("invalid")
        || combined.contains("denied")
        || combined.contains(" 401")
        || combined.contains(" 403")
        || combined.contains("status=401")
        || combined.contains("status=403");
    if !has_errorish_marker {
        return false;
    }

    combined.contains("credential")
        || combined.contains("authentication")
        || combined.contains("api key")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_contains_auth_error_detects_opencode_stdout_credential_error() {
        let stderr = "";
        let log = "[opencode/anthropic/claude-opus-4-5] ✗ Error: This credential is only authorized for use with Claude Code and cannot be used for other API requests.";
        assert!(output_contains_auth_error(stderr, log));
    }

    #[test]
    fn test_output_contains_auth_error_ignores_auth_tips_without_error_marker() {
        let stderr = "";
        let log = "OpenCode debugging tips: Check authentication: opencode auth login";
        assert!(!output_contains_auth_error(stderr, log));
    }

    #[test]
    fn test_output_contains_auth_error_detects_stderr_unauthorized() {
        let stderr = "Error: Unauthorized (401)";
        let log = "";
        assert!(output_contains_auth_error(stderr, log));
    }
}
