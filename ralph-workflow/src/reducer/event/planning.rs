//! Planning phase events.
//!
//! Events related to plan generation and validation within the Planning phase.
//! The planning phase generates a plan for the current development iteration.

use serde::{Deserialize, Serialize};

/// Planning phase events.
///
/// Events related to plan generation and validation within the Planning phase.
/// The planning phase generates a plan for the current development iteration.
///
/// # State Transitions
///
/// - `PhaseStarted`: Sets phase to Planning
/// - `GenerationCompleted(valid=true)`: Transitions to Development
/// - `GenerationCompleted(valid=false)`: Stays in Planning for retry
/// - `PhaseCompleted`: Transitions to Development
///
/// # Emitted By
///
/// - Planning effect handlers in `handler/planning/`
/// - XSD validation handlers
/// - Markdown generation handlers
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PlanningEvent {
    /// Planning phase has started.
    PhaseStarted,
    /// Planning phase completed, ready to proceed.
    PhaseCompleted,
    /// Planning prompt prepared for an iteration.
    PromptPrepared {
        /// The iteration number this plan is for.
        iteration: u32,
    },
    /// Planning agent invoked for an iteration.
    AgentInvoked {
        /// The iteration number this plan is for.
        iteration: u32,
    },
    /// Planning XML extracted for an iteration.
    PlanXmlExtracted {
        /// The iteration number this plan is for.
        iteration: u32,
    },
    /// Planning XML missing for an iteration.
    PlanXmlMissing {
        /// The iteration number this plan is for.
        iteration: u32,
        /// The invalid output attempt count.
        attempt: u32,
    },
    /// Planning XML validated for an iteration.
    PlanXmlValidated {
        /// The iteration number this plan is for.
        iteration: u32,
        /// Whether the generated plan passed validation.
        valid: bool,
        /// Markdown generated from the validated plan XML.
        markdown: Option<String>,
    },
    /// Planning markdown written for an iteration.
    PlanMarkdownWritten {
        /// The iteration number this plan is for.
        iteration: u32,
    },
    /// Planning XML archived for an iteration.
    PlanXmlArchived {
        /// The iteration number this plan is for.
        iteration: u32,
    },
    /// Planning XML cleaned before invoking the planning agent.
    PlanXmlCleaned {
        /// The iteration number this plan is for.
        iteration: u32,
    },
    /// Plan generation completed with validation result.
    ///
    /// This event signals the end of the plan generation attempt.
    /// The reducer uses the `valid` field to decide whether to:
    /// - Transition to Development phase (valid=true)
    /// - Retry with same agent or switch agents (valid=false)
    GenerationCompleted {
        /// The iteration number this plan was for.
        iteration: u32,
        /// Whether the generated plan passed validation.
        valid: bool,
    },

    /// Output validation failed (missing/empty or otherwise invalid plan output).
    ///
    /// Emitted when planning output cannot be validated. The reducer decides
    /// whether to retry (same agent) or switch agents based on the attempt count.
    OutputValidationFailed {
        /// Current iteration number.
        iteration: u32,
        /// Current invalid output attempt number.
        attempt: u32,
    },
}
