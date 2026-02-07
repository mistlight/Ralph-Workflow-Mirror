// XSD retry state transitions.
//
// Tests for XSD retry pending flag, retry count management, session ID handling,
// and XSD retry exhaustion across different phases.

use super::*;
use crate::reducer::state::{CommitValidatedOutcome, PlanningValidatedOutcome};

#[test]
fn test_development_output_validation_failed_sets_xsd_retry_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(1, 0),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry should be pending after validation failure"
    );
    assert!(
        new_state.continuation.xsd_retry_session_reuse_pending,
        "XSD retry should reuse the prior session when available"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
    assert_eq!(
        new_state.continuation.invalid_output_attempts, 1,
        "Invalid output attempts should be incremented"
    );
}

#[test]
fn test_development_output_validation_failed_exhausts_xsd_retries() {
    use crate::reducer::state::ContinuationState;

    // Create state with custom max_xsd_retry_count = 2
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_pending: true,
            same_agent_retry_reason: Some(crate::reducer::state::SameAgentRetryReason::Timeout),
            xsd_retry_count: 1,
            max_xsd_retry_count: 2,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    let new_state = reduce(
        state.clone(),
        PipelineEvent::development_output_validation_failed(1, 0),
    );

    // XSD retries exhausted, should switch agent
    assert!(
        !new_state.continuation.xsd_retry_pending,
        "XSD retry should not be pending after exhaustion"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "XSD retry count should be reset after agent switch"
    );
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should have switched to next agent"
    );
    assert_eq!(
        new_state.continuation.same_agent_retry_count, 0,
        "Same-agent retry budget must not carry across agents"
    );
    assert!(
        !new_state.continuation.same_agent_retry_pending,
        "Same-agent retry pending must be cleared when switching agents"
    );
    assert!(
        new_state.continuation.same_agent_retry_reason.is_none(),
        "Same-agent retry reason must be cleared when switching agents"
    );
}

#[test]
fn test_planning_output_validation_failed_sets_xsd_retry_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 0,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::planning_output_validation_failed(0, 0),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry should be pending after validation failure"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
}

#[test]
fn test_review_output_validation_failed_sets_xsd_retry_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::review_output_validation_failed(0, 0, None),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry should be pending after validation failure"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
}

#[test]
fn test_plan_generation_completed_clears_xsd_retry_state() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Planning,
        continuation: ContinuationState {
            xsd_retry_count: 3,
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be cleared on success"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "XSD retry count should be reset on success"
    );
}

#[test]
fn test_session_established_stores_session_id() {
    let state = create_test_state();

    let new_state = reduce(
        state,
        PipelineEvent::agent_session_established(
            AgentRole::Developer,
            "claude".to_string(),
            "ses_abc123".to_string(),
        ),
    );

    assert_eq!(
        new_state.agent_chain.last_session_id,
        Some("ses_abc123".to_string()),
        "Session ID should be stored"
    );
}

#[test]
fn test_agent_switch_clears_session_id() {
    let state = PipelineState {
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("ses_abc123".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            crate::reducer::event::AgentErrorKind::InternalError,
            false,
        ),
    );

    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "Session ID should be cleared when switching agents"
    );
}

#[test]
fn test_commit_message_validation_failed_sets_xsd_retry_pending() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        continuation: ContinuationState::new(),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be set"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
}

#[test]
fn test_commit_message_validation_failed_does_not_advance_commit_attempt_on_xsd_retry() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        continuation: ContinuationState::new(),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be set"
    );
    match new_state.commit {
        CommitState::Generating { attempt, .. } => assert_eq!(
            attempt, 1,
            "commit attempt should remain stable across XSD retries"
        ),
        other => panic!("expected CommitState::Generating, got: {other:?}"),
    }
}

