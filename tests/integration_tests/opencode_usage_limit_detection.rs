//! Integration tests for OpenCode usage limit detection.
//!
//! Verifies end-to-end OpenCode usage limit detection flow:
//! 1. OpenCode emits JSON error to logfile with usage_limit_exceeded code
//! 2. Error extraction from logfile identifies the error
//! 3. Error classification detects it as RateLimit (not InternalError)
//! 4. RateLimit event triggers agent fallback
//!
//! Observable behaviors tested:
//! - Structured error codes (insufficient_quota, usage_limit_exceeded, quota_exceeded)
//! - Provider-specific errors (anthropic: usage limit, openai: usage limit)
//! - OpenCode Zen branded errors (opencode usage limit, zen usage limit)
//! - RateLimit event emission (not InternalError)
//! - Agent fallback triggered after RateLimit detection
//!
//! # Integration Test Compliance
//!
//! These tests follow [../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md):
//! - Test observable behavior: error detection and agent fallback
//! - Mock at architectural boundaries: MemoryWorkspace for filesystem
//! - Pure reducer tests require no mocks

use crate::test_timeout::with_default_timeout;
use ralph_workflow::pipeline::extract_error_message_from_logfile;
use ralph_workflow::reducer::event::AgentErrorKind;
use ralph_workflow::reducer::fault_tolerant_executor::classify_agent_error;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use std::path::Path;

/// Test that OpenCode structured error code `usage_limit_exceeded` is correctly extracted and classified.
///
/// This is the PRIMARY error format from OpenCode when usage limits are hit.
/// The error code is more reliable than message text for detection.
#[test]
fn test_opencode_structured_code_usage_limit_exceeded() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        // Mock OpenCode logfile with structured error code
        let logfile_content = r#"{"type":"content","content":"Analyzing..."}
{"type":"error","error":{"code":"usage_limit_exceeded"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        // Extract error from logfile
        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert!(
            error_msg.is_some(),
            "Should extract error from OpenCode JSON"
        );
        assert_eq!(
            error_msg.as_deref(),
            Some("usage_limit_exceeded"),
            "Should extract error code"
        );

        // Classify error - should be RateLimit, not InternalError
        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "usage_limit_exceeded should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test that OpenCode error code `insufficient_quota` is correctly detected.
///
/// OpenAI provider uses `insufficient_quota` when quota is exhausted.
/// Source: /packages/opencode/src/provider/error.ts
#[test]
fn test_opencode_insufficient_quota_code() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content = r#"{"type":"content","content":"Processing..."}
{"type":"error","error":{"code":"insufficient_quota"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert_eq!(
            error_msg.as_deref(),
            Some("insufficient_quota"),
            "Should extract insufficient_quota error code"
        );

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "insufficient_quota should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test that OpenCode error code `quota_exceeded` is correctly detected.
///
/// Generic quota exhaustion code used by multiple providers.
#[test]
fn test_opencode_quota_exceeded_code() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content = r#"{"type":"error","error":{"code":"quota_exceeded"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert_eq!(error_msg.as_deref(), Some("quota_exceeded"));

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "quota_exceeded should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test provider-specific error: `anthropic: usage limit reached`.
///
/// OpenCode multi-provider gateway forwards provider errors with provider context.
/// Format: `{"error": {"provider": "anthropic", "message": "usage limit reached"}}`
#[test]
fn test_opencode_provider_specific_anthropic_usage_limit() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content =
            r#"{"type":"error","error":{"provider":"anthropic","message":"usage limit reached"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert!(
            error_msg.is_some(),
            "Should extract provider-specific error"
        );
        // Should get "anthropic: usage limit reached" format
        assert!(
            error_msg.as_deref().unwrap().contains("anthropic"),
            "Should preserve provider context: {}",
            error_msg.as_deref().unwrap()
        );
        assert!(
            error_msg.as_deref().unwrap().contains("usage limit"),
            "Should contain usage limit message: {}",
            error_msg.as_deref().unwrap()
        );

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "Provider-specific usage limit should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test provider-specific error: `openai: usage limit exceeded`.
///
/// OpenAI provider emits usage limit errors through OpenCode multi-provider gateway.
#[test]
fn test_opencode_provider_specific_openai_usage_limit() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content =
            r#"{"type":"error","error":{"provider":"openai","message":"usage limit exceeded"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert!(error_msg.is_some());
        assert!(error_msg.as_deref().unwrap().contains("openai"));
        assert!(error_msg.as_deref().unwrap().contains("usage limit"));

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "OpenAI usage limit should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test OpenCode Zen branded error: `OpenCode Zen usage limit has been reached`.
///
/// OpenCode Zen (paid tier) emits branded usage limit messages.
#[test]
fn test_opencode_zen_usage_limit_message() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content =
            r#"{"type":"error","error":{"message":"OpenCode Zen usage limit has been reached"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert_eq!(
            error_msg.as_deref(),
            Some("OpenCode Zen usage limit has been reached")
        );

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "OpenCode Zen usage limit should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test OpenCode usage limit message: `opencode usage limit reached`.
///
/// Generic OpenCode usage limit message (non-Zen).
#[test]
fn test_opencode_usage_limit_message() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content =
            r#"{"type":"error","error":{"message":"opencode usage limit reached"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert_eq!(error_msg.as_deref(), Some("opencode usage limit reached"));

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "OpenCode usage limit should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test that generic "usage limit reached" message is detected.
///
/// Generic message pattern that may be emitted by various providers.
#[test]
fn test_generic_usage_limit_reached_message() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content = r#"{"type":"error","error":{"message":"usage limit reached"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert_eq!(error_msg.as_deref(), Some("usage limit reached"));

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "Generic usage limit should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test that "usage limit has been reached" message is detected.
///
/// Longer form of the usage limit message.
#[test]
fn test_usage_limit_has_been_reached_message() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content =
            r#"{"type":"error","error":{"message":"usage limit has been reached"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert_eq!(error_msg.as_deref(), Some("usage limit has been reached"));

        let error_kind = classify_agent_error(1, "", error_msg.as_deref());

        assert!(
            matches!(error_kind, AgentErrorKind::RateLimit),
            "usage limit has been reached should be classified as RateLimit, got {:?}",
            error_kind
        );
    });
}

