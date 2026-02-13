//! Error classification for fault-tolerant agent execution.
//!
//! This module provides functions to classify errors from agent execution
//! into categories that determine retry vs fallback behavior.
//!
//! # OpenCode Usage Limit Detection
//!
//! OpenCode and multi-provider gateways emit usage limit errors when underlying
//! providers (OpenAI, Anthropic, Google, etc.) hit quota/usage limits. This module
//! provides comprehensive detection for all OpenCode error formats:
//!
//! ## Detection Methods (Priority Order)
//!
//! 1. **Structured error codes** (JSON in stdout logfile):
//!    - Primary: `usage_limit_exceeded`, `quota_exceeded`, `insufficient_quota`
//!    - Extracted via `extract_error_identifier_from_logfile()`
//!    - Most reliable when available
//!
//! 2. **Message patterns** (stderr or extracted from JSON):
//!    - "usage limit has been reached", "usage limit reached", "usage limit exceeded"
//!    - "OpenCode Zen usage limit", "opencode usage limit"
//!    - Provider-prefixed: "anthropic: usage limit reached"
//!
//! 3. **Exit codes**: OpenCode uses generic exit code 1 for most errors.
//!    - Exit codes are NOT reliable for usage limit detection
//!    - No specific exit codes for rate limits vs other errors
//!    - Exit code-based detection NOT implemented to avoid false positives
//!
//! ## Error Emission Behavior
//!
//! Based on OpenCode source analysis (2026-02-12):
//!
//! - **Primary channel**: JSON events to stdout via `--format json` logfile
//!   - Format: `{"type":"error","error":{"code":"usage_limit_exceeded"}}`
//!   - Written to `.agent/logs/*.log`
//!
//! - **Secondary channel**: stderr for some error messages
//!   - Used as fallback when logfile unavailable
//!
//! - **Exit codes**: Generic (exit code 1 for all errors)
//!   - Source: `/packages/opencode/src/cli/cmd/run.ts`
//!   - Cannot distinguish usage limits from other errors
//!
//! - **Retry logic**: `/packages/opencode/src/session/retry.ts`
//!   - Checks for `FreeUsageLimitError` in response body
//!   - Checks for `json.error?.code?.includes("rate_limit")`
//!
//! ## Edge Cases and Limitations
//!
//! - **Silent failures**: If OpenCode exits before writing error logs, detection
//!   may fall back to logging a warning about potential undetected usage limits
//! - **Empty logfiles**: Can occur if process terminates during initialization
//! - **Timing issues**: Error may appear in stderr but not in logfile if process
//!   exits before flushing stdout buffer
//! - **No heuristics**: Exit code-based detection NOT implemented because OpenCode
//!   uses exit code 1 for all errors (would cause false positives)
//!
//! ## Verification and Maintenance
//!
//! Last Verified: 2026-02-12
//! Last Updated: 2026-02-12
//!
//! To verify patterns remain accurate as OpenCode evolves:
//! 1. Check OpenCode source: <https://github.com/anomalyco/opencode>
//! 2. Review `/packages/opencode/src/cli/cmd/run.ts` for error emission
//! 3. Review `/packages/opencode/src/session/message-v2.ts` for error formats
//! 4. Review `/packages/opencode/src/session/retry.ts` for retry logic
//! 5. Test with OpenCode CLI near usage limit to observe actual messages
//! 6. Update patterns in this file if format changes

use crate::reducer::event::AgentErrorKind;
use serde_json::Value;
use std::io;

