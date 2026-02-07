//! Tests for fault-tolerant agent execution.

use super::*;

/// Test helper that wraps classify_agent_error with None for stdout_error.
/// This maintains backward compatibility for all existing tests.
fn classify_agent_error_test_helper(exit_code: i32, stderr: &str) -> AgentErrorKind {
    classify_agent_error(exit_code, stderr, None)
}
use crate::agents::JsonParserType;
use crate::config::Config;
use crate::logger::{Colors, Logger};
use crate::pipeline::{PipelineRuntime, Timer};
use crate::reducer::event::AgentEvent;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
struct TimedOutWriteWorkspace {
    inner: MemoryWorkspace,
    fail_path: PathBuf,
}

impl TimedOutWriteWorkspace {
    fn new(inner: MemoryWorkspace, fail_path: PathBuf) -> Self {
        Self { inner, fail_path }
    }
}

impl Workspace for TimedOutWriteWorkspace {
    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn read(&self, relative: &Path) -> io::Result<String> {
        self.inner.read(relative)
    }

    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
        self.inner.read_bytes(relative)
    }

    fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
        if relative == self.fail_path.as_path() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "simulated write timeout",
            ));
        }
        self.inner.write(relative, content)
    }

    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.inner.write_bytes(relative, content)
    }

    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.inner.append_bytes(relative, content)
    }

    fn exists(&self, relative: &Path) -> bool {
        self.inner.exists(relative)
    }

    fn is_file(&self, relative: &Path) -> bool {
        self.inner.is_file(relative)
    }

    fn is_dir(&self, relative: &Path) -> bool {
        self.inner.is_dir(relative)
    }

    fn remove(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove(relative)
    }

    fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_if_exists(relative)
    }

    fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(relative)
    }

    fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_dir_all_if_exists(relative)
    }

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.inner.create_dir_all(relative)
    }

    fn read_dir(&self, relative: &Path) -> io::Result<Vec<crate::workspace::DirEntry>> {
        self.inner.read_dir(relative)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }

    fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
        self.inner.write_atomic(relative, content)
    }

    fn set_readonly(&self, relative: &Path) -> io::Result<()> {
        self.inner.set_readonly(relative)
    }

    fn set_writable(&self, relative: &Path) -> io::Result<()> {
        self.inner.set_writable(relative)
    }
}

#[test]
fn test_timeout_error_from_run_with_prompt_err_arm_triggers_timeout_fallback() {
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();

    // Use a workspace that times out when saving the prompt.
    let inner_ws = MemoryWorkspace::new_test();
    let workspace = TimedOutWriteWorkspace::new(inner_ws, PathBuf::from(".agent/last_prompt.txt"));

    let executor = Arc::new(crate::executor::MockProcessExecutor::new());
    let executor_arc: Arc<dyn crate::executor::ProcessExecutor> = executor;

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: Arc::clone(&executor_arc),
        workspace: &workspace,
    };

    let env_vars: HashMap<String, String> = HashMap::new();
    let exec_config = AgentExecutionConfig {
        role: AgentRole::Developer,
        agent_name: "claude",
        cmd_str: "claude -p",
        parser_type: JsonParserType::Claude,
        env_vars: &env_vars,
        prompt: "hello",
        display_name: "claude",
        log_prefix: ".agent/logs/test",
        model_index: 0,
        attempt: 0,
        logfile: ".agent/logs/test.log",
    };

    let result = execute_agent_fault_tolerantly(exec_config, &mut runtime)
        .expect("executor should never return Err");

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::TimedOut { .. })
    ));
}

#[test]
fn test_classify_agent_error_sigsegv() {
    let error_kind = classify_agent_error_test_helper(139, "");
    assert_eq!(error_kind, AgentErrorKind::InternalError);
}

#[test]
fn test_classify_agent_error_sigabrt() {
    let error_kind = classify_agent_error_test_helper(134, "");
    assert_eq!(error_kind, AgentErrorKind::InternalError);
}

#[test]
fn test_classify_agent_error_sigterm() {
    let error_kind = classify_agent_error_test_helper(143, "");
    assert_eq!(error_kind, AgentErrorKind::Timeout);
}

#[test]
fn test_classify_agent_error_timeout_from_stderr() {
    let error_kind = classify_agent_error_test_helper(1, "Connection timeout");
    assert_eq!(error_kind, AgentErrorKind::Timeout);
}

#[test]
fn test_classify_agent_error_network_connection_reset() {
    let error_kind = classify_agent_error_test_helper(1, "Connection reset by peer");
    assert_eq!(error_kind, AgentErrorKind::Network);
}

