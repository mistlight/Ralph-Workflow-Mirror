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
        PipelineEvent::agent_chain_initialized(
            AgentRole::Developer,
            agents.clone(),
            3,
            1000,
            2.0,
            60000,
        ),
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
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            agents.clone(),
            3,
            1000,
            2.0,
            60000,
        ),
    );

    assert_eq!(new_state.agent_chain.agents, agents);
}

#[test]
fn test_agent_invocation_started_resets_agent_chain() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_started(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("model1".to_string()),
        ),
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
        PipelineEvent::agent_invocation_succeeded(AgentRole::Developer, "agent1".to_string()),
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
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            AgentErrorKind::Timeout,
            true,
        ),
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
        PipelineEvent::agent_fallback_triggered(
            AgentRole::Developer,
            "agent1".to_string(),
            "agent2".to_string(),
        ),
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
        PipelineEvent::agent_chain_exhausted(AgentRole::Developer),
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
        PipelineEvent::agent_chain_exhausted(AgentRole::Developer),
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
        PipelineEvent::agent_model_fallback_triggered(
            AgentRole::Developer,
            "agent1".to_string(),
            "model1".to_string(),
            "model2".to_string(),
        ),
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
        PipelineEvent::agent_retry_cycle_started(AgentRole::Developer, 2),
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
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            AgentErrorKind::Timeout,
            true,
        ),
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
        PipelineEvent::agent_fallback_triggered(
            AgentRole::Developer,
            "agent2".to_string(),
            "agent1".to_string(),
        ),
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
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            AgentErrorKind::ParsingError,
            false,
        ),
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
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            AgentErrorKind::Timeout,
            true,
        ),
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
        PipelineEvent::agent_chain_initialized(
            AgentRole::Commit,
            agents.clone(),
            3,
            1000,
            2.0,
            60000,
        ),
    );

    assert_eq!(new_state.agent_chain.agents, agents);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
    assert_eq!(new_state.agent_chain.current_role, AgentRole::Commit);
}

#[test]
fn test_agent_chain_initialized_resets_retry_cycle() {
    let base_state = create_test_state();
    // Setup with non-zero retry_cycle
    let mut agent_chain = base_state.agent_chain.clone();
    agent_chain.retry_cycle = 5; // Start with retry_cycle = 5

    let state = PipelineState {
        agent_chain,
        ..base_state
    };

    assert_eq!(state.agent_chain.retry_cycle, 5);

    let new_agents = vec!["new-agent1".to_string(), "new-agent2".to_string()];
    let new_state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            new_agents.clone(),
            3,
            1000,
            2.0,
            60000,
        ),
    );

    // CRITICAL: AgentChainInitialized uses reset_for_role() which RESETS retry_cycle to 0
    // This is DIFFERENT from reset() which preserves retry_cycle
    assert_eq!(new_state.agent_chain.agents, new_agents);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 0); // RESET to 0, not preserved
    assert_eq!(new_state.agent_chain.current_role, AgentRole::Reviewer);
}

#[test]
fn test_agent_chain_initialized_with_empty_list() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(AgentRole::Developer, vec![], 3, 1000, 2.0, 60000),
    );

    // Empty agent list should be accepted
    assert_eq!(new_state.agent_chain.agents.len(), 0);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
}

#[test]
fn test_agent_chain_initialized_contains_full_fallback_chain() {
    // When AgentChainInitialized event is emitted, it should contain
    // all agents from the fallback config, not just a single agent
    let state = create_test_state();
    let agents = vec![
        "codex".to_string(),
        "opencode".to_string(),
        "claude".to_string(),
    ];

    let new_state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            agents.clone(),
            3,
            1000,
            2.0,
            60000,
        ),
    );

    assert_eq!(
        new_state.agent_chain.agents, agents,
        "Agent chain should contain all agents from the fallback config"
    );
    assert_eq!(
        new_state.agent_chain.current_agent_index, 0,
        "Agent chain should start at index 0"
    );
    assert_eq!(
        new_state.agent_chain.current_agent().map(String::as_str),
        Some("codex"),
        "Current agent should be the first in the chain"
    );
}
