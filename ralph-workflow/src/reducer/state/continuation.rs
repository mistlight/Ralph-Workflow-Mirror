// Continuation state for development and fix iterations.
//
// Contains ContinuationState and its methods.

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
    /// Count of same-agent retry attempts for transient invocation failures.
    ///
    /// This is intentionally separate from XSD retry, which is only for invalid XML outputs.
    #[serde(default)]
    pub same_agent_retry_count: u32,
    /// Whether a same-agent retry is pending.
    ///
    /// Set to true by the reducer when a transient invocation failure occurs (timeout/internal).
    /// Cleared when the retry attempt starts or when switching agents.
    #[serde(default)]
    pub same_agent_retry_pending: bool,
    /// The reason for the pending same-agent retry, for prompt rendering.
    #[serde(default)]
    pub same_agent_retry_reason: Option<SameAgentRetryReason>,
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
    /// Maximum same-agent retry attempts for invocation failures that should not
    /// immediately trigger agent fallback (default 2).
    ///
    /// This is a failure budget for the current agent. For example, with a value of 2:
    /// - 1st failure → retry the same agent
    /// - 2nd failure → fall back to the next agent
    #[serde(default = "default_max_same_agent_retry_count")]
    pub max_same_agent_retry_count: u32,
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

const fn default_max_same_agent_retry_count() -> u32 {
    2
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
            same_agent_retry_count: 0,
            same_agent_retry_pending: false,
            same_agent_retry_reason: None,
            continue_pending: false,
            current_artifact: None,
            max_xsd_retry_count: default_max_xsd_retry_count(),
            max_same_agent_retry_count: default_max_same_agent_retry_count(),
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
    pub fn with_limits(
        max_xsd_retry_count: u32,
        max_continue_count: u32,
        max_same_agent_retry_count: u32,
    ) -> Self {
        Self {
            max_xsd_retry_count,
            max_same_agent_retry_count,
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

    /// Builder: set max same-agent retry count for transient invocation failures.
    ///
    /// Use 0 to disable same-agent retries (immediate agent fallback on timeout/internal error).
    pub fn with_max_same_agent_retry(mut self, max_same_agent_retry_count: u32) -> Self {
        self.max_same_agent_retry_count = max_same_agent_retry_count;
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
            max_same_agent_retry_count: self.max_same_agent_retry_count,
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

    /// Mark a same-agent retry as pending for a transient invocation failure.
    pub fn trigger_same_agent_retry(&self, reason: SameAgentRetryReason) -> Self {
        Self {
            same_agent_retry_pending: true,
            same_agent_retry_count: self.same_agent_retry_count + 1,
            same_agent_retry_reason: Some(reason),
            ..self.clone()
        }
    }

    /// Clear same-agent retry pending flag after starting retry.
    pub fn clear_same_agent_retry_pending(&self) -> Self {
        Self {
            same_agent_retry_pending: false,
            same_agent_retry_reason: None,
            ..self.clone()
        }
    }

    /// Check if same-agent retries are exhausted.
    pub fn same_agent_retries_exhausted(&self) -> bool {
        self.same_agent_retry_count >= self.max_same_agent_retry_count
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
            // Reset same-agent retry state for new continuation attempt
            same_agent_retry_count: 0,
            same_agent_retry_pending: false,
            same_agent_retry_reason: None,
            // Set continue_pending to trigger continuation in orchestration
            continue_pending: true,
            // Preserve artifact type and limits
            current_artifact: self.current_artifact.clone(),
            max_xsd_retry_count: self.max_xsd_retry_count,
            max_same_agent_retry_count: self.max_same_agent_retry_count,
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
