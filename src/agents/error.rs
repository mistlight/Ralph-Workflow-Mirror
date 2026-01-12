//! Error classification for agent failures.
//!
//! This module provides error classification logic to determine appropriate
//! recovery strategies when agents fail. Different error types warrant
//! different responses: retry, fallback to another agent, or abort.

/// Error classification for agent failures.
///
/// Used to determine appropriate recovery strategy when an agent fails:
/// - `should_retry()` - Try same agent again after delay
/// - `should_fallback()` - Switch to next agent in the chain
/// - `is_unrecoverable()` - Abort the pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentErrorKind {
    /// API rate limit exceeded - retry after delay.
    RateLimited,
    /// Token/context limit exceeded - may need different agent.
    TokenExhausted,
    /// API temporarily unavailable (server-side issue) - retry.
    ApiUnavailable,
    /// Network connectivity issue (client-side) - retry.
    NetworkError,
    /// Authentication failure - switch agent.
    AuthFailure,
    /// Command not found - switch agent.
    CommandNotFound,
    /// Disk space exhausted - cannot continue.
    DiskFull,
    /// Process killed (OOM, signal) - may retry with smaller context.
    ProcessKilled,
    /// Invalid JSON response from agent - may retry.
    InvalidResponse,
    /// Request/response timeout - retry.
    Timeout,
    /// Other transient error - retry.
    Transient,
    /// Permanent failure - do not retry.
    Permanent,
}

