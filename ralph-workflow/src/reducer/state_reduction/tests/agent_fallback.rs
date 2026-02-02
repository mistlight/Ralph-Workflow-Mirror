// Agent fallback and rate limit tests.
//
// Tests for agent failure scenarios, model/agent fallback, rate limit handling,
// auth fallback, and agent chain exhaustion.

use super::*;

#[test]
fn test_reduce_agent_fallback_to_next_model() {
    let state = create_test_state();
    let initial_agent = state.agent_chain.current_agent().unwrap().clone();
    let initial_model_index = state.agent_chain.current_model_index;

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            initial_agent.clone(),
            1,
            AgentErrorKind::Network,
            true,
        ),
    );

    assert_ne!(
        new_state.agent_chain.current_model_index,
        initial_model_index
    );
}

#[test]
fn test_reduce_all_agent_failure_scenarios() {
    let state = create_test_state();
    let initial_agent_index = state.agent_chain.current_agent_index;
    let initial_model_index = state.agent_chain.current_model_index;
    let agent_name = state.agent_chain.current_agent().unwrap().clone();

    let network_error_state = reduce(
        state.clone(),
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name.clone(),
            1,
            AgentErrorKind::Network,
            true,
        ),
    );
    assert_eq!(
        network_error_state.agent_chain.current_agent_index,
        initial_agent_index
    );
    assert!(network_error_state.agent_chain.current_model_index > initial_model_index);

    let auth_error_state = reduce(
        state.clone(),
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name.clone(),
            1,
            AgentErrorKind::Authentication,
            false,
        ),
    );
    assert!(auth_error_state.agent_chain.current_agent_index > initial_agent_index);
    assert_eq!(
        auth_error_state.agent_chain.current_model_index,
        initial_model_index
    );

    let internal_error_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name,
            139,
            AgentErrorKind::InternalError,
            false,
        ),
    );
    assert!(internal_error_state.agent_chain.current_agent_index > initial_agent_index);
}

#[test]
fn test_reduce_agent_chain_exhaustion() {
    let state = PipelineState {
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec!["model1".to_string()]],
                AgentRole::Developer,
            )
            .with_max_cycles(3),
        ..create_test_state()
    };

    let exhausted_state = reduce(
        state,
        PipelineEvent::agent_chain_exhausted(AgentRole::Developer),
    );

    assert_eq!(exhausted_state.agent_chain.current_agent_index, 0);
    assert_eq!(exhausted_state.agent_chain.current_model_index, 0);
    assert_eq!(exhausted_state.agent_chain.retry_cycle, 1);
}

#[test]
fn test_reduce_agent_fallback_triggers_fallback_event() {
    let state = create_test_state();
    let agent = state.agent_chain.current_agent().unwrap().clone();

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent.clone(),
            1,
            AgentErrorKind::Authentication,
            false,
        ),
    );

    assert!(new_state.agent_chain.current_agent_index > 0);
}

#[test]
fn test_reduce_model_fallback_triggers_for_network_error() {
    let state = create_test_state();
    let initial_model_index = state.agent_chain.current_model_index;
    let agent_name = state.agent_chain.current_agent().unwrap().clone();

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name,
            1,
            AgentErrorKind::Network,
            true,
        ),
    );

    assert!(new_state.agent_chain.current_model_index > initial_model_index);
}

#[test]
fn test_rate_limit_fallback_switches_agent() {
    let state = create_test_state();
    let initial_agent_index = state.agent_chain.current_agent_index;

    let new_state = reduce(
        state,
        PipelineEvent::agent_rate_limit_fallback(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("test prompt".to_string()),
        ),
    );

    // Should switch to next agent
    assert!(
        new_state.agent_chain.current_agent_index > initial_agent_index,
        "Rate limit should trigger agent fallback, not model fallback"
    );
    // Should preserve prompt
    assert_eq!(
        new_state.agent_chain.rate_limit_continuation_prompt,
        Some("test prompt".to_string())
    );
}

#[test]
fn test_rate_limit_fallback_with_no_prompt_context() {
    let state = create_test_state();
    let initial_agent_index = state.agent_chain.current_agent_index;

    let new_state = reduce(
        state,
        PipelineEvent::agent_rate_limit_fallback(AgentRole::Developer, "agent1".to_string(), None),
    );

    // Should still switch to next agent
    assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
    // Prompt context should be None
    assert!(new_state
        .agent_chain
        .rate_limit_continuation_prompt
        .is_none());
}

