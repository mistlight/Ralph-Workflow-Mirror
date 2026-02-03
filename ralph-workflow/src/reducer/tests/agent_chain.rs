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
fn test_agent_invocation_started_preserves_agent_chain_indices() {
    let base_state = create_test_state();
    let mut agent_chain = base_state.agent_chain.with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![
            vec!["model1".to_string()],
            vec!["model2".to_string(), "model3".to_string()],
        ],
        AgentRole::Developer,
    );
    agent_chain.retry_cycle = 2;
    agent_chain.rate_limit_continuation_prompt = Some("saved prompt".to_string());

    // Start from a non-zero position so the test actually verifies reset behavior.
    let state = PipelineState {
        agent_chain: agent_chain.switch_to_next_agent().advance_to_next_model(),
        ..base_state
    };

    assert_eq!(state.agent_chain.current_agent_index, 1);
    assert_eq!(state.agent_chain.current_model_index, 1);
    assert_eq!(state.agent_chain.retry_cycle, 2);

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_started(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("model1".to_string()),
        ),
    );

    // InvocationStarted should not change indices or cycle tracking, but should
    // clear any saved continuation prompt (prompt consumption is reducer-driven).
    assert_eq!(new_state.agent_chain.current_agent_index, 1);
    assert_eq!(new_state.agent_chain.current_model_index, 1);
    assert_eq!(new_state.agent_chain.retry_cycle, 2);
    assert!(new_state
        .agent_chain
        .rate_limit_continuation_prompt
        .is_none());
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
fn test_agent_invocation_failed_with_retriable_network_advances_model() {
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
            AgentErrorKind::Network,
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
fn test_agent_retry_cycle_started_clears_backoff_pending() {
    let mut base_state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 3,
        reviewer_pass: 1,
        ..create_test_state()
    };
    // Set backoff_pending_ms to verify it gets cleared
    base_state.agent_chain.backoff_pending_ms = Some(1000);

    let new_state = reduce(
        base_state.clone(),
        PipelineEvent::agent_retry_cycle_started(AgentRole::Developer, 2),
    );

    // RetryCycleStarted clears backoff_pending_ms to allow the retry cycle to proceed.
    // Other state fields are preserved.
    assert_eq!(new_state.phase, base_state.phase);
    assert_eq!(new_state.iteration, base_state.iteration);
    assert_eq!(new_state.reviewer_pass, base_state.reviewer_pass);
    assert_eq!(
        new_state.agent_chain.current_agent_index,
        base_state.agent_chain.current_agent_index
    );
    assert_eq!(
        new_state.agent_chain.current_model_index,
        base_state.agent_chain.current_model_index
    );
    // Verify backoff_pending_ms is cleared (the critical behavior)
    assert!(
        new_state.agent_chain.backoff_pending_ms.is_none(),
        "backoff_pending_ms should be cleared after RetryCycleStarted"
    );
}

#[test]
fn test_agent_invocation_failed_retriable_network_on_last_model_wraps_to_first_model() {
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
            AgentErrorKind::Network,
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
fn test_agent_invocation_failed_retriable_network_on_single_model_wraps() {
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
            AgentErrorKind::Network,
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

// ============================================================================
// Timeout Fallback Tests
// ============================================================================

#[test]
fn test_timed_out_retries_same_agent_before_fallback() {
    // Setup: two agents, each with two models. Budget 2 means:
    // - First timeout retries same agent
    // - Second timeout falls back to next agent
    let base_state = create_test_state();
    let state = PipelineState {
        continuation: crate::reducer::state::ContinuationState::with_limits(2, 3),
        agent_chain: base_state.agent_chain.with_agents(
            vec!["agent-a".to_string(), "agent-b".to_string()],
            vec![
                vec!["model-a1".to_string(), "model-a2".to_string()],
                vec!["model-b1".to_string()],
            ],
            AgentRole::Developer,
        ),
        ..base_state
    };

    assert_eq!(
        state.agent_chain.current_agent().map(String::as_str),
        Some("agent-a")
    );
    assert_eq!(state.agent_chain.current_model_index, 0);

    let after_first_timeout = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Developer, "agent-a".to_string()),
    );

    assert_eq!(
        after_first_timeout
            .agent_chain
            .current_agent()
            .map(String::as_str),
        Some("agent-a"),
        "Timeout should retry same agent first"
    );
    assert_eq!(
        after_first_timeout.agent_chain.current_model_index, 0,
        "Timeout retry should not advance model"
    );

    let after_second_timeout = reduce(
        after_first_timeout,
        PipelineEvent::agent_timed_out(AgentRole::Developer, "agent-a".to_string()),
    );

    assert_eq!(
        after_second_timeout
            .agent_chain
            .current_agent()
            .map(String::as_str),
        Some("agent-b"),
        "After retry budget exhaustion, timeout should fall back to next agent"
    );
    assert_eq!(
        after_second_timeout.agent_chain.current_model_index, 0,
        "Model index should reset to 0 when switching agents"
    );
}

