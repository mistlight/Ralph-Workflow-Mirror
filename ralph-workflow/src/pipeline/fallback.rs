//! Fallback logic for agent execution with retries.

use crate::agents::{is_glm_like_agent, AgentErrorKind, JsonParserType};
use crate::common::{format_argv_for_log, split_command, truncate_text};
use std::io;

use super::prompt::{run_with_prompt, PipelineRuntime, PromptCommand};

/// Result of attempting to run an agent with retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryAgentResult {
    /// Agent succeeded - return success from main function
    Success,
    /// Unrecoverable error - abort immediately
    Unrecoverable(i32),
    /// Should fall back to next agent/model
    Fallback,
    /// Should retry (non-retriable error on last retry)
    NoRetry,
}

/// Configuration for attempting an agent with retries.
pub struct AgentAttemptConfig<'a> {
    /// Agent name
    pub agent_name: &'a str,
    /// Optional model flag override
    pub model_flag: Option<&'a str>,
    /// Label for logging
    pub label: &'a str,
    /// Display name for logging
    pub display_name: &'a str,
    /// Command string to execute
    pub cmd_str: &'a str,
    /// Prompt content to send
    pub prompt: &'a str,
    /// Log file path
    pub logfile: &'a str,
    /// JSON parser type
    pub parser_type: JsonParserType,
    /// Environment variables to pass
    pub env_vars: &'a std::collections::HashMap<String, String>,
    /// Model index in chain
    pub model_index: usize,
    /// Agent index in chain
    pub agent_index: usize,
    /// Retry cycle number
    pub cycle: usize,
    /// Fallback configuration
    pub fallback_config: &'a crate::agents::fallback::FallbackConfig,
}

/// Log GLM-specific diagnostic information on first attempt.
fn log_glm_diagnostics(config: &AgentAttemptConfig<'_>, runtime: &PipelineRuntime<'_>) {
    let cmd_argv = split_command(config.cmd_str).ok();
    let full_cmd_log = cmd_argv.as_ref().map_or_else(
        || "<unparseable command>".to_string(),
        |argv| {
            let mut argv_for_log = argv.clone();
            argv_for_log.push("<PROMPT>".to_string());
            truncate_text(&format_argv_for_log(&argv_for_log), 160)
        },
    );

    if runtime.config.verbosity.is_debug() {
        runtime.logger.info(&format!(
            "GLM agent '{}' command configuration:",
            config.agent_name
        ));
        runtime
            .logger
            .info(&format!("  Full command: {full_cmd_log}"));
    }
}

/// Log error diagnostics and recovery advice based on error kind.
fn log_error_diagnostics(
    config: &AgentAttemptConfig<'_>,
    runtime: &PipelineRuntime<'_>,
    error_kind: AgentErrorKind,
    is_glm_agent: bool,
) {
    // GLM-specific diagnostics
    if is_glm_agent
        && matches!(
            error_kind,
            AgentErrorKind::AgentSpecificQuirk | AgentErrorKind::ToolExecutionFailed
        )
    {
        runtime.logger.warn(&format!(
            "{}GLM Agent Issue Detected:{} GLM has known compatibility issues with Ralph.",
            runtime.colors.yellow(),
            runtime.colors.reset()
        ));
        runtime.logger.info("Suggested workarounds:");
        runtime
            .logger
            .info("  1. Try: ralph --reviewer-agent codex");
        runtime
            .logger
            .info("  2. Try: ralph --reviewer-json-parser generic");
        runtime
            .logger
            .info("  3. Skip review: RALPH_REVIEWER_REVIEWS=0 ralph");
        runtime
            .logger
            .info("See docs/agent-compatibility.md for details.");
    }

    // Provide provider-specific auth advice for auth failures
    if matches!(error_kind, AgentErrorKind::AuthFailure) {
        runtime
            .logger
            .info(&crate::agents::auth_failure_advice(config.model_flag));
    } else {
        runtime.logger.info(error_kind.recovery_advice());
    }

    // Provide installation guidance for command not found errors
    if error_kind.is_command_not_found() {
        let binary = config
            .cmd_str
            .split_whitespace()
            .next()
            .unwrap_or(config.agent_name);
        let guidance = crate::platform::InstallGuidance::for_binary(binary);
        runtime.logger.info(&guidance.format());
    }

    // Provide network-specific guidance
    if error_kind.is_network_error() {
        runtime
            .logger
            .info("Tip: Check your internet connection, firewall, or VPN settings.");
    }

    // Provide context reduction hint for memory-related errors
    if error_kind.suggests_smaller_context() {
        runtime.logger.info(
            "Tip: Try reducing context size with RALPH_DEVELOPER_CONTEXT=0 or RALPH_REVIEWER_CONTEXT=0",
        );
    }
}

