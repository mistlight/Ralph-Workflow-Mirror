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

#[path = "event/development.rs"]
mod development;
pub use development::DevelopmentEvent;

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

    /// Review context prepared for a pass.
    ///
    /// Emitted after `Effect::PrepareReviewContext` completes.
    ContextPrepared {
        /// The pass number the context was prepared for.
        pass: u32,
    },

    /// Review prompt prepared for a pass.
    ///
    /// Emitted after `Effect::PrepareReviewPrompt` completes.
    PromptPrepared {
        pass: u32,
    },

    /// Reviewer agent was invoked for a pass.
    ///
    /// Emitted after `Effect::InvokeReviewAgent` completes.
    AgentInvoked {
        pass: u32,
    },

    /// Review issues XML exists and was read successfully for the pass.
    ///
    /// Emitted after `Effect::ExtractReviewIssuesXml` completes.
    IssuesXmlExtracted {
        pass: u32,
    },
    /// Review issues XML missing for the pass.
    ///
    /// Emitted after `Effect::ExtractReviewIssuesXml` when the XML was absent.
    IssuesXmlMissing {
        pass: u32,
        /// The invalid output attempt count.
        attempt: u32,
    },

    /// Review issues XML validated for a pass.
    ///
    /// This event is an observation: the XML was valid and the handler determined
    /// whether issues were found and whether this was an explicit clean-no-issues output.
    IssuesXmlValidated {
        pass: u32,
        issues_found: bool,
        clean_no_issues: bool,
        issues: Vec<String>,
        no_issues_found: Option<String>,
    },

    /// ISSUES.md was written for a pass.
    IssuesMarkdownWritten {
        pass: u32,
    },

    /// Review issue snippets were extracted for a pass.
    IssueSnippetsExtracted {
        pass: u32,
    },

    /// Review issues XML archived for a pass.
    IssuesXmlArchived {
        pass: u32,
    },

    /// Review issues XML cleaned before invoking the reviewer agent.
    IssuesXmlCleaned {
        pass: u32,
    },

    /// Fix prompt prepared for a review pass.
    FixPromptPrepared {
        pass: u32,
    },

    /// Fix agent was invoked for a review pass.
    FixAgentInvoked {
        pass: u32,
    },

    /// Fix result XML exists and was read successfully for the pass.
    FixResultXmlExtracted {
        pass: u32,
    },
    /// Fix result XML missing for the pass.
    FixResultXmlMissing {
        pass: u32,
        /// The invalid output attempt count.
        attempt: u32,
    },

    /// Fix result XML validated for a pass.
    FixResultXmlValidated {
        pass: u32,
        status: crate::reducer::state::FixStatus,
        summary: Option<String>,
    },

    /// Fix result XML cleaned before invoking the fix agent.
    FixResultXmlCleaned {
        pass: u32,
    },

    /// Fix outcome applied for a pass.
    FixOutcomeApplied {
        pass: u32,
    },

    FixResultXmlArchived {
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

    /// Fix attempt completed with incomplete status, needs continuation.
    ///
    /// Emitted when fix output is valid XML but indicates work is not complete
    /// (status is "issues_remain"). Triggers a continuation with new session.
    FixContinuationTriggered {
        /// The pass number this fix was for.
        pass: u32,
        /// Status from the agent (typically IssuesRemain).
        status: crate::reducer::state::FixStatus,
        /// Summary of what was accomplished.
        summary: Option<String>,
    },

    /// Fix continuation succeeded after multiple attempts.
    ///
    /// Emitted when a fix continuation finally reaches a complete state
    /// (all_issues_addressed or no_issues_found).
    FixContinuationSucceeded {
        /// The pass number this fix was for.
        pass: u32,
        /// Total number of continuation attempts it took.
        ///
        /// Note: This field is not used by the reducer for state transitions, but
        /// is kept for observability (event logs, checkpoint serialization, debugging).
        total_attempts: u32,
    },

    /// Fix continuation budget exhausted.
    ///
    /// Emitted when fix continuations have been exhausted without reaching
    /// a complete state. Policy decides whether to proceed to commit or abort.
    FixContinuationBudgetExhausted {
        /// The pass number this fix was for.
        pass: u32,
        /// Total number of continuation attempts made.
        total_attempts: u32,
        /// The last status received (typically IssuesRemain).
        last_status: crate::reducer::state::FixStatus,
    },

    /// Fix output validation failed (XSD/XML parsing error).
    ///
    /// Emitted when fix output cannot be parsed. Reducer decides
    /// whether to retry or switch agents.
    FixOutputValidationFailed {
        /// The pass number this fix was for.
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

    /// Agent hit authentication failure (401/403) - should fallback immediately.
    ///
    /// Unlike rate limits, auth failures indicate a credentials problem with
    /// the current agent/provider. We switch to the next agent without
    /// preserving prompt context since the issue is not transient exhaustion.
    AuthFallback {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent that failed authentication.
        agent: String,
    },

    /// Agent hit idle timeout - should fallback to a different agent.
    ///
    /// Unlike other retriable errors (Network, ModelUnavailable), idle timeouts
    /// indicate the agent may be stuck or the task is too complex for it.
    /// Retrying the same agent would likely hit the same timeout, so we switch
    /// to a different agent instead.
    ///
    /// Unlike `RateLimitFallback`, timeout fallback does not preserve prompt
    /// context since the previous execution may have made partial progress
    /// that is difficult to resume cleanly.
    TimeoutFallback {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent that timed out.
        agent: String,
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

    /// Template rendering failed due to missing required variables or unresolved placeholders.
    ///
    /// Emitted when a prompt template cannot be rendered because required variables
    /// are missing or unresolved placeholders (e.g., `{{VAR}}`) remain in the output.
    /// The reducer decides fallback policy, typically switching to the next agent.
    TemplateVariablesInvalid {
        /// The role whose template failed to render.
        role: AgentRole,
        /// The name of the template that failed.
        template_name: String,
        /// Variables that were required but not provided.
        missing_variables: Vec<String>,
        /// Placeholder patterns that remain unresolved in the rendered output.
        unresolved_placeholders: Vec<String>,
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
    /// Commit diff computed for commit generation.
    DiffPrepared {
        /// True when the diff is empty.
        empty: bool,
    },
    /// Commit diff computation failed.
    DiffFailed {
        /// The error message for the diff failure.
        error: String,
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
        assert_eq!(format!("{}", PipelinePhase::Interrupted), "Interrupted");
    }
}