/// Test that errors are extracted from the last 50 lines of logfile.
///
/// OpenCode may emit multiple events to the same logfile.
/// Error extraction should search recent lines (last 50) in reverse order.
#[test]
fn test_error_extraction_searches_last_50_lines() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        // Create logfile with 100 lines, error at line 80
        let mut lines = Vec::new();
        for i in 1..=79 {
            lines.push(format!(r#"{{"type":"content","content":"Line {}"}}"#, i));
        }
        lines.push(r#"{"type":"error","error":{"code":"usage_limit_exceeded"}}"#.to_string());
        for i in 81..=100 {
            lines.push(format!(r#"{{"type":"content","content":"Line {}"}}"#, i));
        }

        let logfile_content = lines.join("\n");

        workspace
            .write(Path::new(".agent/logs/agent1.log"), &logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert!(
            error_msg.is_some(),
            "Should find error within last 50 lines (line 80 of 100)"
        );
        assert_eq!(error_msg.as_deref(), Some("usage_limit_exceeded"));
    });
}

/// Test that non-error JSON events are ignored.
///
/// OpenCode emits multiple event types (content, thinking, etc.).
/// Only "error" type events should be extracted.
#[test]
fn test_non_error_events_ignored() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content = r#"{"type":"content","content":"Hello"}
{"type":"thinking","thinking":"Analyzing..."}
{"type":"tool_use","name":"read_file"}
{"type":"completion","finish_reason":"stop"}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert!(
            error_msg.is_none(),
            "Should not extract error from non-error events"
        );
    });
}

/// Test that malformed JSON lines are skipped.
///
/// Logfiles may contain partial or malformed JSON.
/// Error extraction should skip invalid lines and continue searching.
#[test]
fn test_malformed_json_skipped() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let logfile_content = r#"{"type":"content","content":"Valid"}
{invalid json line
{"type":"error","error":{"code":"usage_limit_exceeded"}}"#;

        workspace
            .write(Path::new(".agent/logs/agent1.log"), logfile_content)
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert!(
            error_msg.is_some(),
            "Should skip malformed JSON and find valid error"
        );
        assert_eq!(error_msg.as_deref(), Some("usage_limit_exceeded"));
    });
}

