//! Error classification for agent failures.
//!
//! This module provides error classification logic to determine appropriate
//! recovery strategies when agents fail. Different error types warrant
//! different responses: retry, fallback to another agent, or abort.

/// Check if an agent name or command string indicates a GLM-like agent.
///
/// GLM-like agents include GLM, `ZhipuAI`, ZAI, Qwen, and `DeepSeek`.
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
pub fn is_glm_like_agent(s: &str) -> bool {
    let s_lower = s.to_lowercase();
    s_lower.contains("glm")
        || s_lower.contains("zhipuai")
        || s_lower.contains("zai")
        || s_lower.contains("qwen")
        || s_lower.contains("deepseek")
}

/// Check if an agent name indicates OpenAI Codex.
///
/// Codex is OpenAI's coding assistant that may have special compatibility
/// considerations with container security modes.
///
/// # Arguments
///
/// * `s` - The agent name or command string to check
///
/// # Returns
///
/// `true` if the string indicates a Codex agent, `false` otherwise
pub fn is_codex_agent(s: &str) -> bool {
    let s_lower = s.to_lowercase();
    s_lower.contains("codex")
        || s_lower.contains("codeex")
        || s_lower.contains("code-x")
        || s_lower.contains("openai")
}

/// Error classification for agent failures.
///
/// Used to determine appropriate recovery strategy when an agent fails:
/// - `should_retry()` - Try same agent again after delay
/// - `should_fallback()` - Switch to next agent in the chain
/// - `is_unrecoverable()` - Abort the pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorKind {
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
    /// Check if stderr matches rate limiting patterns.
    fn matches_rate_limit(stderr_lower: &str) -> bool {
        stderr_lower.contains("rate limit")
            || stderr_lower.contains("too many requests")
            || stderr_lower.contains("429")
            || stderr_lower.contains("quota exceeded")
    }

    /// Check if stderr matches token exhaustion patterns.
    fn matches_token_exhausted(stderr_lower: &str) -> bool {
        stderr_lower.contains("token")
            || stderr_lower.contains("context length")
            || stderr_lower.contains("maximum context")
            || stderr_lower.contains("too long")
            || stderr_lower.contains("input too large")
    }

    /// Check if stderr matches network error patterns.
    fn matches_network_error(stderr_lower: &str) -> bool {
        stderr_lower.contains("connection refused")
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
    }

    /// Check if stderr matches API unavailable patterns.
    fn matches_api_unavailable(stderr_lower: &str) -> bool {
        stderr_lower.contains("service unavailable")
            || stderr_lower.contains("503")
            || stderr_lower.contains("502")
            || stderr_lower.contains("504")
            || stderr_lower.contains("500")
            || stderr_lower.contains("internal server error")
            || stderr_lower.contains("bad gateway")
            || stderr_lower.contains("gateway timeout")
            || stderr_lower.contains("overloaded")
            || stderr_lower.contains("maintenance")
    }

    /// Check if stderr matches timeout patterns.
    fn matches_timeout(stderr_lower: &str) -> bool {
        stderr_lower.contains("timeout")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("request timeout")
            || stderr_lower.contains("deadline exceeded")
    }

    /// Check if stderr matches authentication failure patterns.
    fn matches_auth_failure(stderr_lower: &str) -> bool {
        stderr_lower.contains("unauthorized")
            || stderr_lower.contains("authentication")
            || stderr_lower.contains("401")
            || stderr_lower.contains("api key")
            || stderr_lower.contains("invalid token")
            || stderr_lower.contains("forbidden")
            || stderr_lower.contains("403")
            || stderr_lower.contains("access denied")
    }

    /// Check if stderr matches disk full patterns.
    fn matches_disk_full(stderr_lower: &str) -> bool {
        stderr_lower.contains("no space left")
            || stderr_lower.contains("disk full")
            || stderr_lower.contains("enospc")
            || stderr_lower.contains("out of disk")
            || stderr_lower.contains("insufficient storage")
    }

    /// Check if exit code or stderr matches process killed patterns.
    fn matches_process_killed(exit_code: i32, stderr_lower: &str) -> bool {
        exit_code == 137
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
    }

    /// Check if stderr matches invalid JSON response patterns.
    fn matches_invalid_response(stderr_lower: &str) -> bool {
        stderr_lower.contains("invalid json")
            || stderr_lower.contains("json parse")
            || stderr_lower.contains("unexpected token")
            || stderr_lower.contains("malformed")
            || stderr_lower.contains("truncated response")
            || stderr_lower.contains("incomplete response")
    }

    /// Check if stderr matches tool execution failure patterns.
    fn matches_tool_execution_failed(stderr_lower: &str) -> bool {
        stderr_lower.contains("write error")
            || stderr_lower.contains("cannot write")
            || stderr_lower.contains("failed to write")
            || stderr_lower.contains("unable to create file")
            || stderr_lower.contains("file creation failed")
            || stderr_lower.contains("i/o error")
            || stderr_lower.contains("io error")
            || stderr_lower.contains("tool failed")
            || stderr_lower.contains("tool execution failed")
            || stderr_lower.contains("tool call failed")
    }

    /// Check if stderr matches permission denied patterns.
    fn matches_permission_denied(stderr_lower: &str) -> bool {
        stderr_lower.contains("permission denied")
            || stderr_lower.contains("operation not permitted")
            || stderr_lower.contains("insufficient permissions")
            || stderr_lower.contains("eacces")
            || stderr_lower.contains("eperm")
    }

    /// Check if stderr matches command not found patterns.
    fn matches_command_not_found(exit_code: i32, stderr_lower: &str) -> bool {
        exit_code == 127
            || exit_code == 126
            || stderr_lower.contains("command not found")
            || stderr_lower.contains("not found")
            || stderr_lower.contains("no such file")
    }

    /// Check if stderr matches transient error patterns.
    fn matches_transient(stderr_lower: &str) -> bool {
        stderr_lower.contains("connection reset")
            || stderr_lower.contains("connection refused")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("timeout")
            || stderr_lower.contains("temporary")
            || stderr_lower.contains("temporarily")
            || stderr_lower.contains("unavailable")
            || stderr_lower.contains("try again")
            || stderr_lower.contains("try again later")
    }

    /// Check if stderr matches GLM/CCS-specific error patterns.
    fn matches_glm_ccs_specific(stderr_lower: &str, exit_code: i32) -> bool {
        if stderr_lower.contains("ccs") || stderr_lower.contains("glm") {
            if exit_code == 1 {
                return true;
            }
            if stderr_lower.contains("ccs") && stderr_lower.contains("failed") {
                return true;
            }
            if stderr_lower.contains("glm")
                && (stderr_lower.contains("permission")
                    || stderr_lower.contains("denied")
                    || stderr_lower.contains("unauthorized"))
            {
                return true;
            }
        }
        false
    }

    /// Determine if this error should trigger a retry.
    pub const fn should_retry(self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::ApiUnavailable
                | Self::NetworkError
                | Self::Timeout
                | Self::InvalidResponse
                | Self::Transient
        )
    }

    /// Determine if this error should trigger a fallback to another agent.
    pub const fn should_fallback(self) -> bool {
        matches!(
            self,
            Self::TokenExhausted
                | Self::AuthFailure
                | Self::CommandNotFound
                | Self::ProcessKilled
                | Self::ToolExecutionFailed
                | Self::AgentSpecificQuirk
        )
    }

    /// Determine if this error is unrecoverable (should abort).
    pub const fn is_unrecoverable(self) -> bool {
        matches!(self, Self::DiskFull | Self::Permanent)
    }

    /// Check if this is a command not found error.
    pub const fn is_command_not_found(self) -> bool {
        matches!(self, Self::CommandNotFound)
    }

    /// Check if this is a network-related error.
    pub const fn is_network_error(self) -> bool {
        matches!(self, Self::NetworkError | Self::Timeout)
    }

    /// Check if this error might be resolved by reducing context size.
    pub const fn suggests_smaller_context(self) -> bool {
        matches!(self, Self::TokenExhausted | Self::ProcessKilled)
    }

    /// Get suggested wait time in milliseconds before retry.
    pub const fn suggested_wait_ms(self) -> u64 {
        match self {
            Self::RateLimited => 5000,               // Rate limit: wait 5 seconds
            Self::ApiUnavailable => 3000,            // Server issue: wait 3 seconds
            Self::NetworkError => 2000,              // Network: wait 2 seconds
            Self::Timeout | Self::Transient => 1000, // Timeout/Transient: 1 second
            Self::InvalidResponse => 500,            // Bad response: quick retry
            _ => 0,                                  // No wait for non-retryable errors
        }
    }

    /// Get a user-friendly description of this error type.
    pub const fn description(self) -> &'static str {
        match self {
            Self::RateLimited => "API rate limit exceeded",
            Self::TokenExhausted => "Token/context limit exceeded",
            Self::ApiUnavailable => "API service temporarily unavailable",
            Self::NetworkError => "Network connectivity issue",
            Self::AuthFailure => "Authentication failure",
            Self::CommandNotFound => "Command not found",
            Self::DiskFull => "Disk space exhausted",
            Self::ProcessKilled => "Process terminated (possibly OOM)",
            Self::InvalidResponse => "Invalid response from agent",
            Self::Timeout => "Request timed out",
            Self::ToolExecutionFailed => "Tool execution failed (e.g., file write)",
            Self::AgentSpecificQuirk => "Known agent-specific issue",
            Self::Transient => "Transient error",
            Self::Permanent => "Permanent error",
        }
    }

    /// Get recovery advice for this error type.
    pub const fn recovery_advice(self) -> &'static str {
        match self {
            Self::RateLimited => {
                "Will retry after delay. Tip: Consider reducing request frequency or using a different provider."
            }
            Self::TokenExhausted => {
                "Switching to alternative agent. Tip: Try RALPH_DEVELOPER_CONTEXT=0 or RALPH_REVIEWER_CONTEXT=0"
            }
            Self::ApiUnavailable => {
                "API server issue. Will retry automatically. Tip: Check status page or try different provider."
            }
            Self::NetworkError => {
                "Check your internet connection. Will retry automatically. Tip: Check firewall/VPN settings."
            }
            Self::AuthFailure => {
                "Check API key or run 'agent auth' to authenticate. Tip: Verify credentials for this provider."
            }
            Self::CommandNotFound => {
                "Agent binary not installed. See installation guidance below. Tip: Run 'ralph --list-available-agents'"
            }
            Self::DiskFull => "Free up disk space and try again. Tip: Check .agent directory size.",
            Self::ProcessKilled => {
                "Process was killed (possible OOM). Trying with smaller context. Tip: Reduce context with RALPH_*_CONTEXT=0"
            }
            Self::InvalidResponse => {
                "Received malformed response. Retrying... Tip: May indicate parser mismatch with this agent."
            }
            Self::Timeout => {
                "Request timed out. Will retry with longer timeout. Tip: Try reducing prompt size or context."
            }
            Self::ToolExecutionFailed => {
                "Tool execution failed (file write/permissions). Switching agent. Tip: Check directory write permissions."
            }
            Self::AgentSpecificQuirk => {
                "Known agent-specific issue. Switching to alternative agent. Tip: See docs/agent-compatibility.md"
            }
            Self::Transient => "Temporary issue. Will retry automatically.",
            Self::Permanent => {
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
            return Self::AgentSpecificQuirk;
        }

        // Check various error patterns using helper methods
        if Self::matches_rate_limit(&stderr_lower) {
            return Self::RateLimited;
        }

        if Self::matches_token_exhausted(&stderr_lower) {
            return Self::TokenExhausted;
        }

        if Self::matches_network_error(&stderr_lower) {
            return Self::NetworkError;
        }

        if Self::matches_api_unavailable(&stderr_lower) {
            return Self::ApiUnavailable;
        }

        if Self::matches_timeout(&stderr_lower) {
            return Self::Timeout;
        }

        if Self::matches_auth_failure(&stderr_lower) {
            return Self::AuthFailure;
        }

        if Self::matches_disk_full(&stderr_lower) {
            return Self::DiskFull;
        }

        if Self::matches_process_killed(exit_code, &stderr_lower) {
            return Self::ProcessKilled;
        }

        if Self::matches_invalid_response(&stderr_lower) {
            return Self::InvalidResponse;
        }

        if Self::matches_tool_execution_failed(&stderr_lower) {
            return Self::ToolExecutionFailed;
        }

        if Self::matches_permission_denied(&stderr_lower) {
            return Self::ToolExecutionFailed;
        }

        if Self::matches_glm_ccs_specific(&stderr_lower, exit_code) {
            return Self::AgentSpecificQuirk;
        }

        // Fallback for GLM with any error and exit code 1
        if stderr_lower.contains("glm") && exit_code == 1 {
            return Self::AgentSpecificQuirk;
        }

        if Self::matches_command_not_found(exit_code, &stderr_lower) {
            return Self::CommandNotFound;
        }

        // Transient errors (exit codes that might succeed on retry)
        if exit_code == 1 && stderr_lower.contains("error") {
            return Self::Transient;
        }

        if Self::matches_transient(&stderr_lower) {
            return Self::Transient;
        }

        // Default to Permanent for unknown errors
        Self::Permanent
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

    #[test]
    fn test_is_glm_like_agent() {
        assert!(is_glm_like_agent("glm"));
        assert!(is_glm_like_agent("zhipuai"));
        assert!(is_glm_like_agent("ZAI"));
        assert!(is_glm_like_agent("qwen"));
        assert!(is_glm_like_agent("deepseek"));

        assert!(!is_glm_like_agent("claude"));
        assert!(!is_glm_like_agent("codex"));
    }

    #[test]
    fn test_is_codex_agent() {
        assert!(is_codex_agent("codex"));
        assert!(is_codex_agent("codeex"));
        assert!(is_codex_agent("CODE-X"));
        assert!(is_codex_agent("openai"));

        assert!(!is_codex_agent("claude"));
        assert!(!is_codex_agent("glm"));
    }
}
