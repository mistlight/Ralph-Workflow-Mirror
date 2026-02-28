//! Rate limit pattern classification tests
//!
//! Comprehensive tests for rate limit error detection across different providers:
//! - `OpenCode` gateway patterns (usage limit, quota exhaustion)
//! - `OpenAI` patterns (HTTP 429, `rate_limit_exceeded`)
//! - Anthropic patterns (`rate_limit_error`, `overloaded_error`)
//! - Google Gemini patterns (`RESOURCE_EXHAUSTED`)
//! - Azure `OpenAI` patterns (quota exceeded, throttled)
//! - Generic HTTP patterns (HTTP 429, status 429)
//! - Negative cases (`usage_limit` in filenames, comments, non-error contexts)
//!
//! Each test documents the provider, error pattern, official documentation,
//! and verification steps to ensure accuracy.

use super::*;

mod opencode;

/// `OpenAI` API rate limit patterns
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
        let error_kind = classify_agent_error(1, stderr, None);
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
        let error_kind = classify_agent_error(1, stderr, None);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }

    #[test]
    fn test_rate_limit_openai_quota_exceeded_short() {
        // Provider: OpenAI API
        // Error Pattern: "Quota exceeded" (shorter variant)
        // Documentation: https://platform.openai.com/docs/guides/error-codes
        // Last Verified: 2026-02-12
        // Context:
        //   OpenCode reformats OpenAI quota errors to "Quota exceeded. Check your plan and billing details."
        //   Source: /packages/opencode/src/provider/error.ts:93-98

        let stderr = "Quota exceeded. Check your plan and billing details.";
        let error_kind = classify_agent_error(1, stderr, None);
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

        let stderr = "HTTP 429: rate_limit_error - Too many requests";
        let error_kind = classify_agent_error(1, stderr, None);
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

        let stderr = "HTTP 529: overloaded_error - The API is temporarily overloaded";
        let error_kind = classify_agent_error(1, stderr, None);
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

        let stderr = "Error: The API is temporarily overloaded, please retry after some time";
        let error_kind = classify_agent_error(1, stderr, None);
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
        let error_kind = classify_agent_error(1, stderr, None);
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

        let stderr = "Error: RESOURCE_EXHAUSTED: You've exceeded the rate limit";
        let error_kind = classify_agent_error(1, stderr, None);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

/// Azure `OpenAI` rate limit patterns
mod azure {
    use super::*;

    #[test]
    fn test_rate_limit_azure_openai() {
        // Provider: Azure OpenAI
        // Error Pattern: Inherits from OpenAI - "Rate limit reached"
        // Documentation: https://learn.microsoft.com/en-us/azure/ai-services/openai/quotas-limits
        //   HTTP Code: 429 - Rate limit patterns similar to OpenAI
        // Last Verified: 2026-02-06

        let stderr = "Error: Rate limit reached for requests. Please retry after some time.";
        let error_kind = classify_agent_error(1, stderr, None);
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
        let error_kind = classify_agent_error(1, stderr, None);
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
        let error_kind = classify_agent_error(1, stderr, None);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
        assert!(is_rate_limit_error(&error_kind));
    }
}

mod negative_cases;
