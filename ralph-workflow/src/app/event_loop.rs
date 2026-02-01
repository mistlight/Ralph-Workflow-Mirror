//! Event loop for reducer-based pipeline architecture.
//!
//! This module implements main event loop that coordinates reducer,
//! effect handlers, and orchestration logic. It provides a unified way to
//! run the pipeline using the event-sourced architecture from RFC-004.

use crate::phases::PhaseContext;
use crate::reducer::state::ContinuationState;
use crate::reducer::{
    determine_next_effect, reduce, EffectHandler, MainEffectHandler, PipelineState,
};
use anyhow::Result;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::Path;

/// Create initial pipeline state with continuation limits from config.
///
/// This function creates a `PipelineState` with XSD retry and continuation limits
/// loaded from the config, ensuring these values are available for the reducer
/// to make deterministic retry decisions.
fn create_initial_state_with_config(ctx: &PhaseContext<'_>) -> PipelineState {
    // Config semantics: max_dev_continuations counts continuation attempts *beyond*
    // the initial attempt. ContinuationState::max_continue_count semantics are
    // "maximum total attempts including initial".
    let max_dev_continuations = ctx.config.max_dev_continuations.unwrap_or(2);
    let max_continue_count = 1 + max_dev_continuations;

    let continuation = ContinuationState::with_limits(
        ctx.config.max_xsd_retries.unwrap_or(10),
        max_continue_count,
    );
    PipelineState::initial_with_continuation(
        ctx.config.developer_iters,
        ctx.config.reviewer_reviews,
        continuation,
    )
}

/// Maximum iterations for the main event loop to prevent infinite loops.
///
/// This is a safety limit - the pipeline should complete well before this limit
/// under normal circumstances. If reached, it indicates either a bug in the
/// reducer logic or an extremely complex project.
pub const MAX_EVENT_LOOP_ITERATIONS: usize = 1000;

/// Configuration for event loop.
#[derive(Clone, Debug)]
pub struct EventLoopConfig {
    /// Maximum number of iterations to prevent infinite loops.
    pub max_iterations: usize,
    /// Whether to enable checkpointing during the event loop.
    pub enable_checkpointing: bool,
}

/// Result of event loop execution.
#[derive(Debug, Clone)]
pub struct EventLoopResult {
    /// Whether pipeline completed successfully.
    pub completed: bool,
    /// Total events processed.
    pub events_processed: usize,
}

const EVENT_LOOP_TRACE_PATH: &str = ".agent/tmp/event_loop_trace.jsonl";
const DEFAULT_EVENT_LOOP_TRACE_CAPACITY: usize = 200;

#[derive(Clone, Serialize, Debug)]
struct EventTraceEntry {
    iteration: usize,
    effect: String,
    event: String,
    phase: String,
    xsd_retry_pending: bool,
    xsd_retry_count: u32,
    invalid_output_attempts: u32,
    agent_index: usize,
    model_index: usize,
    retry_cycle: u32,
}

#[derive(Debug)]
struct EventTraceBuffer {
    capacity: usize,
    entries: VecDeque<EventTraceEntry>,
}

impl EventTraceBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    fn push(&mut self, entry: EventTraceEntry) {
        self.entries.push_back(entry);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }

    fn entries(&self) -> &VecDeque<EventTraceEntry> {
        &self.entries
    }
}

#[derive(Serialize)]
struct EventTraceFinalState<'a> {
    kind: &'static str,
    reason: &'a str,
    state: &'a PipelineState,
}

fn build_trace_entry(
    iteration: usize,
    state: &PipelineState,
    effect: &str,
    event: &str,
) -> EventTraceEntry {
    EventTraceEntry {
        iteration,
        effect: effect.to_string(),
        event: event.to_string(),
        phase: format!("{:?}", state.phase),
        xsd_retry_pending: state.continuation.xsd_retry_pending,
        xsd_retry_count: state.continuation.xsd_retry_count,
        invalid_output_attempts: state.continuation.invalid_output_attempts,
        agent_index: state.agent_chain.current_agent_index,
        model_index: state.agent_chain.current_model_index,
        retry_cycle: state.agent_chain.retry_cycle,
    }
}

