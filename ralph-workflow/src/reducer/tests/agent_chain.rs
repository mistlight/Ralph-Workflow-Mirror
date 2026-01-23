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
