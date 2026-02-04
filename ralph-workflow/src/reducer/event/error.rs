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
//!    The event loop catches `Err()`, downcasts to `ErrorEvent`, and re-emits it as
//!    `PipelineEvent::PromptInput(PromptInputEvent::HandlerError { ... })` so the
//!    reducer can decide recovery strategy without adding new top-level `PipelineEvent`
//!    variants.
//!
//! 3. **Reducer decides recovery strategy**
//!    The reducer processes the error identically to other events (it is still routed
//!    through the main `reduce` function), deciding whether to retry, fallback, skip,
//!    or terminate based on the specific error variant.
//!
//! 4. **Event loop acts on reducer decision**
//!    If the reducer transitions to Interrupted phase, the event loop terminates.
//!    Otherwise, execution continues with the next effect (e.g., by clearing a
//!    "prepared" flag to force re-materialization after a checkpoint resume).
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

/// Serializable subset of `std::io::ErrorKind`.
///
/// `std::io::Error` / `ErrorKind` are not serde-serializable, but reducer error events
/// must be persisted in checkpoints. This enum captures the subset of error kinds we
/// need for recovery policy decisions.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum WorkspaceIoErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidData,
    Other,
}

impl WorkspaceIoErrorKind {
    pub fn from_io_error_kind(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            std::io::ErrorKind::InvalidData => Self::InvalidData,
            _ => Self::Other,
        }
    }
}

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

    /// Workspace read failure that must be handled by the reducer.
    WorkspaceReadFailed {
        /// Workspace-relative path.
        path: String,
        kind: WorkspaceIoErrorKind,
    },
    /// Workspace write failure that must be handled by the reducer.
    WorkspaceWriteFailed {
        /// Workspace-relative path.
        path: String,
        kind: WorkspaceIoErrorKind,
    },
    /// Workspace directory creation failure that must be handled by the reducer.
    WorkspaceCreateDirAllFailed {
        /// Workspace-relative path.
        path: String,
        kind: WorkspaceIoErrorKind,
    },
    /// Workspace remove failure that must be handled by the reducer.
    WorkspaceRemoveFailed {
        /// Workspace-relative path.
        path: String,
        kind: WorkspaceIoErrorKind,
    },

    /// Failed to stage changes before creating a commit.
    ///
    /// Commit creation requires staging (equivalent to `git add -A`). When this fails,
    /// the error must flow through the reducer as a typed event so the reducer can
    /// decide whether to retry, fallback, or terminate.
    GitAddAllFailed { kind: WorkspaceIoErrorKind },

    /// Agent registry lookup failed (unknown agent).
    AgentNotFound { agent: String },

    /// Planning inputs not materialized before preparing/invoking planning prompt.
    PlanningInputsNotMaterialized { iteration: u32 },
    /// Development inputs not materialized before preparing/invoking development prompt.
    DevelopmentInputsNotMaterialized { iteration: u32 },
    /// Commit inputs not materialized before preparing commit prompt.
    CommitInputsNotMaterialized { attempt: u32 },

    /// Prepared planning prompt file missing/unreadable when invoking planning agent.
    PlanningPromptMissing { iteration: u32 },
    /// Prepared development prompt file missing/unreadable when invoking development agent.
    DevelopmentPromptMissing { iteration: u32 },
    /// Prepared review prompt file missing/unreadable when invoking review agent.
    ReviewPromptMissing { pass: u32 },
    /// Prepared commit prompt file missing/unreadable when invoking commit agent.
    CommitPromptMissing { attempt: u32 },

    /// Missing validated planning markdown when writing `.agent/PLAN.md`.
    ValidatedPlanningMarkdownMissing { iteration: u32 },
    /// Missing validated development outcome when applying/writing results.
    ValidatedDevelopmentOutcomeMissing { iteration: u32 },
    /// Missing validated review outcome when applying/writing results.
    ValidatedReviewOutcomeMissing { pass: u32 },
    /// Missing validated fix outcome when applying fixes.
    ValidatedFixOutcomeMissing { pass: u32 },
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
            ErrorEvent::WorkspaceReadFailed { path, kind } => {
                write!(f, "Workspace read failed at {path} ({kind:?})")
            }
            ErrorEvent::WorkspaceWriteFailed { path, kind } => {
                write!(f, "Workspace write failed at {path} ({kind:?})")
            }
            ErrorEvent::WorkspaceCreateDirAllFailed { path, kind } => {
                write!(f, "Workspace create_dir_all failed at {path} ({kind:?})")
            }
            ErrorEvent::WorkspaceRemoveFailed { path, kind } => {
                write!(f, "Workspace remove failed at {path} ({kind:?})")
            }
            ErrorEvent::GitAddAllFailed { kind } => {
                write!(f, "git add -A (stage all changes) failed ({kind:?})")
            }
            ErrorEvent::AgentNotFound { agent } => {
                write!(f, "Agent not found: {agent}")
            }
            ErrorEvent::PlanningInputsNotMaterialized { iteration } => {
                write!(
                    f,
                    "Planning inputs not materialized for iteration {iteration} (expected materialize_planning_inputs before prepare/invoke)"
                )
            }
            ErrorEvent::DevelopmentInputsNotMaterialized { iteration } => {
                write!(
                    f,
                    "Development inputs not materialized for iteration {iteration} (expected materialize_development_inputs before prepare/invoke)"
                )
            }
            ErrorEvent::CommitInputsNotMaterialized { attempt } => {
                write!(
                    f,
                    "Commit inputs not materialized for attempt {attempt} (expected materialize_commit_inputs before prepare)"
                )
            }
            ErrorEvent::PlanningPromptMissing { iteration } => {
                write!(
                    f,
                    "Missing planning prompt at .agent/tmp/planning_prompt.txt for iteration {iteration}"
                )
            }
            ErrorEvent::DevelopmentPromptMissing { iteration } => {
                write!(
                    f,
                    "Missing development prompt at .agent/tmp/development_prompt.txt for iteration {iteration}"
                )
            }
            ErrorEvent::ReviewPromptMissing { pass } => {
                write!(
                    f,
                    "Missing review prompt at .agent/tmp/review_prompt.txt for pass {pass}"
                )
            }
            ErrorEvent::CommitPromptMissing { attempt } => {
                write!(
                    f,
                    "Missing commit prompt at .agent/tmp/commit_prompt.txt for attempt {attempt}"
                )
            }
            ErrorEvent::ValidatedPlanningMarkdownMissing { iteration } => {
                write!(
                    f,
                    "Missing validated planning markdown for iteration {iteration}"
                )
            }
            ErrorEvent::ValidatedDevelopmentOutcomeMissing { iteration } => {
                write!(
                    f,
                    "Missing validated development outcome for iteration {iteration}"
                )
            }
            ErrorEvent::ValidatedReviewOutcomeMissing { pass } => {
                write!(f, "Missing validated review outcome for pass {pass}")
            }
            ErrorEvent::ValidatedFixOutcomeMissing { pass } => {
                write!(f, "Missing validated fix outcome for pass {pass}")
            }
        }
    }
}

impl std::error::Error for ErrorEvent {}

// Note: From<ErrorEvent> for anyhow::Error is provided by anyhow's blanket implementation
// for all types that implement std::error::Error + Send + Sync + 'static.
// This automatically preserves ErrorEvent as the error source for downcasting.
