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
        ctx.config.max_same_agent_retries.unwrap_or(2),
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

/// Extract ErrorEvent from anyhow::Error if present.
///
/// # Error Event Processing Architecture
///
/// Effect handlers return errors through `Err(ErrorEvent::Variant.into())`. This function
/// extracts the original `ErrorEvent` so it can be processed through the reducer.
///
/// ## Why Downcast?
///
/// When an effect handler returns `Err(ErrorEvent::Variant.into())`, the error is wrapped
/// in an `anyhow::Error`. Since `ErrorEvent` implements `std::error::Error`, anyhow's
/// blanket `From` implementation preserves the original error type, allowing us to downcast
/// back to `ErrorEvent` for reducer processing.
///
/// ## Processing Flow
///
/// 1. Handler returns `Err(ErrorEvent::AgentChainExhausted { ... }.into())`
/// 2. Event loop catches the error and calls this function
/// 3. If downcast succeeds, wrap in `PipelineEvent::Error()` and process through reducer
/// 4. If downcast fails, return `Err()` to terminate the event loop (truly unrecoverable error)
///
/// This architecture allows the reducer to decide recovery strategy based on the specific
/// error type, rather than terminating immediately on any `Err()`.
fn extract_error_event(err: &anyhow::Error) -> Option<crate::reducer::event::ErrorEvent> {
    // Try to downcast to ErrorEvent
    err.downcast_ref::<crate::reducer::event::ErrorEvent>()
        .cloned()
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

    // Ensure the trace directory exists. While `Workspace::write` is expected to
    // create parent directories, we do this explicitly so trace dumping is
    // resilient even under stricter workspace implementations.
    if let Err(err) = ctx.workspace.create_dir_all(Path::new(".agent/tmp")) {
        ctx.logger
            .error(&format!("Failed to create trace directory: {err}"));
        return false;
    }

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
            Ok(Err(err)) => {
                // Check if this is an error event that should be processed through the reducer
                if let Some(error_event) = extract_error_event(&err) {
                    // Process error event through reducer like a normal event
                    ctx.logger.warn(&format!("Error event: {error_event:?}"));
                    crate::reducer::effect::EffectResult::event(
                        crate::reducer::event::PipelineEvent::Error(error_event),
                    )
                } else {
                    // Truly unrecoverable error - cannot continue
                    return Err(err);
                }
            }
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

    include!("event_loop/tests_trace_dump.rs");
    include!("event_loop/tests_checkpoint.rs");
    include!("event_loop/tests_review_flow.rs");
}
