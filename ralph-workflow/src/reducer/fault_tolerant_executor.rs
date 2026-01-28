//! Fault-tolerant agent executor.
//!
//! This module provides bulletproof agent execution wrapper that:
//! - Catches all panics from subprocess execution
//! - Catches all I/O errors and non-zero exit codes
//! - Never returns errors - always emits PipelineEvents
//! - Provides detailed error classification for retry vs fallback decisions
//! - Logs all failures but continues pipeline execution
//!
//! Key design principle: **Agent failures should NEVER crash the pipeline**.

use crate::agents::{AgentRole, JsonParserType};
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::reducer::event::{AgentErrorKind, PipelineEvent};
use anyhow::Result;
use std::io;

/// Configuration for fault-tolerant agent execution.
#[derive(Clone, Copy)]
pub struct AgentExecutionConfig<'a> {
    /// Agent role (developer, reviewer, commit agent)
    pub role: AgentRole,
    /// Agent name from registry
    pub agent_name: &'a str,
    /// Agent command to execute
    pub cmd_str: &'a str,
    /// JSON parser type
    pub parser_type: JsonParserType,
    /// Environment variables for agent
    pub env_vars: &'a std::collections::HashMap<String, String>,
    /// Prompt to send to agent
    pub prompt: &'a str,
    /// Display name for logging
    pub display_name: &'a str,
    /// Log file path
    pub logfile: &'a str,
}

/// Execute an agent with bulletproof error handling.
///
/// This function:
/// 1. Uses `catch_unwind` to catch panics from subprocess
/// 2. Catches all I/O errors and non-zero exit codes
/// 3. Never returns errors - always emits PipelineEvents
/// 4. Classifies errors for retry/fallback decisions
/// 5. Logs failures but continues pipeline
///
/// # Arguments
///
/// * `config` - Agent execution configuration
/// * `runtime` - Pipeline runtime
///
/// # Returns
///
/// Returns `Ok(PipelineEvent)` with either:
/// - `AgentInvocationSucceeded` - agent completed successfully
/// - `AgentInvocationFailed` - agent failed with error classification
///
/// This function never returns `Err` - all errors are converted to events.
pub fn execute_agent_fault_tolerantly(
    config: AgentExecutionConfig<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> Result<PipelineEvent> {
    let role = config.role;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_agent_execution(config, runtime)
    }));

    match result {
        Ok(event_result) => event_result,
        Err(_) => {
            let error_kind = AgentErrorKind::InternalError;
            let retriable = is_retriable_agent_error(&error_kind);

            Ok(PipelineEvent::AgentInvocationFailed {
                role,
                agent: config.agent_name.to_string(),
                exit_code: 1,
                error_kind,
                retriable,
            })
        }
    }
}

