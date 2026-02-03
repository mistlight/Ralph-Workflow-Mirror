//! Error classification for fault-tolerant agent execution.
//!
//! This module provides functions to classify errors from agent execution
//! into categories that determine retry vs fallback behavior.

use crate::reducer::event::AgentErrorKind;
use serde_json::Value;
use std::io;

/// Classify agent error from exit code and stderr.
pub fn classify_agent_error(exit_code: i32, stderr: &str) -> AgentErrorKind {
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
pub fn classify_io_error(error: &io::Error) -> AgentErrorKind {
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
/// Non-retriable errors are reported as facts; the reducer decides retry vs fallback.
///
/// # Non-retriable errors with dedicated fact events:
///
/// - **RateLimit (429)**: Emitted as `AgentEvent::RateLimited` with prompt context.
///   The reducer typically switches to the next agent immediately.
///
/// - **Timeout**: Emitted as `AgentEvent::TimedOut`.
///   The reducer retries the same agent first and only falls back after exhausting
///   the configured retry budget.
///
/// - **Authentication**: Emitted as `AgentEvent::AuthFailed`.
///   The reducer typically switches to the next agent immediately.
pub fn is_retriable_agent_error(error_kind: &AgentErrorKind) -> bool {
    matches!(
        error_kind,
        AgentErrorKind::Network | AgentErrorKind::ModelUnavailable
    )
}

/// Check if an error kind represents a timeout error.
pub fn is_timeout_error(error_kind: &AgentErrorKind) -> bool {
    matches!(error_kind, AgentErrorKind::Timeout)
}

/// Check if an error kind represents a rate limit (429) error.
///
/// Rate limit errors are emitted as `AgentEvent::RateLimited` instead of a generic
/// InvocationFailed so the reducer can apply deterministic policy.
pub fn is_rate_limit_error(error_kind: &AgentErrorKind) -> bool {
    matches!(error_kind, AgentErrorKind::RateLimit)
}

/// Check if an error kind represents an authentication error.
///
/// Auth errors are emitted as `AgentEvent::AuthFailed` instead of a generic
/// InvocationFailed so the reducer can apply deterministic policy.
pub fn is_auth_error(error_kind: &AgentErrorKind) -> bool {
    matches!(error_kind, AgentErrorKind::Authentication)
}