#[test]
fn test_commit_xsd_retry_exhausted_switches_agent() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_pending: true,
            same_agent_retry_reason: Some(crate::reducer::state::SameAgentRetryReason::Other),
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Commit,
        ),
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    // Should have switched to next agent
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "XSD retry count should be reset"
    );
    assert!(
        !new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be cleared"
    );
    assert_eq!(
        new_state.continuation.same_agent_retry_count, 0,
        "Same-agent retry budget must not carry across agents"
    );
    assert!(
        !new_state.continuation.same_agent_retry_pending,
        "Same-agent retry pending must be cleared when switching agents"
    );
    assert!(
        new_state.continuation.same_agent_retry_reason.is_none(),
        "Same-agent retry reason must be cleared when switching agents"
    );
}

#[test]
fn test_planning_prompt_prepared_clears_xsd_retry_pending_but_preserves_session_reuse_signal() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            xsd_retry_session_reuse_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::planning_prompt_prepared(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Prompt preparation should clear xsd retry pending"
    );
    assert!(
        new_state.continuation.xsd_retry_session_reuse_pending,
        "Prompt preparation must preserve the session reuse signal for the upcoming retry"
    );
}

#[test]
fn test_planning_agent_invoked_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::planning_agent_invoked(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Agent invocation should clear xsd retry pending"
    );
}

#[test]
fn test_review_prompt_prepared_clears_xsd_retry_pending_but_preserves_session_reuse_signal() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            xsd_retry_session_reuse_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_prompt_prepared(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Prompt preparation should clear xsd retry pending"
    );
    assert!(
        new_state.continuation.xsd_retry_session_reuse_pending,
        "Prompt preparation must preserve the session reuse signal for the upcoming retry"
    );
}

#[test]
fn test_review_agent_invoked_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_agent_invoked(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Agent invocation should clear xsd retry pending"
    );
}

#[test]
fn test_commit_prompt_prepared_clears_xsd_retry_pending_but_preserves_session_reuse_signal() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            xsd_retry_session_reuse_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::commit_prompt_prepared(1));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Prompt preparation should clear xsd retry pending"
    );
    assert!(
        new_state.continuation.xsd_retry_session_reuse_pending,
        "Prompt preparation must preserve the session reuse signal for the upcoming retry"
    );
}

#[test]
fn test_commit_agent_invoked_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::commit_agent_invoked(1));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Agent invocation should clear xsd retry pending"
    );
}

#[test]
fn test_review_pass_completed_clean_resets_commit_diff_flags() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        commit_diff_prepared: true,
        commit_diff_empty: true,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

    assert!(!new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
}

/// Test that review output validation failure resets agent invocation state
/// so the agent gets re-invoked with the XSD retry prompt.
///
/// This is the regression test for the bug where XSD retry prompts were
/// prepared but never sent to the AI agent because `review_agent_invoked_pass`
/// was not reset.
#[test]
fn test_review_output_validation_failed_resets_agent_invoked_pass() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        review_agent_invoked_pass: Some(0), // Agent was invoked
        review_issues_xml_extracted_pass: None, // Extraction not done yet
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::review_output_validation_failed(0, 0, None),
    );

    // After validation failure, agent invocation should be reset so orchestration
    // can re-invoke the agent with the XSD retry prompt
    assert!(
        new_state.review_agent_invoked_pass.is_none(),
        "review_agent_invoked_pass should be reset after validation failure, got {:?}",
        new_state.review_agent_invoked_pass
    );
}

/// Test that review issues.xml missing resets agent invocation state
/// so the agent gets re-invoked with the XSD retry prompt.
#[test]
fn test_review_issues_xml_missing_resets_agent_invoked_pass() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        review_agent_invoked_pass: Some(0), // Agent was invoked
        review_issues_xml_extracted_pass: None,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_issues_xml_missing(0, 0, None));

    // After missing XML is detected, agent invocation should be reset so orchestration
    // can re-invoke the agent with the XSD retry prompt
    assert!(
        new_state.review_agent_invoked_pass.is_none(),
        "review_agent_invoked_pass should be reset after issues.xml missing, got {:?}",
        new_state.review_agent_invoked_pass
    );
}

