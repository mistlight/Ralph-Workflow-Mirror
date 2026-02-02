use super::*;

fn classify(exit_code: i32, stderr: &str) -> AgentErrorKind {
    AgentErrorKind::classify_with_agent(exit_code, stderr, None, None)
}

#[test]
fn glm_like_detection_excludes_opencode() {
    assert!(is_glm_like_agent("ccs/glm"));
    assert!(is_glm_like_agent("claude -m glm-4"));
    assert!(!is_glm_like_agent("opencode/opencode/glm-4.7-free"));
    assert!(!is_glm_like_agent("glm-4.7-free"));
}

#[test]
fn kind_retry_and_fallback_semantics_are_stable() {
    assert!(!AgentErrorKind::RateLimited.should_retry());
    assert!(AgentErrorKind::RateLimited.should_immediate_agent_fallback());
    assert!(AgentErrorKind::ApiUnavailable.should_retry());
    assert!(AgentErrorKind::TokenExhausted.should_fallback());
    assert!(AgentErrorKind::DiskFull.is_unrecoverable());
}

#[test]
fn classify_detects_api_and_auth_and_token_errors() {
    assert_eq!(
        classify(1, "rate limit exceeded"),
        AgentErrorKind::RateLimited
    );
    assert_eq!(classify(1, "invalid token"), AgentErrorKind::AuthFailure);
    assert_eq!(
        classify(1, "context length exceeded"),
        AgentErrorKind::TokenExhausted
    );
}

#[test]
fn classify_detects_network_and_server_and_timeouts() {
    assert_eq!(
        classify(1, "connection refused"),
        AgentErrorKind::NetworkError
    );
    assert_eq!(
        classify(1, "503 service unavailable"),
        AgentErrorKind::ApiUnavailable
    );
    assert_eq!(classify(1, "request timed out"), AgentErrorKind::Timeout);
}

#[test]
fn classify_detects_disk_and_killed_process_and_e2big() {
    assert_eq!(
        classify(1, "no space left on device"),
        AgentErrorKind::DiskFull
    );
    assert_eq!(classify(137, ""), AgentErrorKind::ProcessKilled);
    assert_eq!(
        classify(7, "argument list too long"),
        AgentErrorKind::ToolExecutionFailed
    );
}

#[test]
fn classify_handles_glm_agent_heuristics() {
    assert_eq!(
        AgentErrorKind::classify_with_agent(1, "some random error", Some("ccs/glm"), None),
        AgentErrorKind::RetryableAgentQuirk
    );
    assert_eq!(
        AgentErrorKind::classify_with_agent(1, "glm failed", Some("ccs/glm"), None),
        AgentErrorKind::AgentSpecificQuirk
    );
    assert_eq!(
        AgentErrorKind::classify_with_agent(1, "token limit exceeded", Some("ccs/glm"), None),
        AgentErrorKind::TokenExhausted
    );
}

#[test]
fn opencode_glm_agent_uses_normal_classification() {
    assert_eq!(
        AgentErrorKind::classify_with_agent(
            1,
            "some error occurred",
            Some("opencode/opencode/glm-4.7-free"),
            None
        ),
        AgentErrorKind::Transient
    );
}

#[test]
fn description_and_advice_are_non_empty() {
    let error = AgentErrorKind::RateLimited;
    assert!(!error.description().is_empty());
    assert!(!error.recovery_advice().is_empty());
}