/// Classify agent error from exit code, stderr, and optional stdout content.
///
/// # Arguments
///
/// * `exit_code` - Process exit code
/// * `stderr` - Standard error output
/// * `stdout_error` - Optional error message extracted from stdout (e.g., from JSON logs)
///
/// # Stdout Error Detection
///
/// Some agents (like OpenCode) emit errors as JSON to stdout rather than stderr.
/// When `stdout_error` is provided, it is examined for rate limit patterns alongside stderr.
/// This ensures rate limit errors are properly detected regardless of output stream.
pub fn classify_agent_error(
    exit_code: i32,
    stderr: &str,
    stdout_error: Option<&str>,
) -> AgentErrorKind {
    const SIGSEGV: i32 = 139;
    const SIGABRT: i32 = 134;
    const SIGTERM: i32 = 143;

    match exit_code {
        SIGSEGV | SIGABRT => AgentErrorKind::InternalError,
        SIGTERM => AgentErrorKind::Timeout,
        _ => {
            let stderr_lower = stderr.to_lowercase();

            if is_timeout_stderr(&stderr_lower) {
                AgentErrorKind::Timeout
            } else if is_rate_limit_error_from_any_source(&stderr_lower, stderr, stdout_error) {
                // Rate limit detection must run before broad auth heuristics.
                // Some providers encode quota/rate-limit as 403 Forbidden, and we
                // still want the "429 => rate-limit policy" semantics.
                AgentErrorKind::RateLimit
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
            } else if stderr_lower.contains("network") || stderr_lower.contains("connection") {
                AgentErrorKind::Network
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

fn is_timeout_stderr(stderr_lower: &str) -> bool {
    // Be conservative: prioritize patterns that strongly indicate a timeout, and avoid
    // classifying generic network errors as timeouts unless the message says so.
    //
    // Examples observed across providers / runtimes:
    // - "Connection timeout" / "connection timed out"
    // - "timed out"
    // - "ETIMEDOUT"
    // - "deadline exceeded"
    // - "context deadline exceeded"
    contains_timeout_phrase(stderr_lower)
}

fn contains_timeout_phrase(text_lower: &str) -> bool {
    const TIMEOUT_PHRASES: [&str; 11] = [
        "timed out",
        "i/o timeout",
        "io timeout",
        "request timeout",
        "connection timeout",
        "connection timed out",
        "timeout while",
        "timeout waiting",
        "timeout occurred",
        "deadline exceeded",
        "context deadline exceeded",
    ];

    if text_lower.contains("etimedout") {
        return true;
    }

    TIMEOUT_PHRASES
        .iter()
        .any(|timeout_phrase| text_lower.contains(timeout_phrase))
}

/// Check for rate limit errors from both stderr and stdout sources.
///
/// This function examines:
/// 1. stderr (traditional error output)
/// 2. stdout_error (extracted from JSON logs, e.g., OpenCode)
///
/// This dual-source approach ensures rate limit errors are detected
/// regardless of which stream the agent uses for error reporting.
fn is_rate_limit_error_from_any_source(
    stderr_lower: &str,
    stderr_raw: &str,
    stdout_error: Option<&str>,
) -> bool {
    // Check stderr first (traditional path)
    if is_rate_limit_stderr(stderr_lower, stderr_raw) {
        return true;
    }

    // Check stdout error message if available (e.g., from OpenCode JSON logs)
    if let Some(stdout_err) = stdout_error {
        let stdout_lower = stdout_err.to_lowercase();
        if is_rate_limit_stderr(&stdout_lower, stdout_err) {
            return true;
        }
    }

    false
}

fn is_rate_limit_stderr(stderr_lower: &str, stderr_raw: &str) -> bool {
    // PRIORITY 1: Check if input is a direct error code (from JSON extraction)
    // When extract_error_identifier_from_logfile extracts error codes from OpenCode JSON,
    // it returns the bare code string (e.g., "usage_limit_exceeded", not the full JSON).
    // We must check for these codes directly before trying to parse JSON.
    if is_direct_error_code(stderr_raw) {
        return true;
    }

    // PRIORITY 2: Prefer structured JSON formats when available.
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

    // Providers sometimes emit a bare status indication (e.g., "HTTP 429") without additional
    // phrases; treat any clear HTTP/status 429 marker as RateLimit.
    if stderr_lower.contains("http 429") || stderr_lower.contains("status 429") {
        return true;
    }

    // Anthropic Claude API patterns - HTTP 529 overloaded_error (server overload)
    // Distinct from HTTP 429 rate limiting: 529 indicates temporary server capacity issues
    // that should trigger immediate agent fallback rather than retry with the same agent.
    if stderr_lower.contains("http 529")
        || stderr_lower.contains("status 529")
        || (stderr_lower.contains("overloaded")
            && (stderr_lower.contains("api") || stderr_lower.contains("server")))
    {
        return true;
    }

    // Quota exhaustion patterns - align with agents/error.rs
    //
    // Source: /packages/opencode/src/provider/error.ts
    // OpenCode emits "Quota exceeded. Check your plan and billing details."
    // for insufficient_quota errors (verified 2026-02-12)
    if stderr_lower.contains("exceeded your current quota")
        || stderr_lower.contains("quota exceeded")
        || stderr_lower.contains("insufficient_quota")
    {
        return true;
    }

    // Usage limit patterns (observed from OpenCode/multi-provider gateways)
    //
    // Bug Fix Context: OpenCode and similar multi-provider gateways emit
    // "usage limit has been reached [retryin]" when underlying providers
    // (OpenAI, Anthropic, etc.) hit quota/usage limits.
    //
    // The "[retryin]" suffix is misleading - the agent is actually unavailable
    // due to quota exhaustion and should trigger immediate agent fallback, not retry.
    //
    // Detection: Match multiple patterns:
    // 1. "usage limit has been reached" - Full phrase with timeout suffix
    // 2. "usage limit reached" - Shorter variant
    // 3. "usage limit exceeded" - Alternative wording variant
    // 4. OpenCode Zen/OpenCode-specific patterns with provider context
    // 5. Multi-provider gateway forwarding pattern (e.g., "anthropic: usage limit reached")
    // 6. Bare "usage limit" - With API error context to avoid false positives
    //
    // For the bare "usage limit" pattern, we require API error context to avoid
    // false positives from filenames (e.g., "usage_limit.rs") or non-error text.
    // Context markers: "error:" prefix, sentence punctuation, or HTTP status codes.
    //
    // Last Verified: 2026-02-12
    // Source: OpenCode production logs and multi-provider gateway behavior
    // How to verify:
    //   1. Check OpenCode source at https://github.com/anomalyco/opencode
    //   2. Review /packages/opencode/src/cli/cmd/run.ts for error emission
    //   3. Test with OpenCode CLI near usage limit to observe actual messages
    //   4. Update patterns if format changes
    //
    // Providers affected: OpenCode (multi-provider), Claude API wrappers
    // Related patterns: "quota exceeded", "rate limit exceeded"
    if stderr_lower.contains("usage limit has been reached")
        || stderr_lower.contains("usage limit reached")
    {
        return true;
    }

    // OpenCode alternative wording: "usage limit exceeded"
    // Some providers use "exceeded" instead of "reached"
    if stderr_lower.contains("usage limit exceeded") {
        return true;
    }

    // OpenCode Zen/OpenCode-specific patterns with provider context
    // Pattern: "zen usage limit" or "opencode usage limit"
    // These indicate usage limit errors from OpenCode Zen or OpenCode gateway
    if (stderr_lower.contains("zen") || stderr_lower.contains("opencode"))
        && stderr_lower.contains("usage limit")
    {
        return true;
    }

    // Multi-provider gateway forwarding pattern
    // Pattern: "<provider>: usage limit" (e.g., "anthropic: usage limit reached")
    // OpenCode multi-provider gateway forwards errors from underlying providers
    // with a provider prefix to distinguish error sources.
    //
    // IMPORTANT: This check must exclude filename patterns to avoid false positives.
    // The pattern "error: usage limit.rs file not found" should NOT match because
    // "usage limit" is followed by a file extension, not additional error context.
    if stderr_lower.contains(": usage limit") {
        // Exclude patterns where "usage limit" is followed by a file extension
        // (e.g., "error: usage limit.rs file not found")
        if !is_followed_by_file_extension_generic(stderr_lower, "usage limit") {
            return true;
        }
    }

    // Bare "usage limit" pattern with context requirements
    // Match only when in API error context to avoid false positives
    if stderr_lower.contains("usage limit") {
        // First, exclude filename patterns to avoid false positives
        // File patterns like "usage_limit.rs" or "usage limit.rs" should NOT match
        //
        // We need to check two types of filename patterns:
        // 1. Compiler/source error format: "usage_limit.rs:123" (with trailing colon)
        // 2. File-not-found format: "error: usage_limit.rs file not found" (no colon after extension)
        //
        // For both cases, we need to exclude patterns where a file extension (e.g., .rs, .py, .js, .ts, .go, .rb, .java, .cpp, .c, .php, .cs, etc.)
        // appears immediately after "usage limit" or "usage_limit" in an error context.
        //
        // We use a general pattern to match any file extension: a dot followed by 1-5 alphanumeric characters.
        // This covers all common programming language file extensions (.rs, .py, .js, .ts, .go, .rb, .java, .cpp, .c, .h, .php, .cs, .swift, .kt, .scala, .rs, .sh, .bash, .zsh, .fish, etc.)
        // and is future-proof for new file extensions.
        if is_followed_by_file_extension_generic(stderr_lower, "usage limit")
            || is_followed_by_file_extension_generic(stderr_lower, "usage_limit")
        {
            return false;
        }

        // Check for API error context markers:
        // - Preceded by "error:" or similar error indicators
        // - Followed by sentence-ending punctuation (., !, ;) but NOT file extension
        // - Preceded by HTTP status markers (already partially covered above)
        let has_error_prefix = stderr_lower.contains("error: usage limit")
            || stderr_lower.contains("usage limit.")
            || stderr_lower.contains("usage limit!")
            || stderr_lower.contains("usage limit;")
            || stderr_lower.contains("usage limit,")
            || (stderr_lower.contains("http 429") && stderr_lower.contains("usage limit"))
            || (stderr_lower.contains("status 429") && stderr_lower.contains("usage limit"));

        if has_error_prefix {
            return true;
        }
    }

    // Google Gemini API patterns - RESOURCE_EXHAUSTED status (HTTP 429)
    if stderr_lower.contains("resource_exhausted") {
        return true;
    }

    false
}

/// Check if input is a direct error code (not wrapped in JSON).
///
/// When `extract_error_identifier_from_logfile` extracts error codes from OpenCode JSON logfiles,
/// it returns the bare error code string (e.g., `"usage_limit_exceeded"`), not the full JSON.
///
/// This function detects these bare error codes so they are properly classified as RateLimit.
///
/// Supported codes:
/// - `usage_limit_exceeded`: Usage/quota limit reached
/// - `quota_exceeded`: Quota exhausted
/// - `usage_limit_reached`: Alternative usage limit code
/// - `insufficient_quota`: OpenAI quota exhaustion
/// - `rate_limit_exceeded`: Standard rate limit error
fn is_direct_error_code(text: &str) -> bool {
    // Trim whitespace to handle cases where the code might have surrounding whitespace
    let trimmed = text.trim();

    // Check if the entire string is exactly one of the known error codes
    // This avoids false positives from JSON or other text containing these codes
    matches!(
        trimmed,
        "usage_limit_exceeded"
            | "quota_exceeded"
            | "usage_limit_reached"
            | "insufficient_quota"
            | "rate_limit_exceeded"
    )
}

fn is_structured_rate_limit_error(stderr: &str) -> bool {
    // OpenCode (and some providers) emit structured JSON errors containing a stable code.
    // Example observed in CI:
    //   "✗ Error: {\"type\":\"error\",...,\"error\":{\"code\":\"rate_limit_exceeded\",...}}"
    //
    // Supported error codes:
    // - rate_limit_exceeded: Standard rate limit error (HTTP 429)
    // - usage_limit_exceeded: Usage/quota limit reached (OpenCode-specific)
    // - quota_exceeded: Quota exhausted (provider-specific)
    // - usage_limit_reached: Alternative usage limit code (OpenCode-specific)
    //
    // Error codes are more stable than message text and should be preferred
    // for detection when available.
    let Some(value) = try_parse_json_object(stderr) else {
        return false;
    };

    let code = extract_error_code(&value);

    // Standard rate limit code
    if matches!(code.as_deref(), Some("rate_limit_exceeded")) {
        return true;
    }

    // OpenCode-specific usage limit error codes
    // These indicate quota/usage exhaustion from underlying providers
    //
    // Source: /packages/opencode/src/provider/error.ts in OpenCode repository
    // - insufficient_quota: OpenAI quota exhaustion (verified 2026-02-12)
    // - usage_limit_exceeded: Generic usage limit (OpenCode gateway)
    // - quota_exceeded: Provider quota exhaustion
    // - usage_limit_reached: Alternative usage limit code
    if matches!(
        code.as_deref(),
        Some("usage_limit_exceeded")
            | Some("quota_exceeded")
            | Some("usage_limit_reached")
            | Some("insufficient_quota")
    ) {
        return true;
    }

    false
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

            if contains_timeout_phrase(&error_msg) {
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

/// Check if a pattern is followed by a file extension (with optional whitespace).
///
/// This prevents false positives where "usage limit" appears as part of a filename
/// (e.g., "error: usage limit.rs file not found" or "error: usage limit .rs file not found")
/// rather than as an API error message.
///
/// Uses a regex pattern to match any file extension (dot followed by 1-5 alphanumeric chars).
/// This covers all common programming language file extensions and is future-proof.
///
/// The regex is compiled once at startup using LazyLock for efficiency.
///
/// # Arguments
/// * `text` - The full text to search in (lowercase)
/// * `pattern` - The pattern to check (e.g., "usage limit", "usage_limit")
///
/// # Returns
/// `true` if the pattern is found and is followed by a file extension pattern
fn is_followed_by_file_extension_generic(text: &str, pattern: &str) -> bool {
    /// LazyLock for one-time regex initialization (compiled on first use, then cached).
    /// Pattern matches: optional whitespace + dot + 1-10 alphanumeric chars + non-alphanumeric or end.
    /// Matches common file extensions: .rs, .py, .js, .ts, .go, .rb, .java, .cpp, .c, .h, .php, .cs, .swift, .kt, .scala, .sh, .properties, .markdown, .terraform, etc.
    /// Handles edge case of whitespace between pattern and extension: "usage limit .rs"
    /// Updated from 1-5 to 1-10 to support longer extensions like .properties, .markdown
    static EXTENSION_REGEX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^\s*\.[a-z0-9]{1,10}([^a-z0-9]|$)").unwrap()
    });

    let Some(pos) = text.find(pattern) else {
        return false;
    };

    // Check if the character after the pattern (with optional whitespace) is a dot
    // followed by 1-5 alphanumeric chars.
    // This matches common file extensions like: .rs, .py, .js, .ts, .go, .rb, .java, .cpp, .c, .h, .php, .cs, .swift, .kt, .scala, .sh, etc.
    // Also handles the edge case where there's whitespace between the pattern and the extension
    // (e.g., "usage limit .rs file not found")
    let after_pattern = text.get(pos + pattern.len()..);
    match after_pattern {
        None | Some("") => false, // Pattern is at end of string, no extension
        Some(rest) => {
            // Check if it starts with optional whitespace, then a dot followed by 1-5 alphanumeric characters
            // The pattern is: optional whitespace + "." + [a-z0-9]{1,5}
            // After the extension, there should be a non-alphanumeric character or end of string
            EXTENSION_REGEX.is_match(rest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_agent_error_does_not_treat_filename_timeout_rs_as_timeout() {
        // Regression test: naive `contains("timeout")` matching can incorrectly classify
        // compiler/file path diagnostics (e.g., `timeout.rs:1:1`) as a timeout error.
        let error_kind = classify_agent_error(1, "timeout.rs:1:1: error: unexpected token", None);

        assert_eq!(error_kind, AgentErrorKind::InternalError);
    }
}