/// Test that fix output validation failure resets agent invocation state
/// so the agent gets re-invoked with the XSD retry prompt.
#[test]
fn test_fix_output_validation_failed_resets_agent_invoked_pass() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        review_issues_found: true,       // Indicates we're in fix mode
        fix_agent_invoked_pass: Some(0), // Agent was invoked
        fix_result_xml_extracted_pass: None,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::fix_output_validation_failed(0, 0, None),
    );

    // After validation failure, agent invocation should be reset so orchestration
    // can re-invoke the agent with the XSD retry prompt
    assert!(
        new_state.fix_agent_invoked_pass.is_none(),
        "fix_agent_invoked_pass should be reset after validation failure, got {:?}",
        new_state.fix_agent_invoked_pass
    );
}

/// Test that fix result.xml missing resets agent invocation state
/// so the agent gets re-invoked with the XSD retry prompt.
#[test]
fn test_fix_result_xml_missing_resets_agent_invoked_pass() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        review_issues_found: true,       // Indicates we're in fix mode
        fix_agent_invoked_pass: Some(0), // Agent was invoked
        fix_result_xml_extracted_pass: None,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::fix_result_xml_missing(0, 0, None));

    // After missing XML is detected, agent invocation should be reset so orchestration
    // can re-invoke the agent with the XSD retry prompt
    assert!(
        new_state.fix_agent_invoked_pass.is_none(),
        "fix_agent_invoked_pass should be reset after fix_result.xml missing, got {:?}",
        new_state.fix_agent_invoked_pass
    );
}

// =========================================================================
// Planning XSD retry orchestration reset tests
// =========================================================================

/// Test that planning output validation failure resets agent invocation state
/// so the agent gets re-invoked with the XSD retry prompt.
#[test]
fn test_planning_output_validation_failed_resets_agent_invoked_iteration() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 0,
        planning_agent_invoked_iteration: Some(0), // Agent was invoked
        planning_xml_extracted_iteration: None,    // Extraction not done yet
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::planning_output_validation_failed(0, 0),
    );

    // After validation failure, agent invocation should be reset so orchestration
    // can re-invoke the agent with the XSD retry prompt
    assert!(
        new_state.planning_agent_invoked_iteration.is_none(),
        "planning_agent_invoked_iteration should be reset after validation failure, got {:?}",
        new_state.planning_agent_invoked_iteration
    );
    assert!(
        new_state.planning_prompt_prepared_iteration.is_none(),
        "planning_prompt_prepared_iteration should be reset for XSD retry prompt preparation"
    );
}

/// Test that plan.xml missing resets agent invocation state
/// so the agent gets re-invoked with the XSD retry prompt.
#[test]
fn test_planning_plan_xml_missing_resets_agent_invoked_iteration() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 0,
        planning_agent_invoked_iteration: Some(0), // Agent was invoked
        planning_xml_extracted_iteration: None,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::planning_xml_missing(0, 0));

    // After missing XML is detected, agent invocation should be reset so orchestration
    // can re-invoke the agent with the XSD retry prompt
    assert!(
        new_state.planning_agent_invoked_iteration.is_none(),
        "planning_agent_invoked_iteration should be reset after plan.xml missing, got {:?}",
        new_state.planning_agent_invoked_iteration
    );
}

// =========================================================================
// Development XSD retry orchestration reset tests
// =========================================================================

/// Test that development output validation failure resets analysis agent invocation
/// so the analysis agent gets re-invoked with the XSD retry.
///
/// Note: Development XSD retry is for the ANALYSIS agent output, not the developer agent.
/// So we preserve developer progress and only reset analysis_agent_invoked_iteration.
#[test]
fn test_development_output_validation_failed_resets_analysis_agent_invoked() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        development_agent_invoked_iteration: Some(1), // Developer was invoked
        analysis_agent_invoked_iteration: Some(1),    // Analysis was invoked
        development_xml_extracted_iteration: None,    // Extraction not done yet
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(1, 0),
    );

    // Developer progress should be preserved
    assert_eq!(
        new_state.development_agent_invoked_iteration,
        Some(1),
        "development_agent_invoked_iteration should be preserved (XSD retry is for analysis)"
    );
    // Analysis agent should be reset for retry
    assert!(
        new_state.analysis_agent_invoked_iteration.is_none(),
        "analysis_agent_invoked_iteration should be reset for XSD retry, got {:?}",
        new_state.analysis_agent_invoked_iteration
    );
}

