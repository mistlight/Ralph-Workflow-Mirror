//! Event loop for reducer-based pipeline architecture.
//!
//! This module implements main event loop that coordinates reducer,
//! effect handlers, and orchestration logic. It provides a unified way to
//! run the pipeline using the event-sourced architecture from RFC-004.
//!
//! # Non-Terminating Pipeline Architecture
//!
//! The pipeline is designed to be **non-terminating by default** for unattended operation.
//! It must NEVER exit early due to internal failures, budget exhaustion, or agent errors.
//!
//! ## Failure Handling Flow
//!
//! 1. Any terminal failure (Status: Failed, budget exhausted, agent chain exhausted)
//!    transitions to `AwaitingDevFix` phase
//! 2. `TriggerDevFixFlow` effect writes completion marker to `.agent/tmp/completion_marker`
//! 3. Dev-fix agent is optionally dispatched for remediation attempt
//! 4. `CompletionMarkerEmitted` event transitions to `Interrupted` phase
//! 5. `SaveCheckpoint` effect saves state for resume
//! 6. Event loop returns `EventLoopResult { completed: true, ... }`
//!
//! ## Acceptable Termination Reasons
//!
//! The ONLY acceptable reasons for pipeline termination are catastrophic external events:
//! - Process termination (SIGKILL)
//! - Machine outage / power loss
//! - OS kill signal
//! - Unrecoverable panic in effect handler (caught and logged)
//!
//! All internal errors route through the failure handling flow above.

use crate::phases::PhaseContext;
use crate::reducer::effect::{Effect, EffectResult};
use crate::reducer::event::{AwaitingDevFixEvent, CheckpointTrigger, PipelineEvent, PipelinePhase};
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
    /// Final reducer phase when the loop stopped.
    pub final_phase: PipelinePhase,
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
/// 3. If downcast succeeds, wrap in `PipelineEvent::PromptInput(PromptInputEvent::HandlerError { ... })`
///    and process through reducer
/// 4. If downcast fails, return `Err()` to terminate the event loop (truly unrecoverable error)
///
/// This architecture allows the reducer to decide recovery strategy based on the specific
/// error type, rather than terminating immediately on any `Err()`.
fn extract_error_event(err: &anyhow::Error) -> Option<crate::reducer::event::ErrorEvent> {
    // Handlers are allowed to wrap typed ErrorEvents with additional context
    // (e.g. via `anyhow::Context`). Search the full error chain so we still
    // recover the underlying reducer error event.
    for cause in err.chain() {
        if let Some(error_event) = cause.downcast_ref::<crate::reducer::event::ErrorEvent>() {
            return Some(error_event.clone());
        }
    }
    None
}

enum GuardedEffectResult {
    Ok(Box<EffectResult>),
    Unrecoverable(anyhow::Error),
    Panic,
}

