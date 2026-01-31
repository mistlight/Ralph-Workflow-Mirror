//! Pipeline state types for reducer architecture.
//!
//! Defines immutable state structures that capture complete pipeline execution context.
//! These state structures can be serialized as checkpoints for resume functionality.

use crate::agents::AgentRole;
use crate::checkpoint::execution_history::ExecutionStep;
use crate::checkpoint::{
    PipelineCheckpoint, PipelinePhase as CheckpointPhase, RebaseState as CheckpointRebaseState,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::event::PipelinePhase;

/// Artifact type being processed by the current phase.
///
/// Used to track which XML artifact type is expected for XSD validation,
/// enabling role-specific retry prompts and error messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactType {
    /// Plan XML from planning phase.
    Plan,
    /// DevelopmentResult XML from development phase.
    DevelopmentResult,
    /// Issues XML from review phase.
    Issues,
    /// FixResult XML from fix phase.
    FixResult,
    /// CommitMessage XML from commit message generation.
    CommitMessage,
}

impl std::fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan => write!(f, "plan"),
            Self::DevelopmentResult => write!(f, "development_result"),
            Self::Issues => write!(f, "issues"),
            Self::FixResult => write!(f, "fix_result"),
            Self::CommitMessage => write!(f, "commit_message"),
        }
    }
}

/// Development status from agent output.
///
/// These values map to the `<ralph-status>` element in development_result.xml.
/// Used to track whether work is complete or needs continuation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevelopmentStatus {
    /// Work completed successfully - no continuation needed.
    Completed,
    /// Work partially done - needs continuation.
    Partial,
    /// Work failed - needs continuation with different approach.
    Failed,
}

/// Fix status from agent output.
///
/// These values map to the `<ralph-status>` element in fix_result.xml.
/// Used to track whether fix work is complete or needs continuation.
///
/// # Continuation Semantics
///
/// - `AllIssuesAddressed`: Complete, no continuation needed
/// - `NoIssuesFound`: Complete, no continuation needed
/// - `IssuesRemain`: Work incomplete, needs continuation
/// - `Failed`: Fix attempt failed, needs continuation with different approach
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FixStatus {
    /// All issues have been addressed - no continuation needed.
    #[default]
    AllIssuesAddressed,
    /// Issues remain - needs continuation.
    IssuesRemain,
    /// No issues were found - nothing to fix.
    NoIssuesFound,
    /// Fix attempt failed - needs continuation with different approach.
    ///
    /// This status indicates the fix attempt produced valid XML output but
    /// the agent could not fix the issues (e.g., blocked by external factors,
    /// needs different strategy). Triggers continuation like `IssuesRemain`.
    Failed,
}

impl std::fmt::Display for DevelopmentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completed => write!(f, "completed"),
            Self::Partial => write!(f, "partial"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::fmt::Display for FixStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllIssuesAddressed => write!(f, "all_issues_addressed"),
            Self::IssuesRemain => write!(f, "issues_remain"),
            Self::NoIssuesFound => write!(f, "no_issues_found"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl FixStatus {
    /// Parse a fix status string from XML.
    ///
    /// This is intentionally not implementing std::str::FromStr because it returns
    /// Option<Self> for easier handling of unknown values without error types.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "all_issues_addressed" => Some(Self::AllIssuesAddressed),
            "issues_remain" => Some(Self::IssuesRemain),
            "no_issues_found" => Some(Self::NoIssuesFound),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// Returns true if the fix is complete (no more work needed).
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::AllIssuesAddressed | Self::NoIssuesFound)
    }

    /// Returns true if continuation is needed (incomplete work or failure).
    ///
    /// Both `IssuesRemain` and `Failed` trigger continuation:
    /// - `IssuesRemain`: Some issues fixed, others remain
    /// - `Failed`: Fix attempt failed, needs different approach
    pub fn needs_continuation(&self) -> bool {
        matches!(self, Self::IssuesRemain | Self::Failed)
    }
}

/// Continuation state for development iterations.
///
/// Tracks context from previous attempts within the same iteration to enable
/// continuation-aware prompting when status is "partial" or "failed".
///
/// # When Fields Are Populated
///
/// - `previous_status`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `previous_summary`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `previous_files_changed`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `previous_next_steps`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `continuation_attempt`: Incremented on each continuation within same iteration
///
/// # Reset Triggers
///
/// The continuation state is reset (cleared) when:
/// - A new iteration starts (DevelopmentIterationStarted event)
/// - Status becomes "completed" (ContinuationSucceeded event)
/// - Phase transitions away from Development
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContinuationState {
    /// Status from the previous attempt ("partial" or "failed").
    pub previous_status: Option<DevelopmentStatus>,
    /// Summary of what was accomplished in the previous attempt.
    pub previous_summary: Option<String>,
    /// Files changed in the previous attempt.
    pub previous_files_changed: Option<Vec<String>>,
    /// Agent's recommended next steps from the previous attempt.
    pub previous_next_steps: Option<String>,
    /// Current continuation attempt number (0 = first attempt, 1+ = continuation).
    pub continuation_attempt: u32,
    /// Count of invalid XML outputs for the current iteration.
    #[serde(default)]
    pub invalid_output_attempts: u32,
    /// Whether a continuation context write is pending.
    #[serde(default)]
    pub context_write_pending: bool,
    /// Whether a continuation context cleanup is pending.
    #[serde(default)]
    pub context_cleanup_pending: bool,
    /// Count of XSD validation retry attempts for current artifact.
    ///
    /// Tracks how many times we've retried with the same agent/session due to
    /// XML parsing or XSD validation failures. Reset when switching agents,
    /// artifacts, or on successful validation.
    #[serde(default)]
    pub xsd_retry_count: u32,
    /// Whether an XSD retry is pending (validation failed, need to retry).
    ///
    /// Set to true when XsdValidationFailed event fires.
    /// Cleared when retry attempt starts or max retries exceeded.
    #[serde(default)]
    pub xsd_retry_pending: bool,
    /// Whether a continuation is pending (output valid but work incomplete).
    ///
    /// Set to true when agent output indicates status is "partial" or "failed".
    /// Cleared when continuation attempt starts or max continuations exceeded.
    #[serde(default)]
    pub continue_pending: bool,
    /// Current artifact type being processed.
    ///
    /// Set at the start of each phase to track which XML artifact is expected.
    /// Used for appropriate retry prompts and error messages.
    #[serde(default)]
    pub current_artifact: Option<ArtifactType>,
    /// Maximum XSD retry attempts (default 10).
    ///
    /// Loaded from unified config. After this many retries, falls back to
    /// agent chain advancement.
    #[serde(default = "default_max_xsd_retry_count")]
    pub max_xsd_retry_count: u32,
    /// Maximum continuation attempts (default 3).
    ///
    /// Loaded from unified config. After this many continuations, marks
    /// iteration as complete (even if status is partial/failed).
    #[serde(default = "default_max_continue_count")]
    pub max_continue_count: u32,

    // =========================================================================
    // Fix continuation tracking (parallel to development continuation)
    // =========================================================================
    /// Status from the previous fix attempt.
    #[serde(default)]
    pub fix_status: Option<FixStatus>,
    /// Summary from the previous fix attempt.
    #[serde(default)]
    pub fix_previous_summary: Option<String>,
    /// Current fix continuation attempt number (0 = first attempt, 1+ = continuation).
    #[serde(default)]
    pub fix_continuation_attempt: u32,
    /// Whether a fix continuation is pending (output valid but work incomplete).
    ///
    /// Set to true when fix output indicates status is "issues_remain".
    /// Cleared when continuation attempt starts or max continuations exceeded.
    #[serde(default)]
    pub fix_continue_pending: bool,
    /// Maximum fix continuation attempts (default 3).
    ///
    /// After this many continuations, proceeds to commit even if issues remain.
    #[serde(default = "default_max_continue_count")]
    pub max_fix_continue_count: u32,
}

