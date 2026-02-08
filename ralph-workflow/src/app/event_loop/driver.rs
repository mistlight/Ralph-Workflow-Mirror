//! Main event loop driver implementing the orchestrate-handle-reduce cycle.
//!
//! This module contains the core iteration logic that repeatedly:
//! 1. Determines the next effect from current state (orchestration)
//! 2. Executes the effect through the handler (side effects)
//! 3. Applies the resulting event through the reducer (pure state transition)
//! 4. Repeats until terminal state or max iterations
//!
//! ## Event Loop Architecture
//!
//! ```text
//! State → determine_next_effect → Effect → execute → Event → reduce → Next State
//!         (pure, from orchestrator)       (impure)          (pure)
//! ```
//!
//! The loop maintains strict separation between pure reducer logic and impure
//! effect handlers, with all state transitions driven by events.

use crate::logging::EventLoopLogger;
use crate::phases::PhaseContext;
use crate::reducer::event::{PipelineEvent, PipelinePhase};
use crate::reducer::{determine_next_effect, reduce, EffectHandler, PipelineState};
use anyhow::Result;
use std::path::Path;
use std::time::Instant;

use super::config::{create_initial_state_with_config, EventLoopConfig, EventLoopResult};
use super::error_handling::{
    execute_effect_guarded, write_completion_marker_on_error, GuardedEffectResult,
};
use super::recovery::{
    handle_forced_checkpoint_after_completion, handle_max_iterations_in_awaiting_dev_fix,
    RecoveryResult,
};
use super::trace::{
    build_trace_entry, dump_event_loop_trace, EventTraceBuffer, DEFAULT_EVENT_LOOP_TRACE_CAPACITY,
};
use super::StatefulHandler;