#[test]
fn test_timed_out_clears_session_id_even_when_retrying_same_agent() {
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state
            .agent_chain
            .with_agents(
                vec!["agent-a".to_string(), "agent-b".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("session-123".to_string())),
        ..base_state
    };

    // Verify session ID is set
    assert_eq!(
        state.agent_chain.last_session_id,
        Some("session-123".to_string())
    );

    // Apply timeout fallback
    let new_state = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Developer, "agent-a".to_string()),
    );

    // Session ID should be cleared (new agent, new session)
    assert_eq!(
        new_state.agent_chain.last_session_id, None,
        "TimedOut should clear session ID"
    );
}

#[test]
fn test_timed_out_from_last_agent_increments_retry_cycle_when_budget_exhausted() {
    let base_state = create_test_state();
    let state = PipelineState {
        continuation: crate::reducer::state::ContinuationState::with_limits(1, 3),
        agent_chain: base_state
            .agent_chain
            .with_agents(
                vec!["agent-a".to_string(), "agent-b".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .switch_to_next_agent(), // Move to last agent (agent-b)
        ..base_state
    };

    // Verify we're on the last agent
    assert_eq!(
        state.agent_chain.current_agent().map(String::as_str),
        Some("agent-b")
    );
    assert_eq!(state.agent_chain.retry_cycle, 0);

    // Apply timeout fallback from last agent
    let new_state = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Developer, "agent-b".to_string()),
    );

    // Should wrap back to first agent and increment retry cycle
    assert_eq!(
        new_state.agent_chain.current_agent().map(String::as_str),
        Some("agent-a"),
        "Should wrap back to first agent after falling back"
    );
    assert_eq!(
        new_state.agent_chain.retry_cycle, 1,
        "Should increment retry cycle when wrapping"
    );
}

// ============================================================================
// Integration-Style Tests (Event Loop Simulation)
// ============================================================================

/// Simulates running the event loop to verify backoff wait does not cause infinite loops.
///
/// This test starts with a state that has `backoff_pending_ms=Some(...)` and runs
/// through the effect/reduce cycle to verify the pipeline progresses correctly
/// without getting stuck repeating BackoffWait effects.
#[test]
fn test_backoff_wait_does_not_cause_infinite_loop_in_event_loop_simulation() {
    use crate::reducer::effect::Effect;
    use crate::reducer::orchestration::determine_next_effect;
    use crate::reducer::state::{AgentChainState, ContinuationState};

    let mut state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(2),
        continuation: ContinuationState::default(),
        development_context_prepared_iteration: Some(1),
        development_prompt_prepared_iteration: Some(1),
        development_xml_cleaned_iteration: Some(1),
        ..create_test_state()
    };
    // Set backoff pending to trigger the backoff wait path
    state.agent_chain.backoff_pending_ms = Some(100);

    let max_iterations = 20;
    let mut backoff_wait_count = 0;

    for _ in 0..max_iterations {
        let effect = determine_next_effect(&state);

        match effect {
            Effect::BackoffWait { role, cycle, .. } => {
                backoff_wait_count += 1;
                if backoff_wait_count > 2 {
                    panic!(
                        "BackoffWait repeated {} times - potential infinite loop",
                        backoff_wait_count
                    );
                }
                // Simulate handler emitting RetryCycleStarted
                state = reduce(state, PipelineEvent::agent_retry_cycle_started(role, cycle));
            }
            _ => {
                // Successfully progressed past backoff - test passes
                break;
            }
        }
    }

    // Verify backoff_pending_ms was cleared
    assert!(
        state.agent_chain.backoff_pending_ms.is_none(),
        "backoff_pending_ms should be cleared after RetryCycleStarted event"
    );
}
