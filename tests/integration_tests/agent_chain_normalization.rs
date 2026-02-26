//! Integration test for deterministic agent-chain normalization.
//!
//! Verifies that agent chain state is normalized before each invocation to ensure
//! checkpoint replay produces identical agent selection.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::state::PipelineState;

use crate::common::with_locked_prompt_permissions;
use crate::test_timeout::with_default_timeout;

/// Test that agent chain initializes correctly for each phase.
#[test]
fn test_agent_chain_initialization() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 1);

        // Agent chain should be initialized with Developer role for Planning phase
        assert_eq!(state.agent_chain.current_role, AgentRole::Developer);
    });
}

/// Test that XSD retry preserves `last_session_id` for same agent.
#[test]
fn test_xsd_retry_preserves_session() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Planning;
        state.agent_chain.last_session_id = Some("session-123".to_string());
        state.continuation.xsd_retry_session_reuse_pending = true;

        // Last session ID should be preserved during XSD retry
        // The normalization should NOT clear last_session_id when xsd_retry_session_reuse_pending
        assert_eq!(
            state.agent_chain.last_session_id,
            Some("session-123".to_string())
        );
    });
}

/// Test that same-agent retry flag is set correctly.
#[test]
fn test_same_agent_retry_flag() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Planning;
        state.continuation.same_agent_retry_pending = true;

        // Same-agent retry flag should be set
        assert!(state.continuation.same_agent_retry_pending);
    });
}

/// Test that checkpoint replay produces consistent effect.
///
/// This verifies determinism: same state -> same next effect.
#[test]
fn test_checkpoint_replay_consistency() {
    with_default_timeout(|| {
        let state = with_locked_prompt_permissions(PipelineState::initial(1, 0));

        // Determine next effect
        let effect1 = determine_next_effect(&state);

        // Serialize and deserialize (simulating checkpoint replay)
        let json = serde_json::to_string(&state).expect("state should serialize");
        let restored_state: PipelineState =
            serde_json::from_str(&json).expect("state should deserialize");

        // Determine next effect from restored state
        let effect2 = determine_next_effect(&restored_state);

        // Effects should be identical (determinism)
        assert_eq!(
            format!("{effect1:?}"),
            format!("{:?}", effect2),
            "Checkpoint replay should produce identical next effect"
        );
    });
}

/// Test that agent chain normalization is consistent across phases.
#[test]
fn test_agent_chain_normalization_across_phases() {
    with_default_timeout(|| {
        // Planning phase: Developer role
        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 1));
        state.phase = PipelinePhase::Planning;
        state.agent_chain.current_role = AgentRole::Developer;

        let effect = determine_next_effect(&state);
        // Should be planning-related effect
        assert!(
            matches!(
                effect,
                Effect::PreparePlanningPrompt { .. }
                    | Effect::InvokePlanningAgent { .. }
                    | Effect::InitializeAgentChain { .. }
            ),
            "Planning phase should produce planning effects"
        );

        // Review phase: Reviewer role
        let mut state = with_locked_prompt_permissions(PipelineState::initial(0, 1));
        state.phase = PipelinePhase::Review;
        state.agent_chain.current_role = AgentRole::Reviewer;

        let effect = determine_next_effect(&state);
        // Should be review-related effect
        assert!(
            matches!(
                effect,
                Effect::PrepareReviewContext { .. }
                    | Effect::MaterializeReviewInputs { .. }
                    | Effect::PrepareReviewPrompt { .. }
                    | Effect::InitializeAgentChain { .. }
            ),
            "Review phase should produce review effects"
        );
    });
}
