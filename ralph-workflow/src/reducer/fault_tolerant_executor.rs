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
use serde_json::Value;
use std::io;

/// Result of executing an agent.
///
/// Contains the pipeline event and optional session_id for session continuation.
///
/// # Session ID Handling
///
/// When `session_id` is `Some`, the handler MUST emit a separate `SessionEstablished`
/// event to the reducer. This is the proper way to handle session IDs in the reducer
/// architecture - each piece of information is communicated via a dedicated event.
///
/// The handler should:
/// 1. Process `event` through the reducer
/// 2. If `session_id.is_some()`, emit `SessionEstablished` and process it
///
/// This two-event approach ensures:
/// - Clean separation of concerns (success vs session establishment)
/// - Proper state transitions in the reducer
/// - Session ID is stored in agent_chain.last_session_id for XSD retry reuse
pub struct AgentExecutionResult {
    /// The pipeline event from agent execution (success or failure).
    pub event: PipelineEvent,
    /// Session ID from agent's init event, for XSD retry session continuation.
    ///
    /// When present, handler must emit `SessionEstablished` event separately.
    pub session_id: Option<String>,
}

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
    /// Log prefix (without extension) used to associate artifacts.
    ///
    /// Example: `.agent/logs/planning_1`.
    pub log_prefix: &'a str,
    /// Model fallback index for attribution.
    pub model_index: usize,
    /// Attempt counter for attribution.
    pub attempt: u32,
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
/// Returns `Ok(AgentExecutionResult)` with:
/// - `event`: `AgentInvocationSucceeded` or `AgentInvocationFailed`
/// - `session_id`: Optional session ID for XSD retry session continuation
///
/// The handler MUST emit `SessionEstablished` as a separate event when session_id
/// is present. This ensures proper state management in the reducer.
///
/// This function never returns `Err` - all errors are converted to events.
pub fn execute_agent_fault_tolerantly(
    config: AgentExecutionConfig<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> Result<AgentExecutionResult> {
    let role = config.role;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_agent_execution(config, runtime)
    }));

    match result {
        Ok(event_result) => event_result,
        Err(_) => {
            let error_kind = AgentErrorKind::InternalError;
            let retriable = is_retriable_agent_error(&error_kind);

            Ok(AgentExecutionResult {
                event: PipelineEvent::agent_invocation_failed(
                    role,
                    config.agent_name.to_string(),
                    1,
                    error_kind,
                    retriable,
                ),
                session_id: None,
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
) -> Result<AgentExecutionResult> {
    let prompt_cmd = PromptCommand {
        label: config.agent_name,
        display_name: config.display_name,
        cmd_str: config.cmd_str,
        prompt: config.prompt,
        log_prefix: config.log_prefix,
        model_index: Some(config.model_index),
        attempt: Some(config.attempt),
        logfile: config.logfile,
        parser_type: config.parser_type,
        env_vars: config.env_vars,
    };

    match run_with_prompt(&prompt_cmd, runtime) {
        Ok(result) if result.exit_code == 0 => Ok(AgentExecutionResult {
            event: PipelineEvent::agent_invocation_succeeded(
                config.role,
                config.agent_name.to_string(),
            ),
            session_id: result.session_id,
        }),
        Ok(result) => {
            let exit_code = result.exit_code;
            let error_kind = classify_agent_error(exit_code, &result.stderr);

            // Special handling for rate limit: emit fallback event with prompt context
            if is_rate_limit_error(&error_kind) {
                return Ok(AgentExecutionResult {
                    event: PipelineEvent::agent_rate_limit_fallback(
                        config.role,
                        config.agent_name.to_string(),
                        Some(config.prompt.to_string()),
                    ),
                    session_id: None,
                });
            }

            // Special handling for auth failure: emit fallback event without prompt context
            if is_auth_error(&error_kind) {
                return Ok(AgentExecutionResult {
                    event: PipelineEvent::agent_auth_fallback(
                        config.role,
                        config.agent_name.to_string(),
                    ),
                    session_id: None,
                });
            }

            // Special handling for timeout: emit fallback event to switch agents
            // Unlike rate limits, timeout fallback does not preserve prompt context
            // since the previous execution may have made partial progress.
            if is_timeout_error(&error_kind) {
                return Ok(AgentExecutionResult {
                    event: PipelineEvent::agent_timeout_fallback(
                        config.role,
                        config.agent_name.to_string(),
                    ),
                    session_id: None,
                });
            }

            let retriable = is_retriable_agent_error(&error_kind);

            Ok(AgentExecutionResult {
                event: PipelineEvent::agent_invocation_failed(
                    config.role,
                    config.agent_name.to_string(),
                    exit_code,
                    error_kind,
                    retriable,
                ),
                session_id: None,
            })
        }
        Err(e) => {
            // `run_with_prompt` returns `io::Error` directly. Classify based on the error kind
            // instead of attempting to downcast the inner error payload.
            let error_kind = classify_io_error(&e);

            // Mirror special-case handling from the non-zero exit path.
            // If `run_with_prompt` itself returns an error classified as Timeout,
            // emit TimeoutFallback so the reducer clears any continuation prompt
            // and switches agents (instead of emitting a generic InvocationFailed).
            if is_timeout_error(&error_kind) {
                return Ok(AgentExecutionResult {
                    event: PipelineEvent::agent_timeout_fallback(
                        config.role,
                        config.agent_name.to_string(),
                    ),
                    session_id: None,
                });
            }
            let retriable = is_retriable_agent_error(&error_kind);

            Ok(AgentExecutionResult {
                event: PipelineEvent::agent_invocation_failed(
                    config.role,
                    config.agent_name.to_string(),
                    1,
                    error_kind,
                    retriable,
                ),
                session_id: None,
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
            } else if stderr_lower.contains("unauthorized")
                || stderr_lower.contains("authentication")
                || stderr_lower.contains("401")
                || stderr_lower.contains("api key")
                || stderr_lower.contains("invalid token")
                || stderr_lower.contains("forbidden")
                || stderr_lower.contains("403")
                || stderr_lower.contains("access denied")
                || stderr_lower.contains("credential")
            {
                AgentErrorKind::Authentication
            } else if is_rate_limit_stderr(&stderr_lower, stderr) {
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
            } else if stderr_lower.contains("permission denied")
                || stderr_lower.contains("operation not permitted")
                || stderr_lower.contains("no such file")
            {
                AgentErrorKind::FileSystem
            } else {
                AgentErrorKind::InternalError
            }
        }
    }
}

fn is_rate_limit_stderr(stderr_lower: &str, stderr_raw: &str) -> bool {
    // Prefer structured formats when available.
    if is_structured_rate_limit_error(stderr_raw) {
        return true;
    }

    // Match documented OpenAI 429 wording (avoid broad substring matches like "429" or "quota").
    if stderr_lower.contains("rate limit reached") || stderr_lower.contains("rate limit exceeded") {
        return true;
    }

    if stderr_lower.contains("too many requests") {
        return true;
    }

    if stderr_lower.contains("http 429") || stderr_lower.contains("status 429") {
        return stderr_lower.contains("rate limit") || stderr_lower.contains("too many requests");
    }

    // Quota exhaustion patterns - align with agents/error.rs
    if stderr_lower.contains("exceeded your current quota")
        || stderr_lower.contains("quota exceeded")
    {
        return true;
    }

    false
}

fn is_structured_rate_limit_error(stderr: &str) -> bool {
    // OpenCode (and some providers) emit structured JSON errors containing a stable code.
    // Example observed in CI:
    //   "✗ Error: {\"type\":\"error\",...,\"error\":{\"code\":\"rate_limit_exceeded\",...}}"
    let Some(value) = try_parse_json_object(stderr) else {
        return false;
    };

    let code = extract_error_code(&value);
    matches!(code.as_deref(), Some("rate_limit_exceeded"))
}

fn try_parse_json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let json_str = text.get(start..=end)?;
    serde_json::from_str(json_str).ok()
}

fn extract_error_code(value: &Value) -> Option<String> {
    // Support a couple of common nestings.
    // - OpenCode: {"error": {"code": "rate_limit_exceeded", ...}}
    // - Some SDKs: {"error": {"error": {"code": "..."}}}
    value
        .pointer("/error/code")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| {
            value
                .pointer("/error/error/code")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        })
}

/// Classify I/O error during agent execution.
fn classify_io_error(error: &io::Error) -> AgentErrorKind {
    match error.kind() {
        io::ErrorKind::TimedOut => AgentErrorKind::Timeout,
        io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound => AgentErrorKind::FileSystem,
        io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::NotConnected
        | io::ErrorKind::AddrInUse
        | io::ErrorKind::AddrNotAvailable
        | io::ErrorKind::UnexpectedEof => AgentErrorKind::Network,
        _ => {
            // Some process/executor paths surface `io::ErrorKind::Other` with a message that still
            // carries useful intent; keep message-based heuristics as a fallback.
            let error_msg = error.to_string().to_lowercase();

            if error_msg.contains("timed out") || error_msg.contains("timeout") {
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
    }
}

/// Determine if agent error is retriable.
///
/// Retriable errors should trigger model fallback (same agent, different model).
/// Non-retriable errors should trigger agent fallback (different agent).
///
/// # Non-retriable errors that get special handling:
///
/// - **RateLimit (429)**: Triggers immediate agent fallback via `AgentRateLimitFallback`.
///   The current provider is temporarily exhausted, so switch to next agent immediately.
///
/// - **Timeout**: Triggers immediate agent fallback via `AgentTimeoutFallback`.
///   The agent may be stuck or the task is too complex for it. Retrying the same
///   agent would likely hit the same timeout, so switch to a different agent.
///
/// - **Authentication**: Triggers immediate agent fallback via `AgentAuthFallback`.
///   Credential issues with the current agent require switching to another.
fn is_retriable_agent_error(error_kind: &AgentErrorKind) -> bool {
    matches!(
        error_kind,
        AgentErrorKind::Network | AgentErrorKind::ModelUnavailable
    )
}

/// Check if an error kind represents a timeout error.
///
/// Timeout errors get special handling - they trigger immediate agent
/// fallback via `AgentTimeoutFallback` event. Unlike rate limits, timeout
/// fallback does not preserve prompt context since the previous execution
/// may have made partial progress that is difficult to resume cleanly.
fn is_timeout_error(error_kind: &AgentErrorKind) -> bool {
    matches!(error_kind, AgentErrorKind::Timeout)
}

/// Check if an error kind represents a rate limit (429) error.
///
/// Rate limit errors get special handling - they trigger immediate agent
/// fallback via `AgentRateLimitFallback` event instead of model fallback.
fn is_rate_limit_error(error_kind: &AgentErrorKind) -> bool {
    matches!(error_kind, AgentErrorKind::RateLimit)
}

/// Check if an error kind represents an authentication error.
///
/// Auth errors get special handling - they trigger immediate agent
/// fallback via `AgentAuthFallback` event instead of generic InvocationFailed.
fn is_auth_error(error_kind: &AgentErrorKind) -> bool {
    matches!(error_kind, AgentErrorKind::Authentication)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::JsonParserType;
    use crate::config::Config;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::Timer;
    use crate::reducer::event::AgentEvent;
    use crate::workspace::{MemoryWorkspace, Workspace};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[derive(Debug)]
    struct TimedOutWriteWorkspace {
        inner: MemoryWorkspace,
        fail_path: PathBuf,
    }

    impl TimedOutWriteWorkspace {
        fn new(inner: MemoryWorkspace, fail_path: PathBuf) -> Self {
            Self { inner, fail_path }
        }
    }

    impl Workspace for TimedOutWriteWorkspace {
        fn root(&self) -> &Path {
            self.inner.root()
        }

        fn read(&self, relative: &Path) -> io::Result<String> {
            self.inner.read(relative)
        }

        fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
            self.inner.read_bytes(relative)
        }

        fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
            if relative == self.fail_path.as_path() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "simulated write timeout",
                ));
            }
            self.inner.write(relative, content)
        }

        fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            self.inner.write_bytes(relative, content)
        }

        fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            self.inner.append_bytes(relative, content)
        }

        fn exists(&self, relative: &Path) -> bool {
            self.inner.exists(relative)
        }

        fn is_file(&self, relative: &Path) -> bool {
            self.inner.is_file(relative)
        }

        fn is_dir(&self, relative: &Path) -> bool {
            self.inner.is_dir(relative)
        }

        fn remove(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove(relative)
        }

        fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_if_exists(relative)
        }

        fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_dir_all(relative)
        }

        fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_dir_all_if_exists(relative)
        }

        fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
            self.inner.create_dir_all(relative)
        }

        fn read_dir(&self, relative: &Path) -> io::Result<Vec<crate::workspace::DirEntry>> {
            self.inner.read_dir(relative)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.inner.rename(from, to)
        }

        fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
            self.inner.write_atomic(relative, content)
        }

        fn set_readonly(&self, relative: &Path) -> io::Result<()> {
            self.inner.set_readonly(relative)
        }

        fn set_writable(&self, relative: &Path) -> io::Result<()> {
            self.inner.set_writable(relative)
        }
    }

    #[test]
    fn test_timeout_error_from_run_with_prompt_err_arm_triggers_timeout_fallback() {
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let config = Config::default();

        // Use a workspace that times out when saving the prompt.
        let inner_ws = MemoryWorkspace::new_test();
        let workspace =
            TimedOutWriteWorkspace::new(inner_ws, PathBuf::from(".agent/last_prompt.txt"));

        let executor = Arc::new(crate::executor::MockProcessExecutor::new());
        let executor_arc: Arc<dyn crate::executor::ProcessExecutor> = executor;

        let mut runtime = PipelineRuntime {
            timer: &mut timer,
            logger: &logger,
            colors: &colors,
            config: &config,
            executor: executor_arc.as_ref(),
            executor_arc: Arc::clone(&executor_arc),
            workspace: &workspace,
        };

        let env_vars: HashMap<String, String> = HashMap::new();
        let exec_config = AgentExecutionConfig {
            role: AgentRole::Developer,
            agent_name: "claude",
            cmd_str: "claude -p",
            parser_type: JsonParserType::Claude,
            env_vars: &env_vars,
            prompt: "hello",
            display_name: "claude",
            log_prefix: ".agent/logs/test",
            model_index: 0,
            attempt: 0,
            logfile: ".agent/logs/test.log",
        };

        let result = execute_agent_fault_tolerantly(exec_config, &mut runtime)
            .expect("executor should never return Err");

        assert!(matches!(
            result.event,
            PipelineEvent::Agent(AgentEvent::TimeoutFallback { .. })
        ));
    }

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
        let error_kind = classify_agent_error(1, "HTTP 429: Rate limit reached for requests");
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_opencode_json_error() {
        let stderr = r#"✗ Error: {"type":"error","sequence_number":2,"error":{"type":"tokens","code":"rate_limit_exceeded","message":"Rate limit reached"}}"#;
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_does_not_treat_429_token_count_as_rate_limit() {
        let error_kind = classify_agent_error(1, "Parse error: expected 429 tokens");
        assert_eq!(error_kind, AgentErrorKind::ParsingError);
    }

    #[test]
    fn test_classify_agent_error_does_not_treat_quota_word_as_rate_limit() {
        let error_kind = classify_agent_error(1, "quota.rs:1:1: syntax error");
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
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
        // Network, ModelUnavailable are retriable (model fallback)
        assert!(is_retriable_agent_error(&AgentErrorKind::Network));
        assert!(is_retriable_agent_error(&AgentErrorKind::ModelUnavailable));
        // Timeout is NOT retriable - it triggers immediate agent fallback
        // (retrying the same agent would likely hit the same timeout)
        assert!(!is_retriable_agent_error(&AgentErrorKind::Timeout));
        // RateLimit is NOT retriable - it triggers immediate agent fallback
        assert!(!is_retriable_agent_error(&AgentErrorKind::RateLimit));
        // Non-retriable errors trigger agent fallback
        assert!(!is_retriable_agent_error(&AgentErrorKind::Authentication));
        assert!(!is_retriable_agent_error(&AgentErrorKind::ParsingError));
        assert!(!is_retriable_agent_error(&AgentErrorKind::FileSystem));
        assert!(!is_retriable_agent_error(&AgentErrorKind::InternalError));
    }

    #[test]
    fn test_is_timeout_error() {
        // Only Timeout should match
        assert!(is_timeout_error(&AgentErrorKind::Timeout));
        // All others should NOT be timeout errors
        assert!(!is_timeout_error(&AgentErrorKind::Network));
        assert!(!is_timeout_error(&AgentErrorKind::RateLimit));
        assert!(!is_timeout_error(&AgentErrorKind::ModelUnavailable));
        assert!(!is_timeout_error(&AgentErrorKind::Authentication));
        assert!(!is_timeout_error(&AgentErrorKind::ParsingError));
        assert!(!is_timeout_error(&AgentErrorKind::FileSystem));
        assert!(!is_timeout_error(&AgentErrorKind::InternalError));
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
    fn test_is_auth_error() {
        // Only Authentication should match
        assert!(is_auth_error(&AgentErrorKind::Authentication));
        // All others should NOT be auth errors
        assert!(!is_auth_error(&AgentErrorKind::RateLimit));
        assert!(!is_auth_error(&AgentErrorKind::Network));
        assert!(!is_auth_error(&AgentErrorKind::Timeout));
        assert!(!is_auth_error(&AgentErrorKind::ModelUnavailable));
        assert!(!is_auth_error(&AgentErrorKind::ParsingError));
        assert!(!is_auth_error(&AgentErrorKind::FileSystem));
        assert!(!is_auth_error(&AgentErrorKind::InternalError));
    }

    #[test]
    fn test_classify_agent_error_auth_401() {
        let error_kind = classify_agent_error(1, "HTTP 401 Unauthorized");
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    #[test]
    fn test_classify_agent_error_auth_403_forbidden() {
        let error_kind = classify_agent_error(1, "HTTP 403 Forbidden");
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    #[test]
    fn test_classify_agent_error_auth_invalid_token() {
        let error_kind = classify_agent_error(1, "Error: Invalid token provided");
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    #[test]
    fn test_classify_agent_error_auth_credential() {
        let error_kind = classify_agent_error(1, "Error: This credential is not authorized");
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    #[test]
    fn test_classify_agent_error_auth_access_denied() {
        let error_kind = classify_agent_error(1, "Access denied: insufficient permissions");
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    #[test]
    fn test_classify_io_error_timeout() {
        let error = io::Error::new(io::ErrorKind::TimedOut, "Operation timeout");
        let error_kind = classify_io_error(&error);
        assert_eq!(error_kind, AgentErrorKind::Timeout);
    }

    #[test]
    fn test_classify_io_error_timeout_timed_out_message() {
        // Common OS phrasing is "timed out" (not "timeout"). We must classify
        // based on `io::ErrorKind::TimedOut`, not substring matching.
        let error = io::Error::new(io::ErrorKind::TimedOut, "Operation timed out");
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

    // ========================================================================
    // Step 2: Quota exceeded pattern alignment tests
    // ========================================================================

    #[test]
    fn test_classify_agent_error_rate_limit_quota_exceeded() {
        let error_kind = classify_agent_error(1, "API quota exceeded, please try again later");
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_anthropic_quota() {
        let error_kind =
            classify_agent_error(1, "You have exceeded your current quota for this API tier");
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    // ========================================================================
    // Step 3: Comprehensive tests for auth and rate-limit fallback flow
    // ========================================================================

    #[test]
    fn test_auth_error_triggers_auth_fallback_classification() {
        // All these patterns should result in Authentication error kind
        // which triggers AuthFallback event via is_auth_error()
        let auth_patterns = vec![
            "HTTP 401 Unauthorized",
            "HTTP 403 Forbidden",
            "Error: Invalid API key",
            "Error: Invalid token provided",
            "Access denied: insufficient permissions",
            "This credential is only authorized for use with Claude Code",
            "Authentication failed: bad credentials",
        ];

        for pattern in auth_patterns {
            let error_kind = classify_agent_error(1, pattern);
            assert_eq!(
                error_kind,
                AgentErrorKind::Authentication,
                "Pattern '{}' should classify as Authentication",
                pattern
            );
            assert!(
                is_auth_error(&error_kind),
                "Authentication error kind should trigger auth fallback for pattern '{}'",
                pattern
            );
        }
    }

    #[test]
    fn test_rate_limit_error_triggers_rate_limit_fallback_classification() {
        // All these patterns should result in RateLimit error kind
        // which triggers RateLimitFallback event via is_rate_limit_error()
        let rate_limit_patterns = vec![
            "Rate limit exceeded",
            "Rate limit reached for requests",
            "HTTP 429 Too Many Requests",
            "Error: too many requests, please slow down",
            "exceeded your current quota",
            "API quota exceeded",
        ];

        for pattern in rate_limit_patterns {
            let error_kind = classify_agent_error(1, pattern);
            assert_eq!(
                error_kind,
                AgentErrorKind::RateLimit,
                "Pattern '{}' should classify as RateLimit",
                pattern
            );
            assert!(
                is_rate_limit_error(&error_kind),
                "RateLimit error kind should trigger rate limit fallback for pattern '{}'",
                pattern
            );
        }
    }

    // ========================================================================
    // Step 5: Structured JSON auth error detection tests
    // ========================================================================

    #[test]
    fn test_classify_agent_error_auth_from_json_error() {
        // Auth error embedded in JSON structure (common for some providers)
        let stderr = r#"✗ Error: {"type":"error","error":{"type":"auth","code":"unauthorized","message":"Invalid API key provided"}}"#;
        let error_kind = classify_agent_error(1, stderr);
        // The "unauthorized" keyword should still be detected via substring matching
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    #[test]
    fn test_classify_agent_error_403_from_json_error() {
        let stderr =
            r#"{"error":{"code":"403","message":"Forbidden: API key does not have access"}}"#;
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::Authentication);
    }

    // ========================================================================
    // Step 6: Non-auth, non-rate-limit error behavior tests
    // ========================================================================

    #[test]
    fn test_non_special_errors_maintain_retry_semantics() {
        // Network errors: retriable (model fallback, NOT agent fallback)
        let network_error = classify_agent_error(1, "Connection timeout");
        assert_eq!(network_error, AgentErrorKind::Network);
        assert!(
            is_retriable_agent_error(&network_error),
            "Network should be retriable"
        );
        assert!(
            !is_rate_limit_error(&network_error),
            "Network should not trigger rate limit fallback"
        );
        assert!(
            !is_auth_error(&network_error),
            "Network should not trigger auth fallback"
        );

        // Timeout errors: NOT retriable (triggers agent fallback via TimeoutFallback)
        // Retrying the same agent would likely hit the same timeout
        let timeout_error = classify_agent_error(143, ""); // SIGTERM
        assert_eq!(timeout_error, AgentErrorKind::Timeout);
        assert!(!is_retriable_agent_error(&timeout_error));
        assert!(is_timeout_error(&timeout_error));

        // Model unavailable: retriable
        let model_error = classify_agent_error(1, "Model not found");
        assert_eq!(model_error, AgentErrorKind::ModelUnavailable);
        assert!(is_retriable_agent_error(&model_error));

        // Internal errors: NOT retriable (agent fallback)
        let internal_error = classify_agent_error(139, ""); // SIGSEGV
        assert_eq!(internal_error, AgentErrorKind::InternalError);
        assert!(!is_retriable_agent_error(&internal_error));

        // Parsing errors: NOT retriable
        let parse_error = classify_agent_error(1, "Parse error: invalid syntax");
        assert_eq!(parse_error, AgentErrorKind::ParsingError);
        assert!(!is_retriable_agent_error(&parse_error));

        // Filesystem errors: NOT retriable
        let fs_error = classify_agent_error(1, "Permission denied: /tmp/foo");
        assert_eq!(fs_error, AgentErrorKind::FileSystem);
        assert!(!is_retriable_agent_error(&fs_error));
    }
}
