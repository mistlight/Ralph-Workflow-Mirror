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
//! - [`LifecycleEvent`] - Pipeline start/stop/resume/abort
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
use crate::reducer::state::DevelopmentStatus;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// Event Category Enums
// ============================================================================

/// Pipeline lifecycle events (start, stop, resume, abort).
///
/// These events control the overall pipeline execution lifecycle,
/// distinct from phase-specific transitions. Use these for:
///
/// - Starting or resuming a pipeline run
/// - Completing a successful pipeline execution
/// - Aborting due to unrecoverable errors
///
/// # When to Use
///
/// - `Started`: When a fresh pipeline run begins
/// - `Resumed`: When resuming from a checkpoint
/// - `Completed`: When all phases complete successfully
/// - `Aborted`: When an unrecoverable error occurs
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
    /// Pipeline execution aborted due to an error.
    Aborted {
        /// The reason for aborting.
        reason: String,
    },
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
    /// Plan generation started for an iteration.
    GenerationStarted {
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

/// Development phase events.
///
/// Events related to development iterations, including continuation handling
/// for partial/failed completion states. Development iterations involve
/// invoking developer agents to make code changes.
///
/// # State Transitions
///
/// - `PhaseStarted`: Sets phase to Development
/// - `IterationStarted`: Resets agent chain, clears continuation state
/// - `IterationCompleted(output_valid=true)`: Transitions to CommitMessage
/// - `IterationCompleted(output_valid=false)`: Stays in Development for retry
/// - `ContinuationTriggered`: Saves context for continuation attempt
/// - `ContinuationSucceeded`: Clears continuation, proceeds to CommitMessage
/// - `PhaseCompleted`: Transitions to Review
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DevelopmentEvent {
    /// Development phase has started.
    PhaseStarted,
    /// A development iteration has started.
    IterationStarted {
        /// The iteration number starting.
        iteration: u32,
    },
    /// A development iteration completed with validation result.
    IterationCompleted {
        /// The iteration number that completed.
        iteration: u32,
        /// Whether the output passed validation.
        output_valid: bool,
    },
    /// Development phase completed, all iterations done.
    PhaseCompleted,
    /// Continuation triggered due to partial/failed status.
    ///
    /// Emitted only when development output is valid (parseable) but
    /// status is not "completed" (i.e., "partial" or "failed").
    ContinuationTriggered {
        /// Current iteration number.
        iteration: u32,
        /// Status from the agent ("partial" or "failed").
        status: DevelopmentStatus,
        /// Summary of what was accomplished.
        summary: String,
        /// Files changed in this attempt.
        files_changed: Option<Vec<String>>,
        /// Agent's recommended next steps.
        next_steps: Option<String>,
    },
    /// Continuation attempt succeeded with status "completed".
    ContinuationSucceeded {
        /// Current iteration number.
        iteration: u32,
        /// Number of continuation attempts it took.
        total_continuation_attempts: u32,
    },
    /// Output validation failed (XSD/XML parsing error).
    ///
    /// Emitted when development output cannot be parsed or fails XSD validation.
    /// The reducer decides whether to retry (same agent) or switch agents based
    /// on the attempt count in state.
    OutputValidationFailed {
        /// Current iteration number.
        iteration: u32,
        /// Current invalid output attempt number.
        attempt: u32,
    },
    /// Continuation attempts exhausted without reaching completed status.
    ///
    /// Emitted when development iteration has used all allowed continuation
    /// attempts but still hasn't reached status="completed".
    ContinuationBudgetExhausted {
        /// Current iteration number.
        iteration: u32,
        /// Total continuation attempts made.
        total_attempts: u32,
        /// Last status received (Partial or Failed).
        last_status: DevelopmentStatus,
    },
    /// Continuation context file was written successfully.
    ///
    /// Emitted after WriteContinuationContext effect completes. The reducer
    /// clears the `needs_context_write` flag on this event.
    ContinuationContextWritten {
        /// Current iteration number.
        iteration: u32,
        /// Current continuation attempt number.
        attempt: u32,
    },
    /// Continuation context file was cleaned up.
    ///
    /// Emitted after CleanupContinuationContext effect completes.
    ContinuationContextCleaned,
}

/// Review phase events.
///
/// Events related to code review passes and fix attempts. The review phase
/// runs reviewer agents to identify issues and (by default) the same reviewer
/// agent chain to apply any required fixes.
///
/// # State Transitions
///
/// - `PhaseStarted`: Sets phase to Review, resets pass counter
/// - `PassStarted`: Resets agent chain for the pass
/// - `Completed(issues_found=false)`: Advances to next pass or CommitMessage
/// - `Completed(issues_found=true)`: Triggers fix attempt
/// - `FixAttemptCompleted`: Transitions to CommitMessage
/// - `PhaseCompleted`: Transitions to CommitMessage
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ReviewEvent {
    /// Review phase has started.
    PhaseStarted,
    /// A review pass has started.
    PassStarted {
        /// The pass number starting.
        pass: u32,
    },
    /// A review pass completed with results.
    Completed {
        /// The pass number that completed.
        pass: u32,
        /// Whether issues were found requiring fixes.
        issues_found: bool,
    },
    /// A fix attempt for issues has started.
    FixAttemptStarted {
        /// The pass number this fix is for.
        pass: u32,
    },
    /// A fix attempt completed.
    FixAttemptCompleted {
        /// The pass number this fix was for.
        pass: u32,
        /// Whether changes were made.
        changes_made: bool,
    },
    /// Review phase completed, all passes done.
    PhaseCompleted {
        /// Whether the phase exited early (before all passes).
        early_exit: bool,
    },
    /// Review pass found no issues - clean exit.
    ///
    /// Emitted when a review pass completes with no issues found.
    /// This is distinct from `Completed { issues_found: false }` in that
    /// it explicitly signals a clean pass for UI/logging purposes.
    PassCompletedClean {
        /// The pass number that completed.
        pass: u32,
    },
    /// Review output validation failed (XSD/XML parsing error).
    ///
    /// Emitted when review output cannot be parsed. Reducer decides
    /// whether to retry or switch agents.
    OutputValidationFailed {
        /// The pass number.
        pass: u32,
        /// Current invalid output attempt number.
        attempt: u32,
    },
}

/// Agent invocation and chain management events.
///
/// Events related to agent execution, fallback chains, model switching,
/// rate limiting, and retry cycles. The agent chain provides fault tolerance
/// through multiple fallback levels:
///
/// 1. Model level: Try different models for the same agent
/// 2. Agent level: Switch to a fallback agent
/// 3. Retry cycle: Start over with exponential backoff
///
/// # State Transitions
///
/// - `InvocationFailed(retriable=true)`: Advances to next model
/// - `InvocationFailed(retriable=false)`: Switches to next agent
/// - `RateLimitFallback`: Immediate agent switch with prompt preservation
/// - `ChainExhausted`: Starts new retry cycle
/// - `InvocationSucceeded`: Clears continuation prompt
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum AgentEvent {
    /// Agent invocation started.
    InvocationStarted {
        /// The role this agent is fulfilling.
        role: AgentRole,
        /// The agent being invoked.
        agent: String,
        /// The model being used, if specified.
        model: Option<String>,
    },
    /// Agent invocation succeeded.
    InvocationSucceeded {
        /// The role this agent fulfilled.
        role: AgentRole,
        /// The agent that succeeded.
        agent: String,
    },
    /// Agent invocation failed.
    InvocationFailed {
        /// The role this agent was fulfilling.
        role: AgentRole,
        /// The agent that failed.
        agent: String,
        /// The exit code from the agent process.
        exit_code: i32,
        /// The kind of error that occurred.
        error_kind: AgentErrorKind,
        /// Whether this error is retriable with the same agent.
        retriable: bool,
    },
    /// Fallback triggered to switch to a different agent.
    FallbackTriggered {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent being switched from.
        from_agent: String,
        /// The agent being switched to.
        to_agent: String,
    },
    /// Model fallback triggered within the same agent.
    ModelFallbackTriggered {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent whose model is changing.
        agent: String,
        /// The model being switched from.
        from_model: String,
        /// The model being switched to.
        to_model: String,
    },
    /// Retry cycle started (all agents exhausted, starting over).
    RetryCycleStarted {
        /// The role being retried.
        role: AgentRole,
        /// The cycle number starting.
        cycle: u32,
    },
    /// Agent chain exhausted (no more agents/models to try).
    ChainExhausted {
        /// The role whose chain is exhausted.
        role: AgentRole,
    },
    /// Agent chain initialized with available agents.
    ChainInitialized {
        /// The role this chain is for.
        role: AgentRole,
        /// The agents available in this chain.
        agents: Vec<String>,
        /// Maximum number of retry cycles allowed for this chain.
        max_cycles: u32,
        /// Base retry-cycle delay in milliseconds.
        retry_delay_ms: u64,
        /// Exponential backoff multiplier.
        backoff_multiplier: f64,
        /// Maximum backoff delay in milliseconds.
        max_backoff_ms: u64,
    },
    /// Agent hit rate limit (429) - should fallback immediately.
    ///
    /// Unlike other retriable errors (Network, Timeout), rate limits indicate
    /// the current provider is temporarily exhausted. Rather than waiting and
    /// retrying the same agent, we immediately switch to the next agent in the
    /// chain to continue work without delay.
    RateLimitFallback {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent that hit the rate limit.
        agent: String,
        /// The prompt that was being executed when rate limit was hit.
        /// This allows the next agent to continue the same work.
        prompt_context: Option<String>,
    },

    /// Session established with agent.
    ///
    /// Emitted when an agent response includes a session ID that can be
    /// used for XSD retry continuation. This enables reusing the same
    /// session when retrying due to validation failures.
    SessionEstablished {
        /// The role this agent is fulfilling.
        role: AgentRole,
        /// The agent name.
        agent: String,
        /// The session ID returned by the agent.
        session_id: String,
    },

    /// XSD validation failed for agent output.
    ///
    /// Emitted when agent output cannot be parsed or fails XSD validation.
    /// Distinct from OutputValidationFailed events in phase-specific enums,
    /// this is the canonical XSD retry trigger that the reducer uses to
    /// decide whether to retry with the same agent/session or advance the chain.
    XsdValidationFailed {
        /// The role whose output failed validation.
        role: AgentRole,
        /// The artifact type that failed validation.
        artifact: crate::reducer::state::ArtifactType,
        /// Error message from validation.
        error: String,
        /// Current XSD retry count for this artifact.
        retry_count: u32,
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
/// - `Lifecycle` - Pipeline start/stop/resume/abort
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
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PipelineEvent {
    /// Pipeline lifecycle events (start, stop, resume, abort).
    Lifecycle(LifecycleEvent),
    /// Planning phase events.
    Planning(PlanningEvent),
    /// Development phase events.
    Development(DevelopmentEvent),
    /// Review phase events.
    Review(ReviewEvent),
    /// Agent invocation and chain events.
    Agent(AgentEvent),
    /// Rebase operation events.
    Rebase(RebaseEvent),
    /// Commit generation events.
    Commit(CommitEvent),

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

impl PipelineEvent {
    // Lifecycle constructors
    /// Create a PipelineStarted event.
    pub fn pipeline_started() -> Self {
        Self::Lifecycle(LifecycleEvent::Started)
    }

    /// Create a PipelineResumed event.
    pub fn pipeline_resumed(from_checkpoint: bool) -> Self {
        Self::Lifecycle(LifecycleEvent::Resumed { from_checkpoint })
    }

    /// Create a PipelineCompleted event.
    pub fn pipeline_completed() -> Self {
        Self::Lifecycle(LifecycleEvent::Completed)
    }

    /// Create a PipelineAborted event.
    pub fn pipeline_aborted(reason: String) -> Self {
        Self::Lifecycle(LifecycleEvent::Aborted { reason })
    }

    // Planning constructors
    /// Create a PlanningPhaseStarted event.
    pub fn planning_phase_started() -> Self {
        Self::Planning(PlanningEvent::PhaseStarted)
    }

    /// Create a PlanningPhaseCompleted event.
    pub fn planning_phase_completed() -> Self {
        Self::Planning(PlanningEvent::PhaseCompleted)
    }

    /// Create a PlanGenerationStarted event.
    pub fn plan_generation_started(iteration: u32) -> Self {
        Self::Planning(PlanningEvent::GenerationStarted { iteration })
    }

    /// Create a PlanGenerationCompleted event.
    pub fn plan_generation_completed(iteration: u32, valid: bool) -> Self {
        Self::Planning(PlanningEvent::GenerationCompleted { iteration, valid })
    }

    /// Create a PlanningOutputValidationFailed event.
    pub fn planning_output_validation_failed(iteration: u32, attempt: u32) -> Self {
        Self::Planning(PlanningEvent::OutputValidationFailed { iteration, attempt })
    }

    // Development constructors
    /// Create a DevelopmentPhaseStarted event.
    pub fn development_phase_started() -> Self {
        Self::Development(DevelopmentEvent::PhaseStarted)
    }

    /// Create a DevelopmentIterationStarted event.
    pub fn development_iteration_started(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::IterationStarted { iteration })
    }

    /// Create a DevelopmentIterationCompleted event.
    pub fn development_iteration_completed(iteration: u32, output_valid: bool) -> Self {
        Self::Development(DevelopmentEvent::IterationCompleted {
            iteration,
            output_valid,
        })
    }

    /// Create a DevelopmentPhaseCompleted event.
    pub fn development_phase_completed() -> Self {
        Self::Development(DevelopmentEvent::PhaseCompleted)
    }

    /// Create a DevelopmentIterationContinuationTriggered event.
    pub fn development_iteration_continuation_triggered(
        iteration: u32,
        status: DevelopmentStatus,
        summary: String,
        files_changed: Option<Vec<String>>,
        next_steps: Option<String>,
    ) -> Self {
        Self::Development(DevelopmentEvent::ContinuationTriggered {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        })
    }

    /// Create a DevelopmentIterationContinuationSucceeded event.
    pub fn development_iteration_continuation_succeeded(
        iteration: u32,
        total_continuation_attempts: u32,
    ) -> Self {
        Self::Development(DevelopmentEvent::ContinuationSucceeded {
            iteration,
            total_continuation_attempts,
        })
    }

    /// Create a DevelopmentOutputValidationFailed event.
    pub fn development_output_validation_failed(iteration: u32, attempt: u32) -> Self {
        Self::Development(DevelopmentEvent::OutputValidationFailed { iteration, attempt })
    }

    /// Create a DevelopmentContinuationBudgetExhausted event.
    pub fn development_continuation_budget_exhausted(
        iteration: u32,
        total_attempts: u32,
        last_status: DevelopmentStatus,
    ) -> Self {
        Self::Development(DevelopmentEvent::ContinuationBudgetExhausted {
            iteration,
            total_attempts,
            last_status,
        })
    }

    /// Create a DevelopmentContinuationContextWritten event.
    pub fn development_continuation_context_written(iteration: u32, attempt: u32) -> Self {
        Self::Development(DevelopmentEvent::ContinuationContextWritten { iteration, attempt })
    }

    /// Create a DevelopmentContinuationContextCleaned event.
    pub fn development_continuation_context_cleaned() -> Self {
        Self::Development(DevelopmentEvent::ContinuationContextCleaned)
    }

    // Review constructors
    /// Create a ReviewPhaseStarted event.
    pub fn review_phase_started() -> Self {
        Self::Review(ReviewEvent::PhaseStarted)
    }

    /// Create a ReviewPassStarted event.
    pub fn review_pass_started(pass: u32) -> Self {
        Self::Review(ReviewEvent::PassStarted { pass })
    }

    /// Create a ReviewCompleted event.
    pub fn review_completed(pass: u32, issues_found: bool) -> Self {
        Self::Review(ReviewEvent::Completed { pass, issues_found })
    }

    /// Create a FixAttemptStarted event.
    pub fn fix_attempt_started(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixAttemptStarted { pass })
    }

    /// Create a FixAttemptCompleted event.
    pub fn fix_attempt_completed(pass: u32, changes_made: bool) -> Self {
        Self::Review(ReviewEvent::FixAttemptCompleted { pass, changes_made })
    }

    /// Create a ReviewPhaseCompleted event.
    pub fn review_phase_completed(early_exit: bool) -> Self {
        Self::Review(ReviewEvent::PhaseCompleted { early_exit })
    }

    /// Create a ReviewPassCompletedClean event.
    pub fn review_pass_completed_clean(pass: u32) -> Self {
        Self::Review(ReviewEvent::PassCompletedClean { pass })
    }

    /// Create a ReviewOutputValidationFailed event.
    pub fn review_output_validation_failed(pass: u32, attempt: u32) -> Self {
        Self::Review(ReviewEvent::OutputValidationFailed { pass, attempt })
    }

    // Agent constructors
    /// Create an AgentInvocationStarted event.
    pub fn agent_invocation_started(role: AgentRole, agent: String, model: Option<String>) -> Self {
        Self::Agent(AgentEvent::InvocationStarted { role, agent, model })
    }

    /// Create an AgentInvocationSucceeded event.
    pub fn agent_invocation_succeeded(role: AgentRole, agent: String) -> Self {
        Self::Agent(AgentEvent::InvocationSucceeded { role, agent })
    }

    /// Create an AgentInvocationFailed event.
    pub fn agent_invocation_failed(
        role: AgentRole,
        agent: String,
        exit_code: i32,
        error_kind: AgentErrorKind,
        retriable: bool,
    ) -> Self {
        Self::Agent(AgentEvent::InvocationFailed {
            role,
            agent,
            exit_code,
            error_kind,
            retriable,
        })
    }

    /// Create an AgentFallbackTriggered event.
    pub fn agent_fallback_triggered(role: AgentRole, from_agent: String, to_agent: String) -> Self {
        Self::Agent(AgentEvent::FallbackTriggered {
            role,
            from_agent,
            to_agent,
        })
    }

    /// Create an AgentModelFallbackTriggered event.
    pub fn agent_model_fallback_triggered(
        role: AgentRole,
        agent: String,
        from_model: String,
        to_model: String,
    ) -> Self {
        Self::Agent(AgentEvent::ModelFallbackTriggered {
            role,
            agent,
            from_model,
            to_model,
        })
    }

    /// Create an AgentRetryCycleStarted event.
    pub fn agent_retry_cycle_started(role: AgentRole, cycle: u32) -> Self {
        Self::Agent(AgentEvent::RetryCycleStarted { role, cycle })
    }

    /// Create an AgentChainExhausted event.
    pub fn agent_chain_exhausted(role: AgentRole) -> Self {
        Self::Agent(AgentEvent::ChainExhausted { role })
    }

    /// Create an AgentChainInitialized event.
    pub fn agent_chain_initialized(
        role: AgentRole,
        agents: Vec<String>,
        max_cycles: u32,
        retry_delay_ms: u64,
        backoff_multiplier: f64,
        max_backoff_ms: u64,
    ) -> Self {
        Self::Agent(AgentEvent::ChainInitialized {
            role,
            agents,
            max_cycles,
            retry_delay_ms,
            backoff_multiplier,
            max_backoff_ms,
        })
    }

    /// Create an AgentRateLimitFallback event.
    pub fn agent_rate_limit_fallback(
        role: AgentRole,
        agent: String,
        prompt_context: Option<String>,
    ) -> Self {
        Self::Agent(AgentEvent::RateLimitFallback {
            role,
            agent,
            prompt_context,
        })
    }

    /// Create an AgentSessionEstablished event.
    pub fn agent_session_established(role: AgentRole, agent: String, session_id: String) -> Self {
        Self::Agent(AgentEvent::SessionEstablished {
            role,
            agent,
            session_id,
        })
    }

    /// Create an AgentXsdValidationFailed event.
    pub fn agent_xsd_validation_failed(
        role: AgentRole,
        artifact: crate::reducer::state::ArtifactType,
        error: String,
        retry_count: u32,
    ) -> Self {
        Self::Agent(AgentEvent::XsdValidationFailed {
            role,
            artifact,
            error,
            retry_count,
        })
    }

    // Rebase constructors
    /// Create a RebaseStarted event.
    pub fn rebase_started(phase: RebasePhase, target_branch: String) -> Self {
        Self::Rebase(RebaseEvent::Started {
            phase,
            target_branch,
        })
    }

    /// Create a RebaseConflictDetected event.
    pub fn rebase_conflict_detected(files: Vec<PathBuf>) -> Self {
        Self::Rebase(RebaseEvent::ConflictDetected { files })
    }

    /// Create a RebaseConflictResolved event.
    pub fn rebase_conflict_resolved(files: Vec<PathBuf>) -> Self {
        Self::Rebase(RebaseEvent::ConflictResolved { files })
    }

    /// Create a RebaseSucceeded event.
    pub fn rebase_succeeded(phase: RebasePhase, new_head: String) -> Self {
        Self::Rebase(RebaseEvent::Succeeded { phase, new_head })
    }

    /// Create a RebaseFailed event.
    pub fn rebase_failed(phase: RebasePhase, reason: String) -> Self {
        Self::Rebase(RebaseEvent::Failed { phase, reason })
    }

    /// Create a RebaseAborted event.
    pub fn rebase_aborted(phase: RebasePhase, restored_to: String) -> Self {
        Self::Rebase(RebaseEvent::Aborted { phase, restored_to })
    }

    /// Create a RebaseSkipped event.
    pub fn rebase_skipped(phase: RebasePhase, reason: String) -> Self {
        Self::Rebase(RebaseEvent::Skipped { phase, reason })
    }

    // Commit constructors
    /// Create a CommitGenerationStarted event.
    pub fn commit_generation_started() -> Self {
        Self::Commit(CommitEvent::GenerationStarted)
    }

    /// Create a CommitMessageGenerated event.
    pub fn commit_message_generated(message: String, attempt: u32) -> Self {
        Self::Commit(CommitEvent::MessageGenerated { message, attempt })
    }

    /// Create a CommitMessageValidationFailed event.
    pub fn commit_message_validation_failed(reason: String, attempt: u32) -> Self {
        Self::Commit(CommitEvent::MessageValidationFailed { reason, attempt })
    }

    /// Create a CommitCreated event.
    pub fn commit_created(hash: String, message: String) -> Self {
        Self::Commit(CommitEvent::Created { hash, message })
    }

    /// Create a CommitGenerationFailed event.
    pub fn commit_generation_failed(reason: String) -> Self {
        Self::Commit(CommitEvent::GenerationFailed { reason })
    }

    /// Create a CommitSkipped event.
    pub fn commit_skipped(reason: String) -> Self {
        Self::Commit(CommitEvent::Skipped { reason })
    }

    // Miscellaneous constructors
    /// Create a ContextCleaned event.
    pub fn context_cleaned() -> Self {
        Self::ContextCleaned
    }

    /// Create a CheckpointSaved event.
    pub fn checkpoint_saved(trigger: CheckpointTrigger) -> Self {
        Self::CheckpointSaved { trigger }
    }

    /// Create a FinalizingStarted event.
    pub fn finalizing_started() -> Self {
        Self::FinalizingStarted
    }

    /// Create a PromptPermissionsRestored event.
    pub fn prompt_permissions_restored() -> Self {
        Self::PromptPermissionsRestored
    }
}

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
        assert_eq!(format!("{}", PipelinePhase::Interrupted), "Interrupted");
    }
}