const fn default_max_xsd_retry_count() -> u32 {
    10
}

const fn default_max_continue_count() -> u32 {
    3
}

impl Default for ContinuationState {
    fn default() -> Self {
        Self {
            previous_status: None,
            previous_summary: None,
            previous_files_changed: None,
            previous_next_steps: None,
            continuation_attempt: 0,
            invalid_output_attempts: 0,
            context_write_pending: false,
            context_cleanup_pending: false,
            xsd_retry_count: 0,
            xsd_retry_pending: false,
            continue_pending: false,
            current_artifact: None,
            max_xsd_retry_count: default_max_xsd_retry_count(),
            max_continue_count: default_max_continue_count(),
            // Fix continuation fields
            fix_status: None,
            fix_previous_summary: None,
            fix_continuation_attempt: 0,
            fix_continue_pending: false,
            max_fix_continue_count: default_max_continue_count(),
        }
    }
}

impl ContinuationState {
    /// Create a new empty continuation state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create continuation state with custom limits (for config loading).
    pub fn with_limits(max_xsd_retry_count: u32, max_continue_count: u32) -> Self {
        Self {
            max_xsd_retry_count,
            max_continue_count,
            max_fix_continue_count: max_continue_count,
            ..Self::default()
        }
    }

    /// Builder: set max XSD retry count.
    ///
    /// Use 0 to disable XSD retries (immediate agent fallback on validation failure).
    pub fn with_max_xsd_retry(mut self, max_xsd_retry_count: u32) -> Self {
        self.max_xsd_retry_count = max_xsd_retry_count;
        self
    }

    /// Check if this is a continuation attempt (not the first attempt).
    pub fn is_continuation(&self) -> bool {
        self.continuation_attempt > 0
    }

    /// Reset the continuation state for a new iteration or phase transition.
    ///
    /// This performs a **hard reset** of ALL continuation and retry state,
    /// preserving only the configured limits (max_xsd_retry_count, max_continue_count,
    /// max_fix_continue_count).
    ///
    /// # What gets reset
    ///
    /// - `continuation_attempt` -> 0
    /// - `continue_pending` -> false
    /// - `invalid_output_attempts` -> 0
    /// - `xsd_retry_count` -> 0
    /// - `xsd_retry_pending` -> false
    /// - `fix_continuation_attempt` -> 0
    /// - `fix_continue_pending` -> false
    /// - `fix_status` -> None
    /// - `current_artifact` -> None
    /// - `previous_status`, `previous_summary`, etc. -> defaults
    ///
    /// # Usage
    ///
    /// Call this when transitioning to a completely new phase or iteration
    /// where prior continuation/retry state should not carry over. For partial
    /// resets (e.g., resetting only fix continuation while preserving development
    /// continuation state), use field-level updates instead.
    pub fn reset(&self) -> Self {
        // Preserve configured limits, reset everything else
        Self {
            max_xsd_retry_count: self.max_xsd_retry_count,
            max_continue_count: self.max_continue_count,
            max_fix_continue_count: self.max_fix_continue_count,
            ..Self::default()
        }
    }

    /// Set the current artifact type being processed.
    pub fn with_artifact(&self, artifact: ArtifactType) -> Self {
        // Reset XSD retry state when switching artifacts, preserve everything else
        Self {
            current_artifact: Some(artifact),
            xsd_retry_count: 0,
            xsd_retry_pending: false,
            ..self.clone()
        }
    }

    /// Mark XSD validation as failed, triggering a retry.
    pub fn trigger_xsd_retry(&self) -> Self {
        Self {
            xsd_retry_pending: true,
            xsd_retry_count: self.xsd_retry_count + 1,
            ..self.clone()
        }
    }

    /// Clear XSD retry pending flag after starting retry.
    pub fn clear_xsd_retry_pending(&self) -> Self {
        Self {
            xsd_retry_pending: false,
            ..self.clone()
        }
    }

    /// Check if XSD retries are exhausted.
    pub fn xsd_retries_exhausted(&self) -> bool {
        self.xsd_retry_count >= self.max_xsd_retry_count
    }

    /// Mark continuation as pending (output valid but work incomplete).
    pub fn trigger_continue(&self) -> Self {
        Self {
            continue_pending: true,
            ..self.clone()
        }
    }

    /// Clear continue pending flag after starting continuation.
    pub fn clear_continue_pending(&self) -> Self {
        Self {
            continue_pending: false,
            ..self.clone()
        }
    }

    /// Check if continuation attempts are exhausted.
    ///
    /// Returns `true` when `continuation_attempt >= max_continue_count`.
    ///
    /// # Semantics
    ///
    /// The `continuation_attempt` counter tracks how many times work has been attempted:
    /// - 0: Initial attempt (before any continuation)
    /// - 1: After first continuation
    /// - 2: After second continuation
    /// - etc.
    ///
    /// With `max_continue_count = 3`:
    /// - Attempts 0, 1, 2 are allowed (3 total)
    /// - Attempt 3+ triggers exhaustion
    ///
    /// # Naming Note
    ///
    /// The field is named `max_continue_count` rather than `max_total_attempts` because
    /// it historically represented the maximum number of continuations. The actual
    /// semantics are "maximum total attempts including initial".
    pub fn continuations_exhausted(&self) -> bool {
        self.continuation_attempt >= self.max_continue_count
    }

    /// Trigger a continuation with context from the previous attempt.
    ///
    /// Sets both `context_write_pending` (to write continuation context) and
    /// `continue_pending` (to trigger the continuation flow in orchestration).
    pub fn trigger_continuation(
        &self,
        status: DevelopmentStatus,
        summary: String,
        files_changed: Option<Vec<String>>,
        next_steps: Option<String>,
    ) -> Self {
        Self {
            previous_status: Some(status),
            previous_summary: Some(summary),
            previous_files_changed: files_changed,
            previous_next_steps: next_steps,
            continuation_attempt: self.continuation_attempt + 1,
            invalid_output_attempts: 0,
            context_write_pending: true,
            context_cleanup_pending: false,
            // Reset XSD retry count for new continuation attempt
            xsd_retry_count: 0,
            xsd_retry_pending: false,
            // Set continue_pending to trigger continuation in orchestration
            continue_pending: true,
            // Preserve artifact type and limits
            current_artifact: self.current_artifact.clone(),
            max_xsd_retry_count: self.max_xsd_retry_count,
            max_continue_count: self.max_continue_count,
            // Preserve fix continuation fields
            fix_status: self.fix_status.clone(),
            fix_previous_summary: self.fix_previous_summary.clone(),
            fix_continuation_attempt: self.fix_continuation_attempt,
            fix_continue_pending: self.fix_continue_pending,
            max_fix_continue_count: self.max_fix_continue_count,
        }
    }

