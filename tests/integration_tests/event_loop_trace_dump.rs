//! Event loop trace dump behavior.
//!
//! These tests verify that when the reducer event loop is exhausted (max iterations)
//! or recovers from a panic, it persists an execution trace to `.agent/tmp/`.

use crate::test_timeout::with_default_timeout;

use anyhow::Result;
use ralph_workflow::agents::AgentRegistry;
use ralph_workflow::app::event_loop::{
    run_event_loop_with_handler, EventLoopConfig, StatefulHandler,
};
use ralph_workflow::checkpoint::{ExecutionHistory, RunContext};
use ralph_workflow::config::Config;
use ralph_workflow::executor::MockProcessExecutor;
use ralph_workflow::logger::{Colors, Logger};
use ralph_workflow::pipeline::Timer;
use ralph_workflow::prompts::template_context::TemplateContext;
use ralph_workflow::reducer::effect::{Effect, EffectHandler, EffectResult};
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::{PipelineEvent, PipelineState};
use ralph_workflow::workspace::MemoryWorkspace;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TRACE_PATH: &str = ".agent/tmp/event_loop_trace.jsonl";
const LOG_PATH: &str = ".agent/tmp/event_loop_trace_test.log";

struct Fixture {
    config: Config,
    colors: Colors,
    logger: Logger,
    timer: Timer,
    
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
            
            template_context: TemplateContext::default(),
            registry,
            executor,
            repo_root,
            workspace,
        }
    }

    fn new_with_workspace_log() -> Self {
        let config = Config::default();
        let colors = Colors::new();
        let repo_root = PathBuf::from("/test/repo");
        let workspace = Arc::new(MemoryWorkspace::new(repo_root.clone()));
        let logger = Logger::new(colors).with_workspace_log(
            Arc::clone(&workspace) as Arc<dyn ralph_workflow::workspace::Workspace>,
            LOG_PATH,
        );
        let registry = AgentRegistry::new().unwrap();
        let executor = Arc::new(MockProcessExecutor::new());

        Self {
            config,
            colors,
            logger,
            timer: Timer::new(),
            
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
            repo_root: Path::new(&self.repo_root),
            workspace: self.workspace.as_ref(),
        }
    }
}

#[derive(Debug)]
struct LoopingHandler {
    state: PipelineState,
}

impl<'ctx> EffectHandler<'ctx> for LoopingHandler {
    fn execute(
        &mut self,
        _effect: Effect,
        _ctx: &mut ralph_workflow::phases::PhaseContext<'_>,
    ) -> Result<EffectResult> {
        // Return a reducer-visible event that does not complete the pipeline.
        Ok(EffectResult::event(PipelineEvent::ContextCleaned))
    }
}

impl StatefulHandler for LoopingHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}

#[derive(Debug)]
struct PanicHandler {
    state: PipelineState,
}

impl<'ctx> EffectHandler<'ctx> for PanicHandler {
    fn execute(
        &mut self,
        _effect: Effect,
        _ctx: &mut ralph_workflow::phases::PhaseContext<'_>,
    ) -> Result<EffectResult> {
        panic!("handler panic");
    }
}

impl StatefulHandler for PanicHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}

#[derive(Debug)]
struct AdditionalEventsHandler {
    state: PipelineState,
}

impl<'ctx> EffectHandler<'ctx> for AdditionalEventsHandler {
    fn execute(
        &mut self,
        _effect: Effect,
        _ctx: &mut ralph_workflow::phases::PhaseContext<'_>,
    ) -> Result<EffectResult> {
        Ok(
            EffectResult::event(PipelineEvent::ContextCleaned).with_additional_event(
                PipelineEvent::CheckpointSaved {
                    trigger: ralph_workflow::reducer::event::CheckpointTrigger::PhaseTransition,
                },
            ),
        )
    }
}

impl StatefulHandler for AdditionalEventsHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}

#[derive(Debug)]
struct PhaseChangingHandler {
    state: PipelineState,
}

impl<'ctx> EffectHandler<'ctx> for PhaseChangingHandler {
    fn execute(
        &mut self,
        _effect: Effect,
        _ctx: &mut ralph_workflow::phases::PhaseContext<'_>,
    ) -> Result<EffectResult> {
        Ok(EffectResult::event(PipelineEvent::FinalizingStarted))
    }
}

impl StatefulHandler for PhaseChangingHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}

