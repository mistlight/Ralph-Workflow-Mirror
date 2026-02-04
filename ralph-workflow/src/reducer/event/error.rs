//! Error events for recoverable and unrecoverable failures.
//!
//! This module implements the error event pattern where ALL errors from effect handlers
//! are represented as typed events that flow through the reducer, enabling the reducer
//! to decide recovery strategy based on semantic meaning.
//!
//! # Error Handling Architecture
//!
//! ## The Pattern
//!
//! 1. **Effect handler encounters error**
//!    ```rust
//!    return Err(ErrorEvent::ReviewInputsNotMaterialized { pass }.into());
//!    ```
//!
//! 2. **Event loop extracts error event**
//!    The event loop catches `Err()`, downcasts to `ErrorEvent`, and wraps it in
//!    `PipelineEvent::Error()` for processing through the reducer.
//!
//! 3. **Reducer decides recovery strategy**
//!    The reducer processes `PipelineEvent::Error()` identically to success events,
//!    deciding whether to retry, fallback, skip, or terminate based on the specific
//!    error variant.
//!
//! 4. **Event loop acts on reducer decision**
//!    If the reducer transitions to Interrupted phase, the event loop terminates.
//!    If the reducer keeps the state unchanged for invariant violations, the event
//!    loop also terminates. Otherwise, execution continues with the next effect.
//!
//! ## Why Not String Errors?
//!
//! String errors (`Err(anyhow::anyhow!("..."))`) would bypass the reducer and prevent
//! recovery logic. The reducer cannot distinguish between "missing optional file"
//! (use fallback) and "permission denied" (abort pipeline) when errors are strings.
//!
//! ## Current Error Categories
//!
//! All current error events represent **invariant violations** (effect sequencing bugs,
//! continuation mode misuse) or **terminal conditions** (agent chain exhaustion). These
//! terminate the pipeline because they indicate bugs in the orchestration logic or
//! exhaustion of all retry attempts.
//!
//! Future error events for recoverable conditions (network timeouts, transient file I/O)
//! will implement retry/fallback strategies in the reducer.

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

    /// Agent chain exhausted for a phase.
    ///
    /// This indicates that all retry attempts have been exhausted for an agent chain
    /// in a specific phase. The reducer decides whether to terminate the pipeline or
    /// attempt recovery based on whether progress has been made.
    AgentChainExhausted {
        /// The role of the agent chain that was exhausted.
        role: crate::agents::AgentRole,
        /// The phase where exhaustion occurred.
        phase: super::PipelinePhase,
        /// The retry cycle number when exhaustion occurred.
        cycle: u32,
    },
}

impl std::fmt::Display for ErrorEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorEvent::ReviewInputsNotMaterialized { pass } => {
                write!(
                    f,
                    "Review inputs not materialized for pass {pass} (expected materialize_review_inputs before prepare_review_prompt)"
                )
            }
            ErrorEvent::PlanningContinuationNotSupported => {
                write!(f, "Planning does not support continuation prompts")
            }
            ErrorEvent::ReviewContinuationNotSupported => {
                write!(f, "Review does not support continuation prompts")
            }
            ErrorEvent::FixContinuationNotSupported => {
                write!(f, "Fix does not support continuation prompts")
            }
            ErrorEvent::CommitContinuationNotSupported => {
                write!(
                    f,
                    "Commit message generation does not support continuation prompts"
                )
            }
            ErrorEvent::FixPromptMissing => {
                write!(f, "Missing fix prompt at .agent/tmp/fix_prompt.txt")
            }
            ErrorEvent::AgentChainExhausted { role, phase, cycle } => {
                write!(
                    f,
                    "Agent chain exhausted for role {:?} in phase {:?} (cycle {})",
                    role, phase, cycle
                )
            }
        }
    }
}

impl std::error::Error for ErrorEvent {}

// Note: From<ErrorEvent> for anyhow::Error is provided by anyhow's blanket implementation
// for all types that implement std::error::Error + Send + Sync + 'static.
// This automatically preserves ErrorEvent as the error source for downcasting.
