//! Pipeline event types for reducer architecture.
//!
//! Defines all possible events that can occur during pipeline execution.
//! Each event represents a **fact** about what happened, not a command about
//! what to do next.
//!
//! # Event-Sourced Reducer Architecture
//!
//! Ralph's pipeline follows an event-sourced reducer pattern:
//!
//! ```text
//! State → Orchestrate → Effect → Handle → Event → Reduce → State'
//! ```
//!
//! ## The Core Contract
//!
//! | Component | Pure? | Responsibility |
//! |-----------|-------|----------------|
//! | **Orchestration** | ✓ | Derives next effect from state only |
//! | **Handler** | ✗ | Executes effect, emits events describing outcome |
//! | **Reducer** | ✓ | Decides new state based on event |
//!
//! **Events are facts, not commands:**
//!
//! ```rust,ignore
//! // ✓ GOOD - Describes what happened (fact)
//! AgentEvent::InvocationFailed { retriable: true, error }
//!
//! // ✗ BAD - Commands what to do next (decision)
//! AgentEvent::RetryAgent { with_backoff: true }
//! ```
//!
//! **The handler reports, the reducer decides:**
//!
//! - Handler: "Agent invocation failed (retriable)"
//! - Reducer: "Increment retry count, stay in same phase"
//! - Orchestration: "Retry count < max? Emit InvokeAgent effect"
//!
//! # Event Categories
//!
//! Events are organized into logical categories for type-safe routing to
//! category-specific reducers. Each category has a dedicated enum:
//!
//! - [`LifecycleEvent`] - Pipeline start/stop/resume
//! - [`PlanningEvent`] - Plan generation events
//! - [`DevelopmentEvent`] - Development iteration and continuation events
//! - [`ReviewEvent`] - Review pass and fix attempt events
//! - [`AgentEvent`] - Agent invocation and chain management events
//! - [`RebaseEvent`] - Git rebase operation events
//! - [`CommitEvent`] - Commit generation events
//! - [`AwaitingDevFixEvent`] - Dev-fix flow events
//! - [`PromptInputEvent`] - Prompt materialization events
//! - [`ErrorEvent`] - Typed error events from handlers
//!
//! The main [`PipelineEvent`] enum wraps these category enums to enable
//! type-safe dispatch in the reducer.
//!
//! # Why This File Is Large (514 lines)
//!
//! This file exceeds the 500-line recommended limit (currently 514 lines) but is an acceptable
//! exception to the 300-line guideline because it's a **comprehensive enum module** with 10+ event
//! category types that must remain together for:
//! - Type-safe event category dispatch in reducers
//! - Exhaustiveness checking across all event variants
//! - Single source of truth for the event vocabulary
//!
//! Splitting would break pattern matching and scatter the event contract across many files, which
//! would be significantly worse for maintainability than the current size. The tradeoff of exceeding
//! the 500-line limit is justified by the cohesion and type-safety benefits.
//!
//! # Module Organization
//!
//! - [`types`] - Core event type definitions (all event enums)
//! - [`constructors`] - Convenience constructors for building events
//! - `development` - DevelopmentEvent and constructors
//! - `review` - ReviewEvent and constructors
//! - `agent` - AgentEvent and constructors
//! - `error` - ErrorEvent and error types
//!
//! # Example: Handler Emitting Events
//!
//! ```rust,ignore
//! use ralph_workflow::reducer::event::{PipelineEvent, AgentEvent};
//! use ralph_workflow::reducer::effect::EffectResult;
//!
//! fn handle_invoke_agent(ctx: &mut PhaseContext) -> Result<EffectResult> {
//!     match invoke_agent_process(ctx) {
//!         Ok(output) => {
//!             // Report fact: invocation succeeded
//!             Ok(EffectResult::event(PipelineEvent::Agent(
//!                 AgentEvent::InvocationSucceeded {
//!                     role: ctx.role,
//!                     output,
//!                 }
//!             )))
//!         }
//!         Err(e) if is_retriable(&e) => {
//!             // Report fact: invocation failed (retriable)
//!             Ok(EffectResult::event(PipelineEvent::Agent(
//!                 AgentEvent::InvocationFailed {
//!                     role: ctx.role,
//!                     error: e.to_string(),
//!                     retriable: true,
//!                 }
//!             )))
//!         }
//!         Err(e) => {
//!             // Report fact: invocation failed (not retriable)
//!             Ok(EffectResult::event(PipelineEvent::Agent(
//!                 AgentEvent::InvocationFailed {
//!                     role: ctx.role,
//!                     error: e.to_string(),
//!                     retriable: false,
//!                 }
//!             )))
//!         }
//!     }
//! }
//! ```
//!
//! # Example: Reducer Making Decisions
//!
//! ```rust,ignore
//! use ralph_workflow::reducer::event::{PipelineEvent, AgentEvent};
//! use ralph_workflow::reducer::state::PipelineState;
//!
//! fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
//!     match event {
//!         PipelineEvent::Agent(AgentEvent::InvocationFailed { retriable, .. }) => {
//!             if retriable && state.retry_count < state.max_retries {
//!                 // Decision: retry same agent
//!                 PipelineState {
//!                     retry_count: state.retry_count + 1,
//!                     ..state
//!                 }
//!             } else if retriable {
//!                 // Decision: switch to next agent in chain
//!                 PipelineState {
//!                     agent_chain_index: state.agent_chain_index + 1,
//!                     retry_count: 0,
//!                     ..state
//!                 }
//!             } else {
//!                 // Decision: non-retriable failure, transition to AwaitingDevFix
//!                 PipelineState {
//!                     phase: PipelinePhase::AwaitingDevFix,
//!                     ..state
//!                 }
//!             }
//!         }
//!         _ => state,
//!     }
//! }
//! ```
//!
//! # Frozen Policy
//!
//! Both [`LifecycleEvent`] and [`PipelineEvent`] are **FROZEN** - adding new variants
//! is prohibited. See their documentation for rationale and alternatives.
//!
//! # See Also
//!
//! - `docs/architecture/event-loop-and-reducers.md` - Detailed architecture doc
//! - `reducer::state_reduction` - Reducer implementations
//! - `reducer::orchestration` - Effect orchestration logic
//! - `reducer::handler` - Effect handler implementations

