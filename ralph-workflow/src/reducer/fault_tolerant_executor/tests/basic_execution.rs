//! Basic execution and error classification tests
//!
//! Tests core fault-tolerant execution behavior:
//! - Timeout handling when saving prompts
//! - Basic error classification for agent errors (signals, network, auth, rate limit)
//! - I/O error classification (timeout, filesystem, network)

use super::*;

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
    let error_kind = classify_agent_error(139, "", None);
    assert_eq!(error_kind, AgentErrorKind::InternalError);
}

#[test]
fn test_classify_agent_error_sigabrt() {
    let error_kind = classify_agent_error(134, "", None);
    assert_eq!(error_kind, AgentErrorKind::InternalError);
}

#[test]
fn test_classify_agent_error_sigterm() {
    let error_kind = classify_agent_error(143, "", None);
    assert_eq!(error_kind, AgentErrorKind::Timeout);
}

#[test]
fn test_classify_agent_error_timeout_from_stderr() {
    let error_kind = classify_agent_error(1, "Connection timeout", None);
    assert_eq!(error_kind, AgentErrorKind::Timeout);
}

#[test]
fn test_classify_agent_error_network_connection_reset() {
    let error_kind = classify_agent_error(1, "Connection reset by peer", None);
    assert_eq!(error_kind, AgentErrorKind::Network);
}

#[test]
fn test_classify_agent_error_rate_limit() {
    let error_kind = classify_agent_error(1, "Rate limit exceeded", None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_matches_http_429() {
    let error_kind = classify_agent_error(1, "HTTP 429: Rate limit reached for requests", None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_matches_bare_http_429() {
    // Providers sometimes emit a bare status without additional wording.
    let error_kind = classify_agent_error(1, "HTTP 429", None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_matches_bare_status_429() {
    // Alternative "status" phrasing seen across SDKs.
    let error_kind = classify_agent_error(1, "status 429", None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_overrides_auth_for_403_forbidden_rate_limit() {
    // Some providers return 403 for quota/rate-limit conditions; in those cases we must
    // treat it as RateLimit to preserve the intended fallback semantics.
    let error_kind = classify_agent_error(1, "HTTP 403 Forbidden: rate limit exceeded", None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_overrides_auth_for_403_forbidden_quota_exceeded() {
    // Quota exhaustion can also surface as 403. It should be treated as RateLimit.
    let error_kind =
        classify_agent_error(1, "HTTP 403 Forbidden: exceeded your current quota", None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_from_opencode_json_error() {
    let stderr = r#"✗ Error: {"type":"error","sequence_number":2,"error":{"type":"tokens","code":"rate_limit_exceeded","message":"Rate limit reached"}}"#;
    let error_kind = classify_agent_error(1, stderr, None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_does_not_treat_429_token_count_as_rate_limit() {
    let error_kind = classify_agent_error(1, "Parse error: expected 429 tokens", None);
    assert_eq!(error_kind, AgentErrorKind::ParsingError);
}

#[test]
fn test_classify_agent_error_does_not_treat_quota_word_as_rate_limit() {
    let error_kind = classify_agent_error(1, "quota.rs:1:1: syntax error", None);
    assert_ne!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_authentication() {
    let error_kind = classify_agent_error(1, "Invalid API key", None);
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_model_unavailable() {
    let error_kind = classify_agent_error(1, "Model not found", None);
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
    let error_kind = classify_agent_error(1, "HTTP 401 Unauthorized", None);
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_403_forbidden() {
    let error_kind = classify_agent_error(1, "HTTP 403 Forbidden", None);
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_invalid_token() {
    let error_kind = classify_agent_error(1, "Error: Invalid token provided", None);
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_credential() {
    let error_kind = classify_agent_error(1, "Error: This credential is not authorized", None);
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_auth_access_denied() {
    let error_kind = classify_agent_error(1, "Access denied: insufficient permissions", None);
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
    let error_kind = classify_agent_error(1, "API quota exceeded, please try again later", None);
    assert_eq!(error_kind, AgentErrorKind::RateLimit);
}

#[test]
fn test_classify_agent_error_rate_limit_anthropic_quota() {
    let error_kind = classify_agent_error(
        1,
        "You have exceeded your current quota for this API tier",
        None,
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
        let error_kind = classify_agent_error(1, pattern, None);
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
        let error_kind = classify_agent_error(1, pattern, None);
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
    let error_kind = classify_agent_error(1, stderr, None);
    // The "unauthorized" keyword should still be detected via substring matching
    assert_eq!(error_kind, AgentErrorKind::Authentication);
}

#[test]
fn test_classify_agent_error_403_from_json_error() {
    let stderr = r#"{"error":{"code":"403","message":"Forbidden: API key does not have access"}}"#;
    let error_kind = classify_agent_error(1, stderr, None);
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
    let network_error = classify_agent_error(1, "Connection refused", None);
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
    let connection_timeout = classify_agent_error(1, "Connection timeout", None);
    assert_eq!(connection_timeout, AgentErrorKind::Timeout);
    assert!(!is_retriable_agent_error(&connection_timeout));
    assert!(is_timeout_error(&connection_timeout));

    // Timeout errors via exit code (SIGTERM): emitted as TimedOut
    let timeout_error = classify_agent_error(143, "", None); // SIGTERM
    assert_eq!(timeout_error, AgentErrorKind::Timeout);
    assert!(!is_retriable_agent_error(&timeout_error));
    assert!(is_timeout_error(&timeout_error));

    // Model unavailable: retriable
    let model_error = classify_agent_error(1, "Model not found", None);
    assert_eq!(model_error, AgentErrorKind::ModelUnavailable);
    assert!(is_retriable_agent_error(&model_error));

    // Internal errors: NOT retriable (agent fallback)
    let internal_error = classify_agent_error(139, "", None); // SIGSEGV
    assert_eq!(internal_error, AgentErrorKind::InternalError);
    assert!(!is_retriable_agent_error(&internal_error));

    // Parsing errors: NOT retriable
    let parse_error = classify_agent_error(1, "Parse error: invalid syntax", None);
    assert_eq!(parse_error, AgentErrorKind::ParsingError);
    assert!(!is_retriable_agent_error(&parse_error));

    // Filesystem errors: NOT retriable
    let fs_error = classify_agent_error(1, "Permission denied: /tmp/foo", None);
    assert_eq!(fs_error, AgentErrorKind::FileSystem);
    assert!(!is_retriable_agent_error(&fs_error));
}