#[test]
fn test_classify_agent_error_rate_limit() {
    let error_kind = classify_agent_error_test_helper(1, "Rate limit exceeded");
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_matches_http_429() {
    let error_kind =
        classify_agent_error_test_helper(1, "HTTP 429: Rate limit reached for requests");
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_matches_bare_http_429() {
    // Providers sometimes emit a bare status without additional wording.
    let error_kind = classify_agent_error_test_helper(1, "HTTP 429");
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_matches_bare_status_429() {
    // Alternative "status" phrasing seen across SDKs.
    let error_kind = classify_agent_error_test_helper(1, "status 429");
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_overrides_auth_for_403_forbidden_rate_limit() {
    // Some providers return 403 for quota/rate-limit conditions; in those cases we must
    // treat it as RateLimit to preserve the intended fallback semantics.
    let error_kind = classify_agent_error_test_helper(1, "HTTP 403 Forbidden: rate limit exceeded");
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_overrides_auth_for_403_forbidden_quota_exceeded() {
    // Quota exhaustion can also surface as 403. It should be treated as RateLimit.
    let error_kind =
        classify_agent_error_test_helper(1, "HTTP 403 Forbidden: exceeded your current quota");
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_from_opencode_json_error() {
    let stderr = r#"✗ Error: {"type":"error","sequence_number":2,"error":{"type":"tokens","code":"rate_limit_exceeded","message":"Rate limit reached"}}"#;
    let error_kind = classify_agent_error_test_helper(1, stderr);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_does_not_treat_429_token_count_as_rate_limit() {
    let error_kind = classify_agent_error_test_helper(1, "Parse error: expected 429 tokens");
    assert_eq!(error_kind, AgentErrorKind::ParsingError);
}

#[test]
fn test_classify_agent_error_does_not_treat_quota_word_as_rate_limit() {
    let error_kind = classify_agent_error_test_helper(1, "quota.rs:1:1: syntax error");
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_authentication() {
    let error_kind = classify_agent_error_test_helper(1, "Invalid API key");
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_model_unavailable() {
    let error_kind = classify_agent_error_test_helper(1, "Model not found");
    assert_eq!(error_kind, AgentErrorKind::ModelUnavailable);
}

#[test]
fn test_is_retriable_agent_error() {
    // Network, ModelUnavailable are retriable (model fallback)
    assert!(is_retriable_agent_error(&AgentErrorKind::Network));
    assert!(is_retriable_agent_error(&AgentErrorKind::ModelUnavailable));
    // Timeout is NOT retriable - it is handled via reducer policy
    // (retry same agent first, then switch agents after budget exhaustion).
    assert!(!is_retriable_agent_error(&AgentErrorKind::Timeout));
    // RateLimit is NOT retriable - it triggers immediate agent fallback
    assert!(!is_retriable_agent_error(&AgentErrorKind::RateLimit));
    // Non-retriable errors trigger agent fallback
    assert!(!is_retriable_agent_error(&AgentErrorKind::Authentication));
    assert!(!is_retriable_agent_error(&AgentErrorKind::ParsingError));
    assert!(!is_retriable_agent_error(&AgentErrorKind::FileSystem));
    assert!(!is_retriable_agent_error(&AgentErrorKind::InternalError));
}

#[test]
fn test_is_timeout_error() {
    // Only Timeout should match
    assert!(is_timeout_error(&AgentErrorKind::Timeout));
    // All others should NOT be timeout errors
    assert!(!is_timeout_error(&AgentErrorKind::Network));
    assert!(!is_timeout_error(&AgentErrorKind::RateLimit));
    assert!(!is_timeout_error(&AgentErrorKind::ModelUnavailable));
    assert!(!is_timeout_error(&AgentErrorKind::Authentication));
    assert!(!is_timeout_error(&AgentErrorKind::ParsingError));
    assert!(!is_timeout_error(&AgentErrorKind::FileSystem));
    assert!(!is_timeout_error(&AgentErrorKind::InternalError));
}

#[test]
fn test_is_rate_limit_error() {
    // Only RateLimit should match
    assert!(is_rate_limit_error(&AgentErrorKind::RateLimit));
    // All others should NOT be rate limit errors
    assert!(!is_rate_limit_error(&AgentErrorKind::Network));
    assert!(!is_rate_limit_error(&AgentErrorKind::Timeout));
    assert!(!is_rate_limit_error(&AgentErrorKind::ModelUnavailable));
    assert!(!is_rate_limit_error(&AgentErrorKind::Authentication));
    assert!(!is_rate_limit_error(&AgentErrorKind::ParsingError));
    assert!(!is_rate_limit_error(&AgentErrorKind::FileSystem));
    assert!(!is_rate_limit_error(&AgentErrorKind::InternalError));
}

#[test]
fn test_is_auth_error() {
    // Only Authentication should match
    assert!(is_auth_error(&AgentErrorKind::Authentication));
    // All others should NOT be auth errors
    assert!(!is_auth_error(&AgentErrorKind::RateLimit));
    assert!(!is_auth_error(&AgentErrorKind::Network));
    assert!(!is_auth_error(&AgentErrorKind::Timeout));
    assert!(!is_auth_error(&AgentErrorKind::ModelUnavailable));
    assert!(!is_auth_error(&AgentErrorKind::ParsingError));
    assert!(!is_auth_error(&AgentErrorKind::FileSystem));
    assert!(!is_auth_error(&AgentErrorKind::InternalError));
}

#[test]
fn test_classify_agent_error_auth_401() {
    let error_kind = classify_agent_error_test_helper(1, "HTTP 401 Unauthorized");
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_403_forbidden() {
    let error_kind = classify_agent_error_test_helper(1, "HTTP 403 Forbidden");
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_invalid_token() {
    let error_kind = classify_agent_error_test_helper(1, "Error: Invalid token provided");
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_credential() {
    let error_kind =
        classify_agent_error_test_helper(1, "Error: This credential is not authorized");
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_access_denied() {
    let error_kind = classify_agent_error_test_helper(1, "Access denied: insufficient permissions");
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_io_error_timeout() {
    let error = io::Error::new(io::ErrorKind::TimedOut, "Operation timeout");
    let error_kind = classify_io_error(&error);
    assert_eq!(error_kind, AgentErrorKind::Timeout);
}

#[test]
fn test_classify_io_error_timeout_timed_out_message() {
    // Common OS phrasing is "timed out" (not "timeout"). We must classify
    // based on `io::ErrorKind::TimedOut`, not substring matching.
    let error = io::Error::new(io::ErrorKind::TimedOut, "Operation timed out");
    let error_kind = classify_io_error(&error);
    assert_eq!(error_kind, AgentErrorKind::Timeout);
}

#[test]
fn test_classify_io_error_filesystem() {
    let error = io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied");
    let error_kind = classify_io_error(&error);
    assert_eq!(error_kind, AgentErrorKind::FileSystem);
}

#[test]
fn test_classify_io_error_network() {
    let error = io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe");
    let error_kind = classify_io_error(&error);
    assert_eq!(error_kind, AgentErrorKind::Network);
}

// ========================================================================
// Step 2: Quota exceeded pattern alignment tests
// ========================================================================

#[test]
fn test_classify_agent_error_rate_limit_quota_exceeded() {
    let error_kind =
        classify_agent_error_test_helper(1, "API quota exceeded, please try again later");
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_anthropic_quota() {
    let error_kind = classify_agent_error_test_helper(
        1,
        "You have exceeded your current quota for this API tier",
    );
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

// ========================================================================
// Step 3: Comprehensive tests for auth and rate-limit fallback flow
// ========================================================================

#[test]
fn test_auth_error_triggers_auth_fallback_classification() {
    // All these patterns should result in Authentication error kind
    // which triggers AuthFailed event via is_auth_error()
    let auth_patterns = vec![
        "HTTP 401 Unauthorized",
        "HTTP 403 Forbidden",
        "Error: Invalid API key",
        "Error: Invalid token provided",
        "Access denied: insufficient permissions",
        "This credential is only authorized for use with Claude Code",
        "Authentication failed: bad credentials",
    ];

    for pattern in auth_patterns {
        let error_kind = classify_agent_error_test_helper(1, pattern);
        assert_eq!(
            error_kind,
            AgentErrorKind::Authentication,
            "Pattern '{}' should classify as Authentication",
            pattern
        );
        assert!(
            is_auth_error(&error_kind),
            "Authentication error kind should trigger auth fallback for pattern '{}'",
            pattern
        );
    }
}

#[test]
fn test_rate_limit_error_triggers_rate_limit_fallback_classification() {
    // All these patterns should result in RateLimit error kind
    // which triggers RateLimited event via is_rate_limit_error()
    let rate_limit_patterns = vec![
        "Rate limit exceeded",
        "Rate limit reached for requests",
        "HTTP 429 Too Many Requests",
        "Error: too many requests, please slow down",
        "exceeded your current quota",
        "API quota exceeded",
    ];

    for pattern in rate_limit_patterns {
        let error_kind = classify_agent_error_test_helper(1, pattern);
        assert_eq!(
            error_kind,
            AgentErrorKind::RateLimit,
            "Pattern '{}' should classify as RateLimit",
            pattern
        );
        assert!(
            is_rate_limit_error(&error_kind),
            "RateLimit error kind should trigger rate limit fallback for pattern '{}'",
            pattern
        );
    }
}

// ========================================================================
// Step 5: Structured JSON auth error detection tests
// ========================================================================

#[test]
fn test_classify_agent_error_auth_from_json_error() {
    // Auth error embedded in JSON structure (common for some providers)
    let stderr = r#"✗ Error: {"type":"error","error":{"type":"auth","code":"unauthorized","message":"Invalid API key provided"}}"#;
    let error_kind = classify_agent_error_test_helper(1, stderr);
    // The "unauthorized" keyword should still be detected via substring matching
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_403_from_json_error() {
    let stderr = r#"{"error":{"code":"403","message":"Forbidden: API key does not have access"}}"#;
    let error_kind = classify_agent_error_test_helper(1, stderr);
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

// ========================================================================
// Step 6: Non-auth, non-rate-limit error behavior tests
// ========================================================================

#[test]
fn test_non_special_errors_maintain_retry_semantics() {
    // Network errors: retriable (model fallback, NOT agent fallback)
    // Note: "Connection timeout" is now classified as Timeout (not Network) because timeout
    // patterns are checked before connection/network patterns - see is_timeout_stderr().
    // Use "Connection refused" or "Connection reset" for pure network errors.
    let network_error = classify_agent_error_test_helper(1, "Connection refused");
    assert_eq!(network_error, AgentErrorKind::Network);
    assert!(
        is_retriable_agent_error(&network_error),
        "Network should be retriable"
    );
    assert!(
        !is_rate_limit_error(&network_error),
        "Network should not trigger rate limit fallback"
    );
    assert!(
        !is_auth_error(&network_error),
        "Network should not trigger auth fallback"
    );

    // Timeout errors via stderr (e.g., "Connection timeout" or "Request timeout")
    // are now classified as Timeout so the reducer can apply retry-first-then-fallback.
    let connection_timeout = classify_agent_error_test_helper(1, "Connection timeout");
    assert_eq!(connection_timeout, AgentErrorKind::Timeout);
    assert!(!is_retriable_agent_error(&connection_timeout));
    assert!(is_timeout_error(&connection_timeout));

    // Timeout errors via exit code (SIGTERM): emitted as TimedOut
    let timeout_error = classify_agent_error_test_helper(143, ""); // SIGTERM
    assert_eq!(timeout_error, AgentErrorKind::Timeout);
    assert!(!is_retriable_agent_error(&timeout_error));
    assert!(is_timeout_error(&timeout_error));

    // Model unavailable: retriable
    let model_error = classify_agent_error_test_helper(1, "Model not found");
    assert_eq!(model_error, AgentErrorKind::ModelUnavailable);
    assert!(is_retriable_agent_error(&model_error));

    // Internal errors: NOT retriable (agent fallback)
    let internal_error = classify_agent_error_test_helper(139, ""); // SIGSEGV
    assert_eq!(internal_error, AgentErrorKind::InternalError);
    assert!(!is_retriable_agent_error(&internal_error));

    // Parsing errors: NOT retriable
    let parse_error = classify_agent_error_test_helper(1, "Parse error: invalid syntax");
    assert_eq!(parse_error, AgentErrorKind::ParsingError);
    assert!(!is_retriable_agent_error(&parse_error));

    // Filesystem errors: NOT retriable
    let fs_error = classify_agent_error_test_helper(1, "Permission denied: /tmp/foo");
    assert_eq!(fs_error, AgentErrorKind::FileSystem);
    assert!(!is_retriable_agent_error(&fs_error));
}

// ========================================================================
// Rate Limit Pattern Tests - Provider-Specific Coverage
// ========================================================================

/// Rate Limit Pattern Tests - Provider-Specific Coverage
///
/// This module contains comprehensive tests for rate limit error pattern detection
/// across all major AI providers used by Ralph. Each test includes:
/// - Provider name and official documentation link
/// - Exact error message pattern being tested
/// - Last verification date
/// - Step-by-step verification instructions
///
/// # Test Organization
///
/// Tests are organized by provider in submodules:
/// - `opencode`: OpenCode multi-provider gateway patterns
/// - `openai`: OpenAI API rate limit patterns
/// - `anthropic`: Anthropic Claude API patterns
/// - `google`: Google Gemini API patterns
/// - `azure`: Azure OpenAI patterns
/// - `generic_http`: Standard HTTP 429 patterns
/// - `negative_cases`: Patterns that should NOT match
///
/// # Maintenance Schedule
///
/// - **Monthly**: Review provider documentation for changes
/// - **On test failure**: Immediately check if provider changed error format
/// - **Before major releases**: Verify all documentation links are still valid
/// - **When adding new provider**: Add full test coverage with documentation links
///
/// # Documentation Verification Process
///
/// For each provider, follow the verification steps in the test comments:
/// 1. Visit the documentation link
/// 2. Search for the error code (e.g., "429", "rate_limit_error")
/// 3. Verify the exact error message text
/// 4. Update the test if the message has changed
/// 5. Update the "Last Verified" date
///
/// # Provider Documentation Links
///
/// - **OpenAI**: https://platform.openai.com/docs/guides/error-codes
/// - **Anthropic**: https://docs.anthropic.com/en/api/errors
/// - **Google Gemini**: https://ai.google.dev/gemini-api/docs/troubleshooting
/// - **Azure OpenAI**: https://learn.microsoft.com/en-us/azure/ai-services/openai/quotas-limits
/// - **OpenCode**: No official docs - patterns observed in production
mod rate_limit_patterns {
    use super::*;

    /// OpenCode gateway tests for usage/quota limit patterns
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
            assert_eq!(error_kind, AgentErrorKind::RateLimit);
            assert!(is_rate_limit_error(&error_kind));
        }

        #[test]
        fn test_rate_limit_case_insensitive() {
            // Verify case-insensitive matching works for usage limit patterns
            let stderr = "ERROR: THE USAGE LIMIT HAS BEEN REACHED [RETRYIN]";
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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

            let stderr = "Error: You exceeded your current quota, please check your plan and billing details";
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
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
            let error_kind = classify_agent_error_test_helper(1, stderr);
            assert_ne!(error_kind, AgentErrorKind::RateLimit);
            assert!(!is_rate_limit_error(&error_kind));
        }
    }
}

/// Tests for stdout error detection
///
/// These tests verify that rate limit errors in stdout (e.g., from OpenCode JSON logs)
/// are properly detected and classified, fixing the bug where usage limit errors
/// from OpenCode were not triggering agent fallback.
mod stdout_error_detection {
    use super::*;

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_usage_limit_reached() {
        // OpenCode emits rate limit errors to stdout as JSON, not stderr
        // Stderr is empty, but stdout contains the error
        let stdout_error = Some("usage limit reached");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_rate_limit_exceeded() {
        // OpenCode JSON error format
        let stdout_error = Some("Rate limit exceeded");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_with_empty_stderr() {
        // Regression test: demonstrate the bug fix
        // Before fix: empty stderr + stdout error = InternalError (BUG)
        // After fix: empty stderr + stdout error with rate limit = RateLimit (FIXED)
        let stdout_error = Some("Error: usage limit has been reached [retryin]");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_stderr_takes_precedence_over_stdout() {
        // When both stderr and stdout have errors, stderr patterns should be detected
        let stdout_error = Some("Some other error");
        let error_kind = classify_agent_error(1, "Rate limit exceeded", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_stdout_error_none_behaves_as_before() {
        // Passing None for stdout_error should behave exactly as before the change
        let error_kind = classify_agent_error(1, "", None);
        assert_eq!(error_kind, AgentErrorKind::InternalError);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_http_429() {
        // HTTP 429 in stdout should trigger rate limit detection
        let stdout_error = Some("HTTP 429: Too Many Requests");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_quota_exceeded() {
        // Quota exceeded in stdout should trigger rate limit detection
        let stdout_error = Some("You have exceeded your current quota");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_too_many_requests() {
        // "too many requests" in stdout should trigger rate limit detection
        let stdout_error = Some("Error: too many requests, please slow down");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_opencode_structured_json() {
        // OpenCode structured JSON error with rate_limit_exceeded code
        let stdout_error =
            Some(r#"{"error": {"code": "rate_limit_exceeded", "message": "Rate limit reached"}}"#);
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_non_rate_limit_stdout_error() {
        // Non-rate-limit errors in stdout should not be classified as RateLimit
        // NOTE: Currently, stdout error detection only applies to rate limit patterns.
        // Other error types (Network, Auth, etc.) are only detected from stderr.
        // This is by design for the initial bug fix - we're specifically fixing
        // OpenCode rate limit detection, not adding general stdout error parsing.
        let stdout_error = Some("Connection refused");
        let error_kind = classify_agent_error(1, "", stdout_error);
        // Since stdout_error detection only handles rate limits, this should be InternalError
        assert_eq!(error_kind, AgentErrorKind::InternalError);
    }

    #[test]
    fn test_classify_agent_error_auth_error_in_stdout() {
        // Authentication errors in stdout should not be detected (stderr only for now)
        // NOTE: Currently, stdout error detection only applies to rate limit patterns.
        // This is by design for the initial bug fix - we're specifically fixing
        // OpenCode rate limit detection, not adding general stdout error parsing.
        let stdout_error = Some("Invalid API key provided");
        let error_kind = classify_agent_error(1, "", stdout_error);
        // Since stdout_error detection only handles rate limits, this should be InternalError
        assert_eq!(error_kind, AgentErrorKind::InternalError);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_overloaded_api() {
        // API overload errors (HTTP 529) in stdout should trigger rate limit detection
        let stdout_error = Some("HTTP 529: The API is temporarily overloaded");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_agent_error_rate_limit_from_stdout_resource_exhausted() {
        // Google Gemini RESOURCE_EXHAUSTED in stdout should trigger rate limit detection
        let stdout_error = Some("Error: RESOURCE_EXHAUSTED: You've exceeded the rate limit");
        let error_kind = classify_agent_error(1, "", stdout_error);
        assert_eq!(error_kind, AgentErrorKind::RateLimit);
    }
}

#[test]
fn test_usage_limit_triggers_rate_limited_event_not_timeout() {
    // Integration test: Usage limit errors trigger immediate agent fallback
    //
    // This test verifies the fix for the bug where "usage limit has been reached"
    // errors from OpenCode/Claude API caused the pipeline to timeout instead of
    // immediately falling back to the next agent.
    //
    // **Bug Report Context:**
    // OpenCode emits "usage limit has been reached [retryin]" when any underlying
    // provider (OpenAI, Anthropic, etc.) hits quota limits. The "[retryin]" suffix
    // is misleading - the agent is actually unavailable due to quota exhaustion.
    //
    // **Expected Behavior:**
    // The error should be classified as AgentErrorKind::RateLimit, which triggers
    // immediate agent fallback via AgentEvent::RateLimited (not timeout).
    //
    // **Verification:**
    // - Mock executor returns "usage limit has been reached [retryin]" error
    // - Executor result is AgentEvent::RateLimited (not TimedOut)
    // - No session_id is returned (provider is unavailable)

    use crate::executor::AgentCommandResult;

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();
    let workspace = MemoryWorkspace::new_test();

    // Mock executor that simulates usage limit error
    let executor = Arc::new(
        crate::executor::MockProcessExecutor::new().with_agent_result(
            "opencode",
            Ok(AgentCommandResult::failure(
                1,
                "Error: The usage limit has been reached [retryin]",
            )),
        ),
    );
    let executor_arc: Arc<dyn crate::executor::ProcessExecutor> = executor;

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: Arc::clone(&executor_arc),
        workspace: &workspace,
    };

    let env_vars: HashMap<String, String> = HashMap::new();
    let exec_config = AgentExecutionConfig {
        role: AgentRole::Developer,
        agent_name: "opencode",
        cmd_str: "opencode -p",
        parser_type: JsonParserType::Claude,
        env_vars: &env_vars,
        prompt: "Test prompt",
        display_name: "opencode",
        log_prefix: ".agent/logs/test",
        model_index: 0,
        attempt: 0,
        logfile: ".agent/logs/test.log",
    };

    let result = execute_agent_fault_tolerantly(exec_config, &mut runtime)
        .expect("executor should never return Err");

    // Verify that RateLimited event is emitted (not TimedOut or InvocationFailed)
    match result.event {
        PipelineEvent::Agent(AgentEvent::RateLimited { role, agent, .. }) => {
            assert_eq!(role, AgentRole::Developer);
            assert_eq!(agent, "opencode");
        }
        other => panic!(
            "Expected AgentEvent::RateLimited, got {:?}. \
             This indicates usage limit errors are not triggering immediate agent fallback.",
            other
        ),
    }

    // Verify no session_id is returned (rate limit = provider unavailable)
    assert!(result.session_id.is_none());
}
