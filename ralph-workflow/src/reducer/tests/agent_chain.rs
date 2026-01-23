//! Tests for agent chain events (initialization, fallback, exhaustion).

use super::*;
use crate::agents::AgentRole;
use crate::reducer::event::AgentErrorKind;

#[test]
fn test_agent_chain_initialized_for_developer() {
    let state = create_test_state();
    let agents = vec!["agent1".to_string(), "agent2".to_string()];

    let new_state = reduce(
        state,
        PipelineEvent::AgentChainInitialized {
            agents: agents.clone(),
            role: AgentRole::Developer,
        },
    );

    assert_eq!(new_state.agent_chain.agents, agents);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

#[test]
fn test_agent_chain_initialized_for_reviewer() {
    let state = create_test_state();
    let agents = vec!["reviewer1".to_string()];

    let new_state = reduce(
        state,
        PipelineEvent::AgentChainInitialized {
            agents: agents.clone(),
            role: AgentRole::Reviewer,
        },
    );

    assert_eq!(new_state.agent_chain.agents, agents);
}

#[test]
fn test_agent_invocation_started_resets_agent_chain() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::AgentInvocationStarted {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            model: Some("model1".to_string()),
        },
    );

    // AgentInvocationStarted resets the agent chain
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

#[test]
fn test_agent_invocation_succeeded_preserves_indices() {
    let state = create_test_state();
    let new_state = reduce(
        state.clone(),
        PipelineEvent::AgentInvocationSucceeded {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
        },
    );

    assert_eq!(
        new_state.agent_chain.current_agent_index,
        state.agent_chain.current_agent_index
    );
    assert_eq!(
        new_state.agent_chain.current_model_index,
        state.agent_chain.current_model_index
    );
}

#[test]
fn test_agent_invocation_failed_with_retriable_advances_model() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec!["agent1".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        ),
        ..base_state
    };

    let new_state = reduce(
        state,
        PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 1,
            error_kind: AgentErrorKind::Timeout,
            retriable: true,
        },
    );

    // Should advance to next model (0 -> 1)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 1);
}

#[test]
fn test_agent_fallback_triggered_switches_agent() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec!["model1".to_string()], vec!["model2".to_string()]],
            AgentRole::Developer,
        ),
        ..base_state
    };

    let new_state = reduce(
        state,
        PipelineEvent::AgentFallbackTriggered {
            role: AgentRole::Developer,
            from_agent: "agent1".to_string(),
            to_agent: "agent2".to_string(),
        },
    );

    // Should switch to next agent (0 -> 1) and reset model (0)
    assert_eq!(new_state.agent_chain.current_agent_index, 1);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

#[test]
fn test_agent_chain_exhausted_increments_retry_cycle() {
    let state = create_test_state();
    let initial_retry_cycle = state.agent_chain.retry_cycle;

    let new_state = reduce(
        state,
        PipelineEvent::AgentChainExhausted {
            role: AgentRole::Developer,
        },
    );

    assert_eq!(new_state.agent_chain.retry_cycle, initial_retry_cycle + 1);
}

#[test]
fn test_agent_chain_exhausted_resets_indices() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![
                vec!["model1".to_string(), "model2".to_string()],
                vec!["model3".to_string()],
            ],
            AgentRole::Developer,
        ),
        ..base_state
    };

    // Manually set indices to non-zero
    let state = PipelineState {
        agent_chain: state
            .agent_chain
            .advance_to_next_model()
            .switch_to_next_agent(),
        ..state
    };

    let new_state = reduce(
        state,
        PipelineEvent::AgentChainExhausted {
            role: AgentRole::Developer,
        },
    );

    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

#[test]
fn test_agent_model_fallback_triggered_advances_to_next_model() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec!["agent1".to_string()],
            vec![vec![
                "model1".to_string(),
                "model2".to_string(),
                "model3".to_string(),
            ]],
            AgentRole::Developer,
        ),
        ..base_state
    };

    // Start at model index 0
    assert_eq!(state.agent_chain.current_model_index, 0);

    let new_state = reduce(
        state,
        PipelineEvent::AgentModelFallbackTriggered {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            from_model: "model1".to_string(),
            to_model: "model2".to_string(),
        },
    );

    // Should advance to next model (0 -> 1)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 1);
}

