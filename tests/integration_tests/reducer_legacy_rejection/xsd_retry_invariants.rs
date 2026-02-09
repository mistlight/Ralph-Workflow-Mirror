//! Integration tests for XSD retry state tracking invariants.
//!
//! Verifies that XSD validation retry logic is tracked explicitly in reducer state
//! rather than hidden in phase handlers, ensuring retry behavior is observable
//! and deterministic.

use crate::test_timeout::with_default_timeout;

// ============================================================================
// XSD RETRY STATE TRACKING TESTS
// ============================================================================

/// Test that XSD validation failures are tracked in reducer state.
///
/// This verifies that `invalid_output_attempts` in ContinuationState is incremented
/// when OutputValidationFailed events are processed, making retry decisions
/// explicit in state rather than hidden in phase code.
#[test]
fn test_xsd_retry_count_in_reducer_state() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Initial state should have zero invalid output attempts
        assert_eq!(
            state.continuation.invalid_output_attempts, 0,
            "Initial state should have 0 invalid_output_attempts"
        );

        // Simulate XSD validation failure - reducer should track this
        let state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );

        // State should track retry count
        assert_eq!(
            state.continuation.invalid_output_attempts, 1,
            "Reducer state must track XSD retry attempts after OutputValidationFailed"
        );

        // Second failure should increment again
        let state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 1),
        );

        assert_eq!(
            state.continuation.invalid_output_attempts, 2,
            "Reducer state must increment invalid_output_attempts on each failure"
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

        // After max retries, invalid_output_attempts should be at max
        assert_eq!(
            current_state.continuation.invalid_output_attempts, 2,
            "Should have expected invalid_output_attempts before exhaustion"
        );

        // One more failure should trigger agent advancement and reset counter
        let final_state = reduce(
            current_state,
            PipelineEvent::development_output_validation_failed(0, 2),
        );

        // Counter should be reset after agent switch
        assert_eq!(
            final_state.continuation.invalid_output_attempts, 0,
            "invalid_output_attempts should reset after agent advancement"
        );

        // Agent chain should have advanced
        assert_eq!(
            final_state.agent_chain.current_agent(),
            Some(&"fallback-agent".to_string()),
            "Agent chain should advance to fallback after exhausting retries"
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

        // Counter should reset for new agent
        assert_eq!(
            current.continuation.invalid_output_attempts, 0,
            "Invalid output attempts must reset after agent switch"
        );
    });
}
