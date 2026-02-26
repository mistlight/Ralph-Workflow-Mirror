//! Event loop recovery and defensive completion logic.
//!
//! This module handles edge cases where the event loop needs defensive recovery:
//! - Forced checkpoint execution when max iterations is reached after completion marker
//! - Max iterations handling in `AwaitingDevFix` phase (defensive completion marker)
//!
//! ## Non-Terminating Pipeline Principle
//!
//! Even when max iterations is reached, the pipeline must NOT terminate early.
//! Instead, it writes a completion marker and transitions to Interrupted phase,
//! allowing the orchestration layer to handle the failure gracefully.

use super::error_handling::{execute_effect_guarded, GuardedEffectResult};
use super::trace::{build_trace_entry, dump_event_loop_trace, EventTraceBuffer};
use super::StatefulHandler;
use crate::phases::PhaseContext;
use crate::reducer::effect::Effect;
use crate::reducer::event::{AwaitingDevFixEvent, CheckpointTrigger, PipelineEvent, PipelinePhase};
use crate::reducer::{reduce, EffectHandler, PipelineState};
use std::path::Path;

/// Result of recovery operations.
pub(super) enum RecoveryResult {
    /// Recovery succeeded, state updated (state, `events_processed`, `trace_dumped`)
    Success(PipelineState, usize, bool),
    /// Recovery failed unrecoverably - return as incomplete (state, `events_processed`, `trace_dumped`)
    FailedUnrecoverable(PipelineState, usize, bool),
    /// Recovery not needed
    NotNeeded,
}

/// Handle forced checkpoint execution after completion marker.
///
/// When max iterations is reached after transitioning to Interrupted from `AwaitingDevFix`,
/// we need to execute `SaveCheckpoint` even though `is_complete()` returns true.
/// This ensures the checkpoint is persisted for proper state tracking.
///
/// Returns the recovery result indicating success or failure.
pub(super) fn handle_forced_checkpoint_after_completion<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    handler: &mut H,
    state: PipelineState,
    mut events_processed: usize,
    trace: &mut EventTraceBuffer,
) -> RecoveryResult
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let should_force_checkpoint = matches!(state.phase, PipelinePhase::Interrupted)
        && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
        && state.checkpoint_saved_count == 0;

    if !should_force_checkpoint {
        return RecoveryResult::NotNeeded;
    }

    ctx.logger
        .warn("Max iterations reached after completion marker; forcing SaveCheckpoint execution");

    let save_effect = Effect::SaveCheckpoint {
        trigger: CheckpointTrigger::Interrupt,
    };
    let save_effect_str = format!("{save_effect:?}");

    match execute_effect_guarded(handler, save_effect, ctx, &state) {
        GuardedEffectResult::Ok(result) => {
            let result = *result;
            let event_str = format!("{:?}", result.event);
            let mut new_state = reduce(state, result.event.clone());
            trace.push(build_trace_entry(
                events_processed,
                &new_state,
                &save_effect_str,
                &event_str,
            ));
            handler.update_state(new_state.clone());
            events_processed += 1;

            for event in result.additional_events {
                let event_str = format!("{event:?}");
                new_state = reduce(new_state, event);
                trace.push(build_trace_entry(
                    events_processed,
                    &new_state,
                    &save_effect_str,
                    &event_str,
                ));
                handler.update_state(new_state.clone());
                events_processed += 1;
            }

            RecoveryResult::Success(new_state, events_processed, false)
        }
        GuardedEffectResult::Unrecoverable(err) => {
            // Even failures while forcing checkpoint completion must route through
            // AwaitingDevFix rather than returning Err. State is already terminal
            // (Interrupted from AwaitingDevFix), but the run did NOT complete cleanly,
            // so report incomplete.
            let dumped = dump_event_loop_trace(ctx, trace, &state, "unrecoverable_handler_error");
            super::error_handling::write_completion_marker_on_error(ctx, &err);

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

            RecoveryResult::FailedUnrecoverable(state, events_processed, dumped)
        }
        GuardedEffectResult::Panic => {
            // Panics during forced completion are internal failures.
            let dumped = dump_event_loop_trace(ctx, trace, &state, "panic");
            if dumped {
                let trace_path = ctx.run_log_context.event_loop_trace();
                ctx.logger.error(&format!(
                    "Event loop recovered from panic (trace: {})",
                    trace_path.display()
                ));
            } else {
                ctx.logger.error("Event loop recovered from panic");
            }

            // Best-effort completion marker
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

            RecoveryResult::FailedUnrecoverable(state, events_processed, dumped)
        }
    }
}