/// Test that development_result.xml missing resets analysis agent invocation.
#[test]
fn test_development_xml_missing_resets_analysis_agent_invoked() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        development_agent_invoked_iteration: Some(1),
        analysis_agent_invoked_iteration: Some(1),
        development_xml_extracted_iteration: None,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::development_xml_missing(1, 0));

    // Developer progress should be preserved
    assert_eq!(
        new_state.development_agent_invoked_iteration,
        Some(1),
        "development_agent_invoked_iteration should be preserved"
    );
    // Analysis agent should be reset for retry
    assert!(
        new_state.analysis_agent_invoked_iteration.is_none(),
        "analysis_agent_invoked_iteration should be reset after xml missing, got {:?}",
        new_state.analysis_agent_invoked_iteration
    );
}

// =========================================================================
// Commit XSD retry orchestration reset tests
// =========================================================================

/// Test that commit message validation failure resets agent invocation state
/// so the agent gets re-invoked with the XSD retry prompt.
#[test]
fn test_commit_message_validation_failed_resets_agent_invoked() {
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        commit_agent_invoked: true, // Agent was invoked
        commit_xml_extracted: false,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    // After validation failure, agent invocation should be reset so orchestration
    // can re-invoke the agent with the XSD retry prompt
    assert!(
        !new_state.commit_agent_invoked,
        "commit_agent_invoked should be reset after validation failure"
    );
    assert!(
        !new_state.commit_prompt_prepared,
        "commit_prompt_prepared should be reset for XSD retry prompt preparation"
    );
}

// =========================================================================
// XSD retry session reuse tests
// =========================================================================

/// Test that XSD retry preserves session ID for reuse via xsd_retry_session_reuse_pending flag.
#[test]
fn test_planning_xsd_retry_sets_session_reuse_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 0,
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("session-123".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::planning_output_validation_failed(0, 0),
    );

    assert!(
        new_state.continuation.xsd_retry_session_reuse_pending,
        "XSD retry should set session reuse pending to preserve session for retry"
    );
    // Session ID should still be present (not cleared)
    assert_eq!(
        new_state.agent_chain.last_session_id,
        Some("session-123".to_string()),
        "Session ID should be preserved for XSD retry reuse"
    );
}

/// Test that XSD retry in review phase preserves session ID for reuse.
#[test]
fn test_review_xsd_retry_sets_session_reuse_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            )
            .with_session_id(Some("session-456".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::review_output_validation_failed(0, 0, None),
    );

    assert!(
        new_state.continuation.xsd_retry_session_reuse_pending,
        "XSD retry should set session reuse pending"
    );
    assert_eq!(
        new_state.agent_chain.last_session_id,
        Some("session-456".to_string()),
        "Session ID should be preserved for XSD retry reuse"
    );
}

/// Test that XSD retry in commit phase preserves session ID for reuse.
#[test]
fn test_commit_xsd_retry_sets_session_reuse_pending() {
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        agent_chain: AgentChainState::initial()
            .with_agents(vec!["agent1".to_string()], vec![vec![]], AgentRole::Commit)
            .with_session_id(Some("session-789".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    assert!(
        new_state.continuation.xsd_retry_session_reuse_pending,
        "XSD retry should set session reuse pending"
    );
    assert_eq!(
        new_state.agent_chain.last_session_id,
        Some("session-789".to_string()),
        "Session ID should be preserved for XSD retry reuse"
    );
}

// =========================================================================
// Agent fallback (XSD exhausted) clears session tests
// =========================================================================

/// Test that planning XSD exhaustion clears session ID when switching agents.
#[test]
fn test_planning_xsd_exhausted_clears_session_id() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 0,
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("session-123".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::planning_output_validation_failed(0, 0),
    );

    // XSD retries exhausted, should switch agent AND clear session
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent"
    );
    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "Session ID should be cleared when switching agents after XSD exhaustion"
    );
    assert!(
        !new_state.continuation.xsd_retry_session_reuse_pending,
        "Session reuse pending should be cleared after agent switch"
    );
}