/// Run the main event loop with the given handler and configuration.
///
/// This function implements the reducer-based event loop cycle, orchestrating
/// pure state transitions with impure effect execution while maintaining panic
/// recovery and defensive completion guarantees.
///
/// # Arguments
///
/// * `ctx` - Phase context for effect handlers
/// * `initial_state` - Optional initial state (if None, creates a new state)
/// * `config` - Event loop configuration
/// * `handler` - Effect handler implementing side effects
///
/// # Returns
///
/// Returns the event loop result containing completion status and final state.
pub(super) fn run_event_loop_driver<'ctx, H>(
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

    // Create event loop logger, continuing from existing log if present (resume case)
    let event_loop_log_path = ctx.run_log_context.event_loop_log();
    let mut event_loop_logger =
        match EventLoopLogger::from_existing_log(ctx.workspace, &event_loop_log_path) {
            Ok(logger) => logger,
            Err(e) => {
                // If reading existing log fails, log a warning and start fresh
                ctx.logger.warn(&format!(
                    "Failed to read existing event loop log, starting fresh: {}",
                    e
                ));
                EventLoopLogger::new()
            }
        };

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
        if should_exit_before_effect(&state) {
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
        let start_time = Instant::now();
        let result = match execute_effect_guarded(handler, effect, ctx, &state) {
            GuardedEffectResult::Ok(result) => *result,
            GuardedEffectResult::Unrecoverable(err) => {
                // Non-terminating-by-default requirement:
                // Even "unrecoverable" handler errors must route through reducer-visible
                // remediation (AwaitingDevFix) so TriggerDevFixFlow can write the completion
                // marker and dispatch dev-fix.
                let mut recovery_ctx = ErrorRecoveryContext {
                    ctx,
                    trace: &trace,
                    state: &state,
                    effect_str: &effect_str,
                    start_time,
                    handler,
                    event_loop_logger: &mut event_loop_logger,
                };
                state = handle_unrecoverable_error(&mut recovery_ctx, &err);
                events_processed += 1;
                continue;
            }
            GuardedEffectResult::Panic => {
                // Handler panics are internal failures and must not terminate the run.
                // Route through AwaitingDevFix so TriggerDevFixFlow writes the completion marker
                // and dispatches dev-fix.
                let mut recovery_ctx = ErrorRecoveryContext {
                    ctx,
                    trace: &trace,
                    state: &state,
                    effect_str: &effect_str,
                    start_time,
                    handler,
                    event_loop_logger: &mut event_loop_logger,
                };
                state = handle_panic(&mut recovery_ctx, events_processed);
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
        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Apply pipeline event to state (reducer remains pure)
        let new_state = reduce(state, result.event.clone());

        // Log to event loop log (best-effort, does not affect correctness)
        log_effect_execution(
            ctx,
            &mut event_loop_logger,
            &new_state,
            &effect_str,
            &event_str,
            &result.additional_events,
            duration_ms,
        );

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
            let additional_state = reduce(state, event);
            trace.push(build_trace_entry(
                events_processed,
                &additional_state,
                &effect_str,
                &event_str,
            ));
            handler.update_state(additional_state.clone());
            state = additional_state;
            events_processed += 1;
        }

        // Update loop detection counters AFTER all events have been processed.
        // This is critical: additional events can change phase, agent chain, etc.,
        // and loop detection must consider the final state after all reductions.
        let current_fingerprint = crate::reducer::compute_effect_fingerprint(&state);
        state = PipelineState {
            continuation: state
                .continuation
                .update_loop_detection_counters(current_fingerprint),
            ..state
        };
        handler.update_state(state.clone());

        // Check completion AFTER effect execution and state update.
        // This ensures that transitions to terminal phases (e.g., Interrupted)
        // have a chance to save their checkpoint before the loop exits.
        if should_exit_after_effect(&state) {
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

    // Handle edge cases when max iterations is reached
    let mut forced_completion = false;
    let mut recovery_failed = false;
    let mut trace_already_dumped = false;

    if events_processed >= config.max_iterations {
        // Try to force checkpoint execution if needed
        let checkpoint_result = handle_forced_checkpoint_after_completion(
            ctx,
            handler,
            state.clone(),
            events_processed,
            &mut trace,
        );
        match checkpoint_result {
            RecoveryResult::Success(new_state, new_events_processed, dumped) => {
                state = new_state;
                events_processed = new_events_processed;
                trace_already_dumped = trace_already_dumped || dumped;
            }
            RecoveryResult::FailedUnrecoverable(new_state, new_events_processed, dumped) => {
                state = new_state;
                events_processed = new_events_processed;
                recovery_failed = true;
                trace_already_dumped = trace_already_dumped || dumped;
            }
            RecoveryResult::NotNeeded => {
                // No checkpoint needed, continue with existing state
            }
        }

        // If not complete yet and checkpoint didn't fail, try defensive recovery in AwaitingDevFix
        if !state.is_complete() && !recovery_failed {
            let dev_fix_result = handle_max_iterations_in_awaiting_dev_fix(
                ctx,
                handler,
                state.clone(),
                events_processed,
                &mut trace,
            );
            match dev_fix_result {
                RecoveryResult::Success(new_state, new_events_processed, dumped) => {
                    state = new_state;
                    events_processed = new_events_processed;
                    forced_completion = true;
                    trace_already_dumped = trace_already_dumped || dumped;
                }
                RecoveryResult::FailedUnrecoverable(new_state, new_events_processed, dumped) => {
                    state = new_state;
                    events_processed = new_events_processed;
                    recovery_failed = true;
                    trace_already_dumped = trace_already_dumped || dumped;
                }
                RecoveryResult::NotNeeded => {
                    // Not in AwaitingDevFix, no recovery needed
                }
            }
        }

        // Dump trace if we hit max iterations (but only if not already dumped)
        if !trace_already_dumped {
            let dumped = dump_event_loop_trace(ctx, &trace, &state, "max_iterations");
            if dumped {
                let trace_path = ctx.run_log_context.event_loop_trace();
                ctx.logger.warn(&format!(
                    "Event loop reached max iterations ({}) without completion (trace: {})",
                    config.max_iterations,
                    trace_path.display()
                ));
            } else {
                ctx.logger.warn(&format!(
                    "Event loop reached max iterations ({}) without completion",
                    config.max_iterations
                ));
            }
        }

        if !forced_completion && !state.is_complete() {
            ctx.logger.error(&format!(
                "Event loop exiting: reason=max_iterations, phase={:?}, checkpoint_saved_count={}, events_processed={}",
                state.phase, state.checkpoint_saved_count, events_processed
            ));
        }
    }

    // Determine if the loop completed successfully.
    // If recovery failed, report incomplete even if state.is_complete() returns true.
    let completed = state.is_complete() && !recovery_failed;

    if !completed {
        ctx.logger.warn(&format!(
            "Event loop exiting without completion: phase={:?}, checkpoint_saved_count={}, \
             previous_phase={:?}, events_processed={}, recovery_failed={}",
            state.phase,
            state.checkpoint_saved_count,
            state.previous_phase,
            events_processed,
            recovery_failed
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
        final_state: state.clone(),
    })
}

/// Determine if we should exit the loop BEFORE executing the next effect.
///
/// Returns true if the state is already complete, with exceptions for:
/// - Interrupted from AwaitingDevFix without checkpoint (need SaveCheckpoint)
/// - AwaitingDevFix without dev_fix_triggered (need TriggerDevFixFlow)
fn should_exit_before_effect(state: &PipelineState) -> bool {
    if !state.is_complete() {
        return false;
    }

    let should_allow_checkpoint_save = matches!(state.phase, PipelinePhase::Interrupted)
        && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
        && state.checkpoint_saved_count == 0;

    let is_awaiting_dev_fix_not_triggered =
        matches!(state.phase, PipelinePhase::AwaitingDevFix) && !state.dev_fix_triggered;

    !should_allow_checkpoint_save && !is_awaiting_dev_fix_not_triggered
}

/// Determine if we should exit the loop AFTER executing an effect.
///
/// Similar logic to should_exit_before_effect, but checks after state transitions.
fn should_exit_after_effect(state: &PipelineState) -> bool {
    should_exit_before_effect(state)
}

/// Context for error recovery operations in the event loop.
///
/// Groups related parameters needed for error handling to avoid
/// excessive function parameters.
struct ErrorRecoveryContext<'a, 'b, H>
where
    H: StatefulHandler,
{
    ctx: &'a mut PhaseContext<'b>,
    trace: &'a EventTraceBuffer,
    state: &'a PipelineState,
    effect_str: &'a str,
    start_time: Instant,
    handler: &'a mut H,
    event_loop_logger: &'a mut EventLoopLogger,
}

/// Handle an unrecoverable error from the effect handler.
///
/// Routes the error through the reducer as a HandlerError event, transitioning
/// to AwaitingDevFix phase. Dumps trace and writes completion marker as best-effort.
fn handle_unrecoverable_error<'ctx, H>(
    recovery_ctx: &mut ErrorRecoveryContext<'_, '_, H>,
    err: &anyhow::Error,
) -> PipelineState
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let ErrorRecoveryContext {
        ctx,
        trace,
        state,
        effect_str,
        start_time,
        handler,
        event_loop_logger,
    } = recovery_ctx;
    let dumped = dump_event_loop_trace(ctx, trace, state, "unrecoverable_handler_error");
    let marker_written = write_completion_marker_on_error(ctx, err);

    if dumped {
        let trace_path = ctx.run_log_context.event_loop_trace();
        ctx.logger.error(&format!(
            "Event loop encountered unrecoverable handler error (trace: {})",
            trace_path.display()
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
    let failure_event =
        PipelineEvent::PromptInput(crate::reducer::event::PromptInputEvent::HandlerError {
            phase: state.phase,
            error: crate::reducer::event::ErrorEvent::WorkspaceWriteFailed {
                path: "(unrecoverable_handler_error)".to_string(),
                kind: crate::reducer::event::WorkspaceIoErrorKind::Other,
            },
        });

    let event_str = format!("{:?}", failure_event);
    let duration_ms = start_time.elapsed().as_millis() as u64;
    let new_state = reduce(state.clone(), failure_event);

    // Log to event loop log (best-effort)
    log_effect_execution(
        ctx,
        event_loop_logger,
        &new_state,
        effect_str,
        &event_str,
        &[],
        duration_ms,
    );

    handler.update_state(new_state.clone());
    new_state
}

/// Handle a panic from the effect handler.
///
/// Routes the panic through the reducer as a HandlerError event, transitioning
/// to AwaitingDevFix phase. Dumps trace and writes completion marker as best-effort.
fn handle_panic<'ctx, H>(
    recovery_ctx: &mut ErrorRecoveryContext<'_, '_, H>,
    events_processed: usize,
) -> PipelineState
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let ErrorRecoveryContext {
        ctx,
        trace,
        state,
        effect_str,
        start_time,
        handler,
        event_loop_logger,
    } = recovery_ctx;
    let dumped = dump_event_loop_trace(ctx, trace, state, "panic");
    if dumped {
        let trace_path = ctx.run_log_context.event_loop_trace();
        ctx.logger.error(&format!(
            "Event loop recovered from panic (trace: {})",
            trace_path.display()
        ));
    } else {
        ctx.logger.error("Event loop recovered from panic");
    }

    // Best-effort completion marker for orchestration
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

    let failure_event =
        PipelineEvent::PromptInput(crate::reducer::event::PromptInputEvent::HandlerError {
            phase: state.phase,
            error: crate::reducer::event::ErrorEvent::WorkspaceWriteFailed {
                path: "(handler_panic)".to_string(),
                kind: crate::reducer::event::WorkspaceIoErrorKind::Other,
            },
        });

    let event_str = format!("{:?}", failure_event);
    let duration_ms = start_time.elapsed().as_millis() as u64;
    let new_state = reduce(state.clone(), failure_event);

    // Log to event loop log (best-effort)
    log_effect_execution(
        ctx,
        event_loop_logger,
        &new_state,
        effect_str,
        &event_str,
        &[],
        duration_ms,
    );

    handler.update_state(new_state.clone());
    new_state
}

/// Log effect execution to the event loop log.
///
/// This is a best-effort operation - failures are logged but do not affect
/// pipeline execution since event loop logging is for observability only.
fn log_effect_execution(
    ctx: &mut PhaseContext<'_>,
    event_loop_logger: &mut EventLoopLogger,
    state: &PipelineState,
    effect_str: &str,
    event_str: &str,
    additional_events: &[PipelineEvent],
    duration_ms: u64,
) {
    let extra_events: Vec<String> = additional_events
        .iter()
        .map(|e| format!("{:?}", e))
        .collect();

    let context_pairs: Vec<(&str, String)> = vec![
        ("iteration", state.iteration.to_string()),
        ("reviewer_pass", state.reviewer_pass.to_string()),
    ];
    let context_refs: Vec<(&str, &str)> = context_pairs
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    if let Err(e) = event_loop_logger.log_effect(crate::logging::LogEffectParams {
        workspace: ctx.workspace,
        log_path: &ctx.run_log_context.event_loop_log(),
        phase: state.phase,
        effect: effect_str,
        primary_event: event_str,
        extra_events: &extra_events,
        duration_ms,
        context: &context_refs,
    }) {
        ctx.logger
            .warn(&format!("Failed to write to event loop log: {}", e));
    }
}