#[test]
fn test_event_loop_dumps_trace_on_max_iterations() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Force an infinite loop by making the handler return a no-progress event.
        let mut initial_state = PipelineState::initial(1, 0);
        initial_state.phase = PipelinePhase::Planning;
        initial_state.context_cleaned = true;

        let mut handler = LoopingHandler {
            state: initial_state.clone(),
        };

        let loop_config = EventLoopConfig { max_iterations: 3 };

        let res =
            run_event_loop_with_handler(&mut ctx, Some(initial_state), loop_config, &mut handler)
                .expect("event loop should run");
        assert!(
            !res.completed,
            "expected pipeline to not complete in loop scenario"
        );

        assert!(
            fixture.workspace.was_written(TRACE_PATH),
            "expected event loop to dump trace to {TRACE_PATH}"
        );

        let trace = fixture
            .workspace
            .get_file(TRACE_PATH)
            .expect("trace file should be readable");
        let line_count = trace.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(
            line_count >= 3,
            "expected at least 3 trace entries, got {line_count}"
        );
    });
}

#[test]
fn test_event_loop_dumps_trace_on_panic() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut initial_state = PipelineState::initial(1, 0);
        initial_state.phase = PipelinePhase::Planning;

        let mut handler = PanicHandler {
            state: initial_state.clone(),
        };

        let loop_config = EventLoopConfig { max_iterations: 10 };

        let res =
            run_event_loop_with_handler(&mut ctx, Some(initial_state), loop_config, &mut handler)
                .expect("event loop should return an EventLoopResult even on panic");

        assert!(
            !res.completed,
            "expected pipeline to be marked incomplete on panic"
        );
        assert!(
            fixture.workspace.was_written(TRACE_PATH),
            "expected event loop to dump trace to {TRACE_PATH} on panic"
        );

        let trace = fixture
            .workspace
            .get_file(TRACE_PATH)
            .expect("trace file should be readable");
        let last_line = trace
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .expect("trace file should have at least one line");
        assert!(last_line.contains("\"kind\":\"final_state\""));
        assert!(last_line.contains("\"reason\":\"panic\""));
    });
}

#[test]
fn test_trace_records_additional_events() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut initial_state = PipelineState::initial(1, 0);
        initial_state.phase = PipelinePhase::Planning;

        let mut handler = AdditionalEventsHandler {
            state: initial_state.clone(),
        };

        let loop_config = EventLoopConfig { max_iterations: 2 };

        let res =
            run_event_loop_with_handler(&mut ctx, Some(initial_state), loop_config, &mut handler)
                .expect("event loop should run");
        assert!(!res.completed);

        let trace = fixture
            .workspace
            .get_file(TRACE_PATH)
            .expect("trace file should be readable");
        assert!(
            trace.contains("\"event\":\"CheckpointSaved"),
            "expected trace to include additional CheckpointSaved event"
        );
        let last_line = trace
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .expect("trace file should have at least one line");
        assert!(last_line.contains("\"kind\":\"final_state\""));
        assert!(last_line.contains("\"reason\":\"max_iterations\""));
    });
}

#[test]
fn test_trace_entry_phase_reflects_state_after_event_applied() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut initial_state = PipelineState::initial(1, 0);
        initial_state.phase = PipelinePhase::Planning;

        let mut handler = PhaseChangingHandler {
            state: initial_state.clone(),
        };

        let loop_config = EventLoopConfig { max_iterations: 1 };

        let _res =
            run_event_loop_with_handler(&mut ctx, Some(initial_state), loop_config, &mut handler)
                .expect("event loop should run");

        let trace = fixture
            .workspace
            .get_file(TRACE_PATH)
            .expect("trace file should be readable");
        let first_line = trace
            .lines()
            .find(|l| !l.trim().is_empty())
            .expect("trace file should have at least one entry line");

        assert!(first_line.contains("\"event\":\"FinalizingStarted\""));
        assert!(
            first_line.contains("\"phase\":\"Finalizing\""),
            "expected trace phase to reflect state after applying FinalizingStarted"
        );
    });
}

#[test]
fn test_max_iterations_logs_trace_path_to_workspace_log() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new_with_workspace_log();
        let mut ctx = fixture.ctx();

        let mut initial_state = PipelineState::initial(1, 0);
        initial_state.phase = PipelinePhase::Planning;
        initial_state.context_cleaned = true;

        let mut handler = LoopingHandler {
            state: initial_state.clone(),
        };

        let loop_config = EventLoopConfig { max_iterations: 1 };

        let _res =
            run_event_loop_with_handler(&mut ctx, Some(initial_state), loop_config, &mut handler)
                .expect("event loop should run");

        let log = fixture
            .workspace
            .get_file(LOG_PATH)
            .expect("workspace log should be written");
        assert!(
            log.contains(TRACE_PATH),
            "expected logs to mention trace path {TRACE_PATH}"
        );
    });
}
