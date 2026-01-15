//! Fallback logic for agent execution with retries.

use crate::agents::{is_glm_like_agent, JsonParserType};
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

/// Try a single agent/model configuration with retries.
///
/// Returns the result of the attempt: success, unrecoverable error, or should-fallback.
pub fn try_agent_with_retries(
    agent_name: &str,
    model_flag: Option<&str>,
    label: &str,
    display_name: &str,
    cmd_str: &str,
    prompt: &str,
    logfile: &str,
    parser_type: JsonParserType,
    env_vars: &std::collections::HashMap<String, String>,
    model_index: usize,
    agent_index: usize,
    cycle: usize,
    runtime: &mut PipelineRuntime<'_>,
    fallback_config: &crate::agents::fallback::FallbackConfig,
) -> io::Result<TryAgentResult> {
    let model_suffix = model_flag
        .as_ref()
        .map(|m| format!(" [{m}]"))
        .unwrap_or_default();
    let is_glm_agent = is_glm_like_agent(agent_name);

    // GLM-specific diagnostic output (only on first try to avoid spam)
    if is_glm_agent && agent_index == 0 && cycle == 0 && model_index == 0 {
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

    // Try with retries
    for retry in 0..fallback_config.max_retries {
        if retry > 0 {
            runtime.logger.info(&format!(
                "Retry {}/{} for {}{}...",
                retry, fallback_config.max_retries, display_name, model_suffix,
            ));
        }

        let result = run_with_prompt(
            &PromptCommand {
                label,
                display_name,
                cmd_str,
                prompt,
                logfile,
                parser_type,
                env_vars,
            },
            runtime,
        )?;

        if result.exit_code == 0 {
            return Ok(TryAgentResult::Success);
        }

        // Classify the error with agent context for better handling
        let error_kind = crate::agents::AgentErrorKind::classify_with_agent(
            result.exit_code,
            &result.stderr,
            Some(agent_name),
            model_flag,
        );

        runtime.logger.warn(&format!(
            "Agent '{}'{} failed: {} (exit code {})",
            agent_name,
            model_suffix,
            error_kind.description(),
            result.exit_code
        ));

        // GLM-specific diagnostics
        if is_glm_agent
            && matches!(
                error_kind,
                crate::agents::AgentErrorKind::AgentSpecificQuirk
                    | crate::agents::AgentErrorKind::ToolExecutionFailed
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
        if matches!(error_kind, crate::agents::AgentErrorKind::AuthFailure) {
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
                "Switching from '{display_name}'{model_suffix} to next configured fallback..."
            ));
            return Ok(TryAgentResult::Fallback);
        }

        if !error_kind.should_retry() {
            runtime.logger.info("Not retrying (non-retriable error)");
            return Ok(TryAgentResult::NoRetry);
        }

        // Otherwise, continue retrying the same model/agent
        if retry + 1 < fallback_config.max_retries {
            runtime.logger.info(&format!(
                "Retrying '{}'{} (attempt {}/{})",
                display_name,
                model_suffix,
                retry + 2,
                fallback_config.max_retries
            ));
            let wait_ms = error_kind
                .suggested_wait_ms()
                .max(fallback_config.retry_delay_ms);
            std::thread::sleep(std::time::Duration::from_millis(wait_ms));
        }
    }

    // All retries exhausted
    Ok(TryAgentResult::Fallback)
}
