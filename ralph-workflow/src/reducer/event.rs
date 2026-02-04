//! Pipeline event types for reducer architecture.
//!
//! Defines all possible events that can occur during pipeline execution.
//! Each event represents a state transition that the reducer handles.
//!
//! # Event Categories
//!
//! Events are organized into logical categories for type-safe routing to
//! category-specific reducers. Each category has a dedicated inner enum:
//!
//! - [`LifecycleEvent`] - Pipeline start/stop/resume
//! - [`PlanningEvent`] - Plan generation events
//! - [`DevelopmentEvent`] - Development iteration and continuation events
//! - [`ReviewEvent`] - Review pass and fix attempt events
//! - [`AgentEvent`] - Agent invocation and chain management events
//! - [`RebaseEvent`] - Git rebase operation events
//! - [`CommitEvent`] - Commit generation events
//!
//! The main [`PipelineEvent`] enum wraps these category enums to enable
//! type-safe dispatch in the reducer.

use crate::agents::AgentRole;
use crate::reducer::state::{DevelopmentStatus, MaterializedPromptInput, PromptInputKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// Event Category Enums
// ============================================================================

/// Pipeline lifecycle events (start, stop, resume).
///
/// These events control the overall pipeline execution lifecycle,
/// distinct from phase-specific transitions. Use these for:
///
/// - Starting or resuming a pipeline run
/// - Completing a successful pipeline execution
///
/// # When to Use
///
/// - `Started`: When a fresh pipeline run begins
/// - `Resumed`: When resuming from a checkpoint
/// - `Completed`: When all phases complete successfully
///
/// # ⚠️ FROZEN - DO NOT ADD VARIANTS ⚠️
///
/// This enum is **FROZEN**. Adding new variants is **PROHIBITED**.
///
/// ## Why is this frozen?
///
/// Lifecycle events control pipeline flow (start/stop/completion). Allowing effect
/// handlers to emit new lifecycle events would violate the core architectural principle:
/// **handlers describe what happened; reducers decide what happens next.**
///
/// ## What to do instead
///
/// If you need to express new observations or failures:
///
/// 1. **Reuse existing phase/category events** - Use `PlanningEvent`, `DevelopmentEvent`,
///    `ReviewEvent`, `CommitEvent`, etc. to describe what happened within that phase.
///    Example: `PlanningEvent::PlanXmlMissing` instead of creating a generic "Aborted" event.
///
/// 2. **Return errors from the event loop** - For truly unrecoverable failures (permission
///    errors, invariant violations), return `Err` from the effect handler. The outer runner
///    will handle termination, not the reducer.
///
/// 3. **Handle in orchestration** - Some conditions don't need events at all and can be
///    handled in the effect handler or runner logic.
///
/// ## Enforcement
///
/// The freeze policy is enforced by the `lifecycle_event_is_frozen` test in this module,
/// which will fail to compile if new variants are added. This is intentional.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum LifecycleEvent {
    /// Pipeline execution started fresh (not from checkpoint).
    Started,
    /// Pipeline execution resumed from a previous state.
    Resumed {
        /// Whether this resume is from a persisted checkpoint.
        from_checkpoint: bool,
    },
    /// Pipeline execution completed successfully.
    Completed,
}

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

