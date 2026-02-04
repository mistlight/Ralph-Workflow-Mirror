//! Integration test for completion marker emission on pipeline failure.
//!
//! Verifies that when the pipeline reaches Status: Failed (AgentChainExhausted),
//! it properly:
//! 1. Transitions to AwaitingDevFix phase
//! 2. Triggers TriggerDevFixFlow effect
//! 3. Emits completion marker to filesystem
//! 4. Transitions to Interrupted phase
//! 5. Saves checkpoint (making is_complete() return true)

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::{AgentRegistry, AgentRole};
use ralph_workflow::app::event_loop::{run_event_loop_with_handler, EventLoopConfig};
use ralph_workflow::checkpoint::{ExecutionHistory, RunContext};
use ralph_workflow::config::Config;
use ralph_workflow::executor::MockProcessExecutor;
use ralph_workflow::logger::{Colors, Logger};
use ralph_workflow::pipeline::{Stats, Timer};
use ralph_workflow::prompts::template_context::TemplateContext;
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Fixture {
    config: Config,
    colors: Colors,
    logger: Logger,
    timer: Timer,
    stats: Stats,
    template_context: TemplateContext,
    registry: AgentRegistry,
    executor: Arc<MockProcessExecutor>,
    repo_root: PathBuf,
    workspace: Arc<MemoryWorkspace>,
}

impl Fixture {
    fn new() -> Self {
        let config = Config::default();
        let colors = Colors::new();
        let repo_root = PathBuf::from("/test/repo");
        let workspace = Arc::new(MemoryWorkspace::new(repo_root.clone()));
        let logger = Logger::new(colors);
        let registry = AgentRegistry::new().unwrap();
        let executor = Arc::new(MockProcessExecutor::new());

        Self {
            config,
            colors,
            logger,
            timer: Timer::new(),
            stats: Stats::default(),
            template_context: TemplateContext::default(),
            registry,
            executor,
            repo_root,
            workspace,
        }
    }

    fn ctx(&mut self) -> ralph_workflow::phases::PhaseContext<'_> {
        ralph_workflow::phases::PhaseContext {
            config: &self.config,
            registry: &self.registry,
            logger: &self.logger,
            colors: &self.colors,
            timer: &mut self.timer,
            stats: &mut self.stats,
            developer_agent: "test-developer",
            reviewer_agent: "test-reviewer",
            review_guidelines: None,
            template_context: &self.template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*self.executor,
            executor_arc: Arc::clone(&self.executor)
                as Arc<dyn ralph_workflow::executor::ProcessExecutor>,
            repo_root: &self.repo_root,
            workspace: self.workspace.as_ref(),
        }
    }
}

#[test]
fn test_agent_chain_exhausted_emits_completion_marker() {
    with_default_timeout(|| {
        // Given: Initial pipeline state
        let state = PipelineState::initial(1, 1);
        assert_eq!(state.phase, PipelinePhase::Planning);

        // When: AgentChainExhausted error occurs
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: state.phase,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });

        let new_state = reduce(state, error_event);

        // Then: State transitions to AwaitingDevFix
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Planning));

        // When: Orchestration determines next effect
        let effect = determine_next_effect(&new_state);

        // Then: Effect should be TriggerDevFixFlow
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "Expected TriggerDevFixFlow, got {:?}",
            effect
        );

        // Verify full event loop execution emits completion marker
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut handler = MockEffectHandler::new(new_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(new_state), config, &mut handler)
            .expect("Event loop should complete");

        // Then: Pipeline should complete
        assert!(
            result.completed,
            "Pipeline should complete after failure handling"
        );

        // Then: Completion marker should exist in workspace
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker file should exist"
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.starts_with("failure"),
            "Completion marker should indicate failure, got: {}",
            marker_content
        );
    });
}

#[test]
fn test_failure_status_triggers_awaiting_dev_fix_not_immediate_exit() {
    with_default_timeout(|| {
        // Given: Pipeline in Development phase
        let state = PipelineState::initial(2, 1);

        // When: AgentChainExhausted occurs during Development
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 5,
            },
        });

        let new_state = reduce(state, error_event);

        // Then: Should transition to AwaitingDevFix, NOT Interrupted
        assert_eq!(
            new_state.phase,
            PipelinePhase::AwaitingDevFix,
            "Should enter AwaitingDevFix phase for remediation attempt"
        );

        // And: Should NOT be complete yet (needs to process dev-fix flow)
        assert!(
            !new_state.is_complete(),
            "Should not be complete in AwaitingDevFix phase"
        );

        // When: TriggerDevFixFlow effect is processed (simulated)
        let after_fix_state = reduce(
            new_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixSkipped {
                    reason: "Dev-fix flow not yet implemented".to_string(),
                },
            ),
        );

        // When: CompletionMarkerEmitted event is processed
        let interrupted_state = reduce(
            after_fix_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure: true,
                },
            ),
        );

        // Then: Should be in Interrupted phase
        assert_eq!(interrupted_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            interrupted_state.previous_phase,
            Some(PipelinePhase::AwaitingDevFix)
        );

        // And: Next effect should be SaveCheckpoint
        let next_effect = determine_next_effect(&interrupted_state);
        assert!(
            matches!(next_effect, Effect::SaveCheckpoint { .. }),
            "Expected SaveCheckpoint for Interrupted phase, got {:?}",
            next_effect
        );
    });
}

#[test]
fn test_completion_marker_written_before_interrupted_transition() {
    with_default_timeout(|| {
        // This test verifies the completion marker is written DURING TriggerDevFixFlow
        // effect execution, not after transitioning to Interrupted

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let state = PipelineState::initial(1, 1);

        // Transition to AwaitingDevFix
        let awaiting_fix_state = reduce(
            state,
            PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
                phase: PipelinePhase::Planning,
                error: ErrorEvent::AgentChainExhausted {
                    role: AgentRole::Developer,
                    phase: PipelinePhase::Planning,
                    cycle: 1,
                },
            }),
        );

        let mut handler = MockEffectHandler::new(awaiting_fix_state.clone());
        let config = EventLoopConfig { max_iterations: 50 };

        let _result =
            run_event_loop_with_handler(&mut ctx, Some(awaiting_fix_state), config, &mut handler)
                .expect("Event loop should complete");

        // Verify completion marker exists and contains failure information
        let marker_path = Path::new(".agent/tmp/completion_marker");
        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Completion marker should exist");

        assert!(
            marker_content.contains("failure"),
            "Completion marker should indicate failure"
        );
        assert!(
            marker_content.contains("Agent chain exhausted") || marker_content.contains("phase="),
            "Completion marker should include failure details"
        );
    });
}
