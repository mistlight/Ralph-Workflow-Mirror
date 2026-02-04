//! Error events for recoverable and unrecoverable failures.
//!
//! These events are returned through `Err()` from effect handlers and processed
//! by the reducer identically to success events. The reducer decides recovery
//! strategy (retry, fallback, skip, or terminate) based on the specific error variant.

use serde::{Deserialize, Serialize};

/// Error events for failures requiring reducer handling.
///
/// Effect handlers communicate failures by returning `Err()` containing error events
/// from this namespace. The event loop extracts these error events and feeds them to
/// the reducer for processing, just like success events.
///
/// # Usage
///
/// Effect handlers return error events through `Err()`:
/// ```ignore
/// return Err(ErrorEvent::ReviewInputsNotMaterialized { pass }.into());
/// ```
///
/// The event loop extracts the error event and feeds it to the reducer.
///
/// # Principles
///
/// 1. **Errors are events**: All `Err()` returns from effect handlers MUST contain
///    events from this namespace, NOT strings.
/// 2. **Err() is a carrier**: The `Err()` mechanism just carries error events to the
///    event loop; it doesn't bypass the reducer.
/// 3. **Reducer owns recovery**: The reducer processes error events identically to
///    success events and decides recovery strategy.
/// 4. **Typed, not strings**: String errors prevent the reducer from handling different
///    failure modes appropriately.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ErrorEvent {
    /// Review inputs not materialized before prepare_review_prompt.
    ///
    /// This indicates an effect sequencing bug where prepare_review_prompt was called
    /// without first materializing the review inputs via materialize_review_inputs.
    ReviewInputsNotMaterialized {
        /// The review pass number.
        pass: u32,
    },
    /// Planning does not support continuation prompts.
    ///
    /// This is an invariant violation - continuation mode should not be passed to
    /// the planning phase.
    PlanningContinuationNotSupported,
    /// Review does not support continuation prompts.
    ///
    /// This is an invariant violation - continuation mode should not be passed to
    /// the review phase.
    ReviewContinuationNotSupported,
    /// Fix does not support continuation prompts.
    ///
    /// This is an invariant violation - continuation mode should not be passed to
    /// the fix flow.
    FixContinuationNotSupported,
    /// Commit message generation does not support continuation prompts.
    ///
    /// This is an invariant violation - continuation mode should not be passed to
    /// the commit phase.
    CommitContinuationNotSupported,
    /// Missing fix prompt file when invoking fix agent.
    ///
    /// This indicates an effect sequencing bug where invoke_fix was called without
    /// first preparing the fix prompt file at .agent/tmp/fix_prompt.txt.
    FixPromptMissing,
}

impl From<ErrorEvent> for anyhow::Error {
    fn from(event: ErrorEvent) -> Self {
        match event {
            ErrorEvent::ReviewInputsNotMaterialized { pass } => {
                anyhow::anyhow!(
                    "Review inputs not materialized for pass {pass} (expected materialize_review_inputs before prepare_review_prompt)"
                )
            }
            ErrorEvent::PlanningContinuationNotSupported => {
                anyhow::anyhow!("Planning does not support continuation prompts")
            }
            ErrorEvent::ReviewContinuationNotSupported => {
                anyhow::anyhow!("Review does not support continuation prompts")
            }
            ErrorEvent::FixContinuationNotSupported => {
                anyhow::anyhow!("Fix does not support continuation prompts")
            }
            ErrorEvent::CommitContinuationNotSupported => {
                anyhow::anyhow!("Commit message generation does not support continuation prompts")
            }
            ErrorEvent::FixPromptMissing => {
                anyhow::anyhow!("Missing fix prompt at .agent/tmp/fix_prompt.txt")
            }
        }
    }
}
