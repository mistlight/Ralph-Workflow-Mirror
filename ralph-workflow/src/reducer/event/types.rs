//! Core event type definitions for the reducer architecture.
//!
//! This module contains all event enum definitions organized by category.
//! Each event represents a fact about what happened during pipeline execution.
//!
//! # Event Architecture
//!
//! Events follow the reducer architecture contract:
//! - **Events are facts** (past-tense, descriptive)
//! - **Events carry data** needed for reducer decisions
//! - **Handlers emit events**, reducers decide what to do next
//!
//! # Event Categories
//!
//! Events are organized into logical categories for type-safe routing:
//! - [`LifecycleEvent`] - Pipeline start/stop/resume
//! - [`PlanningEvent`] - Plan generation events
//! - [`PromptInputEvent`] - Prompt input materialization
//! - [`RebaseEvent`] - Git rebase operations
//! - [`CommitEvent`] - Commit generation
//! - [`AwaitingDevFixEvent`] - Dev-fix flow events
//!
//! Other categories are defined in separate files:
//! - `DevelopmentEvent` in `development.rs`
//! - `ReviewEvent` in `review.rs`
//! - `AgentEvent` in `agent.rs`
//! - `ErrorEvent` in `error.rs`

use crate::agents::AgentRole;
use serde::{Deserialize, Serialize};

// Re-export common types used in events
pub use std::path::PathBuf;

// Re-export types from state module that are used in events
pub use crate::reducer::state::{MaterializedPromptInput, PromptInputKind};