fn dump_event_loop_trace(
    ctx: &mut PhaseContext<'_>,
    trace: &EventTraceBuffer,
    final_state: &PipelineState,
    reason: &str,
) -> bool {
    let mut out = String::new();

    for entry in trace.entries() {
        match serde_json::to_string(entry) {
            Ok(line) => {
                out.push_str(&line);
                out.push('\n');
            }
            Err(err) => {
                ctx.logger.error(&format!(
                    "Failed to serialize event loop trace entry: {err}"
                ));
            }
        }
    }

    let final_line = match serde_json::to_string(&EventTraceFinalState {
        kind: "final_state",
        reason,
        state: final_state,
    }) {
        Ok(line) => line,
        Err(err) => {
            ctx.logger.error(&format!(
                "Failed to serialize event loop final state: {err}"
            ));
            // Ensure the file still contains something useful.
            format!(
                "{{\"kind\":\"final_state\",\"reason\":{},\"phase\":{}}}",
                serde_json::to_string(reason).unwrap_or_else(|_| "\"unknown\"".to_string()),
                serde_json::to_string(&format!("{:?}", final_state.phase))
                    .unwrap_or_else(|_| "\"unknown\"".to_string())
            )
        }
    };
    out.push_str(&final_line);
    out.push('\n');

    match ctx.workspace.write(Path::new(EVENT_LOOP_TRACE_PATH), &out) {
        Ok(()) => true,
        Err(err) => {
            ctx.logger
                .error(&format!("Failed to write event loop trace: {err}"));
            false
        }
    }
}

fn run_event_loop_with_handler_traced<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    initial_state: Option<PipelineState>,
    config: EventLoopConfig,
    handler: &mut H,
) -> Result<EventLoopResult>
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let mut state = initial_state.unwrap_or_else(|| create_initial_state_with_config(ctx));

    handler.update_state(state.clone());
    let mut events_processed = 0;
    let mut trace = EventTraceBuffer::new(DEFAULT_EVENT_LOOP_TRACE_CAPACITY);

    ctx.logger.info("Starting reducer-based event loop");

    while events_processed < config.max_iterations {
        let effect = determine_next_effect(&state);

        if state.is_complete() {
            break;
        }

        let effect_str = format!("{effect:?}");

        // Execute returns EffectResult with both PipelineEvent and UIEvents.
        // Catch panics here so we can still dump a best-effort trace.
        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler.execute(effect, ctx)
        })) {
            Ok(Ok(result)) => result,
            Ok(Err(err)) => return Err(err),
            Err(_) => {
                let dumped = dump_event_loop_trace(ctx, &trace, &state, "panic");
                if dumped {
                    ctx.logger.error(&format!(
                        "Event loop recovered from panic (trace: {EVENT_LOOP_TRACE_PATH})"
                    ));
                } else {
                    ctx.logger.error("Event loop recovered from panic");
                }

                return Ok(EventLoopResult {
                    completed: false,
                    events_processed,
                });
            }
        };

        // Display UI events (does not affect state)
        for ui_event in &result.ui_events {
            ctx.logger
                .info(&crate::rendering::render_ui_event(ui_event));
        }

        let event_str = format!("{:?}", result.event);

        // Apply pipeline event to state (reducer remains pure)
        let mut new_state = reduce(state, result.event.clone());
        trace.push(build_trace_entry(
            events_processed,
            &new_state,
            &effect_str,
            &event_str,
        ));
        handler.update_state(new_state.clone());
        state = new_state;
        events_processed += 1;

        // Apply additional pipeline events in order.
        for event in result.additional_events {
            let event_str = format!("{:?}", event);
            new_state = reduce(state, event);
            trace.push(build_trace_entry(
                events_processed,
                &new_state,
                &effect_str,
                &event_str,
            ));
            handler.update_state(new_state.clone());
            state = new_state;
            events_processed += 1;
        }
    }

    if events_processed >= config.max_iterations && !state.is_complete() {
        let dumped = dump_event_loop_trace(ctx, &trace, &state, "max_iterations");
        if dumped {
            ctx.logger.warn(&format!(
                "Event loop reached max iterations ({}) without completion (trace: {EVENT_LOOP_TRACE_PATH})",
                config.max_iterations
            ));
        } else {
            ctx.logger.warn(&format!(
                "Event loop reached max iterations ({}) without completion",
                config.max_iterations
            ));
        }
    }

    Ok(EventLoopResult {
        completed: state.is_complete(),
        events_processed,
    })
}