impl AgentErrorKind {
    /// Determine if this error should trigger a retry.
    pub fn should_retry(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::RateLimited
                | AgentErrorKind::ApiUnavailable
                | AgentErrorKind::NetworkError
                | AgentErrorKind::Timeout
                | AgentErrorKind::InvalidResponse
                | AgentErrorKind::Transient
        )
    }

    /// Determine if this error should trigger a fallback to another agent.
    pub fn should_fallback(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::TokenExhausted
                | AgentErrorKind::AuthFailure
                | AgentErrorKind::CommandNotFound
                | AgentErrorKind::ProcessKilled
        )
    }

    /// Determine if this error is unrecoverable (should abort).
    pub fn is_unrecoverable(&self) -> bool {
        matches!(self, AgentErrorKind::DiskFull | AgentErrorKind::Permanent)
    }

    /// Check if this is a command not found error.
    pub fn is_command_not_found(&self) -> bool {
        matches!(self, AgentErrorKind::CommandNotFound)
    }

    /// Check if this is a network-related error.
    pub fn is_network_error(&self) -> bool {
        matches!(self, AgentErrorKind::NetworkError | AgentErrorKind::Timeout)
    }

    /// Check if this error might be resolved by reducing context size.
    pub fn suggests_smaller_context(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::TokenExhausted | AgentErrorKind::ProcessKilled
        )
    }

    /// Get suggested wait time in milliseconds before retry.
    pub fn suggested_wait_ms(&self) -> u64 {
        match self {
            AgentErrorKind::RateLimited => 5000, // Rate limit: wait 5 seconds
            AgentErrorKind::ApiUnavailable => 3000, // Server issue: wait 3 seconds
            AgentErrorKind::NetworkError => 2000, // Network: wait 2 seconds
            AgentErrorKind::Timeout => 1000,     // Timeout: short wait
            AgentErrorKind::InvalidResponse => 500, // Bad response: quick retry
            AgentErrorKind::Transient => 1000,   // Transient: 1 second
            _ => 0,                              // No wait for non-retryable errors
        }
    }

    /// Get a user-friendly description of this error type.
    pub fn description(&self) -> &'static str {
        match self {
            AgentErrorKind::RateLimited => "API rate limit exceeded",
            AgentErrorKind::TokenExhausted => "Token/context limit exceeded",
            AgentErrorKind::ApiUnavailable => "API service temporarily unavailable",
            AgentErrorKind::NetworkError => "Network connectivity issue",
            AgentErrorKind::AuthFailure => "Authentication failure",
            AgentErrorKind::CommandNotFound => "Command not found",
            AgentErrorKind::DiskFull => "Disk space exhausted",
            AgentErrorKind::ProcessKilled => "Process terminated (possibly OOM)",
            AgentErrorKind::InvalidResponse => "Invalid response from agent",
            AgentErrorKind::Timeout => "Request timed out",
            AgentErrorKind::Transient => "Transient error",
            AgentErrorKind::Permanent => "Permanent error",
        }
    }

    /// Get recovery advice for this error type.
    pub fn recovery_advice(&self) -> &'static str {
        match self {
            AgentErrorKind::RateLimited => {
                "Will retry after delay. Consider reducing request frequency."
            }
            AgentErrorKind::TokenExhausted => {
                "Switching to alternative agent. Consider reducing context size."
            }
            AgentErrorKind::ApiUnavailable => "API server issue. Will retry automatically.",
            AgentErrorKind::NetworkError => {
                "Check your internet connection. Will retry automatically."
            }
            AgentErrorKind::AuthFailure => "Check API key or run 'agent auth' to authenticate.",
            AgentErrorKind::CommandNotFound => {
                "Agent binary not installed. See installation guidance."
            }
            AgentErrorKind::DiskFull => "Free up disk space and try again.",
            AgentErrorKind::ProcessKilled => {
                "Process was killed (possible OOM). Trying with smaller context."
            }
            AgentErrorKind::InvalidResponse => "Received malformed response. Retrying...",
            AgentErrorKind::Timeout => "Request timed out. Will retry with longer timeout.",
            AgentErrorKind::Transient => "Temporary issue. Will retry automatically.",
            AgentErrorKind::Permanent => "Unrecoverable error. Check agent logs for details.",
        }
    }

    /// Classify an error from exit code and output.
    ///
    /// Analyzes the exit code and stderr output to determine the error type.
    /// This enables appropriate recovery strategies (retry, fallback, abort).
    pub fn classify(exit_code: i32, stderr: &str) -> Self {
        let stderr_lower = stderr.to_lowercase();

        // Rate limiting indicators (API-side)
        if stderr_lower.contains("rate limit")
            || stderr_lower.contains("too many requests")
            || stderr_lower.contains("429")
            || stderr_lower.contains("quota exceeded")
        {
            return AgentErrorKind::RateLimited;
        }

        // Token/context exhaustion (API-side)
        if stderr_lower.contains("token")
            || stderr_lower.contains("context length")
            || stderr_lower.contains("maximum context")
            || stderr_lower.contains("too long")
            || stderr_lower.contains("input too large")
        {
            return AgentErrorKind::TokenExhausted;
        }

        // Network errors (client-side connectivity issues)
        if stderr_lower.contains("connection refused")
            || stderr_lower.contains("network unreachable")
            || stderr_lower.contains("dns resolution")
            || stderr_lower.contains("name resolution")
            || stderr_lower.contains("no route to host")
            || stderr_lower.contains("network is down")
            || stderr_lower.contains("host unreachable")
            || stderr_lower.contains("connection reset")
            || stderr_lower.contains("broken pipe")
            || stderr_lower.contains("econnrefused")
            || stderr_lower.contains("enetunreach")
        {
            return AgentErrorKind::NetworkError;
        }

        // API unavailable (server-side issues)
        if stderr_lower.contains("service unavailable")
            || stderr_lower.contains("503")
            || stderr_lower.contains("502")
            || stderr_lower.contains("504")
            || stderr_lower.contains("500")
            || stderr_lower.contains("internal server error")
            || stderr_lower.contains("bad gateway")
            || stderr_lower.contains("gateway timeout")
            || stderr_lower.contains("overloaded")
            || stderr_lower.contains("maintenance")
        {
            return AgentErrorKind::ApiUnavailable;
        }

        // Request timeout
        if stderr_lower.contains("timeout")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("request timeout")
            || stderr_lower.contains("deadline exceeded")
        {
            return AgentErrorKind::Timeout;
        }

        // Auth failures
        if stderr_lower.contains("unauthorized")
            || stderr_lower.contains("authentication")
            || stderr_lower.contains("401")
            || stderr_lower.contains("api key")
            || stderr_lower.contains("invalid token")
            || stderr_lower.contains("forbidden")
            || stderr_lower.contains("403")
            || stderr_lower.contains("access denied")
        {
            return AgentErrorKind::AuthFailure;
        }

        // Disk space exhaustion
        if stderr_lower.contains("no space left")
            || stderr_lower.contains("disk full")
            || stderr_lower.contains("enospc")
            || stderr_lower.contains("out of disk")
            || stderr_lower.contains("insufficient storage")
        {
            return AgentErrorKind::DiskFull;
        }

        // Process killed (OOM or signals)
        // Exit code 137 = 128 + 9 (SIGKILL), 139 = 128 + 11 (SIGSEGV)
        if exit_code == 137
            || exit_code == 139
            || exit_code == -9
            || stderr_lower.contains("killed")
            || stderr_lower.contains("oom")
            || stderr_lower.contains("out of memory")
            || stderr_lower.contains("memory exhausted")
            || stderr_lower.contains("cannot allocate")
            || stderr_lower.contains("segmentation fault")
            || stderr_lower.contains("sigsegv")
            || stderr_lower.contains("sigkill")
        {
            return AgentErrorKind::ProcessKilled;
        }

        // Invalid JSON response
        if stderr_lower.contains("invalid json")
            || stderr_lower.contains("json parse")
            || stderr_lower.contains("unexpected token")
            || stderr_lower.contains("malformed")
            || stderr_lower.contains("truncated response")
            || stderr_lower.contains("incomplete response")
        {
            return AgentErrorKind::InvalidResponse;
        }

        // Command not found
        if exit_code == 127
            || exit_code == 126
            || stderr_lower.contains("command not found")
            || stderr_lower.contains("not found")
            || stderr_lower.contains("no such file")
            || stderr_lower.contains("permission denied")
            || stderr_lower.contains("operation not permitted")
        {
            return AgentErrorKind::CommandNotFound;
        }

        // Transient errors (exit codes that might succeed on retry)
        if exit_code == 1 && stderr_lower.contains("error") {
            return AgentErrorKind::Transient;
        }

        AgentErrorKind::Permanent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_error_kind_should_retry() {
        assert!(AgentErrorKind::RateLimited.should_retry());
        assert!(AgentErrorKind::ApiUnavailable.should_retry());
        assert!(AgentErrorKind::NetworkError.should_retry());
        assert!(AgentErrorKind::Timeout.should_retry());
        assert!(AgentErrorKind::InvalidResponse.should_retry());
        assert!(AgentErrorKind::Transient.should_retry());

        assert!(!AgentErrorKind::AuthFailure.should_retry());
        assert!(!AgentErrorKind::CommandNotFound.should_retry());
        assert!(!AgentErrorKind::Permanent.should_retry());
    }

    #[test]
    fn test_agent_error_kind_should_fallback() {
        assert!(AgentErrorKind::TokenExhausted.should_fallback());
        assert!(AgentErrorKind::AuthFailure.should_fallback());
        assert!(AgentErrorKind::CommandNotFound.should_fallback());
        assert!(AgentErrorKind::ProcessKilled.should_fallback());

        assert!(!AgentErrorKind::RateLimited.should_fallback());
        assert!(!AgentErrorKind::Permanent.should_fallback());
    }

    #[test]
    fn test_agent_error_kind_is_unrecoverable() {
        assert!(AgentErrorKind::DiskFull.is_unrecoverable());
        assert!(AgentErrorKind::Permanent.is_unrecoverable());

        assert!(!AgentErrorKind::RateLimited.is_unrecoverable());
        assert!(!AgentErrorKind::AuthFailure.is_unrecoverable());
    }

    #[test]
    fn test_agent_error_kind_classify() {
        // Rate limiting
        assert_eq!(
            AgentErrorKind::classify(1, "rate limit exceeded"),
            AgentErrorKind::RateLimited
        );
        assert_eq!(
            AgentErrorKind::classify(1, "error 429"),
            AgentErrorKind::RateLimited
        );

        // Auth failure
        assert_eq!(
            AgentErrorKind::classify(1, "unauthorized"),
            AgentErrorKind::AuthFailure
        );
        assert_eq!(
            AgentErrorKind::classify(1, "error 401"),
            AgentErrorKind::AuthFailure
        );

        // Command not found
        assert_eq!(
            AgentErrorKind::classify(127, ""),
            AgentErrorKind::CommandNotFound
        );
        assert_eq!(
            AgentErrorKind::classify(1, "command not found"),
            AgentErrorKind::CommandNotFound
        );

        // Process killed
        assert_eq!(
            AgentErrorKind::classify(137, ""),
            AgentErrorKind::ProcessKilled
        );
        assert_eq!(
            AgentErrorKind::classify(1, "out of memory"),
            AgentErrorKind::ProcessKilled
        );
    }

    #[test]
    fn test_agent_error_kind_description_and_advice() {
        let error = AgentErrorKind::RateLimited;
        assert!(!error.description().is_empty());
        assert!(!error.recovery_advice().is_empty());
    }

    #[test]
    fn test_agent_error_kind_suggested_wait_ms() {
        assert_eq!(AgentErrorKind::RateLimited.suggested_wait_ms(), 5000);
        assert_eq!(AgentErrorKind::Permanent.suggested_wait_ms(), 0);
    }

    #[test]
    fn test_agent_error_kind_suggests_smaller_context() {
        assert!(AgentErrorKind::TokenExhausted.suggests_smaller_context());
        assert!(AgentErrorKind::ProcessKilled.suggests_smaller_context());
        assert!(!AgentErrorKind::RateLimited.suggests_smaller_context());
    }
}
