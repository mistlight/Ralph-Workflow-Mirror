//! Error handling and panic recovery for the event loop.
//!
//! This module provides infrastructure for recovering from handler errors and panics
//! while maintaining the non-terminating pipeline guarantee. All errors are routed
//! through the reducer state machine to ensure proper remediation flow.

use crate::phases::PhaseContext;
use crate::reducer::effect::{Effect, EffectResult};
use crate::reducer::EffectHandler;
use crate::reducer::PipelineState;
use std::path::Path;

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
pub(super) fn extract_error_event(
    err: &anyhow::Error,
) -> Option<crate::reducer::event::ErrorEvent> {
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

/// Result of guarded effect execution.
///
/// Distinguishes between:
/// - `Ok`: Effect executed successfully, returning an EffectResult
/// - `Unrecoverable`: Handler returned an error that cannot be downcast to ErrorEvent
/// - `Panic`: Handler panicked during execution
pub(super) enum GuardedEffectResult {
    Ok(Box<EffectResult>),
    Unrecoverable(anyhow::Error),
    Panic,
}

/// Execute an effect with panic recovery.
///
/// Catches panics from the effect handler and converts them to `GuardedEffectResult::Panic`.
/// Handler errors are analyzed via `extract_error_event()`:
/// - If the error is a typed `ErrorEvent`, it's wrapped as a `HandlerError` event
/// - Otherwise, it's returned as `Unrecoverable`
pub(super) fn execute_effect_guarded<'ctx, H>(
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

/// Write a completion marker on unrecoverable handler error.
///
/// This is a best-effort operation to ensure orchestration is notified even when
/// the dev-fix flow cannot execute normally. Returns `true` if the marker was
/// successfully written.
pub(super) fn write_completion_marker_on_error(
    ctx: &mut PhaseContext<'_>,
    err: &anyhow::Error,
) -> bool {
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