    // =========================================================================
    // Fix continuation methods
    // =========================================================================

    /// Check if fix continuations are exhausted.
    ///
    /// Semantics match `continuations_exhausted()`: with default `max_fix_continue_count`
    /// of 3, attempts 0, 1, 2 are allowed (3 total), attempt 3+ is exhausted.
    pub fn fix_continuations_exhausted(&self) -> bool {
        self.fix_continuation_attempt >= self.max_fix_continue_count
    }

    /// Trigger a fix continuation with status context.
    pub fn trigger_fix_continuation(&self, status: FixStatus, summary: Option<String>) -> Self {
        Self {
            fix_status: Some(status),
            fix_previous_summary: summary,
            fix_continuation_attempt: self.fix_continuation_attempt + 1,
            fix_continue_pending: true,
            // Reset XSD retry state for new continuation
            xsd_retry_count: 0,
            xsd_retry_pending: false,
            // Reset invalid output attempts for new continuation
            invalid_output_attempts: 0,
            // Clear other pending flags
            context_write_pending: false,
            context_cleanup_pending: false,
            continue_pending: false,
            // Preserve all other fields via spread operator
            ..self.clone()
        }
    }

    /// Clear fix continuation pending flag after starting continuation.
    pub fn clear_fix_continue_pending(&self) -> Self {
        Self {
            fix_continue_pending: false,
            ..self.clone()
        }
    }

    /// Reset fix continuation state (e.g., when entering a new review pass).
    pub fn reset_fix_continuation(&self) -> Self {
        Self {
            fix_status: None,
            fix_previous_summary: None,
            fix_continuation_attempt: 0,
            fix_continue_pending: false,
            ..self.clone()
        }
    }
}