fn execute_effect_guarded<'ctx, H>(
    handler: &mut H,
    effect: Effect,
    ctx: &mut PhaseContext<'_>,
    state: &PipelineState,
) -> GuardedEffectResult
where
    H: EffectHandler<'ctx>,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handler.execute(effect, ctx)
    })) {
        Ok(Ok(result)) => GuardedEffectResult::Ok(Box::new(result)),
        Ok(Err(err)) => {
            if let Some(error_event) = extract_error_event(&err) {
                GuardedEffectResult::Ok(Box::new(crate::reducer::effect::EffectResult::event(
                    crate::reducer::event::PipelineEvent::PromptInput(
                        crate::reducer::event::PromptInputEvent::HandlerError {
                            phase: state.phase,
                            error: error_event,
                        },
                    ),
                )))
            } else {
                GuardedEffectResult::Unrecoverable(err)
            }
        }
        Err(_) => GuardedEffectResult::Panic,
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

fn write_completion_marker_on_error(ctx: &mut PhaseContext<'_>, err: &anyhow::Error) -> bool {
    if let Err(err) = ctx.workspace.create_dir_all(Path::new(".agent/tmp")) {
        ctx.logger.error(&format!(
            "Failed to create completion marker directory: {err}"
        ));
        return false;
    }

    let marker_path = Path::new(".agent/tmp/completion_marker");
    let content = format!("failure\nUnrecoverable handler error: {err}");
    match ctx.workspace.write(marker_path, &content) {
        Ok(()) => true,
        Err(err) => {
            ctx.logger.error(&format!(
                "Failed to write completion marker for unrecoverable handler error: {err}"
            ));
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
        // Check if we're already in a terminal state before executing any effect.
        // This handles the case where the initial state is already complete
        // (e.g., resuming from an Interrupted checkpoint).
        //
        // Special case: If we just transitioned to Interrupted from AwaitingDevFix
        // without a checkpoint, allow one more iteration to execute SaveCheckpoint.
        //
        // CRITICAL: If we're in AwaitingDevFix and haven't executed TriggerDevFixFlow yet,
        // allow at least one iteration to execute it, even if approaching max iterations.
        // This ensures completion marker is ALWAYS written and dev-fix agent is ALWAYS
        // dispatched before the event loop can exit.
        let should_allow_checkpoint_save = matches!(state.phase, PipelinePhase::Interrupted)
            && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
            && state.checkpoint_saved_count == 0;

        let is_awaiting_dev_fix_not_triggered =
            matches!(state.phase, PipelinePhase::AwaitingDevFix) && !state.dev_fix_triggered;

        if state.is_complete()
            && !should_allow_checkpoint_save
            && !is_awaiting_dev_fix_not_triggered
        {
            ctx.logger.info(&format!(
                "Event loop: state already complete (phase: {:?}, checkpoint_saved_count: {})",
                state.phase, state.checkpoint_saved_count
            ));
            break;
        }

        let effect = determine_next_effect(&state);
        let effect_str = format!("{effect:?}");

        // Execute returns EffectResult with both PipelineEvent and UIEvents.
        // Catch panics here so we can still dump a best-effort trace.
        let result = match execute_effect_guarded(handler, effect, ctx, &state) {
            GuardedEffectResult::Ok(result) => *result,
            GuardedEffectResult::Unrecoverable(err) => {
                // Non-terminating-by-default requirement:
                // Even "unrecoverable" handler errors must route through reducer-visible
                // remediation (AwaitingDevFix) so TriggerDevFixFlow can write the completion
                // marker and dispatch dev-fix.
                let dumped =
                    dump_event_loop_trace(ctx, &trace, &state, "unrecoverable_handler_error");
                let marker_written = write_completion_marker_on_error(ctx, &err);
                if dumped {
                    ctx.logger.error(&format!(
                        "Event loop encountered unrecoverable handler error (trace: {EVENT_LOOP_TRACE_PATH})"
                    ));
                } else {
                    ctx.logger
                        .error("Event loop encountered unrecoverable handler error");
                }
                if marker_written {
                    ctx.logger
                        .info("Completion marker written for unrecoverable handler error");
                }

                // Emit a reducer-visible error that transitions us into AwaitingDevFix.
                // We don't preserve the original error as a typed ErrorEvent; this is a last-resort
                // path to guarantee remediation and completion marker semantics.
                let failure_event = PipelineEvent::PromptInput(
                    crate::reducer::event::PromptInputEvent::HandlerError {
                        phase: state.phase,
                        error: crate::reducer::event::ErrorEvent::WorkspaceWriteFailed {
                            path: "(unrecoverable_handler_error)".to_string(),
                            kind: crate::reducer::event::WorkspaceIoErrorKind::Other,
                        },
                    },
                );

                let event_str = format!("{:?}", failure_event);
                let new_state = reduce(state, failure_event);
                trace.push(build_trace_entry(
                    events_processed,
                    &new_state,
                    &effect_str,
                    &event_str,
                ));
                handler.update_state(new_state.clone());
                state = new_state;
                events_processed += 1;

                continue;
            }
            GuardedEffectResult::Panic => {
                // Handler panics are internal failures and must not terminate the run.
                // Route through AwaitingDevFix so TriggerDevFixFlow writes the completion marker and
                // dispatches dev-fix.
                let dumped = dump_event_loop_trace(ctx, &trace, &state, "panic");
                if dumped {
                    ctx.logger.error(&format!(
                        "Event loop recovered from panic (trace: {EVENT_LOOP_TRACE_PATH})"
                    ));
                } else {
                    ctx.logger.error("Event loop recovered from panic");
                }

                // Best-effort completion marker for orchestration, even if the dev-fix flow fails.
                if let Err(err) = ctx.workspace.create_dir_all(Path::new(".agent/tmp")) {
                    ctx.logger.error(&format!(
                        "Failed to create completion marker directory: {err}"
                    ));
                }
                let marker_path = Path::new(".agent/tmp/completion_marker");
                let content = format!(
                    "failure\nHandler panic in effect execution (phase={:?}, events_processed={})",
                    state.phase, events_processed
                );
                if let Err(err) = ctx.workspace.write(marker_path, &content) {
                    ctx.logger.error(&format!(
                        "Failed to write completion marker for handler panic: {err}"
                    ));
                }

                let failure_event = PipelineEvent::PromptInput(
                    crate::reducer::event::PromptInputEvent::HandlerError {
                        phase: state.phase,
                        error: crate::reducer::event::ErrorEvent::WorkspaceWriteFailed {
                            path: "(handler_panic)".to_string(),
                            kind: crate::reducer::event::WorkspaceIoErrorKind::Other,
                        },
                    },
                );

                let event_str = format!("{:?}", failure_event);
                let new_state = reduce(state, failure_event);
                trace.push(build_trace_entry(
                    events_processed,
                    &new_state,
                    &effect_str,
                    &event_str,
                ));
                handler.update_state(new_state.clone());
                state = new_state;
                events_processed += 1;

                continue;
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

        // Check completion AFTER effect execution and state update.
        // This ensures that transitions to terminal phases (e.g., Interrupted)
        // have a chance to save their checkpoint before the loop exits.
        //
        // Special case: If we just transitioned to Interrupted from AwaitingDevFix
        // without a checkpoint, allow one more iteration to execute SaveCheckpoint.
        // This is needed because TriggerDevFixFlow already wrote the completion marker,
        // making is_complete() return true, but we still want to save the checkpoint
        // for proper state persistence.
        //
        // CRITICAL: If we're in AwaitingDevFix and haven't executed TriggerDevFixFlow yet,
        // allow at least one iteration to execute it, even if approaching max iterations.
        let should_allow_checkpoint_save = matches!(state.phase, PipelinePhase::Interrupted)
            && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
            && state.checkpoint_saved_count == 0;

        let is_awaiting_dev_fix_not_triggered =
            matches!(state.phase, PipelinePhase::AwaitingDevFix) && !state.dev_fix_triggered;

        if state.is_complete()
            && !should_allow_checkpoint_save
            && !is_awaiting_dev_fix_not_triggered
        {
            ctx.logger.info(&format!(
                "Event loop: state became complete (phase: {:?}, checkpoint_saved_count: {})",
                state.phase, state.checkpoint_saved_count
            ));

            // DEFENSIVE: If we're in Interrupted from AwaitingDevFix without a checkpoint,
            // warn that SaveCheckpoint should execute next
            if matches!(state.phase, PipelinePhase::Interrupted)
                && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
                && state.checkpoint_saved_count == 0
            {
                ctx.logger.warn(
                    "Interrupted phase reached from AwaitingDevFix without checkpoint saved. \
                     SaveCheckpoint effect should execute on next iteration.",
                );
            }

            break;
        }
    }

    // Track if we had to force-complete due to max iterations in AwaitingDevFix
    let mut forced_completion = false;

    let should_force_checkpoint_after_completion =
        matches!(state.phase, PipelinePhase::Interrupted)
            && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
            && state.checkpoint_saved_count == 0;

    if events_processed >= config.max_iterations && should_force_checkpoint_after_completion {
        ctx.logger.warn(
            "Max iterations reached after completion marker; forcing SaveCheckpoint execution",
        );

        let save_effect = Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt,
        };
        let save_effect_str = format!("{save_effect:?}");
        match execute_effect_guarded(handler, save_effect, ctx, &state) {
            GuardedEffectResult::Ok(result) => {
                let result = *result;
                let event_str = format!("{:?}", result.event);
                state = reduce(state, result.event.clone());
                trace.push(build_trace_entry(
                    events_processed,
                    &state,
                    &save_effect_str,
                    &event_str,
                ));
                handler.update_state(state.clone());
                events_processed += 1;

                for event in result.additional_events {
                    let event_str = format!("{:?}", event);
                    state = reduce(state, event);
                    trace.push(build_trace_entry(
                        events_processed,
                        &state,
                        &save_effect_str,
                        &event_str,
                    ));
                    handler.update_state(state.clone());
                    events_processed += 1;
                }
            }
            GuardedEffectResult::Unrecoverable(err) => {
                // Non-terminating-by-default: even failures while forcing checkpoint completion
                // must route through AwaitingDevFix rather than returning Err.
                let dumped =
                    dump_event_loop_trace(ctx, &trace, &state, "unrecoverable_handler_error");
                let marker_written = write_completion_marker_on_error(ctx, &err);
                if dumped {
                    ctx.logger.error(&format!(
                        "Event loop encountered unrecoverable handler error (trace: {EVENT_LOOP_TRACE_PATH})"
                    ));
                } else {
                    ctx.logger
                        .error("Event loop encountered unrecoverable handler error");
                }
                if marker_written {
                    ctx.logger
                        .info("Completion marker written for unrecoverable handler error");
                }

                // We can't safely continue execution here (we are outside the main loop).
                // State is already terminal (Interrupted from AwaitingDevFix), so return completion
                // even if SaveCheckpoint fails.
                return Ok(EventLoopResult {
                    completed: true,
                    events_processed,
                    final_phase: state.phase,
                });
            }
            GuardedEffectResult::Panic => {
                // Panics during forced completion are internal failures; route through AwaitingDevFix.
                let dumped = dump_event_loop_trace(ctx, &trace, &state, "panic");
                if dumped {
                    ctx.logger.error(&format!(
                        "Event loop recovered from panic (trace: {EVENT_LOOP_TRACE_PATH})"
                    ));
                } else {
                    ctx.logger.error("Event loop recovered from panic");
                }

                if let Err(err) = ctx.workspace.create_dir_all(Path::new(".agent/tmp")) {
                    ctx.logger.error(&format!(
                        "Failed to create completion marker directory: {err}"
                    ));
                }
                let marker_path = Path::new(".agent/tmp/completion_marker");
                let content = format!(
                    "failure\nHandler panic during forced completion (phase={:?}, events_processed={})",
                    state.phase, events_processed
                );
                if let Err(err) = ctx.workspace.write(marker_path, &content) {
                    ctx.logger.error(&format!(
                        "Failed to write completion marker for handler panic: {err}"
                    ));
                }

                // We can't safely continue execution here (we are outside the main loop).
                // State is already terminal (Interrupted from AwaitingDevFix), so return completion
                // even if SaveCheckpoint fails.
                return Ok(EventLoopResult {
                    completed: true,
                    events_processed,
                    final_phase: state.phase,
                });
            }
        }
    }

    if events_processed >= config.max_iterations && !state.is_complete() {
        let dumped = dump_event_loop_trace(ctx, &trace, &state, "max_iterations");

        // DEFENSIVE: If max iterations reached in AwaitingDevFix without dev_fix_triggered,
        // this is a bug (TriggerDevFixFlow should execute first). However, to maintain the
        // non-terminating pipeline principle, we force completion:
        // 1. Write completion marker (signals orchestration)
        // 2. Emit CompletionMarkerEmitted event (transitions to Interrupted)
        // 3. Execute SaveCheckpoint (makes is_complete() return true)
        //
        // This ensures the pipeline NEVER exits early due to internal logic bugs.
        // Budget exhaustion should transition to commit/finalization, not terminate.
        if matches!(state.phase, PipelinePhase::AwaitingDevFix) {
            ctx.logger.error(
                "BUG: Hit max iterations in AwaitingDevFix phase. \
                 TriggerDevFixFlow should have executed before reaching this point. \
                 Applying defensive recovery logic.",
            );
            ctx.logger
                .warn("Max iterations reached in AwaitingDevFix - forcing completion marker");

            // Write completion marker
            if let Err(err) = ctx.workspace.create_dir_all(Path::new(".agent/tmp")) {
                ctx.logger.error(&format!(
                    "Failed to create completion marker directory: {err}"
                ));
            }
            let marker_path = Path::new(".agent/tmp/completion_marker");
            let content = format!(
                "failure\nMax iterations reached in AwaitingDevFix phase (events_processed={})",
                events_processed
            );
            match ctx.workspace.write(marker_path, &content) {
                Ok(()) => {
                    ctx.logger
                        .info("Completion marker written for max iterations failure");
                }
                Err(err) => {
                    ctx.logger.error(&format!(
                        "Failed to write completion marker for max iterations failure: {err}"
                    ));
                }
            }

            let completion_event =
                PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure: true,
                });
            let completion_event_str = format!("{:?}", completion_event);
            state = reduce(state, completion_event);
            trace.push(build_trace_entry(
                events_processed,
                &state,
                "ForcedCompletionMarker",
                &completion_event_str,
            ));
            handler.update_state(state.clone());
            events_processed += 1;

            let save_effect = Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::Interrupt,
            };
            let save_effect_str = format!("{save_effect:?}");
            match execute_effect_guarded(handler, save_effect, ctx, &state) {
                GuardedEffectResult::Ok(result) => {
                    let result = *result;
                    let event_str = format!("{:?}", result.event);
                    state = reduce(state, result.event.clone());
                    trace.push(build_trace_entry(
                        events_processed,
                        &state,
                        &save_effect_str,
                        &event_str,
                    ));
                    handler.update_state(state.clone());
                    events_processed += 1;

                    for event in result.additional_events {
                        let event_str = format!("{:?}", event);
                        state = reduce(state, event);
                        trace.push(build_trace_entry(
                            events_processed,
                            &state,
                            &save_effect_str,
                            &event_str,
                        ));
                        handler.update_state(state.clone());
                        events_processed += 1;
                    }
                }
                GuardedEffectResult::Unrecoverable(err) => {
                    // Non-terminating-by-default: even errors during forced completion must route
                    // through AwaitingDevFix instead of returning Err.
                    let dumped =
                        dump_event_loop_trace(ctx, &trace, &state, "unrecoverable_handler_error");
                    let marker_written = write_completion_marker_on_error(ctx, &err);
                    if dumped {
                        ctx.logger.error(&format!(
                            "Event loop encountered unrecoverable handler error (trace: {EVENT_LOOP_TRACE_PATH})"
                        ));
                    } else {
                        ctx.logger
                            .error("Event loop encountered unrecoverable handler error");
                    }
                    if marker_written {
                        ctx.logger
                            .info("Completion marker written for unrecoverable handler error");
                    }

                    // We can't safely continue execution here (we are outside the main loop).
                    // State is already terminal (Interrupted from AwaitingDevFix).
                    // However, the run did NOT complete cleanly, so report incomplete while still
                    // having written a best-effort completion marker above.
                    return Ok(EventLoopResult {
                        completed: false,
                        events_processed,
                        final_phase: state.phase,
                    });
                }
                GuardedEffectResult::Panic => {
                    // Panics during forced completion are internal failures; route through AwaitingDevFix.
                    let dumped = dump_event_loop_trace(ctx, &trace, &state, "panic");
                    if dumped {
                        ctx.logger.error(&format!(
                            "Event loop recovered from panic (trace: {EVENT_LOOP_TRACE_PATH})"
                        ));
                    } else {
                        ctx.logger.error("Event loop recovered from panic");
                    }

                    if let Err(err) = ctx.workspace.create_dir_all(Path::new(".agent/tmp")) {
                        ctx.logger.error(&format!(
                            "Failed to create completion marker directory: {err}"
                        ));
                    }
                    let marker_path = Path::new(".agent/tmp/completion_marker");
                    let content = format!(
                        "failure\nHandler panic during forced completion (phase={:?}, events_processed={})",
                        state.phase, events_processed
                    );
                    if let Err(err) = ctx.workspace.write(marker_path, &content) {
                        ctx.logger.error(&format!(
                            "Failed to write completion marker for handler panic: {err}"
                        ));
                    }

                    // We can't safely continue execution here (we are outside the main loop).
                    // State is already terminal (Interrupted from AwaitingDevFix).
                    // However, the run did NOT complete cleanly, so report incomplete while still
                    // having written a best-effort completion marker above.
                    return Ok(EventLoopResult {
                        completed: false,
                        events_processed,
                        final_phase: state.phase,
                    });
                }
            }

            forced_completion = true;

            ctx.logger
                .info("Forced transition to Interrupted phase to satisfy termination requirements");
        }

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

        if !forced_completion {
            ctx.logger.error(&format!(
                "Event loop exiting: reason=max_iterations, phase={:?}, checkpoint_saved_count={}, events_processed={}",
                state.phase, state.checkpoint_saved_count, events_processed
            ));
        }
    }

    let completed = state.is_complete();
    if !completed {
        ctx.logger.warn(&format!(
            "Event loop exiting without completion: phase={:?}, checkpoint_saved_count={}, \
             previous_phase={:?}, events_processed={}",
            state.phase, state.checkpoint_saved_count, state.previous_phase, events_processed
        ));
        ctx.logger.info(&format!(
            "Final state: agent_chain.retry_cycle={}, agent_chain.current_role={:?}",
            state.agent_chain.retry_cycle, state.agent_chain.current_role
        ));
    }

    Ok(EventLoopResult {
        completed,
        events_processed,
        final_phase: state.phase,
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