/// Try to execute agent without panic catching.
///
/// This function does the actual agent execution and returns
/// either success or failure events. It's wrapped by
/// `execute_agent_fault_tolerantly` which handles panics.
fn try_agent_execution(
    config: AgentExecutionConfig<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> Result<PipelineEvent> {
    let prompt_cmd = PromptCommand {
        label: config.agent_name,
        display_name: config.display_name,
        cmd_str: config.cmd_str,
        prompt: config.prompt,
        logfile: config.logfile,
        parser_type: config.parser_type,
        env_vars: config.env_vars,
    };

    match run_with_prompt(&prompt_cmd, runtime) {
        Ok(result) if result.exit_code == 0 => Ok(PipelineEvent::AgentInvocationSucceeded {
            role: config.role,
            agent: config.agent_name.to_string(),
        }),
        Ok(result) => {
            let exit_code = result.exit_code;
            let error_kind = classify_agent_error(exit_code, &result.stderr);

            // Special handling for rate limit: emit fallback event with prompt context
            if is_rate_limit_error(&error_kind) {
                return Ok(PipelineEvent::AgentRateLimitFallback {
                    role: config.role,
                    agent: config.agent_name.to_string(),
                    prompt_context: Some(config.prompt.to_string()),
                });
            }

            let retriable = is_retriable_agent_error(&error_kind);

            Ok(PipelineEvent::AgentInvocationFailed {
                role: config.role,
                agent: config.agent_name.to_string(),
                exit_code,
                error_kind,
                retriable,
            })
        }
        Err(e) => {
            let error_kind = if let Ok(io_err) = e.downcast::<io::Error>() {
                classify_io_error(&io_err)
            } else {
                AgentErrorKind::InternalError
            };
            let retriable = is_retriable_agent_error(&error_kind);

            Ok(PipelineEvent::AgentInvocationFailed {
                role: config.role,
                agent: config.agent_name.to_string(),
                exit_code: 1,
                error_kind,
                retriable,
            })
        }
    }
}

/// Classify agent error from exit code and stderr.
fn classify_agent_error(exit_code: i32, stderr: &str) -> AgentErrorKind {
    const SIGSEGV: i32 = 139;
    const SIGABRT: i32 = 134;
    const SIGTERM: i32 = 143;

    match exit_code {
        SIGSEGV | SIGABRT => AgentErrorKind::InternalError,
        SIGTERM => AgentErrorKind::Timeout,
        _ => {
            let stderr_lower = stderr.to_lowercase();

            if stderr_lower.contains("network")
                || stderr_lower.contains("connection")
                || stderr_lower.contains("timeout")
            {
                AgentErrorKind::Network
            } else if stderr_lower.contains("auth")
                || stderr_lower.contains("api key")
                || stderr_lower.contains("unauthorized")
            {
                AgentErrorKind::Authentication
            } else if stderr_lower.contains("rate limit")
                || stderr_lower.contains("quota")
                || stderr_lower.contains("too many requests")
                || stderr_lower.contains("429")
                || stderr_lower.contains("rate_limit_exceeded")
            {
                AgentErrorKind::RateLimit
            } else if stderr_lower.contains("model")
                && (stderr_lower.contains("not found") || stderr_lower.contains("unavailable"))
            {
                AgentErrorKind::ModelUnavailable
            } else if stderr_lower.contains("parse")
                || stderr_lower.contains("invalid")
                || stderr_lower.contains("malformed")
            {
                AgentErrorKind::ParsingError
            } else if stderr_lower.contains("permission")
                || stderr_lower.contains("access denied")
                || stderr_lower.contains("file")
            {
                AgentErrorKind::FileSystem
            } else {
                AgentErrorKind::InternalError
            }
        }
    }
}

/// Classify I/O error during agent execution.
fn classify_io_error(error: &io::Error) -> AgentErrorKind {
    let error_msg = error.to_string().to_lowercase();

    if error_msg.contains("timeout") {
        AgentErrorKind::Timeout
    } else if error_msg.contains("permission")
        || error_msg.contains("access denied")
        || error_msg.contains("no such file")
        || error_msg.contains("not found")
    {
        AgentErrorKind::FileSystem
    } else if error_msg.contains("broken pipe") || error_msg.contains("connection") {
        AgentErrorKind::Network
    } else {
        AgentErrorKind::InternalError
    }
}

/// Determine if agent error is retriable.
///
/// Retriable errors should trigger model fallback (same agent, different model).
/// Non-retriable errors should trigger agent fallback (different agent).
///
/// Note: RateLimit (429) is intentionally NOT retriable - it triggers immediate
/// agent fallback to continue work without waiting. This is handled specially
/// via the `AgentRateLimitFallback` event which switches to the next agent
/// immediately rather than retrying with the same agent.
fn is_retriable_agent_error(error_kind: &AgentErrorKind) -> bool {
    matches!(
        error_kind,
        AgentErrorKind::Network | AgentErrorKind::Timeout | AgentErrorKind::ModelUnavailable
    )
}

/// Check if an error kind represents a rate limit (429) error.
///
/// Rate limit errors get special handling - they trigger immediate agent
/// fallback via `AgentRateLimitFallback` event instead of model fallback.
fn is_rate_limit_error(error_kind: &AgentErrorKind) -> bool {
    matches!(error_kind, AgentErrorKind::RateLimit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_agent_error_sigsegv() {
        let error_kind = classify_agent_error(139, "");
        assert_eq!(error_kind, AgentErrorKind::InternalError);
    }

    #[test]
    fn test_classify_agent_error_sigabrt() {
        let error_kind = classify_agent_error(134, "");
        assert_eq!(error_kind, AgentErrorKind::InternalError);
    }

    #[test]
    fn test_classify_agent_error_sigterm() {
        let error_kind = classify_agent_error(143, "");
        assert_eq!(error_kind, AgentErrorKind::Timeout);
    }

    #[test]
    fn test_classify_agent_error_network() {
        let error_kind = classify_agent_error(1, "Connection timeout");
        assert_eq!(error_kind, AgentErrorKind::Network);
    }

    #[test]
    fn test_classify_agent_error_rate_limit() {
        let error_kind = classify_agent_error(1, "Rate limit exceeded");
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_matches_http_429() {
        let error_kind = classify_agent_error(1, "error 429");
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_authentication() {
        let error_kind = classify_agent_error(1, "Invalid API key");
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    #[test]
    fn test_classify_agent_error_model_unavailable() {
        let error_kind = classify_agent_error(1, "Model not found");
        assert_eq!(error_kind, AgentErrorKind::ModelUnavailable);
    }

    #[test]
    fn test_is_retriable_agent_error() {
        // Network, Timeout, ModelUnavailable are retriable (model fallback)
        assert!(is_retriable_agent_error(&AgentErrorKind::Network));
        assert!(is_retriable_agent_error(&AgentErrorKind::Timeout));
        assert!(is_retriable_agent_error(&AgentErrorKind::ModelUnavailable));
        // RateLimit is NOT retriable - it triggers immediate agent fallback
        assert!(!is_retriable_agent_error(&AgentErrorKind::RateLimit));
        // Non-retriable errors trigger agent fallback
        assert!(!is_retriable_agent_error(&AgentErrorKind::Authentication));
        assert!(!is_retriable_agent_error(&AgentErrorKind::ParsingError));
        assert!(!is_retriable_agent_error(&AgentErrorKind::FileSystem));
        assert!(!is_retriable_agent_error(&AgentErrorKind::InternalError));
    }

    #[test]
    fn test_is_rate_limit_error() {
        // Only RateLimit should match
        assert!(is_rate_limit_error(&AgentErrorKind::RateLimit));
        // All others should NOT be rate limit errors
        assert!(!is_rate_limit_error(&AgentErrorKind::Network));
        assert!(!is_rate_limit_error(&AgentErrorKind::Timeout));
        assert!(!is_rate_limit_error(&AgentErrorKind::ModelUnavailable));
        assert!(!is_rate_limit_error(&AgentErrorKind::Authentication));
        assert!(!is_rate_limit_error(&AgentErrorKind::ParsingError));
        assert!(!is_rate_limit_error(&AgentErrorKind::FileSystem));
        assert!(!is_rate_limit_error(&AgentErrorKind::InternalError));
    }

    #[test]
    fn test_classify_io_error_timeout() {
        let error = io::Error::new(io::ErrorKind::TimedOut, "Operation timeout");
        let error_kind = classify_io_error(&error);
        assert_eq!(error_kind, AgentErrorKind::Timeout);
    }

    #[test]
    fn test_classify_io_error_filesystem() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied");
        let error_kind = classify_io_error(&error);
        assert_eq!(error_kind, AgentErrorKind::FileSystem);
    }

    #[test]
    fn test_classify_io_error_network() {
        let error = io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe");
        let error_kind = classify_io_error(&error);
        assert_eq!(error_kind, AgentErrorKind::Network);
    }
}