use super::{ErrorEvent, PipelinePhase};

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
/// The freeze policy is enforced by the `lifecycle_event_is_frozen` test in the parent module,
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
///
/// # Emitted By
///
/// - Rebase handlers in `handler/rebase.rs`
/// - Git integration layer
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RebaseEvent {
    /// Rebase operation started.
    ///
    /// Emitted when a rebase begins. The reducer uses this to:
    /// - Track which rebase phase is active (initial or post-review)
    /// - Record the target branch for observability
    Started {
        /// The rebase phase (initial or post-review).
        phase: RebasePhase,
        /// The target branch to rebase onto.
        target_branch: String,
    },
    /// Merge conflict detected during rebase.
    ///
    /// Emitted when git detects merge conflicts. The handler will attempt
    /// automated resolution; the reducer tracks which files are conflicted.
    ConflictDetected {
        /// The files with conflicts.
        files: Vec<PathBuf>,
    },
    /// Merge conflicts were resolved.
    ///
    /// Emitted after successful conflict resolution. The reducer uses this
    /// to clear the conflict state and allow rebase to continue.
    ConflictResolved {
        /// The files that were resolved.
        files: Vec<PathBuf>,
    },
    /// Rebase completed successfully.
    ///
    /// Emitted when rebase finishes without errors. The reducer uses this to:
    /// - Mark rebase as complete
    /// - Record the new HEAD commit
    /// - Transition to the next pipeline phase
    Succeeded {
        /// The rebase phase that completed.
        phase: RebasePhase,
        /// The new HEAD after rebase.
        new_head: String,
    },
    /// Rebase failed and was reset.
    ///
    /// Emitted when rebase encounters an unrecoverable error. The reducer
    /// uses this to decide whether to retry or abort the pipeline.
    Failed {
        /// The rebase phase that failed.
        phase: RebasePhase,
        /// The reason for failure.
        reason: String,
    },
    /// Rebase was aborted and state restored.
    ///
    /// Emitted when rebase is explicitly aborted (e.g., user interrupt).
    /// The reducer marks rebase as not attempted.
    Aborted {
        /// The rebase phase that was aborted.
        phase: RebasePhase,
        /// The commit that was restored.
        restored_to: String,
    },
    /// Rebase was skipped (e.g., already up to date).
    ///
    /// Emitted when rebase is unnecessary. The reducer marks rebase as
    /// complete without actually performing the operation.
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
///
/// # Emitted By
///
/// - Commit generation handlers in `handler/commit/`
/// - Commit message validation handlers
/// - Git commit handlers
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CommitEvent {
    /// Commit message generation started.
    GenerationStarted,
    /// Commit diff computed for commit generation.
    ///
    /// Emitted after preparing the diff that will be committed. The reducer
    /// uses the `empty` flag to decide whether to skip commit creation.
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
///
/// # When This Occurs
///
/// The AwaitingDevFix phase is entered when the pipeline encounters a terminal
/// failure condition (e.g., agent chain exhausted) in any phase. Instead of
/// immediately terminating, the pipeline gives the development agent one final
/// chance to diagnose and fix the issue.
///
/// # State Flow
///
/// 1. Terminal failure detected (e.g., AgentChainExhausted)
/// 2. Reducer transitions to AwaitingDevFix phase
/// 3. DevFixTriggered event emitted
/// 4. Development agent invoked with failure context
/// 5. DevFixCompleted event emitted
/// 6. CompletionMarkerEmitted event signals transition to Interrupted
/// 7. Checkpoint saved
/// 8. Pipeline exits
///
/// # Emitted By
///
/// - Dev-fix flow handlers in `handler/dev_fix/`
/// - Completion marker handlers
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum AwaitingDevFixEvent {
    /// Dev-fix flow was triggered.
    ///
    /// Emitted when entering the dev-fix phase. Records which phase and agent
    /// failed, providing context for the development agent.
    DevFixTriggered {
        /// Phase where the failure occurred.
        failed_phase: PipelinePhase,
        /// Agent role that failed.
        failed_role: AgentRole,
    },
    /// Dev-fix flow was skipped (not yet implemented or disabled).
    DevFixSkipped {
        /// Reason for skipping.
        reason: String,
    },
    /// Dev-fix flow completed (may or may not have fixed the issue).
    ///
    /// Emitted after the development agent finishes its fix attempt.
    /// The `success` field indicates whether the agent believes it fixed
    /// the issue, but does not guarantee the pipeline will succeed on retry.
    DevFixCompleted {
        /// Whether the fix attempt succeeded.
        success: bool,
        /// Optional summary of what was fixed.
        summary: Option<String>,
    },
    /// Dev-fix agent is unavailable (quota/usage limit).
    ///
    /// Emitted when the dev-fix agent cannot be invoked due to resource limits.
    /// The pipeline will proceed to termination without a fix attempt.
    DevFixAgentUnavailable {
        /// Phase where the failure occurred.
        failed_phase: PipelinePhase,
        /// Reason for unavailability.
        reason: String,
    },
    /// Completion marker was emitted to filesystem.
    ///
    /// Emitted after writing the completion marker to `.agent/tmp/completion_marker`.
    /// The reducer uses this event to transition from AwaitingDevFix to Interrupted,
    /// enabling the pipeline to complete gracefully.
    CompletionMarkerEmitted {
        /// Whether this is a failure completion (true) or success (false).
        is_failure: bool,
    },
}

/// Rebase phase (initial or post-review).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebasePhase {
    /// Initial rebase before development starts.
    Initial,
    /// Post-review rebase after review fixes.
    PostReview,
}

/// Checkpoint save trigger.
///
/// Records what caused a checkpoint to be saved, enabling analysis of
/// checkpoint patterns and frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointTrigger {
    /// Checkpoint saved during phase transition.
    PhaseTransition,
    /// Checkpoint saved after iteration completion.
    IterationComplete,
    /// Checkpoint saved before risky operation (rebase).
    BeforeRebase,
    /// Checkpoint saved due to interrupt signal.
    Interrupt,
}

/// Error kind for agent failures.
///
/// Classifies agent invocation failures to enable retry/fallback decisions in the reducer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentErrorKind {
    /// Network connectivity failure.
    Network,
    /// Authentication or authorization failure.
    Authentication,
    /// Rate limiting or quota exceeded.
    RateLimit,
    /// Request timeout.
    Timeout,
    /// Internal server error from agent API.
    InternalError,
    /// Requested model is unavailable.
    ModelUnavailable,
    /// Output parsing or validation error.
    ParsingError,
    /// Filesystem error during agent invocation.
    FileSystem,
}

/// Conflict resolution strategy.
///
/// Determines how the pipeline should handle merge conflicts during rebase operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Abort the rebase and restore original state.
    Abort,
    /// Continue rebase after conflict resolution.
    Continue,
    /// Skip the conflicting commit.
    Skip,
}
