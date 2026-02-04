//! Integration tests for development continuation budget exhaustion and agent fallback.
//!
//! These tests verify that when a development iteration's continuation budget is
//! exhausted (due to repeated failed/partial status), the pipeline switches to the
//! next agent in the fallback chain rather than immediately aborting.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
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

#[test]
fn test_agent_chain_exhausted_emits_completion_marker() {
    with_default_timeout(|| {
        // Given: A state where agent chain is exhausted
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Development;

        // Create an exhausted agent chain by setting retry_cycle to max_cycles
        let agent_chain = state
            .agent_chain
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec![]],
                ralph_workflow::agents::AgentRole::Developer,
            )
            .with_max_cycles(1);

        // Set retry_cycle to max_cycles to make it exhausted
        state.agent_chain = AgentChainState {
            retry_cycle: 1,
            ..agent_chain
        };

        // Verify it's exhausted
        assert!(
            state.agent_chain.is_exhausted(),
            "Agent chain should be exhausted"
        );

        // When: The orchestration determines the next effect
        let effect = determine_next_effect(&state);

        // Then: It should report agent chain exhaustion
        assert!(
            matches!(effect, Effect::ReportAgentChainExhausted { .. }),
            "Expected ReportAgentChainExhausted, got {:?}",
            effect
        );

        // When: The error event is reduced
        let error_event = ErrorEvent::AgentChainExhausted {
            role: ralph_workflow::agents::AgentRole::Developer,
            phase: PipelinePhase::Development,
            cycle: 1,
        };
        let new_state = reduce(
            state,
            PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
                phase: PipelinePhase::Development,
                error: error_event,
            }),
        );

        // Then: State transitions to Interrupted
        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
        assert!(
            !new_state.is_complete(),
            "Should not be complete yet (no checkpoint)"
        );

        // When: Orchestration determines next effect for Interrupted phase
        let next_effect = determine_next_effect(&new_state);

        // Then: It should save a checkpoint
        assert!(
            matches!(next_effect, Effect::SaveCheckpoint { .. }),
            "Expected SaveCheckpoint for Interrupted phase, got {:?}",
            next_effect
        );

        // When: Checkpoint is saved (simulate by applying CheckpointSaved event)
        let final_state = reduce(
            new_state,
            PipelineEvent::checkpoint_saved(ralph_workflow::reducer::CheckpointTrigger::Interrupt),
        );

        // Then: Pipeline is marked as complete
        assert!(
            final_state.is_complete(),
            "Pipeline should be complete after saving checkpoint in Interrupted phase"
        );
    });
}