#[test]
fn test_success_clears_rate_limit_continuation_prompt() {
    let mut state = create_test_state();
    state.agent_chain.rate_limit_continuation_prompt = Some("old prompt".to_string());

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_succeeded(AgentRole::Developer, "agent1".to_string()),
    );

    assert!(
        new_state
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "Success should clear rate limit continuation prompt"
    );
}

#[test]
fn test_auth_fallback_clears_session_and_advances_agent() {
    let mut chain = AgentChainState::initial()
        .with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        )
        .with_session_id(Some("session-123".to_string()));
    chain.rate_limit_continuation_prompt = Some("some saved prompt".to_string());

    let state = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: chain,
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(
        state,
        PipelineEvent::agent_auth_fallback(AgentRole::Developer, "agent1".to_string()),
    );

    // Should advance to next agent
    assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent2");

    // Session should be cleared
    assert!(new_state.agent_chain.last_session_id.is_none());

    // Auth fallback semantics: switch agents WITHOUT prompt context.
    // Any previously-saved rate-limit continuation prompt must be cleared so we
    // don't accidentally carry prompt context across an auth fallback.
    assert!(
        new_state
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "AuthFallback must clear any existing continuation prompt"
    );
}

#[test]
fn test_rate_limit_fallback_clears_session_id() {
    // RateLimitFallback preserves prompt context, but MUST NOT preserve session IDs
    // across agents.
    let chain = AgentChainState::initial()
        .with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        )
        .with_session_id(Some("session-123".to_string()));

    let state = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: chain,
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(
        state,
        PipelineEvent::agent_rate_limit_fallback(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("preserved prompt".to_string()),
        ),
    );

    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "RateLimitFallback must clear session IDs when switching agents"
    );
    assert_eq!(
        new_state.agent_chain.rate_limit_continuation_prompt,
        Some("preserved prompt".to_string()),
        "RateLimitFallback should preserve prompt context"
    );
}

#[test]
fn test_auth_fallback_does_not_set_continuation_prompt() {
    // Setup: state with NO existing continuation prompt
    let chain = AgentChainState::initial().with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![vec![], vec![]],
        AgentRole::Developer,
    );

    let state = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: chain,
        ..PipelineState::initial(5, 2)
    };

    // Auth fallback should NOT set a continuation prompt
    let new_state = reduce(
        state,
        PipelineEvent::agent_auth_fallback(AgentRole::Developer, "agent1".to_string()),
    );

    // Key assertion: AuthFallback does NOT set prompt context
    // (unlike RateLimitFallback which preserves the prompt)
    assert!(
        new_state
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "AuthFallback should not set continuation prompt (only RateLimitFallback does)"
    );
}

#[test]
fn test_rate_limit_vs_auth_fallback_prompt_semantics() {
    // This test documents the key semantic difference between the two fallback types
    let base_chain = AgentChainState::initial().with_agents(
        vec![
            "agent1".to_string(),
            "agent2".to_string(),
            "agent3".to_string(),
        ],
        vec![vec![], vec![], vec![]],
        AgentRole::Developer,
    );

    // Test 1: RateLimitFallback preserves prompt
    let state1 = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: base_chain.clone(),
        ..PipelineState::initial(5, 2)
    };

    let after_rate_limit = reduce(
        state1,
        PipelineEvent::agent_rate_limit_fallback(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("preserved prompt".to_string()),
        ),
    );

    assert_eq!(
        after_rate_limit.agent_chain.rate_limit_continuation_prompt,
        Some("preserved prompt".to_string()),
        "RateLimitFallback should preserve prompt context"
    );

    // Test 2: AuthFallback does NOT set prompt (credentials issue, not exhaustion)
    let state2 = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: base_chain,
        ..PipelineState::initial(5, 2)
    };

    let after_auth = reduce(
        state2,
        PipelineEvent::agent_auth_fallback(AgentRole::Developer, "agent1".to_string()),
    );

    assert!(
        after_auth
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "AuthFallback should not set prompt context (credentials issue, not exhaustion)"
    );
}