/// Handle max iterations defensive recovery in `AwaitingDevFix` phase.
///
/// When max iterations is reached while in `AwaitingDevFix` phase before `TriggerDevFixFlow`
/// executes, this is a bug. However, to maintain the non-terminating pipeline principle,
/// we force completion:
/// 1. Write completion marker (signals orchestration)
/// 2. Emit `CompletionMarkerEmitted` event (transitions to Interrupted)
/// 3. Execute `SaveCheckpoint` (makes `is_complete()` return true)
///
/// Returns the recovery result indicating success or failure.
pub(super) fn handle_max_iterations_in_awaiting_dev_fix<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    handler: &mut H,
    state: PipelineState,
    mut events_processed: usize,
    trace: &mut EventTraceBuffer,
) -> RecoveryResult
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    if !matches!(state.phase, PipelinePhase::AwaitingDevFix) {
        return RecoveryResult::NotNeeded;
    }

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
        "failure\nMax iterations reached in AwaitingDevFix phase (events_processed={events_processed})"
    );
    let marker_written = match ctx.workspace.write(marker_path, &content) {
        Ok(()) => {
            ctx.logger
                .info("Completion marker written for max iterations failure");
            true
        }
        Err(err) => {
            ctx.logger.error(&format!(
                "Failed to write completion marker for max iterations failure: {err}"
            ));
            false
        }
    };

    // CRITICAL: Do not transition to Interrupted unless the completion marker exists.
    // External orchestration relies on the marker to observe termination.
    if !marker_written {
        return RecoveryResult::FailedUnrecoverable(state, events_processed, false);
    }

    // Emit CompletionMarkerEmitted event
    let completion_event =
        PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
            is_failure: true,
        });
    let completion_event_str = format!("{completion_event:?}");
    let mut new_state = reduce(state, completion_event);
    trace.push(build_trace_entry(
        events_processed,
        &new_state,
        "ForcedCompletionMarker",
        &completion_event_str,
    ));
    handler.update_state(new_state.clone());
    events_processed += 1;

    // Execute SaveCheckpoint
    let save_effect = Effect::SaveCheckpoint {
        trigger: CheckpointTrigger::Interrupt,
    };
    let save_effect_str = format!("{save_effect:?}");

    match execute_effect_guarded(handler, save_effect, ctx, &new_state) {
        GuardedEffectResult::Ok(result) => {
            let result = *result;
            let event_str = format!("{:?}", result.event);
            new_state = reduce(new_state, result.event.clone());
            trace.push(build_trace_entry(
                events_processed,
                &new_state,
                &save_effect_str,
                &event_str,
            ));
            handler.update_state(new_state.clone());
            events_processed += 1;

            for event in result.additional_events {
                let event_str = format!("{event:?}");
                new_state = reduce(new_state, event);
                trace.push(build_trace_entry(
                    events_processed,
                    &new_state,
                    &save_effect_str,
                    &event_str,
                ));
                handler.update_state(new_state.clone());
                events_processed += 1;
            }

            RecoveryResult::Success(new_state, events_processed, false)
        }
        GuardedEffectResult::Unrecoverable(err) => {
            let dumped =
                dump_event_loop_trace(ctx, trace, &new_state, "unrecoverable_handler_error");
            super::error_handling::write_completion_marker_on_error(ctx, &err);

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

            // We can't safely continue execution here. State is already terminal (Interrupted
            // from AwaitingDevFix). However, the run did NOT complete cleanly.
            RecoveryResult::FailedUnrecoverable(new_state, events_processed, dumped)
        }
        GuardedEffectResult::Panic => {
            let dumped = dump_event_loop_trace(ctx, trace, &new_state, "panic");
            if dumped {
                let trace_path = ctx.run_log_context.event_loop_trace();
                ctx.logger.error(&format!(
                    "Event loop recovered from panic (trace: {})",
                    trace_path.display()
                ));
            } else {
                ctx.logger.error("Event loop recovered from panic");
            }

            // Best-effort completion marker
            if let Err(err) = ctx.workspace.create_dir_all(Path::new(".agent/tmp")) {
                ctx.logger.error(&format!(
                    "Failed to create completion marker directory: {err}"
                ));
            }
            let marker_path = Path::new(".agent/tmp/completion_marker");
            let content = format!(
                "failure\nHandler panic during forced completion (phase={:?}, events_processed={})",
                new_state.phase, events_processed
            );
            if let Err(err) = ctx.workspace.write(marker_path, &content) {
                ctx.logger.error(&format!(
                    "Failed to write completion marker for handler panic: {err}"
                ));
            }

            RecoveryResult::FailedUnrecoverable(new_state, events_processed, dumped)
        }
    }
}