/// Prompt input oversize detection and materialization events.
///
/// These events make reducer-visible any transformation that affects the
/// agent-visible prompt content (inline vs file reference, truncation, etc.).
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PromptInputEvent {
    OversizeDetected {
        phase: PipelinePhase,
        kind: PromptInputKind,
        content_id_sha256: String,
        size_bytes: u64,
        limit_bytes: u64,
        policy: String,
    },
    PlanningInputsMaterialized {
        iteration: u32,
        prompt: MaterializedPromptInput,
    },
    DevelopmentInputsMaterialized {
        iteration: u32,
        prompt: MaterializedPromptInput,
        plan: MaterializedPromptInput,
    },
    ReviewInputsMaterialized {
        pass: u32,
        plan: MaterializedPromptInput,
        diff: MaterializedPromptInput,
    },
    CommitInputsMaterialized {
        attempt: u32,
        diff: MaterializedPromptInput,
    },
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

#[path = "event/development.rs"]
mod development;
pub use development::DevelopmentEvent;

#[path = "event/review.rs"]
mod review;
pub use review::ReviewEvent;

#[path = "event/agent.rs"]
mod agent;
pub use agent::AgentEvent;

#[path = "event/error.rs"]
mod error;
pub use error::ErrorEvent;
pub use error::WorkspaceIoErrorKind;

/// Rebase operation events.
///
/// Events related to git rebase operations including conflict detection
/// and resolution. Rebase operations can occur at multiple points in the
/// pipeline (initial and post-review).
///
/// # State Machine
///
/// ```text
/// NotStarted -> InProgress -> Conflicted -> InProgress -> Completed
///                    |                           |
///                    +---------> Skipped <-------+
///                    |
///                    +---------> Failed (resets to NotStarted)
/// ```
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RebaseEvent {
    /// Rebase operation started.
    Started {
        /// The rebase phase (initial or post-review).
        phase: RebasePhase,
        /// The target branch to rebase onto.
        target_branch: String,
    },
    /// Merge conflict detected during rebase.
    ConflictDetected {
        /// The files with conflicts.
        files: Vec<PathBuf>,
    },
    /// Merge conflicts were resolved.
    ConflictResolved {
        /// The files that were resolved.
        files: Vec<PathBuf>,
    },
    /// Rebase completed successfully.
    Succeeded {
        /// The rebase phase that completed.
        phase: RebasePhase,
        /// The new HEAD after rebase.
        new_head: String,
    },
    /// Rebase failed and was reset.
    Failed {
        /// The rebase phase that failed.
        phase: RebasePhase,
        /// The reason for failure.
        reason: String,
    },
    /// Rebase was aborted and state restored.
    Aborted {
        /// The rebase phase that was aborted.
        phase: RebasePhase,
        /// The commit that was restored.
        restored_to: String,
    },
    /// Rebase was skipped (e.g., already up to date).
    Skipped {
        /// The rebase phase that was skipped.
        phase: RebasePhase,
        /// The reason for skipping.
        reason: String,
    },
}

/// Commit generation events.
///
/// Events related to commit message generation, validation, and creation.
/// Commit generation occurs after development iterations and review fixes.
///
/// # State Machine
///
/// ```text
/// NotStarted -> Generating -> Generated -> Committed
///                    |              |
///                    +--> (retry) --+
///                    |
///                    +--> Skipped
/// ```
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CommitEvent {
    /// Commit message generation started.
    GenerationStarted,
    /// Commit diff computed for commit generation.
    DiffPrepared {
        /// True when the diff is empty.
        empty: bool,
        /// Content identifier (sha256 hex) of the prepared diff content.
        ///
        /// This is used to guard against reusing stale materialized inputs when the
        /// diff content changes across checkpoints or retries.
        content_id_sha256: String,
    },
    /// Commit diff computation failed.
    DiffFailed {
        /// The error message for the diff failure.
        error: String,
    },
    /// Commit diff is no longer available and must be recomputed.
    ///
    /// This is used for recoverability when `.agent/tmp` artifacts are cleaned between
    /// checkpoints or when required diff files go missing during resume.
    DiffInvalidated {
        /// Reason for invalidation.
        reason: String,
    },
    /// Commit prompt prepared for a commit attempt.
    PromptPrepared {
        /// The attempt number.
        attempt: u32,
    },
    /// Commit agent invoked for a commit attempt.
    AgentInvoked {
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message XML extracted for a commit attempt.
    CommitXmlExtracted {
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message XML missing for a commit attempt.
    CommitXmlMissing {
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message XML validated successfully.
    CommitXmlValidated {
        /// The generated commit message.
        message: String,
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message XML validation failed.
    CommitXmlValidationFailed {
        /// The reason for validation failure.
        reason: String,
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message XML archived.
    CommitXmlArchived {
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message XML cleaned before invoking the commit agent.
    CommitXmlCleaned {
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message was generated.
    MessageGenerated {
        /// The generated commit message.
        message: String,
        /// The attempt number.
        attempt: u32,
    },
    /// Commit message validation failed.
    MessageValidationFailed {
        /// The reason for validation failure.
        reason: String,
        /// The attempt number that failed.
        attempt: u32,
    },
    /// Commit was created successfully.
    Created {
        /// The commit hash.
        hash: String,
        /// The commit message used.
        message: String,
    },
    /// Commit generation failed completely.
    GenerationFailed {
        /// The reason for failure.
        reason: String,
    },
    /// Commit was skipped (e.g., no changes to commit).
    Skipped {
        /// The reason for skipping.
        reason: String,
    },
}

/// Events for AwaitingDevFix phase.
///
/// This phase handles pipeline failure remediation by invoking the development
/// agent to diagnose and fix the root cause before termination.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum AwaitingDevFixEvent {
    /// Dev-fix flow was triggered.
    DevFixTriggered {
        failed_phase: PipelinePhase,
        failed_role: AgentRole,
    },
    /// Dev-fix flow was skipped (not yet implemented or disabled).
    DevFixSkipped { reason: String },
    /// Dev-fix flow completed (may or may not have fixed the issue).
    DevFixCompleted {
        success: bool,
        summary: Option<String>,
    },
    /// Completion marker was emitted to filesystem.
    CompletionMarkerEmitted { is_failure: bool },
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Pipeline phases for checkpoint tracking.
///
/// These phases represent the major stages of the Ralph pipeline.
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
/// ```ignore
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
}

// ============================================================================
// Convenience Constructors
// ============================================================================

#[path = "event/constructors.rs"]
mod constructors;

/// Rebase phase (initial or post-review).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebasePhase {
    Initial,
    PostReview,
}

/// Error kind for agent failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentErrorKind {
    Network,
    Authentication,
    RateLimit,
    Timeout,
    InternalError,
    ModelUnavailable,
    ParsingError,
    FileSystem,
}

/// Conflict resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    Abort,
    Continue,
    Skip,
}

/// Checkpoint save trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointTrigger {
    PhaseTransition,
    IterationComplete,
    BeforeRebase,
    Interrupt,
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
                // DO NOT ADD _ WILDCARD - intentionally exhaustive
            }
        }
        let _ = exhaustive_match(PipelineEvent::ContextCleaned);
    }
}
