// XSD retry state transitions.
//
// Tests for XSD retry pending flag, retry count management, session ID handling,
// and XSD retry exhaustion across different phases.

use super::*;

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