/// Immutable pipeline state - the single source of truth for pipeline progress.
///
/// This struct captures complete execution context and doubles as the checkpoint
/// data structure for resume functionality. Serialize it to JSON to save state;
/// deserialize to resume interrupted runs.
///
/// # Invariants
///
/// - `iteration` is always `<= total_iterations`
/// - `reviewer_pass` is always `<= total_reviewer_passes`
/// - `agent_chain` maintains fallback order and retry counts
/// - State transitions only occur through the [`reduce`](super::reduce) function
///
/// # See Also
///
/// - [`reduce`](super::reduce) for state transitions
/// - [`determine_next_effect`](super::determine_next_effect) for effect derivation
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PipelineState {
    pub phase: PipelinePhase,
    pub previous_phase: Option<PipelinePhase>,
    pub iteration: u32,
    pub total_iterations: u32,
    pub reviewer_pass: u32,
    pub total_reviewer_passes: u32,
    pub review_issues_found: bool,
    /// Tracks whether review context was prepared for the current pass.
    ///
    /// Used to sequence single-task review effects (PrepareReviewContext -> RunReviewPass).
    #[serde(default)]
    pub review_context_prepared_pass: Option<u32>,
    /// Tracks whether the review prompt was prepared for the current pass.
    #[serde(default)]
    pub review_prompt_prepared_pass: Option<u32>,
    /// Tracks whether the reviewer agent was invoked for the current pass.
    #[serde(default)]
    pub review_agent_invoked_pass: Option<u32>,
    /// Tracks whether `.agent/tmp/issues.xml` was successfully extracted for the current pass.
    #[serde(default)]
    pub review_issues_xml_extracted_pass: Option<u32>,
    /// Stores the validated outcome for the current review pass.
    ///
    /// This is used to sequence post-validation single-task effects (write markdown,
    /// archive XML) before the reducer advances to the next pass/phase.
    #[serde(default)]
    pub review_validated_outcome: Option<ReviewValidatedOutcome>,
    /// Tracks whether ISSUES.md has been written for the current pass.
    #[serde(default)]
    pub review_issues_markdown_written_pass: Option<u32>,
    #[serde(default)]
    pub review_issues_xml_archived_pass: Option<u32>,
    pub context_cleaned: bool,
    pub agent_chain: AgentChainState,
    pub rebase: RebaseState,
    pub commit: CommitState,
    pub execution_history: Vec<ExecutionStep>,
    /// Count of CheckpointSaved events applied to state.
    ///
    /// This is a reducer-visible record of checkpoint saves, intended for
    /// observability and tests that enforce checkpointing happens via effects.
    #[serde(default)]
    pub checkpoint_saved_count: u32,
    /// Continuation state for development iterations.
    ///
    /// Tracks context from previous attempts when status is "partial" or "failed"
    /// to enable continuation-aware prompting.
    #[serde(default)]
    pub continuation: ContinuationState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewValidatedOutcome {
    pub pass: u32,
    pub issues_found: bool,
    pub clean_no_issues: bool,
}

impl PipelineState {
    pub fn initial(developer_iters: u32, reviewer_reviews: u32) -> Self {
        Self::initial_with_continuation(developer_iters, reviewer_reviews, ContinuationState::new())
    }

    /// Create initial state with custom continuation limits from config.
    ///
    /// Use this when you need to load XSD retry and continuation limits from unified config.
    /// Example:
    /// ```ignore
    /// let continuation = ContinuationState::with_limits(
    ///     config.general.max_xsd_retries,
    ///     config.general.max_dev_continuations,
    /// );
    /// let state = PipelineState::initial_with_continuation(dev_iters, reviews, continuation);
    /// ```
    pub fn initial_with_continuation(
        developer_iters: u32,
        reviewer_reviews: u32,
        continuation: ContinuationState,
    ) -> Self {
        // Determine initial phase based on what work needs to be done
        let initial_phase = if developer_iters == 0 {
            // No development iterations → skip Planning and Development
            if reviewer_reviews == 0 {
                // No review passes either → go straight to commit
                PipelinePhase::CommitMessage
            } else {
                PipelinePhase::Review
            }
        } else {
            PipelinePhase::Planning
        };

        Self {
            phase: initial_phase,
            previous_phase: None,
            iteration: 0,
            total_iterations: developer_iters,
            reviewer_pass: 0,
            total_reviewer_passes: reviewer_reviews,
            review_issues_found: false,
            review_context_prepared_pass: None,
            review_prompt_prepared_pass: None,
            review_agent_invoked_pass: None,
            review_issues_xml_extracted_pass: None,
            review_validated_outcome: None,
            review_issues_markdown_written_pass: None,
            review_issues_xml_archived_pass: None,
            context_cleaned: false,
            agent_chain: AgentChainState::initial(),
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            execution_history: Vec::new(),
            checkpoint_saved_count: 0,
            continuation,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.phase, PipelinePhase::Complete)
            || (matches!(self.phase, PipelinePhase::Interrupted) && self.checkpoint_saved_count > 0)
    }

    pub fn current_head(&self) -> String {
        self.rebase
            .current_head()
            .unwrap_or_else(|| "HEAD".to_string())
    }
}

impl From<PipelineCheckpoint> for PipelineState {
    fn from(checkpoint: PipelineCheckpoint) -> Self {
        let rebase_state = map_checkpoint_rebase_state(&checkpoint.rebase_state);
        let agent_chain = AgentChainState::initial();

        PipelineState {
            phase: map_checkpoint_phase(checkpoint.phase),
            previous_phase: None,
            iteration: checkpoint.iteration,
            total_iterations: checkpoint.total_iterations,
            reviewer_pass: checkpoint.reviewer_pass,
            total_reviewer_passes: checkpoint.total_reviewer_passes,
            review_issues_found: false,
            review_context_prepared_pass: None,
            review_prompt_prepared_pass: None,
            review_agent_invoked_pass: None,
            review_issues_xml_extracted_pass: None,
            review_validated_outcome: None,
            review_issues_markdown_written_pass: None,
            review_issues_xml_archived_pass: None,
            context_cleaned: false,
            agent_chain,
            rebase: rebase_state,
            commit: CommitState::NotStarted,
            execution_history: checkpoint
                .execution_history
                .map(|h| h.steps)
                .unwrap_or_default(),
            checkpoint_saved_count: 0,
            continuation: ContinuationState::new(),
        }
    }
}

fn map_checkpoint_phase(phase: CheckpointPhase) -> PipelinePhase {
    match phase {
        CheckpointPhase::Rebase => PipelinePhase::Planning,
        CheckpointPhase::Planning => PipelinePhase::Planning,
        CheckpointPhase::Development => PipelinePhase::Development,
        CheckpointPhase::Review => PipelinePhase::Review,
        CheckpointPhase::CommitMessage => PipelinePhase::CommitMessage,
        CheckpointPhase::FinalValidation => PipelinePhase::FinalValidation,
        CheckpointPhase::Complete => PipelinePhase::Complete,
        CheckpointPhase::PreRebase => PipelinePhase::Planning,
        CheckpointPhase::PreRebaseConflict => PipelinePhase::Planning,
        CheckpointPhase::PostRebase => PipelinePhase::CommitMessage,
        CheckpointPhase::PostRebaseConflict => PipelinePhase::CommitMessage,
        CheckpointPhase::Interrupted => PipelinePhase::Interrupted,
    }
}

fn map_checkpoint_rebase_state(rebase_state: &CheckpointRebaseState) -> RebaseState {
    match rebase_state {
        CheckpointRebaseState::NotStarted => RebaseState::NotStarted,
        CheckpointRebaseState::PreRebaseInProgress { upstream_branch } => RebaseState::InProgress {
            original_head: "HEAD".to_string(),
            target_branch: upstream_branch.clone(),
        },
        CheckpointRebaseState::PreRebaseCompleted { commit_oid } => RebaseState::Completed {
            new_head: commit_oid.clone(),
        },
        CheckpointRebaseState::PostRebaseInProgress { upstream_branch } => {
            RebaseState::InProgress {
                original_head: "HEAD".to_string(),
                target_branch: upstream_branch.clone(),
            }
        }
        CheckpointRebaseState::PostRebaseCompleted { commit_oid } => RebaseState::Completed {
            new_head: commit_oid.clone(),
        },
        CheckpointRebaseState::HasConflicts { files } => RebaseState::Conflicted {
            original_head: "HEAD".to_string(),
            target_branch: "main".to_string(),
            files: files.iter().map(PathBuf::from).collect(),
            resolution_attempts: 0,
        },
        CheckpointRebaseState::Failed { .. } => RebaseState::Skipped,
    }
}

/// Agent fallback chain state (explicit, not loop indices).
///
/// Tracks position in the multi-level fallback chain:
/// - Agent level (primary → fallback1 → fallback2)
/// - Model level (within each agent, try different models)
/// - Retry cycle (exhaust all agents, start over with exponential backoff)
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentChainState {
    pub agents: Vec<String>,
    pub current_agent_index: usize,
    pub models_per_agent: Vec<Vec<String>>,
    pub current_model_index: usize,
    pub retry_cycle: u32,
    pub max_cycles: u32,
    /// Base delay between retry cycles in milliseconds.
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    /// Multiplier for exponential backoff.
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// Maximum backoff delay in milliseconds.
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
    /// Pending backoff delay (milliseconds) that must be waited before continuing.
    #[serde(default)]
    pub backoff_pending_ms: Option<u64>,
    pub current_role: AgentRole,
    /// Prompt context preserved from rate-limited agent for continuation.
    ///
    /// When an agent hits 429, we save the prompt here so the next agent
    /// can continue the same work instead of starting from scratch.
    #[serde(default)]
    pub rate_limit_continuation_prompt: Option<String>,
    /// Session ID from the last agent response.
    ///
    /// Used for XSD retry to continue with the same session when possible.
    /// Agents that support sessions (e.g., Claude Code) emit session IDs
    /// that can be passed back for continuation.
    #[serde(default)]
    pub last_session_id: Option<String>,
}

const fn default_retry_delay_ms() -> u64 {
    1000
}

const fn default_backoff_multiplier() -> f64 {
    2.0
}

const fn default_max_backoff_ms() -> u64 {
    60000
}

impl AgentChainState {
    pub fn initial() -> Self {
        Self {
            agents: Vec::new(),
            current_agent_index: 0,
            models_per_agent: Vec::new(),
            current_model_index: 0,
            retry_cycle: 0,
            max_cycles: 3,
            retry_delay_ms: default_retry_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            max_backoff_ms: default_max_backoff_ms(),
            backoff_pending_ms: None,
            current_role: AgentRole::Developer,
            rate_limit_continuation_prompt: None,
            last_session_id: None,
        }
    }

    pub fn with_agents(
        mut self,
        agents: Vec<String>,
        models_per_agent: Vec<Vec<String>>,
        role: AgentRole,
    ) -> Self {
        self.agents = agents;
        self.models_per_agent = models_per_agent;
        self.current_role = role;
        self
    }

    /// Builder method to set the maximum number of retry cycles.
    ///
    /// A retry cycle is when all agents have been exhausted and we start
    /// over with exponential backoff.
    pub fn with_max_cycles(mut self, max_cycles: u32) -> Self {
        self.max_cycles = max_cycles;
        self
    }

    pub fn with_backoff_policy(
        mut self,
        retry_delay_ms: u64,
        backoff_multiplier: f64,
        max_backoff_ms: u64,
    ) -> Self {
        self.retry_delay_ms = retry_delay_ms;
        self.backoff_multiplier = backoff_multiplier;
        self.max_backoff_ms = max_backoff_ms;
        self
    }

    pub fn current_agent(&self) -> Option<&String> {
        self.agents.get(self.current_agent_index)
    }

    /// Get the currently selected model for the current agent.
    ///
    /// Returns `None` if:
    /// - No models are configured
    /// - The current agent index is out of bounds
    /// - The current model index is out of bounds
    pub fn current_model(&self) -> Option<&String> {
        self.models_per_agent
            .get(self.current_agent_index)
            .and_then(|models| models.get(self.current_model_index))
    }

    pub fn is_exhausted(&self) -> bool {
        self.retry_cycle >= self.max_cycles
            && self.current_agent_index == 0
            && self.current_model_index == 0
    }

    pub fn advance_to_next_model(&self) -> Self {
        let new = self.clone();

        // When models are configured, we try each model for the current agent once.
        // If the models list is exhausted, advance to the next agent/retry cycle
        // instead of looping models indefinitely.
        match new.models_per_agent.get(new.current_agent_index) {
            Some(models) if !models.is_empty() => {
                if new.current_model_index + 1 < models.len() {
                    let mut advanced = new;
                    advanced.current_model_index += 1;
                    advanced
                } else {
                    new.switch_to_next_agent()
                }
            }
            _ => new.switch_to_next_agent(),
        }
    }

    pub fn switch_to_next_agent(&self) -> Self {
        let mut new = self.clone();
        if new.current_agent_index + 1 < new.agents.len() {
            new.current_agent_index += 1;
            new.current_model_index = 0;
            new.backoff_pending_ms = None;
        } else {
            new.current_agent_index = 0;
            new.current_model_index = 0;
            new.retry_cycle += 1;
            if new.is_exhausted() {
                new.backoff_pending_ms = None;
            } else {
                new.backoff_pending_ms = Some(new.calculate_backoff_delay_ms_for_retry_cycle());
            }
        }
        new
    }

    /// Switch to next agent after rate limit, preserving prompt for continuation.
    ///
    /// This is used when an agent hits a 429 rate limit error. Instead of
    /// retrying with the same agent (which would likely hit rate limits again),
    /// we switch to the next agent and preserve the prompt so the new agent
    /// can continue the same work.
    pub fn switch_to_next_agent_with_prompt(&self, prompt: Option<String>) -> Self {
        let mut next = self.switch_to_next_agent();
        next.rate_limit_continuation_prompt = prompt;
        next
    }

    /// Clear continuation prompt after successful execution.
    ///
    /// Called when an agent successfully completes its task, clearing any
    /// saved prompt context from previous rate-limited agents.
    pub fn clear_continuation_prompt(&self) -> Self {
        let mut new = self.clone();
        new.rate_limit_continuation_prompt = None;
        new
    }

    pub fn reset_for_role(&self, role: AgentRole) -> Self {
        let mut new = self.clone();
        new.current_role = role;
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.retry_cycle = 0;
        new.backoff_pending_ms = None;
        new.rate_limit_continuation_prompt = None;
        new.last_session_id = None;
        new
    }

    pub fn reset(&self) -> Self {
        let mut new = self.clone();
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.backoff_pending_ms = None;
        new.rate_limit_continuation_prompt = None;
        new.last_session_id = None;
        new
    }

    /// Store session ID from agent response for potential reuse.
    pub fn with_session_id(&self, session_id: Option<String>) -> Self {
        let mut new = self.clone();
        new.last_session_id = session_id;
        new
    }

    /// Clear session ID (e.g., when switching agents or starting new work).
    pub fn clear_session_id(&self) -> Self {
        let mut new = self.clone();
        new.last_session_id = None;
        new
    }

    pub fn start_retry_cycle(&self) -> Self {
        let mut new = self.clone();
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.retry_cycle += 1;
        if new.is_exhausted() {
            new.backoff_pending_ms = None;
        } else {
            new.backoff_pending_ms = Some(new.calculate_backoff_delay_ms_for_retry_cycle());
        }
        new
    }

    pub fn clear_backoff_pending(&self) -> Self {
        let mut new = self.clone();
        new.backoff_pending_ms = None;
        new
    }

    fn calculate_backoff_delay_ms_for_retry_cycle(&self) -> u64 {
        // The first retry cycle should use the base delay.
        let cycle_index = self.retry_cycle.saturating_sub(1);
        calculate_backoff_delay_ms(
            self.retry_delay_ms,
            self.backoff_multiplier,
            self.max_backoff_ms,
            cycle_index,
        )
    }
}

// Backoff computation helpers.
// These mirror the semantics in `crate::agents::fallback::FallbackConfig::calculate_backoff`
// but live in reducer state so orchestration can derive BackoffWait effects purely.

const IEEE_754_EXP_BIAS: i32 = 1023;
const IEEE_754_EXP_MASK: u64 = 0x7FF;
const IEEE_754_MANTISSA_MASK: u64 = 0x000F_FFFF_FFFF_FFFF;
const IEEE_754_IMPLICIT_ONE: u64 = 1u64 << 52;

fn f64_to_u64_via_bits(value: f64) -> u64 {
    if !value.is_finite() || value < 0.0 {
        return 0;
    }
    let bits = value.to_bits();
    let exp_biased = ((bits >> 52) & IEEE_754_EXP_MASK) as i32;
    let mantissa = bits & IEEE_754_MANTISSA_MASK;
    if exp_biased == 0 {
        return 0;
    }
    let exp = exp_biased - IEEE_754_EXP_BIAS;
    if exp < 0 {
        return 0;
    }
    let full_mantissa = mantissa | IEEE_754_IMPLICIT_ONE;
    let shift = 52i32 - exp;
    if shift <= 0 {
        u64::MAX
    } else if shift < 64 {
        full_mantissa >> shift
    } else {
        0
    }
}

fn multiplier_hundredths(backoff_multiplier: f64) -> u64 {
    const EPSILON: f64 = 0.0001;
    let m = backoff_multiplier;
    if (m - 1.0).abs() < EPSILON {
        return 100;
    } else if (m - 1.5).abs() < EPSILON {
        return 150;
    } else if (m - 2.0).abs() < EPSILON {
        return 200;
    } else if (m - 2.5).abs() < EPSILON {
        return 250;
    } else if (m - 3.0).abs() < EPSILON {
        return 300;
    } else if (m - 4.0).abs() < EPSILON {
        return 400;
    } else if (m - 5.0).abs() < EPSILON {
        return 500;
    } else if (m - 10.0).abs() < EPSILON {
        return 1000;
    }

    let clamped = m.clamp(0.0, 1000.0);
    let multiplied = clamped * 100.0;
    let rounded = multiplied.round();
    f64_to_u64_via_bits(rounded)
}

fn calculate_backoff_delay_ms(
    retry_delay_ms: u64,
    backoff_multiplier: f64,
    max_backoff_ms: u64,
    cycle: u32,
) -> u64 {
    let mult_hundredths = multiplier_hundredths(backoff_multiplier);
    let mut delay_hundredths = retry_delay_ms.saturating_mul(100);
    for _ in 0..cycle {
        delay_hundredths = delay_hundredths.saturating_mul(mult_hundredths);
        delay_hundredths = delay_hundredths.saturating_div(100);
    }
    delay_hundredths.div_euclid(100).min(max_backoff_ms)
}

/// Rebase operation state.
///
/// Tracks rebase progress through the state machine:
/// NotStarted → InProgress → Conflicted → Completed/Skipped
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RebaseState {
    NotStarted,
    InProgress {
        original_head: String,
        target_branch: String,
    },
    Conflicted {
        original_head: String,
        target_branch: String,
        files: Vec<PathBuf>,
        resolution_attempts: u32,
    },
    Completed {
        new_head: String,
    },
    Skipped,
}

impl RebaseState {
    #[doc(hidden)]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_terminal(&self) -> bool {
        matches!(self, RebaseState::Completed { .. } | RebaseState::Skipped)
    }

    pub fn current_head(&self) -> Option<String> {
        match self {
            RebaseState::NotStarted | RebaseState::Skipped => None,
            RebaseState::InProgress { original_head, .. } => Some(original_head.clone()),
            RebaseState::Conflicted { .. } => None,
            RebaseState::Completed { new_head } => Some(new_head.clone()),
        }
    }

    #[doc(hidden)]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            RebaseState::InProgress { .. } | RebaseState::Conflicted { .. }
        )
    }
}

