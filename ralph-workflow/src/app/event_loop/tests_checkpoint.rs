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

#[test]
fn test_event_loop_result_completed_true_for_interrupted_with_checkpoint() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
    use crate::reducer::event::PipelinePhase;
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Debug)]
    struct PanicHandler;

    impl<'ctx> EffectHandler<'ctx> for PanicHandler {
        fn execute(
            &mut self,
            _effect: Effect,
            _ctx: &mut PhaseContext<'_>,
        ) -> Result<EffectResult> {
            panic!("event loop should not execute effects when initial state is terminal");
        }
    }

    impl super::StatefulHandler for PanicHandler {
        fn update_state(&mut self, _state: PipelineState) {}
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

    let state = PipelineState {
        phase: PipelinePhase::Interrupted,
        checkpoint_saved_count: 1,
        ..PipelineState::initial(0, 0)
    };

    let mut handler = PanicHandler;
    let loop_config = EventLoopConfig { max_iterations: 10 };

    let result =
        run_event_loop_with_handler(&mut ctx, Some(state.clone()), loop_config, &mut handler)
            .expect("event loop should run");

    assert!(
        result.completed,
        "Interrupted+checkpoint is terminal and should be reported as completed (matches state.is_complete())"
    );
    assert_eq!(
        result.final_phase,
        PipelinePhase::Interrupted,
        "event loop should report the final phase"
    );
    assert!(
        state.is_complete(),
        "State.is_complete() should return true for Interrupted+checkpoint"
    );
    assert_eq!(result.events_processed, 0);
}

#[test]
fn test_event_loop_returns_incomplete_result_on_handler_panic() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
    use crate::workspace::MemoryWorkspace;
    use crate::workspace::Workspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Debug)]
    struct PanickingHandler;

    impl<'ctx> EffectHandler<'ctx> for PanickingHandler {
        fn execute(
            &mut self,
            _effect: Effect,
            _ctx: &mut PhaseContext<'_>,
        ) -> Result<EffectResult> {
            panic!("boom")
        }
    }

    impl super::StatefulHandler for PanickingHandler {
        fn update_state(&mut self, _state: PipelineState) {}
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

    let state = PipelineState::initial(0, 0);
    let mut handler = PanickingHandler;
    let loop_config = EventLoopConfig { max_iterations: 1 };

    let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
        .expect("event loop should return an EventLoopResult even on panic");
    assert!(!result.completed);
    assert!(workspace.exists(std::path::Path::new(super::EVENT_LOOP_TRACE_PATH)));
}

#[test]
fn test_max_iterations_in_awaiting_dev_fix_runs_save_checkpoint_effect() {
    use crate::agents::{AgentRegistry, AgentRole};
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
    use crate::reducer::event::{AwaitingDevFixEvent, PipelinePhase};
    use crate::reducer::PipelineEvent;
    use crate::workspace::{MemoryWorkspace, Workspace};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[derive(Debug)]
    struct StallingHandler {
        state: PipelineState,
        effects: Vec<Effect>,
    }

    impl StallingHandler {
        fn new(state: PipelineState) -> Self {
            Self {
                state,
                effects: Vec::new(),
            }
        }
    }

    impl<'ctx> EffectHandler<'ctx> for StallingHandler {
        fn execute(&mut self, effect: Effect, _ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
            self.effects.push(effect.clone());
            let event = match effect {
                Effect::SaveCheckpoint { trigger } => PipelineEvent::checkpoint_saved(trigger),
                _ => PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixTriggered {
                    failed_phase: PipelinePhase::Development,
                    failed_role: AgentRole::Developer,
                }),
            };
            Ok(EffectResult::event(event))
        }
    }

    impl super::StatefulHandler for StallingHandler {
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

    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::AwaitingDevFix;
    state.previous_phase = Some(PipelinePhase::Development);

    let mut handler = StallingHandler::new(state.clone());
    let loop_config = EventLoopConfig { max_iterations: 2 };

    let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
        .expect("event loop should run");

    assert!(
        result.completed,
        "expected forced completion when max iterations reached in AwaitingDevFix"
    );
    assert!(
        handler
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::SaveCheckpoint { .. })),
        "forced completion should execute SaveCheckpoint instead of mutating state directly"
    );
    assert!(
        workspace.exists(Path::new(".agent/tmp/completion_marker")),
        "completion marker should be written when max iterations are exceeded"
    );
}

#[test]
fn test_max_iterations_after_completion_marker_runs_save_checkpoint() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
    use crate::reducer::event::{AwaitingDevFixEvent, PipelinePhase};
    use crate::reducer::PipelineEvent;
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Debug)]
    struct BoundaryHandler {
        state: PipelineState,
        effects: Vec<Effect>,
    }

    impl BoundaryHandler {
        fn new(state: PipelineState) -> Self {
            Self {
                state,
                effects: Vec::new(),
            }
        }
    }

    impl<'ctx> EffectHandler<'ctx> for BoundaryHandler {
        fn execute(&mut self, effect: Effect, _ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
            self.effects.push(effect.clone());
            match effect {
                Effect::TriggerDevFixFlow {
                    failed_phase,
                    failed_role,
                    ..
                } => {
                    let mut result = EffectResult::event(PipelineEvent::AwaitingDevFix(
                        AwaitingDevFixEvent::DevFixTriggered {
                            failed_phase,
                            failed_role,
                        },
                    ));
                    result = result.with_additional_event(PipelineEvent::AwaitingDevFix(
                        AwaitingDevFixEvent::DevFixCompleted {
                            success: false,
                            summary: None,
                        },
                    ));
                    result = result.with_additional_event(PipelineEvent::AwaitingDevFix(
                        AwaitingDevFixEvent::CompletionMarkerEmitted { is_failure: true },
                    ));
                    Ok(result)
                }
                Effect::SaveCheckpoint { trigger } => Ok(EffectResult::event(
                    PipelineEvent::checkpoint_saved(trigger),
                )),
                _ => panic!("Unexpected effect: {effect:?}"),
            }
        }
    }

    impl super::StatefulHandler for BoundaryHandler {
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

    let state = PipelineState {
        phase: PipelinePhase::AwaitingDevFix,
        previous_phase: Some(PipelinePhase::Development),
        ..PipelineState::initial(1, 1)
    };

    let mut handler = BoundaryHandler::new(state.clone());
    let loop_config = EventLoopConfig { max_iterations: 3 };

    let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
        .expect("event loop should run");

    assert!(
        result.completed,
        "expected completion after AwaitingDevFix transitions to Interrupted"
    );
    assert!(
        handler
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::SaveCheckpoint { .. })),
        "SaveCheckpoint should run even when max iterations reached after completion marker"
    );
}

#[test]
fn test_create_initial_state_with_config_plumbs_max_same_agent_retry_count() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    let mut config = Config::default();
    config.max_same_agent_retries = Some(5);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());

    let ctx = PhaseContext {
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

    let state = super::create_initial_state_with_config(&ctx);
    assert_eq!(state.continuation.max_same_agent_retry_count, 5);
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
