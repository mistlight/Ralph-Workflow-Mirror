//! Negative test cases - patterns that should NOT match rate limit.
//!
//! These tests prevent false positives by ensuring the pattern matching
//! is precise and only triggers for actual API rate limit errors.

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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
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
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.rs file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    // Should classify as FileSystem or InternalError, not RateLimit
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_with_space_in_error_prefix_not_rate_limit() {
    // "error: usage limit.py file not found" - variant with space in filename.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.py file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_with_underscore_in_error_prefix_not_rate_limit() {
    // "error: usage_limit.js file not found" - variant with underscore in filename.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage_limit.js file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_go_extension_not_rate_limit() {
    // "error: usage limit.go file not found" - Go file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.go file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_rb_extension_not_rate_limit() {
    // "error: usage_limit.rb file not found" - Ruby file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage_limit.rb file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_java_extension_not_rate_limit() {
    // "error: usage limit.java file not found" - Java file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.java file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_cpp_extension_not_rate_limit() {
    // "error: usage limit.cpp file not found" - C++ file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.cpp file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_c_extension_not_rate_limit() {
    // "error: usage limit.c file not found" - C file extension.
    //
    // Context: Single-letter extensions are a valid edge case.
    // Without proper file extension detection, this would incorrectly
    // match "error: usage limit" and be classified as a RateLimit error.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.c file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_php_extension_not_rate_limit() {
    // "error: usage limit.php file not found" - PHP file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.php file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_cs_extension_not_rate_limit() {
    // "error: usage_limit.cs file not found" - C# file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage_limit.cs file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_swift_extension_not_rate_limit() {
    // "error: usage limit.swift file not found" - Swift file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.swift file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_kt_extension_not_rate_limit() {
    // "error: usage_limit.kt file not found" - Kotlin file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage_limit.kt file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_scala_extension_not_rate_limit() {
    // "error: usage limit.scala file not found" - Scala file extension (5 chars).
    //
    // Context: Tests the upper bound of the 2-5 character extension pattern.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.scala file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_sh_extension_not_rate_limit() {
    // "error: usage_limit.sh file not found" - Shell script file extension.
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage_limit.sh file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_bash_extension_not_rate_limit() {
    // "error: usage limit.bash file not found" - Bash script file extension (4 chars).
    //
    // Expected: FileSystem or InternalError, NOT RateLimit
    let stderr = "error: usage limit.bash file not found";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}

#[test]
fn test_usage_limit_filename_compiler_error_format() {
    // "usage_limit.go:123:1: syntax error" - Compiler error format with .go file.
    //
    // Expected: ParsingError, NOT RateLimit
    let stderr = "usage_limit.go:123:1: syntax error: unexpected token";
    let error_kind = classify_agent_error(1, stderr, None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
    assert!(!is_rate_limit_error(&error_kind));
}
