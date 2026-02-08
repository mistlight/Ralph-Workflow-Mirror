//! Main event loop implementation.
//!
//! This module contains the core orchestrate-handle-reduce cycle that drives
//! the reducer-based pipeline. The event loop coordinates pure reducer logic
//! with impure effect handlers, maintaining strict separation of concerns.
//!
//! ## Event Loop Cycle
//!
//! ```text
//! State → Orchestrate → Effect → Handle → Event → Reduce → Next State
//!         (pure)                 (impure)         (pure)
//! ```
//!
//! The loop continues until reaching a terminal state (Interrupted, Completed)
//! or until max iterations is exceeded.

use super::config::{create_initial_state_with_config, EventLoopConfig, EventLoopResult};
use super::error_handling::{
    execute_effect_guarded, write_completion_marker_on_error, GuardedEffectResult,
};
use super::trace::{
    build_trace_entry, dump_event_loop_trace, EventTraceBuffer, DEFAULT_EVENT_LOOP_TRACE_CAPACITY,
};
use crate::logging::EventLoopLogger;
use crate::phases::PhaseContext;
use crate::reducer::effect::Effect;
use crate::reducer::event::{AwaitingDevFixEvent, CheckpointTrigger, PipelineEvent, PipelinePhase};
use crate::reducer::{
    determine_next_effect, reduce, EffectHandler, MainEffectHandler, PipelineState,
};
use anyhow::Result;
use std::path::Path;
use std::time::Instant;

/// Trait for handlers that maintain internal state.
///
/// This trait allows the event loop to update the handler's internal state
/// after each event is processed.
pub trait StatefulHandler {
    /// Update the handler's internal state.
    fn update_state(&mut self, state: PipelineState);
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
        let start_time = Instant::now();
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
                let duration_ms = start_time.elapsed().as_millis() as u64;
                let new_state = reduce(state, failure_event);

                // Log to event loop log (best-effort, does not affect correctness)
                let context_pairs: Vec<(&str, String)> = vec![
                    ("iteration", new_state.iteration.to_string()),
                    ("reviewer_pass", new_state.reviewer_pass.to_string()),
                    ("error_kind", "unrecoverable_failure".to_string()),
                    ("effect", effect_str.clone()),
                ];
                let context_refs: Vec<(&str, &str)> = context_pairs
                    .iter()
                    .map(|(k, v)| (*k, v.as_str()))
                    .collect();
                if let Err(e) = event_loop_logger.log_effect(crate::logging::LogEffectParams {
                    workspace: ctx.workspace,
                    log_path: &ctx.run_log_context.event_loop_log(),
                    phase: new_state.phase,
                    effect: &effect_str,
                    primary_event: &event_str,
                    extra_events: &[],
                    duration_ms,
                    context: &context_refs,
                }) {
                    ctx.logger
                        .warn(&format!("Failed to write to event loop log: {}", e));
                }

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
                    let trace_path = ctx.run_log_context.event_loop_trace();
                    ctx.logger.error(&format!(
                        "Event loop recovered from panic (trace: {})",
                        trace_path.display()
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
                let duration_ms = start_time.elapsed().as_millis() as u64;
                let new_state = reduce(state, failure_event);

                // Log to event loop log (best-effort, does not affect correctness)
                let context_pairs: Vec<(&str, String)> = vec![
                    ("iteration", new_state.iteration.to_string()),
                    ("reviewer_pass", new_state.reviewer_pass.to_string()),
                    ("error_kind", "handler_panic".to_string()),
                    ("effect", effect_str.clone()),
                ];
                let context_refs: Vec<(&str, &str)> = context_pairs
                    .iter()
                    .map(|(k, v)| (*k, v.as_str()))
                    .collect();
                if let Err(e) = event_loop_logger.log_effect(crate::logging::LogEffectParams {
                    workspace: ctx.workspace,
                    log_path: &ctx.run_log_context.event_loop_log(),
                    phase: new_state.phase,
                    effect: &effect_str,
                    primary_event: &event_str,
                    extra_events: &[],
                    duration_ms,
                    context: &context_refs,
                }) {
                    ctx.logger
                        .warn(&format!("Failed to write to event loop log: {}", e));
                }

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
        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Apply pipeline event to state (reducer remains pure)
        let new_state = reduce(state, result.event.clone());

        // Log to event loop log (best-effort, does not affect correctness)
        let extra_events: Vec<String> = result
            .additional_events
            .iter()
            .map(|e| format!("{:?}", e))
            .collect();
        let context_pairs: Vec<(&str, String)> = vec![
            ("iteration", new_state.iteration.to_string()),
            ("reviewer_pass", new_state.reviewer_pass.to_string()),
        ];
        let context_refs: Vec<(&str, &str)> = context_pairs
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect();
        if let Err(e) = event_loop_logger.log_effect(crate::logging::LogEffectParams {
            workspace: ctx.workspace,
            log_path: &ctx.run_log_context.event_loop_log(),
            phase: new_state.phase,
            effect: &effect_str,
            primary_event: &event_str,
            extra_events: &extra_events,
            duration_ms,
            context: &context_refs,
        }) {
            ctx.logger
                .warn(&format!("Failed to write to event loop log: {}", e));
        }

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

                // We can't safely continue execution here (we are outside the main loop).
                // State is already terminal (Interrupted from AwaitingDevFix), so return completion
                // even if SaveCheckpoint fails.
                return Ok(EventLoopResult {
                    completed: true,
                    events_processed,
                    final_phase: state.phase,
                    final_state: state.clone(),
                });
            }
            GuardedEffectResult::Panic => {
                // Panics during forced completion are internal failures; route through AwaitingDevFix.
                let dumped = dump_event_loop_trace(ctx, &trace, &state, "panic");
                if dumped {
                    let trace_path = ctx.run_log_context.event_loop_trace();
                    ctx.logger.error(&format!(
                        "Event loop recovered from panic (trace: {})",
                        trace_path.display()
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
                    final_state: state.clone(),
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

                    // We can't safely continue execution here (we are outside the main loop).
                    // State is already terminal (Interrupted from AwaitingDevFix).
                    // However, the run did NOT complete cleanly, so report incomplete while still
                    // having written a best-effort completion marker above.
                    return Ok(EventLoopResult {
                        completed: false,
                        events_processed,
                        final_phase: state.phase,
                        final_state: state.clone(),
                    });
                }
                GuardedEffectResult::Panic => {
                    // Panics during forced completion are internal failures; route through AwaitingDevFix.
                    let dumped = dump_event_loop_trace(ctx, &trace, &state, "panic");
                    if dumped {
                        let trace_path = ctx.run_log_context.event_loop_trace();
                        ctx.logger.error(&format!(
                            "Event loop recovered from panic (trace: {})",
                            trace_path.display()
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
                        final_state: state.clone(),
                    });
                }
            }

            forced_completion = true;

            ctx.logger
                .info("Forced transition to Interrupted phase to satisfy termination requirements");
        }

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
        final_state: state.clone(),
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