/// Maximum number of retry attempts when XML/format validation fails.
///
/// This applies across the pipeline for:
/// - Commit message generation validation failures
/// - Plan generation validation failures  
/// - Development output validation failures
/// - Review output validation failures
///
/// When an agent produces output that fails XML parsing or format validation,
/// we retry with corrective prompts up to this many times before giving up.
pub const MAX_VALIDATION_RETRY_ATTEMPTS: u32 = 100;

/// Maximum number of developer validation retry attempts before giving up.
///
/// Specifically for developer iterations - this is for XSD validation failures
/// (malformed XML). After exhausting these retries, the system will fall back
/// to a continuation attempt with a fresh prompt rather than failing entirely.
/// This separates XSD retry (can't parse the response) from continuation
/// (understood the response but work is incomplete).
pub const MAX_DEV_VALIDATION_RETRY_ATTEMPTS: u32 = 10;

/// Maximum number of invalid XML output reruns before aborting the iteration.
pub const MAX_DEV_INVALID_OUTPUT_RERUNS: u32 = 2;

/// Maximum number of invalid planning output reruns before switching agents.
pub const MAX_PLAN_INVALID_OUTPUT_RERUNS: u32 = 2;

/// Commit generation state.
///
/// Tracks commit message generation progress through retries:
/// NotStarted → Generating → Generated → Committed/Skipped
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CommitState {
    NotStarted,
    Generating { attempt: u32, max_attempts: u32 },
    Generated { message: String },
    Committed { hash: String },
    Skipped,
}