/// Run the main event loop for the reducer-based pipeline.
///
/// This function orchestrates pipeline execution by repeatedly:
/// 1. Determining the next effect based on the current state
/// 2. Executing the effect through the effect handler (which performs side effects)
/// 3. Applying the resulting event to state through the reducer (pure function)
/// 4. Repeating until a terminal state is reached or max iterations exceeded
///
/// The entire event loop is wrapped in panic recovery to ensure the pipeline
/// never crashes due to agent failures (panics only; aborts/segfaults cannot be recovered).
///
/// # Arguments
///
/// * `ctx` - Phase context for effect handlers
/// * `initial_state` - Optional initial state (if None, creates a new state)
/// * `config` - Event loop configuration
///
/// # Returns
///
/// Returns the event loop result containing the completion status and final state.
pub fn run_event_loop(
    ctx: &mut PhaseContext<'_>,
    initial_state: Option<PipelineState>,
    config: EventLoopConfig,
) -> Result<EventLoopResult> {
    let state = initial_state.unwrap_or_else(|| create_initial_state_with_config(ctx));
    let mut handler = MainEffectHandler::new(state.clone());
    run_event_loop_with_handler_traced(ctx, Some(state), config, &mut handler)
}

/// Run the event loop with a custom effect handler.
///
/// This variant allows injecting a custom effect handler for testing.
/// The handler must implement `EffectHandler` and `StatefulHandler` traits.
///
/// # Arguments
///
/// * `ctx` - Phase context for effect handlers
/// * `initial_state` - Optional initial state (if None, creates a new state)
/// * `config` - Event loop configuration
/// * `handler` - Custom effect handler (e.g., MockEffectHandler for testing)
///
/// # Returns
///
/// Returns the event loop result containing the completion status and final state.
pub fn run_event_loop_with_handler<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    initial_state: Option<PipelineState>,
    config: EventLoopConfig,
    handler: &mut H,
) -> Result<EventLoopResult>
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    run_event_loop_with_handler_traced(ctx, initial_state, config, handler)
}

/// Trait for handlers that maintain internal state.
///
/// This trait allows the event loop to update the handler's internal state
/// after each event is processed.
pub trait StatefulHandler {
    /// Update the handler's internal state.
    fn update_state(&mut self, state: PipelineState);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_loop_config_creation() {
        let config = EventLoopConfig {
            max_iterations: 1000,
            enable_checkpointing: true,
        };
        assert_eq!(config.max_iterations, 1000);
        assert!(config.enable_checkpointing);
    }

    #[test]
    fn test_create_initial_state_with_config_counts_total_attempts() {
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
        // Semantics: max_dev_continuations counts *continuations beyond initial*.
        // Total attempts should be 1 + max_dev_continuations.
        config.max_dev_continuations = Some(2);
        config.max_xsd_retries = Some(10);

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

        let state = create_initial_state_with_config(&ctx);

        assert_eq!(
            state.continuation.max_continue_count, 3,
            "max_continue_count should be total attempts (1 + max_dev_continuations)"
        );
    }

    /// Regression test: event loop must apply EffectResult.additional_events.
    ///
    /// Without this, AgentEvent::SessionEstablished is never reduced and same-session
    /// XSD retry cannot work.
    #[test]
    fn test_event_loop_applies_additional_events_in_order() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
        use crate::reducer::PipelineEvent;
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
                Ok(
                    EffectResult::event(PipelineEvent::prompt_permissions_restored())
                        .with_additional_event(PipelineEvent::agent_session_established(
                            crate::agents::AgentRole::Developer,
                            "test-agent".to_string(),
                            "session-123".to_string(),
                        )),
                )
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

        let state = PipelineState::initial(1, 0);
        let mut handler = TestHandler::new(state);
        let loop_config = EventLoopConfig {
            max_iterations: 10,
            enable_checkpointing: false,
        };