#[test]
fn test_agent_retry_cycle_started_is_noop() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 3,
        reviewer_pass: 1,
        ..create_test_state()
    };
    let new_state = reduce(
        state.clone(),
        PipelineEvent::AgentRetryCycleStarted {
            role: AgentRole::Developer,
            cycle: 2,
        },
    );

    // AgentRetryCycleStarted is a no-op - all state should be preserved
    assert_eq!(new_state.phase, state.phase);
    assert_eq!(new_state.iteration, state.iteration);
    assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
    assert_eq!(
        new_state.agent_chain.current_agent_index,
        state.agent_chain.current_agent_index
    );
    assert_eq!(
        new_state.agent_chain.current_model_index,
        state.agent_chain.current_model_index
    );
}

#[test]
fn test_agent_invocation_failed_on_last_model_wraps_to_first_model() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state
            .agent_chain
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec!["model1".to_string(), "model2".to_string()]],
                AgentRole::Developer,
            )
            .advance_to_next_model(), // Move to model index 1 (last model)
        ..base_state
    };

    // Verify we're on the last model
    assert_eq!(state.agent_chain.current_model_index, 1);

    let new_state = reduce(
        state,
        PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 1,
            error_kind: AgentErrorKind::Timeout,
            retriable: true,
        },
    );

    // Should wrap back to first model (1 -> 0)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

#[test]
fn test_agent_fallback_from_last_agent_wraps_and_increments_cycle() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state
            .agent_chain
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec!["model1".to_string()], vec!["model2".to_string()]],
                AgentRole::Developer,
            )
            .switch_to_next_agent(), // Move to agent index 1 (last agent)
        ..base_state
    };

    // Verify we're on the last agent and cycle is 0
    assert_eq!(state.agent_chain.current_agent_index, 1);
    assert_eq!(state.agent_chain.retry_cycle, 0);

    let new_state = reduce(
        state,
        PipelineEvent::AgentFallbackTriggered {
            role: AgentRole::Developer,
            from_agent: "agent2".to_string(),
            to_agent: "agent1".to_string(),
        },
    );

    // Should wrap back to first agent (1 -> 0) and increment retry_cycle (0 -> 1)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 1);
}

#[test]
fn test_agent_invocation_failed_non_retriable_switches_agent() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec!["model1".to_string()], vec!["model2".to_string()]],
            AgentRole::Developer,
        ),
        ..base_state
    };

    // Start on first agent
    assert_eq!(state.agent_chain.current_agent_index, 0);

    let new_state = reduce(
        state,
        PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 1,
            error_kind: AgentErrorKind::ParsingError,
            retriable: false,
        },
    );

    // Non-retriable error should switch to next agent (0 -> 1)
    assert_eq!(new_state.agent_chain.current_agent_index, 1);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

#[test]
fn test_agent_invocation_failed_retriable_on_single_model_wraps() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec!["agent1".to_string()],
            vec![vec!["model1".to_string()]], // Only one model
            AgentRole::Developer,
        ),
        ..base_state
    };

    let new_state = reduce(
        state,
        PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 1,
            error_kind: AgentErrorKind::Timeout,
            retriable: true,
        },
    );

    // With only one model, should wrap back to index 0
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

// ============================================================================
// TIER 3: Completeness Tests - Additional Roles and Edge Cases
// ============================================================================

#[test]
fn test_agent_chain_initialized_for_commit_role() {
    let state = create_test_state();
    let agents = vec!["commit-agent1".to_string()];

    let new_state = reduce(
        state,
        PipelineEvent::AgentChainInitialized {
            agents: agents.clone(),
            role: AgentRole::Commit,
        },
    );

    assert_eq!(new_state.agent_chain.agents, agents);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
}

#[test]
fn test_agent_chain_initialized_with_empty_list() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::AgentChainInitialized {
            agents: vec![],
            role: AgentRole::Developer,
        },
    );

    // Empty agent list should be accepted
    assert_eq!(new_state.agent_chain.agents.len(), 0);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
}