/// Test that empty logfile returns None.
#[test]
fn test_empty_logfile_returns_none() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        workspace
            .write(Path::new(".agent/logs/agent1.log"), "")
            .unwrap();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/agent1.log",
            &workspace as &dyn Workspace,
        );

        assert!(error_msg.is_none(), "Empty logfile should return None");
    });
}

/// Test that missing logfile returns None.
#[test]
fn test_missing_logfile_returns_none() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let error_msg = extract_error_message_from_logfile(
            ".agent/logs/nonexistent.log",
            &workspace as &dyn Workspace,
        );

        assert!(error_msg.is_none(), "Missing logfile should return None");
    });
}

/// Test full pipeline: OpenCode usage limit → agent fallback via reducer.
///
/// This test verifies the complete flow:
/// 1. Agent invocation fails with usage limit error
/// 2. Error is classified as RateLimit
/// 3. Reducer emits RateLimit event
/// 4. Agent chain advances to next agent
#[test]
fn test_full_usage_limit_triggers_agent_fallback() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRole;
        use ralph_workflow::reducer::event::PipelineEvent;
        use ralph_workflow::reducer::state::{AgentChainState, PipelineState};
        use ralph_workflow::reducer::state_reduction::reduce;

        // Setup state with two agents
        let state = PipelineState {
            phase: ralph_workflow::reducer::event::PipelinePhase::Development,
            agent_chain: AgentChainState::initial().with_agents(
                vec!["opencode-claude".to_string(), "opencode-codex".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            ),
            ..crate::common::with_locked_prompt_permissions(PipelineState::initial(5, 2))
        };

        assert_eq!(
            state.agent_chain.current_agent().unwrap(),
            "opencode-claude",
            "Should start with first agent"
        );

        // Simulate usage limit error via RateLimit event
        let new_state = reduce(
            state,
            PipelineEvent::agent_rate_limited(
                AgentRole::Developer,
                "opencode-claude".to_string(),
                Some("continue work".to_string()),
            ),
        );

        // Should switch to next agent
        assert_eq!(
            new_state.agent_chain.current_agent().unwrap(),
            "opencode-codex",
            "RateLimit should trigger agent fallback"
        );

        // Should preserve continuation prompt
        assert!(
            new_state
                .agent_chain
                .rate_limit_continuation_prompt
                .is_some(),
            "Should preserve prompt for continuation"
        );
    });
}

/// Test that usage limit detection works with stderr fallback.
///
/// If OpenCode error is not in logfile JSON, it may be in stderr.
/// Error classification should detect usage limit patterns in stderr.
#[test]
fn test_usage_limit_detection_in_stderr() {
    with_default_timeout(|| {
        // Test stderr patterns
        let test_cases = vec![
            ("anthropic: usage limit reached", AgentErrorKind::RateLimit),
            ("openai: usage limit exceeded", AgentErrorKind::RateLimit),
            (
                "zen usage limit has been reached",
                AgentErrorKind::RateLimit,
            ),
            ("opencode usage limit reached", AgentErrorKind::RateLimit),
            ("usage limit has been reached", AgentErrorKind::RateLimit),
            ("usage limit reached", AgentErrorKind::RateLimit),
            ("usage limit exceeded", AgentErrorKind::RateLimit),
        ];

        for (stderr, expected_kind) in test_cases {
            let error_kind = classify_agent_error(1, stderr, None);

            assert!(
                matches!(error_kind, k if k == expected_kind),
                "stderr '{}' should be classified as {:?}, got {:?}",
                stderr,
                expected_kind,
                error_kind
            );
        }
    });
}

/// Test that non-usage-limit errors are NOT classified as RateLimit.
///
/// Ensure other error types are correctly distinguished from rate limit errors.
#[test]
fn test_non_usage_limit_errors_not_classified_as_rate_limit() {
    with_default_timeout(|| {
        // Test various error messages that should NOT be classified as RateLimit
        let non_rate_limit_errors = vec![
            "network connection failed",
            "authentication failed",
            "internal server error",
            "timed out",
            "file not found",
            "parse error",
            "model not found",
        ];

        for stderr in non_rate_limit_errors {
            let error_kind = classify_agent_error(1, stderr, None);

            assert!(
                !matches!(error_kind, AgentErrorKind::RateLimit),
                "stderr '{}' should NOT be RateLimit, got {:?}",
                stderr,
                error_kind
            );
        }
    });
}
