#[test]
fn test_event_loop_does_not_bypass_save_checkpoint_when_checkpointing_disabled() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Debug)]
    struct TestHandler {
        state: PipelineState,
    }

    impl TestHandler {
        fn new(state: PipelineState) -> Self {
            Self { state }
        }
    }

    impl<'ctx> EffectHandler<'ctx> for TestHandler {
        fn execute(
            &mut self,
            _effect: Effect,
            _ctx: &mut PhaseContext<'_>,
        ) -> Result<EffectResult> {
            // If SaveCheckpoint is executed through the handler, force completion.
            Ok(EffectResult::event(
                crate::reducer::PipelineEvent::prompt_permissions_restored(),
            ))
        }
    }

    impl super::StatefulHandler for TestHandler {
        fn update_state(&mut self, state: PipelineState) {
            self.state = state;
        }
    }

    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());

    let mut ctx = PhaseContext {
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
        executor_arc: Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: &repo_root,
        workspace: &workspace,
    };

    // Construct a boundary state that deterministically derives SaveCheckpoint.
    // Development with iteration >= total_iterations returns SaveCheckpoint.
    //
    // With checkpointing disabled, the event loop MUST still execute the effect via the
    // handler; bypassing it would spin on synthetic CheckpointSaved events.
    let state = PipelineState {
        phase: crate::reducer::event::PipelinePhase::Development,
        iteration: 1,
        total_iterations: 1,
        agent_chain: PipelineState::initial(1, 0).agent_chain.with_agents(
            vec!["test-agent".to_string()],
            vec![vec![]],
            crate::agents::AgentRole::Developer,
        ),
        ..PipelineState::initial(1, 0)
    };
    let mut handler = TestHandler::new(state);

    let loop_config = EventLoopConfig { max_iterations: 10 };

    let result = run_event_loop_with_handler(
        &mut ctx,
        Some(handler.state.clone()),
        loop_config,
        &mut handler,
    )
    .expect("event loop should run");

    assert!(
            result.completed,
            "expected pipeline to complete; SaveCheckpoint should not be bypassed when checkpointing is disabled"
        );
}

/// TDD test: run_event_loop_with_handler should accept a generic EffectHandler
/// allowing MockEffectHandler to be injected for testing.
#[cfg(feature = "test-utils")]
#[test]
fn test_run_event_loop_with_mock_handler() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::phases::PhaseContext;
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::mock_effect_handler::MockEffectHandler;
    use crate::reducer::PipelineState;
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    // Create test fixtures
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());

    // Create PhaseContext
    let mut ctx = PhaseContext {
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
        executor_arc: Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: &repo_root,
        workspace: &workspace,
    };

    // Create mock handler
    let state = PipelineState::initial(1, 0);
    let mut handler = MockEffectHandler::new(state.clone());

    let loop_config = EventLoopConfig {
        max_iterations: 100,
    };

    // This should compile and run with the mock handler
    let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler);

    assert!(result.is_ok(), "Event loop should complete successfully");

    // Mock handler should have captured effects
    assert!(
        handler.effect_count() > 0,
        "Mock handler should have captured at least one effect"
    );
}