use crate::agents::AgentRole;
use crate::reducer::state::DevelopmentStatus;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// Type Definitions Module
// ============================================================================

#[path = "types.rs"]
mod types;

// Re-export all type definitions
pub use types::{
    AgentErrorKind, AwaitingDevFixEvent, CheckpointTrigger, CommitEvent, ConflictStrategy,
    LifecycleEvent, MaterializedPromptInput, PlanningEvent, PromptInputEvent, PromptInputKind,
    RebaseEvent, RebasePhase,
};

// ============================================================================
// Category Event Modules
// ============================================================================

#[path = "development.rs"]
mod development;
pub use development::DevelopmentEvent;

#[path = "review.rs"]
mod review;
pub use review::ReviewEvent;

#[path = "agent.rs"]
mod agent;
pub use agent::AgentEvent;

#[path = "error.rs"]
mod error;
pub use error::ErrorEvent;
pub use error::WorkspaceIoErrorKind;

// ============================================================================
// Constructor Module
// ============================================================================

#[path = "constructors.rs"]
mod constructors;

// ============================================================================
// Main Event Enum and Supporting Types
// ============================================================================

/// Pipeline phases for checkpoint tracking.
///
/// These phases represent the major stages of the Ralph pipeline.
/// Reducers transition between phases based on events.
///
/// # Phase Transitions
///
/// ```text
/// Planning → Development → Review → CommitMessage → FinalValidation → Finalizing → Complete
///              ↓             ↓            ↓
///         AwaitingDevFix → Interrupted
/// ```
///
/// # Phase Descriptions
///
/// - **Planning**: Generate implementation plan for the iteration
/// - **Development**: Execute plan, write code
/// - **Review**: Review code changes, identify issues
/// - **CommitMessage**: Generate commit message
/// - **FinalValidation**: Final checks before completion
/// - **Finalizing**: Cleanup operations (restore permissions, etc.)
/// - **Complete**: Pipeline completed successfully
/// - **AwaitingDevFix**: Terminal failure occurred, dev agent diagnosing
/// - **Interrupted**: Pipeline terminated (success or failure)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelinePhase {
    Planning,
    Development,
    Review,
    CommitMessage,
    FinalValidation,
    /// Finalizing phase for cleanup operations before completion.
    ///
    /// This phase handles:
    /// - Restoring PROMPT.md write permissions
    /// - Any other cleanup that must go through the effect system
    Finalizing,
    Complete,
    /// Awaiting development agent to fix pipeline failure.
    ///
    /// This phase occurs when the pipeline encounters a terminal failure condition
    /// (e.g., agent chain exhausted) but before transitioning to Interrupted. It
    /// signals that the development agent should be invoked to diagnose and fix
    /// the failure root cause.
    ///
    /// ## Failure Handling Flow
    ///
    /// 1. ErrorEvent::AgentChainExhausted occurs in any phase
    /// 2. Reducer transitions state to AwaitingDevFix
    /// 3. Orchestration determines Effect::TriggerDevFixFlow
    /// 4. Handler executes TriggerDevFixFlow:
    ///    a. Writes completion marker to .agent/tmp/completion_marker (failure status)
    ///    b. Emits DevFixTriggered event
    ///    c. Dispatches dev-fix agent
    ///    d. Emits DevFixCompleted event
    ///    e. Emits CompletionMarkerEmitted event
    /// 5. DevFixTriggered/DevFixCompleted events: no state change (stays in AwaitingDevFix)
    /// 6. CompletionMarkerEmitted event: transitions to Interrupted
    /// 7. Orchestration determines Effect::SaveCheckpoint for Interrupted
    /// 8. Handler saves checkpoint, increments checkpoint_saved_count
    /// 9. Event loop recognizes is_complete() == true and exits successfully
    ///
    /// ## Event Loop Termination Guarantees
    ///
    /// The event loop MUST NOT exit with completed=false when in AwaitingDevFix phase.
    /// The failure handling flow is designed to always complete with:
    /// - Completion marker written to filesystem
    /// - State transitioned to Interrupted
    /// - Checkpoint saved (checkpoint_saved_count > 0)
    /// - Event loop returning completed=true
    ///
    /// If the event loop exits with completed=false from AwaitingDevFix, this indicates
    /// a critical bug (e.g., max iterations reached before checkpoint saved).
    ///
    /// ## Completion Marker Requirement
    ///
    /// The completion marker MUST be written before transitioning to Interrupted.
    /// This ensures external orchestration systems (CI, monitoring) can detect
    /// pipeline termination even if the event loop exits unexpectedly.
    ///
    /// ## Agent Chain Exhaustion Handling
    ///
    /// When in AwaitingDevFix phase with an exhausted agent chain, orchestration
    /// falls through to phase-specific logic (TriggerDevFixFlow) instead of reporting
    /// exhaustion again. This prevents infinite loops where exhaustion is reported
    /// repeatedly.
    ///
    /// Transitions:
    /// - From: Any phase where AgentChainExhausted error occurs
    /// - To: Interrupted (after dev-fix attempt completes or fails)
    AwaitingDevFix,
    Interrupted,
}