        let result = run_event_loop_with_handler(
            &mut ctx,
            Some(PipelineState::initial(1, 0)),
            loop_config,
            &mut handler,
        )
        .expect("event loop should run");

        assert!(
            result.completed,
            "pipeline should complete (PromptPermissionsRestored)"
        );
        assert_eq!(
            handler.state.agent_chain.last_session_id.as_deref(),
            Some("session-123"),
            "additional SessionEstablished event should be reduced and stored"
        );
    }

    /// Regression test: when checkpointing is disabled, the event loop must still
    /// execute the SaveCheckpoint effect via the handler.
    ///
    /// If the event loop short-circuits SaveCheckpoint into a synthetic CheckpointSaved
    /// event, the handler never runs and the pipeline can spin at a phase boundary.
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

        let loop_config = EventLoopConfig {
            max_iterations: 10,
            enable_checkpointing: false,
        };

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
            enable_checkpointing: false,
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

    #[test]
    fn test_event_loop_includes_review_when_reviewer_reviews_nonzero() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::effect::Effect;
        use crate::reducer::event::PipelinePhase;
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        let config = Config {
            developer_iters: 1,
            reviewer_reviews: 1,
            ..Config::default()
        };
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

        let state = super::create_initial_state_with_config(&ctx);
        let mut handler = MockEffectHandler::new(state.clone());
        let loop_config = EventLoopConfig {
            max_iterations: 500,
            enable_checkpointing: false,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
            .expect("event loop should run");
        assert!(result.completed, "expected pipeline to complete");
        assert_eq!(handler.state.phase, PipelinePhase::Complete);

        let effects = handler.captured_effects();
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::PrepareReviewContext { .. })),
            "expected review to run when reviewer_reviews>0"
        );
    }

    #[test]
    fn test_event_loop_skips_review_when_reviewer_reviews_zero_but_still_commits_dev_iteration() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::effect::Effect;
        use crate::reducer::event::PipelinePhase;
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        let config = Config {
            developer_iters: 1,
            reviewer_reviews: 0,
            ..Config::default()
        };
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

        let state = super::create_initial_state_with_config(&ctx);
        let mut handler = MockEffectHandler::new(state.clone());
        let loop_config = EventLoopConfig {
            max_iterations: 500,
            enable_checkpointing: false,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
            .expect("event loop should run");
        assert!(result.completed, "expected pipeline to complete");
        assert_eq!(handler.state.phase, PipelinePhase::Complete);

        let effects = handler.captured_effects();
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::PrepareDevelopmentContext { .. })),
            "expected development chain to run when developer_iters>0"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::CreateCommit { .. })),
            "expected commit to be created for dev iteration"
        );
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::PrepareReviewContext { .. })),
            "expected review to be skipped when reviewer_reviews=0"
        );
    }

    #[test]
    fn test_event_loop_effect_order_dev_then_commit_then_review_then_complete() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::effect::Effect;
        use crate::reducer::event::PipelinePhase;
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        let config = Config {
            developer_iters: 1,
            reviewer_reviews: 1,
            ..Config::default()
        };
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

        let state = super::create_initial_state_with_config(&ctx);
        let mut handler = MockEffectHandler::new(state.clone());
        let loop_config = EventLoopConfig {
            max_iterations: 500,
            enable_checkpointing: false,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
            .expect("event loop should run");
        assert!(result.completed, "expected pipeline to complete");
        assert_eq!(handler.state.phase, PipelinePhase::Complete);

        let effects = handler.captured_effects();

        fn idx(effects: &[Effect], pred: impl Fn(&Effect) -> bool) -> Option<usize> {
            effects.iter().position(pred)
        }

        let dev_idx = idx(&effects, |e| {
            matches!(e, Effect::ApplyDevelopmentOutcome { .. })
        })
        .expect("expected development outcome effect");
        let commit_idx = idx(&effects, |e| matches!(e, Effect::CreateCommit { .. }))
            .expect("expected commit creation effect");
        let review_ctx_idx = idx(&effects, |e| {
            matches!(e, Effect::PrepareReviewContext { .. })
        })
        .expect("expected review context preparation effect");
        let restore_idx = idx(&effects, |e| matches!(e, Effect::RestorePromptPermissions))
            .expect("expected restore prompt permissions effect");

        assert!(
            dev_idx < commit_idx,
            "expected development to occur before commit (dev_idx={dev_idx}, commit_idx={commit_idx})"
        );
        assert!(
            commit_idx < review_ctx_idx,
            "expected commit to occur before review (commit_idx={commit_idx}, review_ctx_idx={review_ctx_idx})"
        );
        assert!(
            review_ctx_idx < restore_idx,
            "expected review to occur before finalizing/complete (review_ctx_idx={review_ctx_idx}, restore_idx={restore_idx})"
        );
    }

    #[test]
    fn test_event_loop_skips_planning_and_development_when_developer_iters_zero() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::effect::Effect;
        use crate::reducer::event::PipelinePhase;
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        let config = Config {
            developer_iters: 0,
            reviewer_reviews: 1,
            ..Config::default()
        };
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

        let state = super::create_initial_state_with_config(&ctx);
        let mut handler = MockEffectHandler::new(state.clone());
        let loop_config = EventLoopConfig {
            max_iterations: 500,
            enable_checkpointing: false,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
            .expect("event loop should run");
        assert!(result.completed, "expected pipeline to complete");
        assert_eq!(handler.state.phase, PipelinePhase::Complete);

        let effects = handler.captured_effects();
        assert!(
            !effects.iter().any(|e| matches!(
                e,
                Effect::PreparePlanningPrompt { .. }
                    | Effect::InvokePlanningAgent { .. }
                    | Effect::ExtractPlanningXml { .. }
                    | Effect::ValidatePlanningXml { .. }
                    | Effect::WritePlanningMarkdown { .. }
                    | Effect::ArchivePlanningXml { .. }
                    | Effect::ApplyPlanningOutcome { .. }
                    | Effect::PrepareDevelopmentContext { .. }
                    | Effect::PrepareDevelopmentPrompt { .. }
                    | Effect::InvokeDevelopmentAgent { .. }
                    | Effect::ExtractDevelopmentXml { .. }
                    | Effect::ValidateDevelopmentXml { .. }
                    | Effect::ApplyDevelopmentOutcome { .. }
                    | Effect::ArchiveDevelopmentXml { .. }
            )),
            "expected no planning/development effects when developer_iters=0"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::PrepareReviewContext { .. })),
            "expected review effects when reviewer_reviews>0"
        );
    }

    #[test]
    fn test_event_loop_reviews_and_commits_when_developer_iters_zero_and_reviewer_reviews_nonzero()
    {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::effect::Effect;
        use crate::reducer::event::PipelinePhase;
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        let config = Config {
            developer_iters: 0,
            reviewer_reviews: 1,
            ..Config::default()
        };
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

        let state = super::create_initial_state_with_config(&ctx);
        let mut handler = MockEffectHandler::new(state.clone());
        let loop_config = EventLoopConfig {
            max_iterations: 500,
            enable_checkpointing: false,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
            .expect("event loop should run");
        assert!(result.completed, "expected pipeline to complete");
        assert_eq!(handler.state.phase, PipelinePhase::Complete);

        let effects = handler.captured_effects();
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::PrepareReviewContext { .. })),
            "expected review to run when reviewer_reviews>0"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::CreateCommit { .. })),
            "expected commit to occur after review"
        );
    }

    #[test]
    fn test_event_trace_buffer_keeps_last_n_entries() {
        fn entry(iteration: usize) -> EventTraceEntry {
            EventTraceEntry {
                iteration,
                effect: format!("Effect{iteration}"),
                event: format!("Event{iteration}"),
                phase: "Planning".to_string(),
                xsd_retry_pending: false,
                xsd_retry_count: 0,
                invalid_output_attempts: 0,
                agent_index: 0,
                model_index: 0,
                retry_cycle: 0,
            }
        }

        let mut buf = EventTraceBuffer::new(3);
        for i in 0..5 {
            buf.push(entry(i));
        }

        let iterations: Vec<usize> = buf.entries().iter().map(|e| e.iteration).collect();
        assert_eq!(iterations, vec![2, 3, 4]);
    }
}