impl CommitState {
    #[doc(hidden)]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_terminal(&self) -> bool {
        matches!(self, CommitState::Committed { .. } | CommitState::Skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::state::{AgentConfigSnapshot, CheckpointParams, CliArgsSnapshot};

    #[test]
    fn test_pipeline_state_initial() {
        let state = PipelineState::initial(5, 2);
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.total_iterations, 5);
        assert_eq!(state.total_reviewer_passes, 2);
        assert!(!state.is_complete());
    }

    #[test]
    fn test_agent_chain_initial() {
        let chain = AgentChainState::initial();
        assert!(chain.agents.is_empty());
        assert_eq!(chain.current_agent_index, 0);
        assert_eq!(chain.retry_cycle, 0);
    }

    #[test]
    fn test_agent_chain_with_agents() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string(), "codex".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        assert_eq!(chain.agents.len(), 2);
        assert_eq!(chain.current_agent(), Some(&"claude".to_string()));
        assert_eq!(chain.max_cycles, 3);
    }

    #[test]
    fn test_agent_chain_advance_to_next_model() {
        let chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        );

        let new_chain = chain.advance_to_next_model();
        assert_eq!(new_chain.current_model_index, 1);
        assert_eq!(new_chain.current_model(), Some(&"model2".to_string()));
    }

    #[test]
    fn test_agent_chain_advance_to_next_model_switches_agent_when_models_exhausted() {
        let chain = AgentChainState::initial().with_agents(
            vec!["agent-a".to_string(), "agent-b".to_string()],
            vec![
                vec!["a1".to_string(), "a2".to_string()],
                vec!["b1".to_string()],
            ],
            AgentRole::Developer,
        );

        let chain = chain.advance_to_next_model(); // a1 -> a2
        assert_eq!(chain.current_agent(), Some(&"agent-a".to_string()));
        assert_eq!(chain.current_model(), Some(&"a2".to_string()));

        // Exhausted models for agent-a; should move to agent-b instead of looping models.
        let chain = chain.advance_to_next_model();
        assert_eq!(chain.current_agent(), Some(&"agent-b".to_string()));
        assert_eq!(chain.current_model_index, 0);
        assert_eq!(chain.current_model(), Some(&"b1".to_string()));
    }

    #[test]
    fn test_agent_chain_switch_to_next_agent() {
        let chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string(), "codex".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        let new_chain = chain.switch_to_next_agent();
        assert_eq!(new_chain.current_agent_index, 1);
        assert_eq!(new_chain.current_agent(), Some(&"codex".to_string()));
        assert_eq!(new_chain.retry_cycle, 0);
    }

    #[test]
    fn test_agent_chain_exhausted() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        let chain = chain.start_retry_cycle();
        let chain = chain.start_retry_cycle();
        let chain = chain.start_retry_cycle();

        assert!(chain.is_exhausted());
    }

    #[test]
    fn test_rebase_state_not_started() {
        let state = RebaseState::NotStarted;
        assert!(!state.is_terminal());
        assert!(state.current_head().is_none());
        assert!(!state.is_in_progress());
    }

    #[test]
    fn test_rebase_state_in_progress() {
        let state = RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        };
        assert!(!state.is_terminal());
        assert_eq!(state.current_head(), Some("abc123".to_string()));
        assert!(state.is_in_progress());
    }

    #[test]
    fn test_rebase_state_completed() {
        let state = RebaseState::Completed {
            new_head: "def456".to_string(),
        };
        assert!(state.is_terminal());
        assert_eq!(state.current_head(), Some("def456".to_string()));
        assert!(!state.is_in_progress());
    }

    fn make_checkpoint_for_state(
        phase: CheckpointPhase,
        rebase_state: CheckpointRebaseState,
    ) -> PipelineCheckpoint {
        let run_id = uuid::Uuid::new_v4().to_string();
        PipelineCheckpoint::from_params(CheckpointParams {
            phase,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args: CliArgsSnapshot::new(5, 2, None, true, 2, false, None),
            developer_agent_config: AgentConfigSnapshot::new(
                "claude".into(),
                "cmd".into(),
                "-o".into(),
                None,
                true,
            ),
            reviewer_agent_config: AgentConfigSnapshot::new(
                "codex".into(),
                "cmd".into(),
                "-o".into(),
                None,
                true,
            ),
            rebase_state,
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: 2,
            actual_reviewer_runs: 0,
            working_dir: "/test/repo".to_string(),
            prompt_md_checksum: None,
            config_path: None,
            config_checksum: None,
        })
    }

    #[test]
    fn test_pipeline_state_from_checkpoint_phase_mapping() {
        let checkpoint = make_checkpoint_for_state(
            CheckpointPhase::Development,
            CheckpointRebaseState::NotStarted,
        );
        let state: PipelineState = checkpoint.into();
        assert_eq!(state.phase, PipelinePhase::Development);
    }

    #[test]
    fn test_pipeline_state_from_checkpoint_rebase_conflicts() {
        let checkpoint = make_checkpoint_for_state(
            CheckpointPhase::PreRebaseConflict,
            CheckpointRebaseState::HasConflicts {
                files: vec!["file1.rs".to_string()],
            },
        );
        let state: PipelineState = checkpoint.into();
        assert!(matches!(state.rebase, RebaseState::Conflicted { .. }));
    }

    #[test]
    fn test_commit_state_not_started() {
        let state = CommitState::NotStarted;
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_commit_state_generating() {
        let state = CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        };
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_commit_state_committed() {
        let state = CommitState::Committed {
            hash: "abc123".to_string(),
        };
        assert!(state.is_terminal());
    }

    #[test]
    fn test_is_complete_during_finalizing() {
        // Finalizing phase should NOT be complete - event loop must continue
        // to execute the RestorePromptPermissions effect
        let state = PipelineState {
            phase: PipelinePhase::Finalizing,
            ..PipelineState::initial(5, 2)
        };
        assert!(
            !state.is_complete(),
            "Finalizing phase should not be complete - event loop must continue"
        );
    }

    #[test]
    fn test_is_complete_after_finalization() {
        // Complete phase IS complete
        let state = PipelineState {
            phase: PipelinePhase::Complete,
            ..PipelineState::initial(5, 2)
        };
        assert!(state.is_complete(), "Complete phase should be complete");
    }

    // =========================================================================
    // Continuation state tests
    // =========================================================================

    #[test]
    fn test_continuation_state_initial() {
        let state = ContinuationState::new();
        assert!(!state.is_continuation());
        assert_eq!(state.continuation_attempt, 0);
        assert!(state.previous_status.is_none());
        assert!(state.previous_summary.is_none());
        assert!(state.previous_files_changed.is_none());
        assert!(state.previous_next_steps.is_none());
    }

    #[test]
    fn test_continuation_state_default() {
        let state = ContinuationState::default();
        assert!(!state.is_continuation());
        assert_eq!(state.continuation_attempt, 0);
    }

    #[test]
    fn test_continuation_trigger_partial() {
        let state = ContinuationState::new();
        let new_state = state.trigger_continuation(
            DevelopmentStatus::Partial,
            "Did some work".to_string(),
            Some(vec!["file1.rs".to_string()]),
            Some("Continue with tests".to_string()),
        );

        assert!(new_state.is_continuation());
        assert_eq!(new_state.continuation_attempt, 1);
        assert_eq!(new_state.previous_status, Some(DevelopmentStatus::Partial));
        assert_eq!(
            new_state.previous_summary,
            Some("Did some work".to_string())
        );
        assert_eq!(
            new_state.previous_files_changed,
            Some(vec!["file1.rs".to_string()])
        );
        assert_eq!(
            new_state.previous_next_steps,
            Some("Continue with tests".to_string())
        );
    }

    #[test]
    fn test_continuation_trigger_failed() {
        let state = ContinuationState::new();
        let new_state = state.trigger_continuation(
            DevelopmentStatus::Failed,
            "Build failed".to_string(),
            None,
            Some("Fix errors".to_string()),
        );

        assert!(new_state.is_continuation());
        assert_eq!(new_state.continuation_attempt, 1);
        assert_eq!(new_state.previous_status, Some(DevelopmentStatus::Failed));
        assert_eq!(new_state.previous_summary, Some("Build failed".to_string()));
        assert!(new_state.previous_files_changed.is_none());
        assert_eq!(
            new_state.previous_next_steps,
            Some("Fix errors".to_string())
        );
    }

    #[test]
    fn test_continuation_reset() {
        let state = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );

        let reset = state.reset();
        assert!(!reset.is_continuation());
        assert_eq!(reset.continuation_attempt, 0);
        assert!(reset.previous_status.is_none());
        assert!(reset.previous_summary.is_none());
    }

    #[test]
    fn test_multiple_continuations() {
        let state = ContinuationState::new()
            .trigger_continuation(
                DevelopmentStatus::Partial,
                "First".to_string(),
                Some(vec!["a.rs".to_string()]),
                None,
            )
            .trigger_continuation(
                DevelopmentStatus::Partial,
                "Second".to_string(),
                Some(vec!["b.rs".to_string()]),
                Some("Do more".to_string()),
            );

        assert_eq!(state.continuation_attempt, 2);
        assert_eq!(state.previous_summary, Some("Second".to_string()));
        assert_eq!(state.previous_files_changed, Some(vec!["b.rs".to_string()]));
        assert_eq!(state.previous_next_steps, Some("Do more".to_string()));
    }

    #[test]
    fn test_development_status_display() {
        assert_eq!(format!("{}", DevelopmentStatus::Completed), "completed");
        assert_eq!(format!("{}", DevelopmentStatus::Partial), "partial");
        assert_eq!(format!("{}", DevelopmentStatus::Failed), "failed");
    }

    #[test]
    fn test_pipeline_state_initial_has_empty_continuation() {
        let state = PipelineState::initial(5, 2);
        assert!(!state.continuation.is_continuation());
        assert_eq!(state.continuation.continuation_attempt, 0);
    }

    #[test]
    fn test_agent_chain_reset_clears_rate_limit_continuation_prompt() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        chain.rate_limit_continuation_prompt = Some("saved".to_string());

        let reset = chain.reset();
        assert!(
            reset.rate_limit_continuation_prompt.is_none(),
            "reset() should clear rate_limit_continuation_prompt"
        );
    }

    #[test]
    fn test_agent_chain_reset_for_role_clears_rate_limit_continuation_prompt() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        chain.rate_limit_continuation_prompt = Some("saved".to_string());

        let reset = chain.reset_for_role(AgentRole::Reviewer);
        assert!(
            reset.rate_limit_continuation_prompt.is_none(),
            "reset_for_role() should clear rate_limit_continuation_prompt"
        );
    }

    #[test]
    fn test_switch_to_next_agent_with_prompt_advances_retry_cycle_when_single_agent() {
        let chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let next = chain.switch_to_next_agent_with_prompt(Some("prompt".to_string()));
        assert!(
            !next.is_exhausted(),
            "single-agent rate limit fallback should not immediately exhaust the chain"
        );
        assert_eq!(next.retry_cycle, 1);
    }

    #[test]
    fn test_switch_to_next_agent_with_prompt_advances_retry_cycle_on_wraparound() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        chain.current_agent_index = 1;

        let next = chain.switch_to_next_agent_with_prompt(Some("prompt".to_string()));
        assert!(
            !next.is_exhausted(),
            "rate limit fallback should not immediately exhaust on wraparound"
        );
        assert_eq!(next.retry_cycle, 1);
    }

    // =========================================================================
    // XSD retry and session tracking tests
    // =========================================================================

    #[test]
    fn test_artifact_type_display() {
        assert_eq!(format!("{}", ArtifactType::Plan), "plan");
        assert_eq!(
            format!("{}", ArtifactType::DevelopmentResult),
            "development_result"
        );
        assert_eq!(format!("{}", ArtifactType::Issues), "issues");
        assert_eq!(format!("{}", ArtifactType::FixResult), "fix_result");
        assert_eq!(format!("{}", ArtifactType::CommitMessage), "commit_message");
    }

    #[test]
    fn test_continuation_state_with_limits() {
        let state = ContinuationState::with_limits(5, 2);
        assert_eq!(state.max_xsd_retry_count, 5);
        assert_eq!(state.max_continue_count, 2);
        assert!(!state.is_continuation());
    }

    #[test]
    fn test_continuation_state_default_limits() {
        let state = ContinuationState::new();
        assert_eq!(state.max_xsd_retry_count, 10);
        assert_eq!(state.max_continue_count, 3);
    }

    #[test]
    fn test_continuation_reset_preserves_limits() {
        let state = ContinuationState::with_limits(5, 2)
            .trigger_xsd_retry()
            .trigger_xsd_retry();
        assert_eq!(state.xsd_retry_count, 2);

        let reset = state.reset();
        assert_eq!(reset.xsd_retry_count, 0);
        assert_eq!(reset.max_xsd_retry_count, 5);
        assert_eq!(reset.max_continue_count, 2);
    }

    #[test]
    fn test_continuation_with_artifact() {
        let state = ContinuationState::new().with_artifact(ArtifactType::DevelopmentResult);
        assert_eq!(
            state.current_artifact,
            Some(ArtifactType::DevelopmentResult)
        );
        assert_eq!(state.xsd_retry_count, 0);
        assert!(!state.xsd_retry_pending);
    }

    #[test]
    fn test_xsd_retry_trigger() {
        let state = ContinuationState::new()
            .with_artifact(ArtifactType::Plan)
            .trigger_xsd_retry();

        assert!(state.xsd_retry_pending);
        assert_eq!(state.xsd_retry_count, 1);
        assert_eq!(state.current_artifact, Some(ArtifactType::Plan));
    }

    #[test]
    fn test_xsd_retry_clear_pending() {
        let state = ContinuationState::new()
            .trigger_xsd_retry()
            .clear_xsd_retry_pending();

        assert!(!state.xsd_retry_pending);
        assert_eq!(state.xsd_retry_count, 1);
    }

    #[test]
    fn test_xsd_retries_exhausted() {
        let state = ContinuationState::with_limits(2, 3);
        assert!(!state.xsd_retries_exhausted());

        let state = state.trigger_xsd_retry();
        assert!(!state.xsd_retries_exhausted());

        let state = state.trigger_xsd_retry();
        assert!(state.xsd_retries_exhausted());
    }

    #[test]
    fn test_continue_trigger() {
        let state = ContinuationState::new().trigger_continue();
        assert!(state.continue_pending);
    }

    #[test]
    fn test_continue_clear_pending() {
        let state = ContinuationState::new()
            .trigger_continue()
            .clear_continue_pending();
        assert!(!state.continue_pending);
    }

    #[test]
    fn test_continuations_exhausted() {
        let state = ContinuationState::with_limits(10, 2);
        assert!(!state.continuations_exhausted());

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "First".to_string(), None, None);
        assert!(!state.continuations_exhausted());

        let state = state.trigger_continuation(
            DevelopmentStatus::Partial,
            "Second".to_string(),
            None,
            None,
        );
        assert!(state.continuations_exhausted());
    }

    #[test]
    fn test_continuations_exhausted_semantics() {
        // Test the documented semantics: max_continue_count=3 means 3 total attempts
        // Attempts 0, 1, 2 are allowed; attempt 3+ triggers exhaustion
        let state = ContinuationState::with_limits(10, 3);
        assert_eq!(state.continuation_attempt, 0);
        assert!(
            !state.continuations_exhausted(),
            "attempt 0 should not be exhausted"
        );

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "1".to_string(), None, None);
        assert_eq!(state.continuation_attempt, 1);
        assert!(
            !state.continuations_exhausted(),
            "attempt 1 should not be exhausted"
        );

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "2".to_string(), None, None);
        assert_eq!(state.continuation_attempt, 2);
        assert!(
            !state.continuations_exhausted(),
            "attempt 2 should not be exhausted"
        );

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "3".to_string(), None, None);
        assert_eq!(state.continuation_attempt, 3);
        assert!(
            state.continuations_exhausted(),
            "attempt 3 should be exhausted with max_continue_count=3"
        );
    }

    #[test]
    fn test_xsd_retries_exhausted_with_zero_max() {
        // max_xsd_retry_count=0 means XSD retries are disabled (immediate agent fallback)
        let state = ContinuationState::with_limits(10, 3).with_max_xsd_retry(0);
        assert!(
            state.xsd_retries_exhausted(),
            "0 max retries should be immediately exhausted"
        );
    }

    #[test]
    fn test_trigger_continuation_resets_xsd_retry() {
        let state = ContinuationState::new()
            .with_artifact(ArtifactType::DevelopmentResult)
            .trigger_xsd_retry()
            .trigger_xsd_retry()
            .trigger_continuation(
                DevelopmentStatus::Partial,
                "Work done".to_string(),
                None,
                None,
            );

        assert_eq!(state.xsd_retry_count, 0);
        assert!(!state.xsd_retry_pending);
        // continue_pending is now set to true by trigger_continuation to enable
        // orchestration to derive the continuation effect
        assert!(state.continue_pending);
        assert_eq!(
            state.current_artifact,
            Some(ArtifactType::DevelopmentResult)
        );
    }

    #[test]
    fn test_agent_chain_session_id() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("session-123".to_string()));

        assert_eq!(chain.last_session_id, Some("session-123".to_string()));
    }

    #[test]
    fn test_agent_chain_clear_session_id() {
        let chain = AgentChainState::initial()
            .with_session_id(Some("session-123".to_string()))
            .clear_session_id();

        assert!(chain.last_session_id.is_none());
    }

    #[test]
    fn test_agent_chain_reset_clears_session_id() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        chain.last_session_id = Some("session-123".to_string());

        let reset = chain.reset();
        assert!(
            reset.last_session_id.is_none(),
            "reset() should clear last_session_id"
        );
    }

    #[test]
    fn test_agent_chain_reset_for_role_clears_session_id() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        chain.last_session_id = Some("session-123".to_string());

        let reset = chain.reset_for_role(AgentRole::Reviewer);
        assert!(
            reset.last_session_id.is_none(),
            "reset_for_role() should clear last_session_id"
        );
    }

    // =========================================================================
    // FixStatus tests
    // =========================================================================

    #[test]
    fn test_fix_status_parse() {
        assert_eq!(
            FixStatus::parse("all_issues_addressed"),
            Some(FixStatus::AllIssuesAddressed)
        );
        assert_eq!(
            FixStatus::parse("issues_remain"),
            Some(FixStatus::IssuesRemain)
        );
        assert_eq!(
            FixStatus::parse("no_issues_found"),
            Some(FixStatus::NoIssuesFound)
        );
        assert_eq!(FixStatus::parse("failed"), Some(FixStatus::Failed));
        assert_eq!(FixStatus::parse("unknown"), None);
    }

    #[test]
    fn test_fix_status_display() {
        assert_eq!(
            format!("{}", FixStatus::AllIssuesAddressed),
            "all_issues_addressed"
        );
        assert_eq!(format!("{}", FixStatus::IssuesRemain), "issues_remain");
        assert_eq!(format!("{}", FixStatus::NoIssuesFound), "no_issues_found");
        assert_eq!(format!("{}", FixStatus::Failed), "failed");
    }

    #[test]
    fn test_fix_status_is_complete() {
        assert!(FixStatus::AllIssuesAddressed.is_complete());
        assert!(FixStatus::NoIssuesFound.is_complete());
        assert!(!FixStatus::IssuesRemain.is_complete());
        assert!(!FixStatus::Failed.is_complete());
    }

    #[test]
    fn test_fix_status_needs_continuation() {
        assert!(!FixStatus::AllIssuesAddressed.needs_continuation());
        assert!(!FixStatus::NoIssuesFound.needs_continuation());
        assert!(FixStatus::IssuesRemain.needs_continuation());
        assert!(
            FixStatus::Failed.needs_continuation(),
            "Failed status should trigger continuation like IssuesRemain"
        );
    }
}
