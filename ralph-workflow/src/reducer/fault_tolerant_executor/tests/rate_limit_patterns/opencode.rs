use super::*;

#[test]
fn test_rate_limit_usage_limit_has_been_reached() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: "The usage limit has been reached [retryin]"
    // Documentation: No official docs - observed in production
    // Last Verified: 2026-02-06
    // How to verify:
    //   This error is emitted by OpenCode when any underlying provider
    //   (OpenAI, Anthropic, etc.) hits usage/quota limits.
    // Context:
    //   The "[retryin]" suffix is misleading - the agent is actually
    //   unavailable due to quota exhaustion and should trigger immediate
    //   agent fallback, not retry with the same agent.

    let stderr = "Error: The usage limit has been reached [retryin]";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_usage_limit_reached_short() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: "usage limit reached" (shorter variant)
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-06

    let stderr = "Error: usage limit reached";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_case_insensitive() {
    // Verify case-insensitive matching works for usage limit patterns
    let stderr = "ERROR: THE USAGE LIMIT HAS BEEN REACHED [RETRYIN]";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_rate_limit_bare_usage_limit_with_error_prefix() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Bare "usage limit" with "error:" prefix
    // Documentation: No official docs - observed in production
    // Last Verified: 2026-02-06
    // Context:
    //   Some providers emit a concise "error: usage limit" message
    //   without additional qualifying words like "reached" or "exceeded".
    //   This test verifies the bare pattern is recognized with API error context.

    let stderr = "error: usage limit";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_bare_usage_limit_with_period() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Bare "usage limit." with sentence-ending punctuation
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-06
    // Context:
    //   Sentence-ending punctuation indicates this is a standalone
    //   error message, not part of a filename or other context.

    let stderr = "Error: usage limit.";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_bare_usage_limit_with_exclamation() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Bare "usage limit!" with exclamation mark
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-06
    // Context:
    //   Exclamation mark indicates this is an error message,
    //   not part of a filename or other context.

    let stderr = "usage limit!";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_bare_usage_limit_with_semicolon() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Bare "usage limit;" with semicolon
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-06
    // Context:
    //   Semicolon indicates this is part of an error message,
    //   not part of a filename or other context.

    let stderr = "Error: usage limit; please retry later";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_bare_usage_limit_with_comma() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Bare "usage limit," with comma
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-06
    // Context:
    //   Comma indicates this is part of an error message,
    //   not part of a filename or other context.

    let stderr = "usage limit, please try again later";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_bare_usage_limit_with_http_429() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Bare "usage limit" with HTTP 429 status
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-06
    // Context:
    //   HTTP 429 status code combined with "usage limit" indicates
    //   API rate limiting, not a filename or other context.

    let stderr = "HTTP 429: usage limit";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_bare_usage_limit_case_insensitive() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Bare "USAGE LIMIT" (uppercase)
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-06
    // Context:
    //   Verify case-insensitive matching for bare "usage limit" pattern.

    let stderr = "ERROR: USAGE LIMIT";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_usage_limit_exceeded() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: "usage limit exceeded" (alternative wording)
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-12
    // Context:
    //   Some providers use "exceeded" instead of "reached" for usage limit errors.
    //   This alternative wording should be detected for comprehensive coverage.

    let stderr = "Error: usage limit exceeded";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_zen_usage_limit() {
    // Provider: OpenCode Zen
    // Error Pattern: "OpenCode Zen usage limit reached"
    // Documentation: No official docs - observed in OpenCode Zen production
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode Zen is a hosted multi-provider service that adds "Zen" or
    //   "OpenCode Zen" prefix to usage limit errors to distinguish them from
    //   direct provider errors.

    let stderr = "Error: OpenCode Zen usage limit reached";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_opencode_usage_limit() {
    // Provider: OpenCode
    // Error Pattern: "opencode usage limit has been reached"
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode gateway may emit "opencode usage limit" for quota errors.

    let stderr = "Error: opencode usage limit has been reached";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_provider_forwarded() {
    // Provider: OpenCode Multi-Provider Gateway
    // Error Pattern: "<provider>: usage limit reached"
    // Documentation: No official docs - observed in multi-provider gateway
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode multi-provider gateway forwards errors from underlying providers
    //   with a provider prefix to distinguish error sources. Examples:
    //   - "anthropic: usage limit reached"
    //   - "openai: usage limit exceeded"
    //   - "google: usage limit has been reached"

    let stderr = "Error: anthropic: usage limit reached";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_provider_forwarded_openai() {
    // Provider: OpenCode Multi-Provider Gateway (OpenAI)
    // Error Pattern: "openai: usage limit exceeded"
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-12

    let stderr = "Error: openai: usage limit exceeded";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_structured_usage_limit_exceeded_code() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Structured JSON with "usage_limit_exceeded" code
    // Documentation: No official docs - observed in OpenCode JSON error events
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode emits structured JSON errors with stable error codes.
    //   Error codes are more reliable than message text for detection.
    //   Example: {"error": {"code": "usage_limit_exceeded", "message": "..."}}

    let stderr = r#"{"error": {"code": "usage_limit_exceeded", "message": "Usage limit reached"}}"#;
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_structured_quota_exceeded_code() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Structured JSON with "quota_exceeded" code
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-12
    // Context:
    //   Some providers use "quota_exceeded" instead of "usage_limit_exceeded"
    //   to indicate quota exhaustion.

    let stderr = r#"{"error": {"code": "quota_exceeded", "message": "Quota limit reached"}}"#;
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_structured_usage_limit_reached_code() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Structured JSON with "usage_limit_reached" code
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-12
    // Context:
    //   Alternative error code using "reached" instead of "exceeded".

    let stderr =
        r#"{"error": {"code": "usage_limit_reached", "message": "Usage limit has been reached"}}"#;
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_from_stdout_json_error() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Usage limit error extracted from stdout JSON log
    // Documentation: No official docs - critical for OpenCode usage limit detection
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode emits errors as JSON to stdout rather than stderr.
    //   The error extraction logic in streaming.rs extracts these errors
    //   and passes them as stdout_error to classify_agent_error().
    //   This test verifies that usage limit errors from stdout are correctly
    //   classified as RateLimit errors.

    let stdout_error = Some("usage limit has been reached [retryin]");
    let error_kind = classify_agent_error(1, "", stdout_error);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_provider_specific_stdout() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Provider-specific usage limit error from stdout
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-12
    // Context:
    //   Tests that provider-prefixed errors from stdout are correctly classified.

    let stdout_error = Some("openai: usage limit exceeded");
    let error_kind = classify_agent_error(1, "", stdout_error);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_zen_usage_limit_stdout() {
    // Provider: OpenCode Zen
    // Error Pattern: OpenCode Zen usage limit from stdout
    // Documentation: No official docs - observed variant
    // Last Verified: 2026-02-12
    // Context:
    //   Tests that OpenCode Zen usage limit errors from stdout are correctly classified.

    let stdout_error = Some("OpenCode Zen usage limit has been reached");
    let error_kind = classify_agent_error(1, "", stdout_error);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_structured_insufficient_quota_code() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Structured JSON with "insufficient_quota" code
    // Documentation: /packages/opencode/src/provider/error.ts
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode emits "insufficient_quota" error code when OpenAI providers
    //   hit quota limits. This is parsed in parseStreamError() and returned
    //   as a non-retriable API error.
    //   Source: https://github.com/anomalyco/opencode
    //   Reference: /packages/opencode/src/provider/error.ts:93-98

    let stderr = r#"{"error": {"code": "insufficient_quota", "message": "Quota exceeded. Check your plan and billing details."}}"#;
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_insufficient_quota_message() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: "insufficient_quota" in error message
    // Documentation: /packages/opencode/src/provider/error.ts
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode may emit "insufficient_quota" as part of the error message text.
    //   This ensures detection works for both structured codes and message text.

    let stderr = "Error: insufficient_quota - Quota exceeded";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}

#[test]
fn test_rate_limit_opencode_event_type_error() {
    // Provider: OpenCode (Multi-Provider Gateway)
    // Error Pattern: Full OpenCode error event with type:"error"
    // Documentation: /packages/opencode/src/cli/cmd/run.ts:439-447
    // Last Verified: 2026-02-12
    // Context:
    //   OpenCode emits errors via session.error events in the format:
    //   {"type":"error","error":{...}}
    //   This verifies the full event structure is properly handled.
    //   Source: https://github.com/anomalyco/opencode

    let stderr =
        r#"{"type":"error","error":{"code":"insufficient_quota","message":"Quota exceeded"}}"#;
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
    assert!(is_rate_limit_error(&error_kind));
}