/// Test that review XSD exhaustion clears session ID when switching agents.
#[test]
fn test_review_xsd_exhausted_clears_session_id() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Reviewer,
            )
            .with_session_id(Some("session-456".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::review_output_validation_failed(0, 0, None),
    );

    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent"
    );
    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "Session ID should be cleared when switching agents after XSD exhaustion"
    );
}

/// Test that fix XSD exhaustion clears session ID when switching agents.
#[test]
fn test_fix_xsd_exhausted_clears_session_id() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        review_issues_found: true,
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Reviewer,
            )
            .with_session_id(Some("session-789".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::fix_output_validation_failed(0, 0, None),
    );

    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent"
    );
    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "Session ID should be cleared when switching agents after XSD exhaustion"
    );
}

/// Test that development XSD exhaustion clears session ID when switching agents.
#[test]
fn test_development_xsd_exhausted_clears_session_id() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Analysis,
            )
            .with_session_id(Some("session-analysis".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(1, 0),
    );

    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent"
    );
    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "Session ID should be cleared when switching agents after XSD exhaustion"
    );
}

// =========================================================================
// Agent fallback resets full orchestration state tests
// =========================================================================

/// Test that planning XSD exhaustion resets ALL orchestration flags for full rollback.
#[test]
fn test_planning_xsd_exhausted_resets_all_orchestration_flags() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 0,
        planning_prompt_prepared_iteration: Some(0),
        planning_xml_cleaned_iteration: Some(0),
        planning_agent_invoked_iteration: Some(0),
        planning_xml_extracted_iteration: Some(0),
        planning_validated_outcome: Some(PlanningValidatedOutcome {
            iteration: 0,
            valid: false,
            markdown: None,
        }),
        planning_markdown_written_iteration: Some(0),
        planning_xml_archived_iteration: Some(0),
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::planning_output_validation_failed(0, 0),
    );

    // ALL orchestration flags should be reset for full rollback
    assert!(
        new_state.planning_prompt_prepared_iteration.is_none(),
        "planning_prompt_prepared_iteration should be reset"
    );
    assert!(
        new_state.planning_xml_cleaned_iteration.is_none(),
        "planning_xml_cleaned_iteration should be reset"
    );
    assert!(
        new_state.planning_agent_invoked_iteration.is_none(),
        "planning_agent_invoked_iteration should be reset"
    );
    assert!(
        new_state.planning_xml_extracted_iteration.is_none(),
        "planning_xml_extracted_iteration should be reset"
    );
    assert!(
        new_state.planning_validated_outcome.is_none(),
        "planning_validated_outcome should be reset"
    );
    assert!(
        new_state.planning_markdown_written_iteration.is_none(),
        "planning_markdown_written_iteration should be reset"
    );
    assert!(
        new_state.planning_xml_archived_iteration.is_none(),
        "planning_xml_archived_iteration should be reset"
    );
    // Continuation state should be reset
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "xsd_retry_count should be reset"
    );
    assert!(
        !new_state.continuation.xsd_retry_pending,
        "xsd_retry_pending should be cleared"
    );
}

/// Test that commit XSD exhaustion resets ALL orchestration flags for full rollback.
#[test]
fn test_commit_xsd_exhausted_resets_all_orchestration_flags() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        commit_prompt_prepared: true,
        commit_agent_invoked: true,
        commit_xml_cleaned: true,
        commit_xml_extracted: true,
        commit_validated_outcome: Some(CommitValidatedOutcome {
            attempt: 1,
            message: None,
            reason: Some("Invalid".to_string()),
        }),
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    // ALL orchestration flags should be reset for full rollback
    assert!(
        !new_state.commit_prompt_prepared,
        "commit_prompt_prepared should be reset"
    );
    assert!(
        !new_state.commit_agent_invoked,
        "commit_agent_invoked should be reset"
    );
    assert!(
        !new_state.commit_xml_cleaned,
        "commit_xml_cleaned should be reset"
    );
    assert!(
        !new_state.commit_xml_extracted,
        "commit_xml_extracted should be reset"
    );
    assert!(
        new_state.commit_validated_outcome.is_none(),
        "commit_validated_outcome should be reset"
    );
    // Continuation state should be reset
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "xsd_retry_count should be reset"
    );
}
