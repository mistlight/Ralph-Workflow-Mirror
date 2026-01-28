//! Error classification for agent failures.
//!
//! This module provides error classification logic to determine appropriate
//! recovery strategies when agents fail. Different error types warrant
//! different responses: retry, fallback to another agent, or abort.

/// Check if a string contains a GLM-like model name.
///
/// GLM-like models include GLM, ZhipuAI, ZAI, Qwen, and DeepSeek.
/// Use this for detecting GLM models in any context (e.g., prompt selection).
/// For detecting CCS/Claude-based GLM agents specifically (error handling),
/// use `is_glm_like_agent` instead.
pub fn contains_glm_model(s: &str) -> bool {
    let s_lower = s.to_lowercase();
    s_lower.contains("glm")
        || s_lower.contains("zhipuai")
        || s_lower.contains("zai")
        || s_lower.contains("qwen")
        || s_lower.contains("deepseek")
}

/// Check if an agent is a CCS/Claude-based agent using a GLM-like model.
///
/// These agents have known compatibility issues because they use Claude CLI
/// with GLM models via CCS (Claude Code Switch). They require:
/// - The `-p` flag for non-interactive mode
/// - Special error handling for GLM-specific quirks
///
/// This does NOT match OpenCode agents using GLM models, as OpenCode has
/// its own mechanism (`--auto-approve`) and JSON format.
///
/// # Arguments
///
/// * `s` - The agent name or command string to check
///
/// # Returns
///
/// `true` if this is a CCS/Claude agent using a GLM-like model, `false` otherwise
pub fn is_glm_like_agent(s: &str) -> bool {
    let s_lower = s.to_lowercase();

    // Must contain a GLM-like model name
    if !contains_glm_model(&s_lower) {
        return false;
    }

    // Exclude OpenCode agents - they have their own mechanism
    if s_lower.starts_with("opencode") {
        return false;
    }

    // Match CCS agents (ccs/glm, ccs/zai, etc.) or claude-based commands
    s_lower.starts_with("ccs") || s_lower.contains("claude")
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
    /// Agent-specific issue that may be transient - should retry before falling back.
    RetryableAgentQuirk,
    /// Other transient error - retry.
    Transient,
    /// Permanent failure - do not retry.
    Permanent,
}

impl AgentErrorKind {
    /// Determine if this error should trigger a retry.
    ///
    /// Note: `RateLimited` is intentionally excluded - it triggers immediate agent fallback
    /// via `should_immediate_agent_fallback()` instead of retrying with the same agent.
    pub const fn should_retry(self) -> bool {
        matches!(
            self,
            Self::ApiUnavailable
                | Self::NetworkError
                | Self::Timeout
                | Self::InvalidResponse
                | Self::RetryableAgentQuirk
                | Self::Transient
        )
    }

