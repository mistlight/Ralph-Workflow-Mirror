//! Integration tests for development continuation budget exhaustion and agent fallback.
//!
//! These tests verify that when a development iteration's continuation budget is
//! exhausted (due to repeated failed/partial status), the pipeline switches to the
//! next agent in the fallback chain rather than immediately aborting.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRegistry;
use ralph_workflow::app::event_loop::{run_event_loop_with_handler, EventLoopConfig};
use ralph_workflow::checkpoint::{ExecutionHistory, RunContext};
use ralph_workflow::config::Config;
use ralph_workflow::executor::MockProcessExecutor;
use ralph_workflow::logger::{Colors, Logger};
use ralph_workflow::pipeline::{Stats, Timer};
use ralph_workflow::prompts::template_context::TemplateContext;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::{AgentChainState, DevelopmentStatus, PipelineState};
use ralph_workflow::reducer::state_reduction::reduce;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

        // Then: State transitions to AwaitingDevFix (not directly to Interrupted)
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Development));
        assert!(
            !new_state.is_complete(),
            "Should not be complete yet (still in AwaitingDevFix)"
        );

        // When: Orchestration determines next effect for AwaitingDevFix phase
        let next_effect = determine_next_effect(&new_state);

        // Then: It should trigger dev-fix flow
        assert!(
            matches!(next_effect, Effect::TriggerDevFixFlow { .. }),
            "Expected TriggerDevFixFlow for AwaitingDevFix phase, got {:?}",
            next_effect
        );

        // When: Dev-fix flow is skipped and completion marker is emitted
        let after_skip_state = reduce(
            new_state.clone(),
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixSkipped {
                    reason: "Dev-fix flow not yet implemented".to_string(),
                },
            ),
        );
        assert_eq!(after_skip_state.phase, PipelinePhase::AwaitingDevFix);

        let interrupted_state = reduce(
            after_skip_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure: true,
                },
            ),
        );

        // Then: State transitions to Interrupted after completion marker
        assert_eq!(interrupted_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            interrupted_state.previous_phase,
            Some(PipelinePhase::AwaitingDevFix)
        );

        // When: Orchestration determines next effect for Interrupted phase
        let final_effect = determine_next_effect(&interrupted_state);

        // Then: It should save a checkpoint
        assert!(
            matches!(final_effect, Effect::SaveCheckpoint { .. }),
            "Expected SaveCheckpoint for Interrupted phase, got {:?}",
            final_effect
        );

        // When: Checkpoint is saved (simulate by applying CheckpointSaved event)
        let final_state = reduce(
            interrupted_state,
            PipelineEvent::checkpoint_saved(ralph_workflow::reducer::CheckpointTrigger::Interrupt),
        );

        // Then: Pipeline is marked as complete
        assert!(
            final_state.is_complete(),
            "Pipeline should be complete after checkpoint saved in Interrupted phase"
        );
    });
}

#[test]
fn test_completion_marker_file_written_on_failure() {
    with_default_timeout(|| {
        // Given: A memory workspace and phase context
        let repo_root = PathBuf::from("/test/repo");
        let workspace = Arc::new(MemoryWorkspace::new(repo_root.clone()));

        // Create test dependencies
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let colors = Colors::new();
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();
        let executor = Arc::new(MockProcessExecutor::new());

        let mut phase_ctx = ralph_workflow::phases::PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "test-developer",
            reviewer_agent: "test-reviewer",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*executor,
            executor_arc: Arc::clone(&executor)
                as Arc<dyn ralph_workflow::executor::ProcessExecutor>,
            repo_root: &repo_root,
            workspace: workspace.as_ref(),
        };

        // Given: A state where agent chain will be exhausted immediately
        let mut initial_state = PipelineState::initial(1, 0);
        initial_state.phase = PipelinePhase::Development;

        // Create an exhausted agent chain
        let agent_chain = initial_state
            .agent_chain
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec![]],
                ralph_workflow::agents::AgentRole::Developer,
            )
            .with_max_cycles(1);

        initial_state.agent_chain = AgentChainState {
            retry_cycle: 1,
            ..agent_chain
        };

        // Verify it's exhausted
        assert!(initial_state.agent_chain.is_exhausted());

        // When: Run the event loop with mock handler
        let mut handler = MockEffectHandler::new(initial_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result =
            run_event_loop_with_handler(&mut phase_ctx, Some(initial_state), config, &mut handler)
                .expect("Event loop should complete");

        // Then: Pipeline should complete (reach Interrupted with checkpoint)
        assert!(result.completed, "Pipeline should complete");

        // Then: Completion marker file should be written
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            workspace.exists(marker_path),
            "Completion marker file should exist"
        );

        let marker_content = workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.starts_with("failure"),
            "Completion marker should indicate failure, got: {}",
            marker_content
        );
    });
}
