//! Prompt input oversize detection and materialization events.
//!
//! These events make reducer-visible any transformation that affects the
//! agent-visible prompt content (inline vs file reference, truncation, etc.).

use crate::reducer::state::{MaterializedPromptInput, PromptInputKind};
use serde::{Deserialize, Serialize};

// Import from the parent event module, which defines PipelinePhase in mod.rs
// and ErrorEvent in error.rs
use crate::reducer::event::{ErrorEvent, PipelinePhase};

/// Prompt input oversize detection and materialization events.
///
/// These events make reducer-visible any transformation that affects the
/// agent-visible prompt content (inline vs file reference, truncation, etc.).
///
/// # Purpose
///
/// Large prompt inputs (PROMPT.md, PLAN.md, diffs) may exceed model context limits.
/// When this occurs, handlers materialize the content as file references instead of
/// inline text. These events record the materialization strategy for observability
/// and to enable the reducer to track content transformations.
///
/// # Emitted By
///
/// - Prompt preparation handlers in `handler/*/prepare_prompt.rs`
/// - XSD retry handlers
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PromptInputEvent {
    /// Oversize content detected, will be materialized as file reference.
    OversizeDetected {
        /// Pipeline phase where oversize was detected.
        phase: PipelinePhase,
        /// Type of content (prompt, plan, diff, etc.).
        kind: PromptInputKind,
        /// SHA256 hex digest of the content.
        content_id_sha256: String,
        /// Actual content size in bytes.
        size_bytes: u64,
        /// Configured size limit in bytes.
        limit_bytes: u64,
        /// Materialization policy applied.
        policy: String,
    },
    /// Planning prompt inputs materialized.
    PlanningInputsMaterialized {
        /// Iteration number.
        iteration: u32,
        /// Materialized prompt input.
        prompt: MaterializedPromptInput,
    },
    /// Development prompt inputs materialized.
    DevelopmentInputsMaterialized {
        /// Iteration number.
        iteration: u32,
        /// Materialized prompt input.
        prompt: MaterializedPromptInput,
        /// Materialized plan input.
        plan: MaterializedPromptInput,
    },
    /// Review prompt inputs materialized.
    ReviewInputsMaterialized {
        /// Review pass number.
        pass: u32,
        /// Materialized plan input.
        plan: MaterializedPromptInput,
        /// Materialized diff input.
        diff: MaterializedPromptInput,
    },
    /// Commit prompt inputs materialized.
    CommitInputsMaterialized {
        /// Commit attempt number.
        attempt: u32,
        /// Materialized diff input.
        diff: MaterializedPromptInput,
    },
    /// XSD retry last output materialized.
    XsdRetryLastOutputMaterialized {
        /// Phase that produced the invalid output being retried.
        phase: PipelinePhase,
        /// Scope id within the phase (iteration/pass/attempt).
        scope_id: u32,
        /// Materialized representation of the last invalid output.
        last_output: MaterializedPromptInput,
    },
    /// A typed error event returned by an effect handler.
    ///
    /// Effect handlers surface failures by returning `Err(ErrorEvent::... .into())`.
    /// The event loop extracts the underlying `ErrorEvent` and re-emits it through
    /// this existing category so the reducer can decide recovery strategy without
    /// adding new top-level `PipelineEvent` variants.
    HandlerError {
        /// Phase during which the error occurred (best-effort; derived from current state).
        phase: PipelinePhase,
        /// The typed error event.
        error: ErrorEvent,
    },
}