    /// Determine if this error requires immediate agent fallback (without retry).
    ///
    /// Rate limit (429) errors indicate the current provider is temporarily exhausted.
    /// Rather than waiting and retrying the same agent (which wastes time), we should
    /// immediately switch to the next agent in the fallback chain to continue work.
    pub const fn should_immediate_agent_fallback(self) -> bool {
        matches!(self, Self::RateLimited)
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
            // RateLimited: no wait - we immediately fallback to next agent
            Self::RateLimited => 0,
            Self::ApiUnavailable => 3000, // Server issue: wait 3 seconds
            Self::NetworkError => 2000,   // Network: wait 2 seconds
            Self::Timeout | Self::Transient | Self::RetryableAgentQuirk => 1000, // Timeout/Transient: short wait
            Self::InvalidResponse => 500, // Bad response: quick retry
            _ => 0,                       // No wait for non-retryable errors
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
            Self::RetryableAgentQuirk => "Agent-specific issue (may be transient)",
            Self::Transient => "Transient error",
            Self::Permanent => "Permanent error",
        }
    }

    /// Get recovery advice for this error type.
    pub const fn recovery_advice(self) -> &'static str {
        match self {
            Self::RateLimited => {
                "Switching to next agent immediately. Rate limit indicates provider exhaustion."
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
            Self::RetryableAgentQuirk => {
                "Agent-specific issue that may be transient. Retrying... Tip: See docs/agent-compatibility.md"
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

        // Check for specific error patterns FIRST, before applying agent-specific heuristics.
        // This ensures that token exhaustion is detected even for GLM-like agents.
        if let Some(err) = Self::check_api_errors(&stderr_lower) {
            return err;
        }

        if let Some(err) = Self::check_network_errors(&stderr_lower) {
            return err;
        }

        if let Some(err) = Self::check_resource_errors(exit_code, &stderr_lower) {
            return err;
        }

        if let Some(err) = Self::check_tool_failures(&stderr_lower) {
            return err;
        }

        // If we know this is a GLM-like agent and it failed with exit code 1
        // (and we haven't matched a specific error pattern above),
        // classify based on stderr content:
        // - If stderr is empty or contains only generic messages, treat as RetryableAgentQuirk
        // - If stderr contains specific error patterns, it will be caught by check_agent_specific_quirks below
        let is_problematic_agent =
            agent_name.is_some_and(is_glm_like_agent) || model_flag.is_some_and(is_glm_like_agent);

        if is_problematic_agent && exit_code == 1 {
            // Check if stderr has known problematic patterns that indicate unrecoverable issues
            let has_known_problematic_pattern = stderr_lower.contains("permission")
                || stderr_lower.contains("denied")
                || stderr_lower.contains("unauthorized")
                || stderr_lower.contains("auth")
                || stderr_lower.contains("token")
                || stderr_lower.contains("limit")
                || stderr_lower.contains("quota")
                || stderr_lower.contains("disk")
                || stderr_lower.contains("space")
                // Agent-specific known patterns (from check_agent_specific_quirks)
                || (stderr_lower.contains("glm") && stderr_lower.contains("failed"))
                || (stderr_lower.contains("ccs") && stderr_lower.contains("failed"));

            if has_known_problematic_pattern {
                // Known issue - should fallback
                return Self::AgentSpecificQuirk;
            }

            // Unknown error - may be transient, should retry
            return Self::RetryableAgentQuirk;
        }

        if let Some(err) = Self::check_agent_specific_quirks(&stderr_lower, exit_code) {
            return err;
        }

        if let Some(err) = Self::check_command_not_found(exit_code, &stderr_lower) {
            return err;
        }

        // Transient errors (exit codes that might succeed on retry)
        // This is now a more specific catch-all for actual transient issues
        if exit_code == 1 && stderr_lower.contains("error") {
            // But only if it's not a known permanent issue pattern
            // (permission, tool failures, GLM issues are already handled above)
            return Self::Transient;
        }

        Self::Permanent
    }

    /// Check for API-level errors (rate limiting, auth, server issues).
    fn check_api_errors(stderr_lower: &str) -> Option<Self> {
        // Rate limiting indicators (API-side)
        if stderr_lower.contains("rate limit")
            || stderr_lower.contains("too many requests")
            || stderr_lower.contains("429")
            || stderr_lower.contains("quota exceeded")
        {
            return Some(Self::RateLimited);
        }

        // Token/context exhaustion (API-side)
        // Check this BEFORE GLM agent-specific fallback to ensure TokenExhausted is detected
        // Note: "too long" is specifically for API token limits, not OS argument limits
        // We exclude "argument list too long" which is an E2BIG OS error
        if stderr_lower.contains("token")
            || stderr_lower.contains("context length")
            || stderr_lower.contains("maximum context")
            || stderr_lower.contains("input too large")
            || (stderr_lower.contains("too long")
                && !stderr_lower.contains("argument list too long"))
        {
            return Some(Self::TokenExhausted);
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
            return Some(Self::AuthFailure);
        }

        None
    }

    /// Check for network and server-side errors.
    fn check_network_errors(stderr_lower: &str) -> Option<Self> {
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
            return Some(Self::NetworkError);
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
            return Some(Self::ApiUnavailable);
        }

        // Request timeout
        if stderr_lower.contains("timeout")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("request timeout")
            || stderr_lower.contains("deadline exceeded")
        {
            return Some(Self::Timeout);
        }

        None
    }

    /// Check for resource exhaustion errors (disk, memory, process, arg list).
    fn check_resource_errors(exit_code: i32, stderr_lower: &str) -> Option<Self> {
        // Disk space exhaustion
        if stderr_lower.contains("no space left")
            || stderr_lower.contains("disk full")
            || stderr_lower.contains("enospc")
            || stderr_lower.contains("out of disk")
            || stderr_lower.contains("insufficient storage")
        {
            return Some(Self::DiskFull);
        }

        // Argument list too long (E2BIG) - prompt exceeds OS limit
        // Exit code 7 is the E2BIG errno value used by spawn_agent_process
        // This should trigger fallback to another agent (the prompt size issue
        // may be transient due to XSD retry context accumulation)
        if exit_code == 7
            || stderr_lower.contains("argument list too long")
            || stderr_lower.contains("e2big")
        {
            return Some(Self::ToolExecutionFailed);
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
            return Some(Self::ProcessKilled);
        }

        None
    }

    /// Check for tool and file operation failures.
    fn check_tool_failures(stderr_lower: &str) -> Option<Self> {
        // Invalid JSON response
        if stderr_lower.contains("invalid json")
            || stderr_lower.contains("json parse")
            || stderr_lower.contains("unexpected token")
            || stderr_lower.contains("malformed")
            || stderr_lower.contains("truncated response")
            || stderr_lower.contains("incomplete response")
        {
            return Some(Self::InvalidResponse);
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
            return Some(Self::ToolExecutionFailed);
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
            return Some(Self::ToolExecutionFailed);
        }

        None
    }

    /// Check for agent-specific quirks that should trigger fallback.
    fn check_agent_specific_quirks(stderr_lower: &str, exit_code: i32) -> Option<Self> {
        // GLM/CCS-specific known issues
        // These are known quirks that should trigger fallback
        // Check for CCS-specific error patterns
        if stderr_lower.contains("ccs") || stderr_lower.contains("glm") {
            // CCS/GLM with exit code 1 is likely a permission/tool issue
            if exit_code == 1 {
                return Some(Self::AgentSpecificQuirk);
            }
            // CCS-specific error patterns
            if stderr_lower.contains("ccs") && stderr_lower.contains("failed") {
                return Some(Self::AgentSpecificQuirk);
            }
            // GLM-specific permission errors
            if stderr_lower.contains("glm")
                && (stderr_lower.contains("permission")
                    || stderr_lower.contains("denied")
                    || stderr_lower.contains("unauthorized"))
            {
                return Some(Self::AgentSpecificQuirk);
            }
        }

        // Fallback for GLM with any error and exit code 1
        if stderr_lower.contains("glm") && exit_code == 1 {
            return Some(Self::AgentSpecificQuirk);
        }

        None
    }

    /// Check for command not found errors.
    fn check_command_not_found(exit_code: i32, stderr_lower: &str) -> Option<Self> {
        // Command not found (keep this after permission checks since permission
        // errors also contain "permission denied")
        if exit_code == 127
            || exit_code == 126
            || stderr_lower.contains("command not found")
            || stderr_lower.contains("not found")
            || stderr_lower.contains("no such file")
        {
            return Some(Self::CommandNotFound);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(exit_code: i32, stderr: &str) -> AgentErrorKind {
        AgentErrorKind::classify_with_agent(exit_code, stderr, None, None)
    }

    #[test]
    fn test_is_glm_like_agent() {
        // CCS GLM agents - should match
        assert!(is_glm_like_agent("ccs/glm"));
        assert!(is_glm_like_agent("ccs/zai"));
        assert!(is_glm_like_agent("ccs/zhipuai"));
        assert!(is_glm_like_agent("ccs/qwen"));
        assert!(is_glm_like_agent("ccs/deepseek"));
        assert!(is_glm_like_agent("CCS/GLM")); // case insensitive

        // Claude with GLM model flag - should match
        assert!(is_glm_like_agent("claude -m glm-4"));

        // OpenCode agents with GLM - should NOT match (OpenCode has own mechanism)
        assert!(!is_glm_like_agent("opencode/opencode/glm-4.7-free"));
        assert!(!is_glm_like_agent("opencode/zai/glm-4.7"));
        assert!(!is_glm_like_agent("opencode run -m glm"));

        // Non-GLM agents - should NOT match
        assert!(!is_glm_like_agent("claude"));
        assert!(!is_glm_like_agent("codex"));
        assert!(!is_glm_like_agent("ccs/work"));
        assert!(!is_glm_like_agent("ccs/personal"));

        // Model name alone without ccs/claude - should NOT match
        assert!(!is_glm_like_agent("glm-4.7-free"));
        assert!(!is_glm_like_agent("zai/glm-4.7"));
    }

    #[test]
    fn test_agent_error_kind_should_retry() {
        // RateLimited should NOT retry - it triggers immediate agent fallback
        assert!(!AgentErrorKind::RateLimited.should_retry());
        assert!(AgentErrorKind::ApiUnavailable.should_retry());
        assert!(AgentErrorKind::NetworkError.should_retry());
        assert!(AgentErrorKind::Timeout.should_retry());
        assert!(AgentErrorKind::InvalidResponse.should_retry());
        assert!(AgentErrorKind::Transient.should_retry());
        assert!(AgentErrorKind::RetryableAgentQuirk.should_retry());

        assert!(!AgentErrorKind::AuthFailure.should_retry());
        assert!(!AgentErrorKind::CommandNotFound.should_retry());
        assert!(!AgentErrorKind::Permanent.should_retry());
    }

    #[test]
    fn test_agent_error_kind_should_immediate_agent_fallback() {
        // Only RateLimited should trigger immediate agent fallback
        assert!(AgentErrorKind::RateLimited.should_immediate_agent_fallback());

        // All other errors should not trigger immediate agent fallback
        assert!(!AgentErrorKind::ApiUnavailable.should_immediate_agent_fallback());
        assert!(!AgentErrorKind::NetworkError.should_immediate_agent_fallback());
        assert!(!AgentErrorKind::Timeout.should_immediate_agent_fallback());
        assert!(!AgentErrorKind::AuthFailure.should_immediate_agent_fallback());
        assert!(!AgentErrorKind::TokenExhausted.should_immediate_agent_fallback());
        assert!(!AgentErrorKind::CommandNotFound.should_immediate_agent_fallback());
        assert!(!AgentErrorKind::Permanent.should_immediate_agent_fallback());
        assert!(!AgentErrorKind::Transient.should_immediate_agent_fallback());
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

        // Argument list too long (E2BIG) - should trigger fallback
        assert_eq!(
            classify(7, "argument list too long"),
            AgentErrorKind::ToolExecutionFailed
        );
        assert_eq!(
            classify(
                7,
                "opencode: Argument list too long (prompt exceeds OS limit)"
            ),
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

        // GLM with unknown error (no specific pattern) should be RetryableAgentQuirk
        assert_eq!(
            AgentErrorKind::classify_with_agent(1, "some random error", Some("ccs/glm"), None),
            AgentErrorKind::RetryableAgentQuirk
        );

        // GLM with known problematic patterns - permission denied is caught by check_tool_failures first
        assert_eq!(
            AgentErrorKind::classify_with_agent(1, "permission denied", Some("ccs/glm"), None),
            AgentErrorKind::ToolExecutionFailed // Caught by earlier check
        );
        assert_eq!(
            AgentErrorKind::classify_with_agent(1, "token limit exceeded", Some("ccs/glm"), None),
            AgentErrorKind::TokenExhausted // Caught by earlier check
        );
        assert_eq!(
            AgentErrorKind::classify_with_agent(1, "disk full", Some("ccs/glm"), None),
            AgentErrorKind::DiskFull // Caught by earlier check (disk pattern)
        );
        // GLM mentioned in stderr with "failed" - AgentSpecificQuirk
        assert_eq!(
            AgentErrorKind::classify_with_agent(1, "glm failed", Some("ccs/glm"), None),
            AgentErrorKind::AgentSpecificQuirk
        );
    }

    #[test]
    fn test_opencode_error_classification_not_treated_as_glm() {
        // OpenCode agents should NOT be treated as GLM-like for error classification
        // They should get normal error classification, not RetryableAgentQuirk

        // OpenCode with exit code 1 and generic error - should be Transient, not RetryableAgentQuirk
        assert_eq!(
            AgentErrorKind::classify_with_agent(
                1,
                "some error occurred",
                Some("opencode/opencode/glm-4.7-free"),
                None
            ),
            AgentErrorKind::Transient
        );

        // OpenCode with exit code 1 and no error in stderr - should be Permanent
        assert_eq!(
            AgentErrorKind::classify_with_agent(
                1,
                "something happened",
                Some("opencode/opencode/glm-4.7-free"),
                None
            ),
            AgentErrorKind::Permanent
        );

        // OpenCode with rate limit - should be RateLimited
        assert_eq!(
            AgentErrorKind::classify_with_agent(
                1,
                "rate limit exceeded",
                Some("opencode/zai/glm-4.7"),
                None
            ),
            AgentErrorKind::RateLimited
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
        // RateLimited: 0ms wait (immediate agent fallback, no retry)
        assert_eq!(AgentErrorKind::RateLimited.suggested_wait_ms(), 0);
        assert_eq!(AgentErrorKind::Permanent.suggested_wait_ms(), 0);
        // Retriable errors should have positive wait times
        assert!(AgentErrorKind::ApiUnavailable.suggested_wait_ms() > 0);
        assert!(AgentErrorKind::NetworkError.suggested_wait_ms() > 0);
    }

    #[test]
    fn test_agent_error_kind_suggests_smaller_context() {
        assert!(AgentErrorKind::TokenExhausted.suggests_smaller_context());
        assert!(AgentErrorKind::ProcessKilled.suggests_smaller_context());
        assert!(!AgentErrorKind::RateLimited.suggests_smaller_context());
    }
}