/// Handle a single retry attempt, returning Some(result) if we should exit the retry loop.
fn handle_retry_attempt(
    config: &AgentAttemptConfig<'_>,
    runtime: &PipelineRuntime<'_>,
    model_suffix: &str,
    is_glm_agent: bool,
    exit_code: i32,
    stderr: &str,
    retry: u32,
) -> Option<TryAgentResult> {
    let error_kind = AgentErrorKind::classify_with_agent(
        exit_code,
        stderr,
        Some(config.agent_name),
        config.model_flag,
    );

    runtime.logger.warn(&format!(
        "Agent '{}'{} failed: {} (exit code {})",
        config.agent_name,
        model_suffix,
        error_kind.description(),
        exit_code
    ));

    log_error_diagnostics(config, runtime, error_kind, is_glm_agent);

    if error_kind.is_unrecoverable() {
        runtime
            .logger
            .error("Unrecoverable error - cannot continue pipeline");
        return Some(TryAgentResult::Unrecoverable(exit_code));
    }

    if error_kind.should_fallback() {
        runtime.logger.info(&format!(
            "Switching from '{}'{} to next configured fallback...",
            config.display_name, model_suffix,
        ));
        return Some(TryAgentResult::Fallback);
    }

    if !error_kind.should_retry() {
        runtime.logger.info("Not retrying (non-retriable error)");
        return Some(TryAgentResult::NoRetry);
    }

    // Continue retrying - sleep if not last retry
    if retry + 1 < config.fallback_config.max_retries {
        runtime.logger.info(&format!(
            "Retrying '{}'{} (attempt {}/{})",
            config.display_name,
            model_suffix,
            retry + 2,
            config.fallback_config.max_retries
        ));
        let wait_ms = error_kind
            .suggested_wait_ms()
            .max(config.fallback_config.retry_delay_ms);
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
    }

    None // Continue retry loop
}

/// Try a single agent/model configuration with retries.
///
/// Returns the result of the attempt: success, unrecoverable error, or should-fallback.
pub fn try_agent_with_retries(
    config: &AgentAttemptConfig<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> io::Result<TryAgentResult> {
    let model_suffix = config
        .model_flag
        .as_ref()
        .map(|m| format!(" [{m}]"))
        .unwrap_or_default();
    let is_glm_agent = is_glm_like_agent(config.agent_name);

    // GLM-specific diagnostic output (only on first try to avoid spam)
    if is_glm_agent && config.agent_index == 0 && config.cycle == 0 && config.model_index == 0 {
        log_glm_diagnostics(config, runtime);
    }

    // Try with retries
    for retry in 0..config.fallback_config.max_retries {
        if retry > 0 {
            runtime.logger.info(&format!(
                "Retry {}/{} for {}{}...",
                retry, config.fallback_config.max_retries, config.display_name, model_suffix,
            ));
        }

        let result = run_with_prompt(
            &PromptCommand {
                label: config.label,
                display_name: config.display_name,
                cmd_str: config.cmd_str,
                prompt: config.prompt,
                logfile: config.logfile,
                parser_type: config.parser_type,
                env_vars: config.env_vars,
            },
            runtime,
        )?;

        if result.exit_code == 0 {
            return Ok(TryAgentResult::Success);
        }

        if let Some(result) = handle_retry_attempt(
            config,
            runtime,
            &model_suffix,
            is_glm_agent,
            result.exit_code,
            &result.stderr,
            retry,
        ) {
            return Ok(result);
        }
    }

    // All retries exhausted
    Ok(TryAgentResult::Fallback)
}
