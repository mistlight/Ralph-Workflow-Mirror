//! Fallback logic for agent execution with retries.

use crate::agents::{is_glm_like_agent, AgentErrorKind, JsonParserType};
use crate::common::{format_argv_for_log, split_command, truncate_text};
use crate::logger::Logger;
use std::io;
use std::path::Path;

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

/// Callback type for validating agent output after execution.
///
/// Takes the log directory path and returns Ok(true) if output is valid,
/// Ok(false) if output is missing/invalid (trigger fallback), or Err(e) for errors.
pub type OutputValidator = fn(log_dir: &Path, logger: &Logger) -> io::Result<bool>;

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
    /// Log file prefix (used for output validation)
    pub logfile_prefix: &'a str,
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
    /// Optional callback to validate output after execution (`exit_code=0`)
    pub output_validator: Option<OutputValidator>,
}

/// Log GLM-specific diagnostic output (only on first try to avoid spam).
fn log_glm_diagnostics(
    agent_name: &str,
    cmd_str: &str,
    agent_index: usize,
    cycle: usize,
    model_index: usize,
    runtime: &PipelineRuntime<'_>,
) {
    if agent_index != 0 || cycle != 0 || model_index != 0 {
        return;
    }

    let cmd_argv = split_command(cmd_str).ok();
    let full_cmd_log = cmd_argv.as_ref().map_or_else(
        || "<unparseable command>".to_string(),
        |argv| {
            let mut argv_for_log = argv.clone();
            argv_for_log.push("<PROMPT>".to_string());
            truncate_text(&format_argv_for_log(&argv_for_log), 160)
        },
    );

    if runtime.config.verbosity.is_debug() {
        runtime
            .logger
            .info(&format!("GLM agent '{agent_name}' command configuration:"));
        runtime
            .logger
            .info(&format!("  Full command: {full_cmd_log}"));
    }
}

/// Handle agent error classification, logging, and user guidance.
///
/// Returns the `AgentErrorKind` for downstream decision logic.
fn handle_agent_error(
    exit_code: i32,
    stderr: &str,
    agent_name: &str,
    model_flag: Option<&str>,
    cmd_str: &str,
    is_glm_agent: bool,
    runtime: &PipelineRuntime<'_>,
) -> AgentErrorKind {
    let error_kind =
        AgentErrorKind::classify_with_agent(exit_code, stderr, Some(agent_name), model_flag);

    let model_suffix = model_flag.map(|m| format!(" [{m}]")).unwrap_or_default();

    runtime.logger.warn(&format!(
        "Agent '{agent_name}{model_suffix}' failed: {} (exit code {exit_code})",
        error_kind.description()
    ));

    // GLM-specific diagnostics
    if is_glm_agent
        && matches!(
            error_kind,
            AgentErrorKind::AgentSpecificQuirk
                | AgentErrorKind::RetryableAgentQuirk
                | AgentErrorKind::ToolExecutionFailed
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
            .info(&crate::agents::auth_failure_advice(model_flag));
    } else {
        runtime.logger.info(error_kind.recovery_advice());
    }

    // Provide installation guidance for command not found errors
    if error_kind.is_command_not_found() {
        let binary = cmd_str.split_whitespace().next().unwrap_or(agent_name);
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
        runtime.logger.info("Tip: Try reducing context size with RALPH_DEVELOPER_CONTEXT=0 or RALPH_REVIEWER_CONTEXT=0");
    }

    error_kind
}

/// Log retry message and wait before next retry.
fn log_retry_message(
    display_name: &str,
    model_suffix: &str,
    retry: u32,
    max_retries: u32,
    error_kind: AgentErrorKind,
    retry_delay_ms: u64,
    logger: &Logger,
) {
    logger.info(&format!(
        "Retrying '{display_name}{model_suffix}' (attempt {}/{max_retries})",
        retry + 1
    ));
    let wait_ms = error_kind.suggested_wait_ms().max(retry_delay_ms);
    std::thread::sleep(std::time::Duration::from_millis(wait_ms));
}

/// Validate agent output after successful execution (`exit_code=0`).
///
/// Returns `Some(true)` if output is valid, `Some(false)` if missing/invalid (trigger fallback),
/// or `None` if no validator is configured.
fn validate_agent_output(
    config: &AgentAttemptConfig<'_>,
    runtime: &PipelineRuntime<'_>,
) -> Option<bool> {
    let validator = config.output_validator?;

    let log_prefix_path = Path::new(config.logfile_prefix);

    match validator(log_prefix_path, runtime.logger) {
        Ok(true) => Some(true),
        Ok(false) => {
            runtime.logger.warn(&format!(
                "Agent '{}' exited with code 0 but produced no valid output",
                config.agent_name
            ));
            runtime
                .logger
                .info("Treating as failure and trying fallback...");
            Some(false)
        }
        Err(e) => {
            runtime.logger.warn(&format!(
                "Output validation failed (continuing anyway): {e}"
            ));
            Some(true)
        }
    }
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
    if is_glm_agent {
        log_glm_diagnostics(
            config.agent_name,
            config.cmd_str,
            config.agent_index,
            config.cycle,
            config.model_index,
            runtime,
        );
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
            // Validate output if a validator is provided
            match validate_agent_output(config, runtime) {
                Some(false) => {
                    // Validation failed - log and retry if retries remain
                    runtime
                        .logger
                        .warn("Agent exited successfully but produced no valid output");
                    if retry + 1 < config.fallback_config.max_retries {
                        // Retry with the same agent
                        runtime.logger.info("Retrying due to validation failure...");
                        log_retry_message(
                            config.display_name,
                            &model_suffix,
                            retry + 1,
                            config.fallback_config.max_retries,
                            AgentErrorKind::ToolExecutionFailed, // Use a retriable error kind
                            config.fallback_config.retry_delay_ms,
                            runtime.logger,
                        );
                        continue;
                    }
                    // All retries exhausted
                    return Ok(TryAgentResult::Fallback);
                }
                Some(true) | None => return Ok(TryAgentResult::Success),
            }
        }

        // Handle error classification, logging, and user guidance
        let error_kind = handle_agent_error(
            result.exit_code,
            &result.stderr,
            config.agent_name,
            config.model_flag,
            config.cmd_str,
            is_glm_agent,
            runtime,
        );

        // Check for unrecoverable errors - abort immediately
        if error_kind.is_unrecoverable() {
            runtime
                .logger
                .error("Unrecoverable error - cannot continue pipeline");
            return Ok(TryAgentResult::Unrecoverable(result.exit_code));
        }

        // Check if we should fallback to next agent
        if error_kind.should_fallback() {
            runtime.logger.info(&format!(
                "Switching from '{}{}' to next configured fallback...",
                config.display_name, model_suffix,
            ));
            return Ok(TryAgentResult::Fallback);
        }

        if !error_kind.should_retry() {
            runtime.logger.info("Not retrying (non-retriable error)");
            return Ok(TryAgentResult::NoRetry);
        }

        // Otherwise, continue retrying the same model/agent
        if retry + 1 < config.fallback_config.max_retries {
            log_retry_message(
                config.display_name,
                &model_suffix,
                retry + 1,
                config.fallback_config.max_retries,
                error_kind,
                config.fallback_config.retry_delay_ms,
                runtime.logger,
            );
        }
    }

    // All retries exhausted
    Ok(TryAgentResult::Fallback)
}