impl std::fmt::Display for PipelinePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "Planning"),
            Self::Development => write!(f, "Development"),
            Self::Review => write!(f, "Review"),
            Self::CommitMessage => write!(f, "Commit Message"),
            Self::FinalValidation => write!(f, "Final Validation"),
            Self::Finalizing => write!(f, "Finalizing"),
            Self::Complete => write!(f, "Complete"),
            Self::AwaitingDevFix => write!(f, "Awaiting Dev Fix"),
            Self::Interrupted => write!(f, "Interrupted"),
        }
    }
}

/// Pipeline events representing all state transitions.
///
/// Events are organized into logical categories for type-safe routing
/// to category-specific reducers. Each category has a dedicated inner enum.
///
/// # Event Categories
///
/// - `Lifecycle` - Pipeline start/stop/resume
/// - `Planning` - Plan generation events
/// - `Development` - Development iteration and continuation events
/// - `Review` - Review pass and fix attempt events
/// - `Agent` - Agent invocation and chain management events
/// - `Rebase` - Git rebase operation events
/// - `Commit` - Commit generation events
/// - Miscellaneous events (context cleanup, checkpoints, finalization)
///
/// # Example
///
/// ```rust,ignore
/// // Type-safe event construction
/// let event = PipelineEvent::Agent(AgentEvent::InvocationStarted {
///     role: AgentRole::Developer,
///     agent: "claude".to_string(),
///     model: Some("opus".to_string()),
/// });
///
/// // Pattern matching routes to category handlers
/// match event {
///     PipelineEvent::Agent(agent_event) => reduce_agent_event(state, agent_event),
///     // ...
/// }
/// ```
///
/// # ⚠️ FROZEN - DO NOT ADD VARIANTS ⚠️
///
/// This enum is **FROZEN**. Adding new top-level variants is **PROHIBITED**.
///
/// ## Why is this frozen?
///
/// `PipelineEvent` provides category-based event routing to the reducer. The existing
/// categories (Lifecycle, Planning, Development, Review, etc.) cover all pipeline phases.
/// Adding new top-level variants would indicate a missing architectural abstraction or
/// an attempt to bypass phase-specific event handling.
///
/// ## What to do instead
///
/// 1. **Express events through existing categories** - Use the category enums:
///    - `PlanningEvent` for planning phase observations
///    - `DevelopmentEvent` for development phase observations
///    - `ReviewEvent` for review phase observations
///    - `CommitEvent` for commit generation observations
///    - `AgentEvent` for agent invocation observations
///    - `RebaseEvent` for rebase state machine transitions
///
/// 2. **Return errors for unrecoverable failures** - Don't create events for conditions
///    that should terminate the pipeline. Return `Err` from the effect handler instead.
///
/// 3. **Extend category enums if needed** - If you truly need a new event within an
///    existing phase, add it to that phase's category enum (e.g., add a new variant to
///    `ReviewEvent` rather than creating a new top-level category).
///
/// ## Enforcement
///
/// The freeze policy is enforced by the `pipeline_event_is_frozen` test in this module,
/// which will fail to compile if new variants are added. This is intentional.
///
/// See `LifecycleEvent` documentation for additional context on the freeze policy rationale.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PipelineEvent {
    /// Pipeline lifecycle events (start, stop, resume).
    Lifecycle(LifecycleEvent),
    /// Planning phase events.
    Planning(PlanningEvent),
    /// Development phase events.
    Development(DevelopmentEvent),
    /// Review phase events.
    Review(ReviewEvent),
    /// Prompt input materialization events.
    PromptInput(PromptInputEvent),
    /// Agent invocation and chain events.
    Agent(AgentEvent),
    /// Rebase operation events.
    Rebase(RebaseEvent),
    /// Commit generation events.
    Commit(CommitEvent),
    /// AwaitingDevFix phase events.
    AwaitingDevFix(AwaitingDevFixEvent),

    // ========================================================================
    // Miscellaneous events that don't fit a category
    // ========================================================================
    /// Context cleanup completed.
    ContextCleaned,
    /// Checkpoint saved.
    CheckpointSaved {
        /// What triggered the checkpoint save.
        trigger: CheckpointTrigger,
    },
    /// Finalization phase started.
    FinalizingStarted,
    /// PROMPT.md permissions restored.
    PromptPermissionsRestored,
    /// Loop recovery triggered (tight loop detected and broken).
    LoopRecoveryTriggered {
        /// String representation of the detected loop.
        detected_loop: String,
        /// Number of times the loop was repeated.
        loop_count: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_phase_display() {
        assert_eq!(format!("{}", PipelinePhase::Planning), "Planning");
        assert_eq!(format!("{}", PipelinePhase::Development), "Development");
        assert_eq!(format!("{}", PipelinePhase::Review), "Review");
        assert_eq!(
            format!("{}", PipelinePhase::CommitMessage),
            "Commit Message"
        );
        assert_eq!(
            format!("{}", PipelinePhase::FinalValidation),
            "Final Validation"
        );
        assert_eq!(format!("{}", PipelinePhase::Finalizing), "Finalizing");
        assert_eq!(format!("{}", PipelinePhase::Complete), "Complete");
        assert_eq!(
            format!("{}", PipelinePhase::AwaitingDevFix),
            "Awaiting Dev Fix"
        );
        assert_eq!(format!("{}", PipelinePhase::Interrupted), "Interrupted");
    }

    /// This test enforces the FROZEN policy on LifecycleEvent.
    ///
    /// If you're here because this test failed to compile after adding
    /// a variant, you are violating the freeze policy. See the FROZEN
    /// comment on LifecycleEvent for alternatives.
    #[test]
    fn lifecycle_event_is_frozen() {
        fn exhaustive_match(e: LifecycleEvent) -> &'static str {
            match e {
                LifecycleEvent::Started => "started",
                LifecycleEvent::Resumed { .. } => "resumed",
                LifecycleEvent::Completed => "completed",
                LifecycleEvent::GitignoreEntriesEnsured { .. } => "gitignore_ensured",
                // DO NOT ADD _ WILDCARD - intentionally exhaustive
            }
        }
        // Just needs to compile; actual call proves exhaustiveness
        let _ = exhaustive_match(LifecycleEvent::Started);
    }

    /// This test enforces the FROZEN policy on PipelineEvent.
    ///
    /// If you're here because this test failed to compile after adding
    /// a variant, you are violating the freeze policy. See the FROZEN
    /// comment on PipelineEvent for alternatives.
    #[test]
    fn pipeline_event_is_frozen() {
        fn exhaustive_match(e: PipelineEvent) -> &'static str {
            match e {
                PipelineEvent::Lifecycle(_) => "lifecycle",
                PipelineEvent::Planning(_) => "planning",
                PipelineEvent::Development(_) => "development",
                PipelineEvent::Review(_) => "review",
                PipelineEvent::PromptInput(_) => "prompt_input",
                PipelineEvent::Agent(_) => "agent",
                PipelineEvent::Rebase(_) => "rebase",
                PipelineEvent::Commit(_) => "commit",
                PipelineEvent::AwaitingDevFix(_) => "awaiting_dev_fix",
                PipelineEvent::ContextCleaned => "context_cleaned",
                PipelineEvent::CheckpointSaved { .. } => "checkpoint_saved",
                PipelineEvent::FinalizingStarted => "finalizing_started",
                PipelineEvent::PromptPermissionsRestored => "prompt_permissions_restored",
                PipelineEvent::LoopRecoveryTriggered { .. } => "loop_recovery_triggered",
                // DO NOT ADD _ WILDCARD - intentionally exhaustive
            }
        }
        let _ = exhaustive_match(PipelineEvent::ContextCleaned);
    }
}
