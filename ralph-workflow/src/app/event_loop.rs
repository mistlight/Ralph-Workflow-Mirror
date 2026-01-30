//! Event loop for reducer-based pipeline architecture.
//!
//! This module implements main event loop that coordinates reducer,
//! effect handlers, and orchestration logic. It provides a unified way to
//! run the pipeline using the event-sourced architecture from RFC-004.

use crate::phases::PhaseContext;
use crate::reducer::{
    determine_next_effect, reduce, EffectHandler, MainEffectHandler, PipelineState,
};
use anyhow::Result;

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

/// Run the main event loop for the reducer-based pipeline.
///
/// This function orchestrates pipeline execution by repeatedly:
/// 1. Determining the next effect based on the current state
/// 2. Executing the effect through the effect handler (which performs side effects)
/// 3. Applying the resulting event to state through the reducer (pure function)
/// 4. Repeating until a terminal state is reached or max iterations exceeded
///
/// The entire event loop is wrapped in panic recovery to ensure the pipeline
/// never crashes due to agent failures (including segmentation faults).
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
    let loop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_event_loop_internal(ctx, initial_state.clone(), config)
    }));

    match loop_result {
        Ok(result) => result,
        Err(_) => {
            ctx.logger.error("Event loop recovered from panic");
            let _fallback_state = initial_state.unwrap_or_else(|| {
                PipelineState::initial(ctx.config.developer_iters, ctx.config.reviewer_reviews)
            });

            Ok(EventLoopResult {
                completed: false,
                events_processed: 0,
            })
        }
    }
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
    let mut state = initial_state.unwrap_or_else(|| {
        PipelineState::initial(ctx.config.developer_iters, ctx.config.reviewer_reviews)
    });

    handler.update_state(state.clone());
    let mut events_processed = 0;

    ctx.logger.info("Starting reducer-based event loop");

    while !state.is_complete() && events_processed < config.max_iterations {
        let effect = determine_next_effect(&state);

        // Execute returns EffectResult with both PipelineEvent and UIEvents
        let result = handler.execute(effect, ctx)?;

        // Display UI events (does not affect state)
        for ui_event in &result.ui_events {
            ctx.logger
                .info(&crate::rendering::render_ui_event(ui_event));
        }

        // Apply pipeline event to state (reducer remains pure)
        let new_state = reduce(state, result.event.clone());

        handler.update_state(new_state.clone());
        state = new_state;

        events_processed += 1;
    }

    if events_processed >= config.max_iterations {
        ctx.logger.warn(&format!(
            "Event loop reached max iterations ({}) without completion",
            config.max_iterations
        ));
    }

    Ok(EventLoopResult {
        completed: state.is_complete(),
        events_processed,
    })
}

/// Trait for handlers that maintain internal state.
///
/// This trait allows the event loop to update the handler's internal state
/// after each event is processed.
pub trait StatefulHandler {
    /// Update the handler's internal state.
    fn update_state(&mut self, state: PipelineState);
}

fn run_event_loop_internal(
    ctx: &mut PhaseContext<'_>,
    initial_state: Option<PipelineState>,
    config: EventLoopConfig,
) -> Result<EventLoopResult> {
    let mut state = initial_state.unwrap_or_else(|| {
        PipelineState::initial(ctx.config.developer_iters, ctx.config.reviewer_reviews)
    });

    let mut handler = MainEffectHandler::new(state.clone());
    let mut events_processed = 0;

    ctx.logger.info("Starting reducer-based event loop");

    while !state.is_complete() && events_processed < config.max_iterations {
        let effect = determine_next_effect(&state);

        // Execute returns EffectResult with both PipelineEvent and UIEvents
        let result = handler.execute(effect, ctx)?;

        // Display UI events (does not affect state)
        for ui_event in &result.ui_events {
            ctx.logger
                .info(&crate::rendering::render_ui_event(ui_event));
        }

        // Apply pipeline event to state (reducer remains pure)
        let new_state = reduce(state, result.event.clone());

        handler.state = new_state.clone();
        state = new_state;

        events_processed += 1;
    }

    if events_processed >= config.max_iterations {
        ctx.logger.warn(&format!(
            "Event loop reached max iterations ({}) without completion",
            config.max_iterations
        ));
    }

    Ok(EventLoopResult {
        completed: state.is_complete(),
        events_processed,
    })
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
}
