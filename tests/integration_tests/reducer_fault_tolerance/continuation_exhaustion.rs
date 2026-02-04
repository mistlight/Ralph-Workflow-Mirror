//! Integration tests for development continuation budget exhaustion and agent fallback.
//!
//! These tests verify that when a development iteration's continuation budget is
//! exhausted (due to repeated failed/partial status), the pipeline switches to the
//! next agent in the fallback chain rather than immediately aborting.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelineEvent;
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::{AgentChainState, DevelopmentStatus, PipelineState};
use ralph_workflow::reducer::state_reduction::reduce;

#[test]
fn test_continuation_exhaustion_triggers_agent_fallback() {
    with_default_timeout(|| {
        // Given: A pipeline with 2 agents in the fallback chain
        let agent_chain = AgentChainState::initial().with_agents(
            vec!["agent-primary".to_string(), "agent-fallback".to_string()],
            vec![vec!["model-1".to_string()], vec!["model-2".to_string()]],
            ralph_workflow::agents::AgentRole::Developer,
        );

        let state = PipelineState::initial(5, 3);
        let state = PipelineState {
            agent_chain,
            ..state
        };

        // When: Continuation budget is exhausted for the primary agent
        let new_state = reduce(
            state,
            PipelineEvent::development_continuation_budget_exhausted(
                0,
                3,
                DevelopmentStatus::Failed,
            ),
        );

        // Then: Pipeline should switch to fallback agent and stay in Development phase
        assert_eq!(
            new_state.agent_chain.current_agent_index, 1,
            "Should switch to fallback agent"
        );
        assert_eq!(
            new_state.phase,
            ralph_workflow::reducer::event::PipelinePhase::Development,
            "Should remain in Development phase to try fallback agent"
        );

        // And: The orchestration should clean up continuation context before preparing context
        let effect = determine_next_effect(&new_state);
        assert!(
            matches!(effect, Effect::CleanupContinuationContext),
            "Should clean up continuation context before preparing for new agent; got {:?}",
            effect
        );
    });
}

#[test]
fn test_all_agents_exhausted_reports_chain_exhaustion() {
    with_default_timeout(|| {
        // Given: A pipeline with 2 agents, max 1 retry cycle
        let agent_chain = AgentChainState::initial()
            .with_agents(
                vec!["agent-a".to_string(), "agent-b".to_string()],
                vec![vec!["model-1".to_string()], vec!["model-2".to_string()]],
                ralph_workflow::agents::AgentRole::Developer,
            )
            .with_max_cycles(1);

        let mut state = PipelineState::initial(5, 3);
        state.agent_chain = agent_chain;
        state.phase = ralph_workflow::reducer::event::PipelinePhase::Development;

        // When: Both agents exhaust their continuation budget
        state = reduce(
            state,
            PipelineEvent::development_continuation_budget_exhausted(
                0,
                3,
                DevelopmentStatus::Failed,
            ),
        );
        assert_eq!(state.agent_chain.current_agent_index, 1); // agent-b

        // Clean up context before next agent
        state = reduce(
            state,
            PipelineEvent::development_continuation_context_cleaned(),
        );

        state = reduce(
            state,
            PipelineEvent::development_continuation_budget_exhausted(
                0,
                3,
                DevelopmentStatus::Failed,
            ),
        );
        // Wraps to agent-a with retry_cycle=1
        assert_eq!(state.agent_chain.current_agent_index, 0);
        assert_eq!(state.agent_chain.retry_cycle, 1);

        // Clean up context again
        state = reduce(
            state,
            PipelineEvent::development_continuation_context_cleaned(),
        );

        // Then: Orchestration should detect agent chain exhaustion
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::ReportAgentChainExhausted { .. }),
            "Should report agent chain exhaustion when all agents tried and cycles exhausted; got {:?}",
            effect
        );
    });
}
