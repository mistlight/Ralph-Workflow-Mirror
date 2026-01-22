#![allow(dead_code)]
//! Event loop for reducer-based pipeline architecture.
//!
//! This module implements main event loop that coordinates reducer,
//! effect handlers, and orchestration logic. It provides a unified way to
//! run the pipeline using the event-sourced architecture from RFC-004.

use crate::phases::PhaseContext;
use crate::reducer::{
    determine_next_effect, reduce, CheckpointTrigger, EffectHandler, MainEffectHandler,
    PipelineEvent, PipelineState,
};
use anyhow::Result;

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
    /// Final pipeline state.
    pub final_state: PipelineState,
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
            let fallback_state = initial_state.unwrap_or_else(|| {
                PipelineState::initial(ctx.config.developer_iters, ctx.config.reviewer_reviews)
            });

            Ok(EventLoopResult {
                completed: false,
                events_processed: 0,
                final_state: fallback_state,
            })
        }
    }
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

        let event = handler.execute(effect, ctx)?;

        let new_state = reduce(state, event.clone());

        handler.state = new_state.clone();
        state = new_state;

        events_processed += 1;

        if config.enable_checkpointing {
            let checkpoint_event = PipelineEvent::CheckpointSaved {
                trigger: CheckpointTrigger::PhaseTransition,
            };
            state = reduce(state, checkpoint_event);
        }
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
        final_state: state,
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
}
