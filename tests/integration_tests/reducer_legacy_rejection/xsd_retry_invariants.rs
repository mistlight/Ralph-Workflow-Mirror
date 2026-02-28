//! Integration tests for XSD retry state tracking invariants.
//!
//! Verifies that XSD validation retry logic is tracked explicitly in reducer state
//! rather than hidden in phase handlers, ensuring retry behavior is observable
//! and deterministic.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../../INTEGRATION_TESTS.md](../../../INTEGRATION_TESTS.md)**.

use crate::test_timeout::with_default_timeout;

// ============================================================================
// XSD RETRY STATE TRACKING TESTS
// ============================================================================

/// Test that XSD validation failures produce observable retry behavior.
///
/// This verifies that when `OutputValidationFailed` events are processed, the reducer
/// produces retry effects (not chain exhaustion), making retry decisions
/// explicit and observable rather than hidden in phase code.
#[test]
fn test_xsd_retry_count_in_reducer_state() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        // Lock permissions so LockPromptPermissions doesn't preempt development effects
        state.prompt_permissions.locked = true;
        state.prompt_permissions.restore_needed = true;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Observable behavior: initial state should start development normally
        let initial_effect = ralph_workflow::reducer::orchestration::determine_next_effect(&state);
        assert!(
            !matches!(
                initial_effect,
                ralph_workflow::reducer::effect::Effect::ReportAgentChainExhausted { .. }
            ),
            "Initial state should begin development normally: {initial_effect:?}"
        );

        // Simulate XSD validation failure - reducer should track this
        let state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );

        // Observable behavior: after a single validation failure, the pipeline
        // should not exhaust the chain (it should retry in some form).
        let effect = ralph_workflow::reducer::orchestration::determine_next_effect(&state);
        assert!(
            !matches!(
                effect,
                ralph_workflow::reducer::effect::Effect::ReportAgentChainExhausted { .. }
            ),
            "After one XSD failure, pipeline should retry (not exhaust chain): {effect:?}"
        );

        // Second failure should still allow retry
        let state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 1),
        );

        let effect = ralph_workflow::reducer::orchestration::determine_next_effect(&state);
        assert!(
            !matches!(
                effect,
                ralph_workflow::reducer::effect::Effect::ReportAgentChainExhausted { .. }
            ),
            "After two XSD failures (under budget), should not exhaust chain: {effect:?}"
        );
    });
}

/// Test that max XSD retries triggers agent advancement via reducer.
///
/// After configured XSD failures, the reducer should
/// advance the agent chain, making fallback behavior explicit in state.
#[test]
fn test_max_xsd_retries_advances_agent_chain_via_reducer() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::{ContinuationState, PipelineState};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::new().with_max_xsd_retry(3);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["primary-agent".to_string(), "fallback-agent".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // Verify we start with primary agent
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"primary-agent".to_string()),
            "Should start with primary agent"
        );

        // Exhaust retries up to configured limit (3)
        let mut current_state = state;
        for attempt in 0..2 {
            current_state = reduce(
                current_state,
                PipelineEvent::development_output_validation_failed(0, attempt),
            );
        }

        // Observable behavior: before exhaustion, pipeline still targets the primary agent
        assert_eq!(
            current_state.agent_chain.current_agent(),
            Some(&"primary-agent".to_string()),
            "Before exhaustion, pipeline should still target primary agent"
        );
        let effect = ralph_workflow::reducer::orchestration::determine_next_effect(&current_state);
        assert!(
            !matches!(
                effect,
                ralph_workflow::reducer::effect::Effect::ReportAgentChainExhausted { .. }
            ),
            "Before exhaustion, pipeline should still retry (not exhaust chain): {effect:?}"
        );

        // One more failure should trigger agent advancement and reset counter
        let final_state = reduce(
            current_state,
            PipelineEvent::development_output_validation_failed(0, 2),
        );

        // Agent chain should have advanced
        assert_eq!(
            final_state.agent_chain.current_agent(),
            Some(&"fallback-agent".to_string()),
            "Agent chain should advance to fallback after exhausting retries"
        );

        // Observable behavior: after agent advancement, pipeline continues development
        // with the fallback agent (not stuck retrying or exhausted)
        let effect = ralph_workflow::reducer::orchestration::determine_next_effect(&final_state);
        assert!(
            !matches!(
                effect,
                ralph_workflow::reducer::effect::Effect::ReportAgentChainExhausted { .. }
            ),
            "After agent advancement, pipeline should continue with fallback agent: {effect:?}"
        );
    });
}

/// Test that XSD retry loop exhaustion triggers reducer state transitions.
///
/// When XSD validation fails repeatedly, the reducer state must track exhaustion
/// and trigger agent advancement. Phase modules must NOT silently give up or
/// make fallback decisions internally.
#[test]
fn test_xsd_retry_exhaustion_triggers_state_transition() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::{ContinuationState, PipelineState};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::new().with_max_xsd_retry(3);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["agent-1".to_string(), "agent-2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // Exhaust retries via reducer events (not hidden in phase code)
        let mut current = state;
        for attempt in 0..3 {
            current = reduce(
                current,
                PipelineEvent::development_output_validation_failed(0, attempt),
            );
        }

        // After exhausting retries, agent chain should advance
        // This proves the retry policy is in reducer, not phase module
        assert_eq!(
            current.agent_chain.current_agent(),
            Some(&"agent-2".to_string()),
            "Agent chain must advance after retry exhaustion (reducer-driven policy)"
        );

        // Observable behavior: after agent switch, pipeline continues development
        // with the new agent (retry counter implicitly reset, pipeline not stuck)
        let effect = ralph_workflow::reducer::orchestration::determine_next_effect(&current);
        assert!(
            !matches!(
                effect,
                ralph_workflow::reducer::effect::Effect::ReportAgentChainExhausted { .. }
            ),
            "After agent switch, pipeline should continue with new agent: {effect:?}"
        );
    });
}
