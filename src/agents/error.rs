//! Error classification for agent failures.
//!
//! This module provides error classification logic to determine appropriate
//! recovery strategies when agents fail. Different error types warrant
//! different responses: retry, fallback to another agent, or abort.

/// Check if an agent name or command string indicates a GLM-like agent.
///
/// GLM-like agents include GLM, ZhipuAI, ZAI, Qwen, and DeepSeek.
/// These agents have known compatibility issues with review tasks and may
/// require special handling or fallback logic.
///
/// # Arguments
///
/// * `s` - The agent name or command string to check
///
/// # Returns
///
/// `true` if the string indicates a GLM-like agent, `false` otherwise
pub(crate) fn is_glm_like_agent(s: &str) -> bool {
    let s_lower = s.to_lowercase();
    s_lower.contains("glm")
        || s_lower.contains("zhipuai")
        || s_lower.contains("zai")
        || s_lower.contains("qwen")
        || s_lower.contains("deepseek")
}

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
    /// Tool execution failed - should fallback (e.g., file write issues).
    ToolExecutionFailed,
    /// Known agent-specific behavioral quirk - should fallback with specific advice.
    AgentSpecificQuirk,
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
                | AgentErrorKind::ToolExecutionFailed
                | AgentErrorKind::AgentSpecificQuirk
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
            AgentErrorKind::ToolExecutionFailed => "Tool execution failed (e.g., file write)",
            AgentErrorKind::AgentSpecificQuirk => "Known agent-specific issue",
            AgentErrorKind::Transient => "Transient error",
            AgentErrorKind::Permanent => "Permanent error",
        }
    }

    /// Get recovery advice for this error type.
    pub fn recovery_advice(&self) -> &'static str {
        match self {
            AgentErrorKind::RateLimited => {
                "Will retry after delay. Tip: Consider reducing request frequency or using a different provider."
            }
            AgentErrorKind::TokenExhausted => {
                "Switching to alternative agent. Tip: Try RALPH_DEVELOPER_CONTEXT=0 or RALPH_REVIEWER_CONTEXT=0"
            }
            AgentErrorKind::ApiUnavailable => {
                "API server issue. Will retry automatically. Tip: Check status page or try different provider."
            }
            AgentErrorKind::NetworkError => {
                "Check your internet connection. Will retry automatically. Tip: Check firewall/VPN settings."
            }
            AgentErrorKind::AuthFailure => {
                "Check API key or run 'agent auth' to authenticate. Tip: Verify credentials for this provider."
            }
            AgentErrorKind::CommandNotFound => {
                "Agent binary not installed. See installation guidance below. Tip: Run 'ralph --list-available-agents'"
            }
            AgentErrorKind::DiskFull => "Free up disk space and try again. Tip: Check .agent directory size.",
            AgentErrorKind::ProcessKilled => {
                "Process was killed (possible OOM). Trying with smaller context. Tip: Reduce context with RALPH_*_CONTEXT=0"
            }
            AgentErrorKind::InvalidResponse => {
                "Received malformed response. Retrying... Tip: May indicate parser mismatch with this agent."
            }
            AgentErrorKind::Timeout => {
                "Request timed out. Will retry with longer timeout. Tip: Try reducing prompt size or context."
            }
            AgentErrorKind::ToolExecutionFailed => {
                "Tool execution failed (file write/permissions). Switching agent. Tip: Check directory write permissions."
            }
            AgentErrorKind::AgentSpecificQuirk => {
                "Known agent-specific issue. Switching to alternative agent. Tip: See docs/agent-compatibility.md"
            }
            AgentErrorKind::Transient => "Temporary issue. Will retry automatically.",
            AgentErrorKind::Permanent => {
                "Unrecoverable error. Check agent logs (.agent/logs/) and see docs/agent-compatibility.md for help."
            }
        }
    }

    /// Classify an error from exit code, output, and agent name.
    ///
    /// This variant takes the agent name into account for better classification.
    /// Some agents have known failure patterns that should trigger fallback
    /// instead of retry, even when the stderr output is generic.
    ///
    /// # Arguments
    ///
    /// * `exit_code` - The process exit code
    /// * `stderr` - The standard error output from the agent
    /// * `agent_name` - Optional agent name for context-aware classification
    pub fn classify_with_agent(
        exit_code: i32,
        stderr: &str,
        agent_name: Option<&str>,
        model_flag: Option<&str>,
    ) -> Self {
        let stderr_lower = stderr.to_lowercase();

        // If we know this is a GLM-like agent and it failed with exit code 1,
        // classify it as AgentSpecificQuirk to trigger fallback instead of retry
        let is_problematic_agent =
            agent_name.is_some_and(is_glm_like_agent) || model_flag.is_some_and(is_glm_like_agent);

        if is_problematic_agent && exit_code == 1 {
            // GLM and similar agents often exit with code 1 for various issues.
            // Treating as AgentSpecificQuirk ensures faster fallback.
            return AgentErrorKind::AgentSpecificQuirk;
        }

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

        // Tool execution failures (file writes, tool calls, etc.)
        // These should trigger fallback, not retry
        if stderr_lower.contains("write error")
            || stderr_lower.contains("cannot write")
            || stderr_lower.contains("failed to write")
            || stderr_lower.contains("unable to create file")
            || stderr_lower.contains("file creation failed")
            || stderr_lower.contains("i/o error")
            || stderr_lower.contains("io error")
            || stderr_lower.contains("tool failed")
            || stderr_lower.contains("tool execution failed")
            || stderr_lower.contains("tool call failed")
        {
            return AgentErrorKind::ToolExecutionFailed;
        }

        // Permission denied errors (specific patterns that should fallback)
        // These need to be checked BEFORE the generic "error" catch-all
        // Note: "access denied" is already caught by AuthFailure above (for HTTP 403)
        // This catches file-system permission errors specifically
        if stderr_lower.contains("permission denied")
            || stderr_lower.contains("operation not permitted")
            || stderr_lower.contains("insufficient permissions")
            || stderr_lower.contains("eacces")
            || stderr_lower.contains("eperm")
        {
            return AgentErrorKind::ToolExecutionFailed;
        }

        // GLM/CCS-specific known issues
        // These are known quirks that should trigger fallback
        // Check for CCS-specific error patterns
        if stderr_lower.contains("ccs") || stderr_lower.contains("glm") {
            // CCS/GLM with exit code 1 is likely a permission/tool issue
            if exit_code == 1 {
                return AgentErrorKind::AgentSpecificQuirk;
            }
            // CCS-specific error patterns
            if stderr_lower.contains("ccs") && stderr_lower.contains("failed") {
                return AgentErrorKind::AgentSpecificQuirk;
            }
            // GLM-specific permission errors
            if stderr_lower.contains("glm")
                && (stderr_lower.contains("permission")
                    || stderr_lower.contains("denied")
                    || stderr_lower.contains("unauthorized"))
            {
                return AgentErrorKind::AgentSpecificQuirk;
            }
        }

        // Fallback for GLM with any error and exit code 1
        if stderr_lower.contains("glm") && exit_code == 1 {
            return AgentErrorKind::AgentSpecificQuirk;
        }

        // Command not found (keep this after permission checks since permission
        // errors also contain "permission denied")
        if exit_code == 127
            || exit_code == 126
            || stderr_lower.contains("command not found")
            || stderr_lower.contains("not found")
            || stderr_lower.contains("no such file")
        {
            return AgentErrorKind::CommandNotFound;
        }

        // Transient errors (exit codes that might succeed on retry)
        // This is now a more specific catch-all for actual transient issues
        if exit_code == 1 && stderr_lower.contains("error") {
            // But only if it's not a known permanent issue pattern
            // (permission, tool failures, GLM issues are already handled above)
            return AgentErrorKind::Transient;
        }

        AgentErrorKind::Permanent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(exit_code: i32, stderr: &str) -> AgentErrorKind {
        AgentErrorKind::classify_with_agent(exit_code, stderr, None, None)
    }

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
        assert!(AgentErrorKind::ToolExecutionFailed.should_fallback());
        assert!(AgentErrorKind::AgentSpecificQuirk.should_fallback());

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
            classify(1, "rate limit exceeded"),
            AgentErrorKind::RateLimited
        );
        assert_eq!(classify(1, "error 429"), AgentErrorKind::RateLimited);

        // Auth failure
        assert_eq!(classify(1, "unauthorized"), AgentErrorKind::AuthFailure);
        assert_eq!(classify(1, "error 401"), AgentErrorKind::AuthFailure);

        // Command not found
        assert_eq!(classify(127, ""), AgentErrorKind::CommandNotFound);
        assert_eq!(
            classify(1, "command not found"),
            AgentErrorKind::CommandNotFound
        );

        // Process killed
        assert_eq!(classify(137, ""), AgentErrorKind::ProcessKilled);
        assert_eq!(classify(1, "out of memory"), AgentErrorKind::ProcessKilled);

        // Tool execution failures (NEW)
        assert_eq!(
            classify(1, "write error"),
            AgentErrorKind::ToolExecutionFailed
        );
        assert_eq!(
            classify(1, "tool failed"),
            AgentErrorKind::ToolExecutionFailed
        );
        assert_eq!(
            classify(1, "failed to write"),
            AgentErrorKind::ToolExecutionFailed
        );

        // Permission denied errors (should fallback, not retry)
        assert_eq!(
            classify(1, "permission denied"),
            AgentErrorKind::ToolExecutionFailed
        );
        assert_eq!(
            classify(1, "operation not permitted"),
            AgentErrorKind::ToolExecutionFailed
        );
        assert_eq!(
            classify(1, "insufficient permissions"),
            AgentErrorKind::ToolExecutionFailed
        );

        // "access denied" is caught by AuthFailure earlier (HTTP 403)
        assert_eq!(classify(1, "access denied"), AgentErrorKind::AuthFailure);

        // GLM-specific known issues (NEW)
        assert_eq!(classify(1, "glm error"), AgentErrorKind::AgentSpecificQuirk);
        assert_eq!(
            classify(1, "ccs glm failed"),
            AgentErrorKind::AgentSpecificQuirk
        );

        // Generic exit code 1 with "error" is now more selective
        // It should NOT match patterns that are handled above
        assert_eq!(classify(1, "some random error"), AgentErrorKind::Transient);

        assert_eq!(
            AgentErrorKind::classify_with_agent(1, "some random error", Some("ccs/glm"), None),
            AgentErrorKind::AgentSpecificQuirk
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
