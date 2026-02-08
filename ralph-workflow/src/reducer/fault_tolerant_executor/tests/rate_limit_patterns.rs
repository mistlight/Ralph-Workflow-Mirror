//! Rate limit pattern classification tests
//!
//! Comprehensive tests for rate limit error detection across different providers:
//! - OpenCode gateway patterns (usage limit, quota exhaustion)
//! - OpenAI patterns (HTTP 429, rate_limit_exceeded)
//! - Anthropic patterns (rate_limit_error, overloaded_error)
//! - Google Gemini patterns (RESOURCE_EXHAUSTED)
//! - Azure OpenAI patterns (quota exceeded, throttled)
//! - Generic HTTP patterns (HTTP 429, status 429)
//! - Negative cases (usage_limit in filenames, comments, non-error contexts)
//!
//! Each test documents the provider, error pattern, official documentation,
//! and verification steps to ensure accuracy.

use super::*;

mod opencode {
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
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_rate_limit_case_insensitive() {
        // Verify case-insensitive matching works for usage limit patterns
        let stderr = "ERROR: THE USAGE LIMIT HAS BEEN REACHED [RETRYIN]";
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
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
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

/// OpenAI API rate limit patterns
mod openai {
    use super::*;

    #[test]
    fn test_rate_limit_openai_rate_limit_reached() {
        // Provider: OpenAI API
        // Error Pattern: "Rate limit reached for requests"
        // Documentation: https://platform.openai.com/docs/guides/error-codes
        //   Section: "ERROR 429 - Rate limit reached for requests"
        // Last Verified: 2026-02-06
        // How to verify:
        //   1. Visit https://platform.openai.com/docs/guides/error-codes
        //   2. Search for "429" or "rate limit"
        //   3. Verify exact error message text in documentation
        //   4. Update this test if message has changed

        let stderr = "Error: Rate limit reached for requests";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_rate_limit_openai_quota_exceeded() {
        // Provider: OpenAI API
        // Error Pattern: "You exceeded your current quota"
        // Documentation: https://platform.openai.com/docs/guides/error-codes
        //   Section: "ERROR 429 - You exceeded your current quota"
        // Last Verified: 2026-02-06

        let stderr =
            "Error: You exceeded your current quota, please check your plan and billing details";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

/// Anthropic Claude API rate limit patterns
mod anthropic {
    use super::*;

    #[test]
    fn test_rate_limit_anthropic_429() {
        // Provider: Anthropic Claude API
        // Error Pattern: HTTP 429 with rate_limit_error
        // Documentation: https://docs.anthropic.com/en/api/errors
        //   HTTP Code: 429 - rate_limit_error (too many requests)
        // Last Verified: 2026-02-06
        // How to verify:
        //   1. Visit https://docs.anthropic.com/en/api/errors
        //   2. Search for "429" or "rate_limit_error"
        //   3. Verify HTTP codes and error types

        let stderr = "HTTP 429: rate_limit_error - Too many requests";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_rate_limit_anthropic_529_overloaded() {
        // Provider: Anthropic Claude API
        // Error Pattern: HTTP 529 with overloaded_error
        // Documentation: https://docs.anthropic.com/en/api/errors
        //   HTTP Code: 529 - overloaded_error (server capacity exceeded)
        // Last Verified: 2026-02-06
        // How to verify:
        //   1. Visit https://docs.anthropic.com/en/api/errors
        //   2. Search for "529" or "overloaded_error"
        //   3. Verify HTTP codes and error types
        //   4. Confirm this is distinct from 429 rate limiting

        let stderr = "HTTP 529: overloaded_error - The API is temporarily overloaded";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_rate_limit_anthropic_overloaded_no_status() {
        // Provider: Anthropic Claude API
        // Error Pattern: "overloaded" without explicit HTTP status
        // Documentation: https://docs.anthropic.com/en/api/errors
        //   Message variant: "The API is temporarily overloaded"
        // Last Verified: 2026-02-06
        // Context: Some error messages may not include explicit HTTP status code
        //   but still indicate server overload via "overloaded" keyword

        let stderr = "Error: The API is temporarily overloaded, please retry after some time";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_rate_limit_anthropic_structured_json() {
        // Provider: Anthropic Claude API
        // Error Pattern: Structured JSON with code "rate_limit_exceeded"
        // Documentation: https://docs.anthropic.com/en/api/errors
        //   JSON structure: {"error": {"code": "rate_limit_exceeded"}}
        // Last Verified: 2026-02-06

        let stderr = r#"{"error": {"type": "rate_limit_error", "code": "rate_limit_exceeded", "message": "Rate limit exceeded"}}"#;
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

/// Google Gemini API rate limit patterns
mod google {
    use super::*;

    #[test]
    fn test_rate_limit_gemini_resource_exhausted() {
        // Provider: Google Gemini API
        // Error Pattern: HTTP 429 with RESOURCE_EXHAUSTED
        // Documentation: https://ai.google.dev/gemini-api/docs/troubleshooting
        //   Status: RESOURCE_EXHAUSTED (HTTP 429)
        // Last Verified: 2026-02-06
        // How to verify:
        //   1. Visit https://ai.google.dev/gemini-api/docs/troubleshooting
        //   2. Search for "RESOURCE_EXHAUSTED" or "429"
        //   3. Verify status codes in error table

        let stderr = "Error: RESOURCE_EXHAUSTED: You've exceeded the rate limit";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

/// Azure OpenAI rate limit patterns
mod azure {
    use super::*;

    #[test]
    fn test_rate_limit_azure_openai() {
        // Provider: Azure OpenAI
        // Error Pattern: Inherits from OpenAI - "Rate limit reached"
        // Documentation: https://learn.microsoft.com/en-us/azure/ai-services/openai/quotas-limits
        //   HTTP Code: 429 - Rate limit patterns similar to OpenAI
        // Last Verified: 2026-02-06
        // Note: Azure OpenAI uses similar error messages to OpenAI API

        let stderr = "Error: Rate limit reached for requests. Please retry after some time.";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

/// Generic HTTP 429 patterns (standard)
mod generic_http {
    use super::*;

    #[test]
    fn test_rate_limit_generic_429_too_many_requests() {
        // Provider: Generic HTTP standard
        // Error Pattern: "too many requests" (standard HTTP 429 message)
        // Documentation: RFC 6585 - HTTP Status Code 429
        // Last Verified: 2026-02-06

        let stderr = "Error: too many requests, please slow down";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_rate_limit_http_429_status() {
        // Provider: Generic HTTP standard
        // Error Pattern: "HTTP 429" or "status 429"
        // Documentation: RFC 6585 - HTTP Status Code 429
        // Last Verified: 2026-02-06

        let stderr = "HTTP 429 - Too Many Requests";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

/// Negative test cases - patterns that should NOT match rate limit
///
/// These tests prevent false positives by ensuring the pattern matching
/// is precise and only triggers for actual API rate limit errors.
mod negative_cases {
    use super::*;

    #[test]
    fn test_auth_error_with_quota_in_message_not_rate_limit() {
        // Authentication errors take precedence even if "quota" keyword appears
        // in the error message. This prevents false positives when error messages
        // mention quota information but the root cause is authentication failure.
        //
        // Classification Priority: Authentication > RateLimit
        // Expected: AgentErrorKind::Authentication
        let stderr = "HTTP 401 Unauthorized: API key quota information unavailable";
        let error_kind = classify_agent_error(1, stderr);
        assert_eq!(error_kind, AgentErrorKind::Authentication);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_filename_with_rate_limit_not_rate_limit() {
        // File paths and source code locations should not trigger rate limit detection,
        // even if they contain keywords like "rate_limit.rs".
        //
        // Context: Compiler errors, linter messages, and stack traces often include
        // file paths that may contain rate_limit keywords but are not API errors.
        //
        // Expected: ParsingError or InternalError, NOT RateLimit
        let stderr = "rate_limit.rs:123:1: syntax error: unexpected token";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as ParsingError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_filename_with_usage_limit_not_rate_limit() {
        // File paths and source code locations should not trigger rate limit detection,
        // even if they contain keywords like "usage_limit.rs".
        //
        // Context: Compiler errors, linter messages, and stack traces often include
        // file paths that may contain usage_limit keywords but are not API errors.
        //
        // This test ensures parity with test_filename_with_rate_limit_not_rate_limit
        // for the "usage limit" patterns added in the bug fix.
        //
        // Expected: ParsingError or InternalError, NOT RateLimit
        let stderr = "usage_limit.rs:123:1: syntax error: unexpected token";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as ParsingError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_connection_limit_not_rate_limit() {
        // Network connection pool limits are distinct from API rate limits.
        // Connection pool exhaustion is a client-side resource issue, not a
        // provider-enforced rate limit.
        //
        // Context: Database connection pools, HTTP client connection pools, etc.
        // may emit "limit reached" messages that should NOT trigger agent fallback.
        //
        // Expected: Network or InternalError, NOT RateLimit
        let stderr = "Connection pool limit reached: max 100 connections";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as Network or InternalError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_file_size_limit_not_rate_limit() {
        // File system limits (file size, disk quota, etc.) are not API rate limits.
        // These errors indicate local storage issues, not provider throttling.
        //
        // Context: File uploads, disk writes, temporary file creation may fail
        // with "limit exceeded" messages that are unrelated to API rate limiting.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "File size limit exceeded: maximum 10MB";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_system_overload_not_rate_limit() {
        // System resource overload (CPU, memory, etc.) should not trigger API rate
        // limit detection. These are local system issues, not provider constraints.
        //
        // Context: High CPU usage, memory pressure, disk I/O saturation may produce
        // "overload" or "throttled" messages that are distinct from API overload (HTTP 529).
        //
        // Expected: InternalError, NOT RateLimit
        let stderr = "Error: System CPU overload detected, process throttled";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as InternalError or other, NOT RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_bare_usage_limit_without_context_not_rate_limit() {
        // Bare "usage limit" without API error context should NOT match.
        //
        // Context: The bare "usage limit" pattern requires API error context
        // (error prefix, punctuation, HTTP status) to avoid false positives.
        // Without such context, it should NOT be classified as RateLimit.
        //
        // Expected: InternalError or other, NOT RateLimit
        let stderr = "usage limit";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as InternalError or other, NOT RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_in_filename_not_rate_limit() {
        // "usage limit" appearing in a filename context should NOT match.
        //
        // Context: Even though the test uses "usage limit" with space (not underscore),
        // it should NOT match because it appears in a filename/source location context,
        // not an API error context.
        //
        // Expected: ParsingError or InternalError, NOT RateLimit
        let stderr = "usage limit.rs:123:1: syntax error: unexpected token";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as ParsingError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_in_comment_not_rate_limit() {
        // "usage limit" appearing in code comments or documentation should NOT match.
        //
        // Context: Comments, documentation, or log messages that mention "usage limit"
        // but are not actual API error responses should NOT trigger rate limit detection.
        //
        // Expected: InternalError, NOT RateLimit
        let stderr = "// TODO: Handle usage limit gracefully\nerror: internal error";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as InternalError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_in_error_prefix_not_rate_limit() {
        // "error: usage limit.rs file not found" should NOT trigger rate limit detection.
        //
        // Context: This is a file-not-found error where "usage limit.rs" is a filename,
        // not an API usage limit error. The pattern "error: usage limit" followed by a
        // file extension (.rs, .py, .js) indicates a filename context, not an API error.
        //
        // Bug Fix Context:
        // The bare "error: usage limit" check on line 184 of error_classification.rs
        // uses contains() which matches "error: usage limit.rs file not found" because
        // it contains "error: usage limit". The filename exclusion on lines 170-176 only
        // catches patterns with a trailing colon (compiler error format like
        // "usage limit.rs:123"), but file-not-found errors don't include the colon.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.rs file not found";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as FileSystem or InternalError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_with_space_in_error_prefix_not_rate_limit() {
        // "error: usage limit.py file not found" - variant with space in filename.
        //
        // Context: Similar to test_usage_limit_filename_in_error_prefix_not_rate_limit
        // but with .py extension instead of .rs extension.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.py file not found";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as FileSystem or InternalError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_with_underscore_in_error_prefix_not_rate_limit() {
        // "error: usage_limit.js file not found" - variant with underscore in filename.
        //
        // Context: Similar to test_usage_limit_filename_in_error_prefix_not_rate_limit
        // but with underscore (usage_limit) instead of space (usage limit) and .js extension.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage_limit.js file not found";
        let error_kind = classify_agent_error(1, stderr);
        // Should classify as FileSystem or InternalError, not RateLimit
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_go_extension_not_rate_limit() {
        // "error: usage limit.go file not found" - Go file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .go files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.go file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_rb_extension_not_rate_limit() {
        // "error: usage_limit.rb file not found" - Ruby file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .rb files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage_limit.rb file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_java_extension_not_rate_limit() {
        // "error: usage limit.java file not found" - Java file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .java files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.java file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_cpp_extension_not_rate_limit() {
        // "error: usage limit.cpp file not found" - C++ file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .cpp files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.cpp file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_c_extension_not_rate_limit() {
        // "error: usage limit.c file not found" - C file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .c files from rate limit detection. Note that single-letter
        // extensions are a valid edge case.
        //
        // This test uses "usage limit.c" (with space) to verify that the
        // file extension detection correctly excludes this pattern from
        // rate limit classification. Without proper file extension detection,
        // this would incorrectly match "error: usage limit" and be classified
        // as a RateLimit error.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.c file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_php_extension_not_rate_limit() {
        // "error: usage limit.php file not found" - PHP file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .php files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.php file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_cs_extension_not_rate_limit() {
        // "error: usage_limit.cs file not found" - C# file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .cs files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage_limit.cs file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_swift_extension_not_rate_limit() {
        // "error: usage limit.swift file not found" - Swift file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .swift files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.swift file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_kt_extension_not_rate_limit() {
        // "error: usage_limit.kt file not found" - Kotlin file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .kt files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage_limit.kt file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_scala_extension_not_rate_limit() {
        // "error: usage limit.scala file not found" - Scala file extension (5 chars).
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .scala files from rate limit detection. This tests the
        // upper bound of the 2-5 character extension pattern.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.scala file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_sh_extension_not_rate_limit() {
        // "error: usage_limit.sh file not found" - Shell script file extension.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .sh files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage_limit.sh file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_bash_extension_not_rate_limit() {
        // "error: usage limit.bash file not found" - Bash script file extension (4 chars).
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes .bash files from rate limit detection.
        //
        // Expected: FileSystem or InternalError, NOT RateLimit
        let stderr = "error: usage limit.bash file not found";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_usage_limit_filename_compiler_error_format() {
        // "usage_limit.go:123:1: syntax error" - Compiler error format with .go file.
        //
        // Context: Verify that the generic file extension pattern correctly
        // excludes compiler error formats with various extensions.
        //
        // Expected: ParsingError, NOT RateLimit
        let stderr = "usage_limit.go:123:1: syntax error: unexpected token";
        let error_kind = classify_agent_error(1, stderr);
        assert_ne!(error_kind, AgentErrorKind::RateLimit);
        assert!(!is_rate_limit_error(&error_kind));
    }
}
