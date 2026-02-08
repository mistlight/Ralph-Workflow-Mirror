//! ContinuationState struct definition.
//!
//! Contains the core state structure for tracking continuation and retry attempts
//! across development and fix iterations.

use super::super::{ArtifactType, DevelopmentStatus, FixStatus, SameAgentRetryReason};
use serde::{Deserialize, Serialize};

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
    /// Whether the next agent invocation should reuse the last session id.
    ///
    /// XSD retry is derived via `xsd_retry_pending`, but `xsd_retry_pending` is cleared
    /// as soon as the retry prompt is prepared to avoid re-deriving the prepare-prompt
    /// effect. This flag preserves the "reuse session id" signal for the subsequent
    /// invocation effect.
    #[serde(default)]
    pub xsd_retry_session_reuse_pending: bool,
    /// Last validation error message for XSD retry prompts (commit phase).
    ///
    /// This is set when validation fails and cleared when the retry attempt starts.
    #[serde(default)]
    pub last_xsd_error: Option<String>,
    /// Last XSD validation error for review issues XML (used in XSD retry prompt).
    ///
    /// This is set when review validation fails and cleared when transitioning away
    /// from review or when validation succeeds.
    #[serde(default)]
    pub last_review_xsd_error: Option<String>,
    /// Last XSD validation error for fix result XML (used in XSD retry prompt).
    ///
    /// This is set when fix validation fails and cleared when transitioning away
    /// from fix or when validation succeeds.
    #[serde(default)]
    pub last_fix_xsd_error: Option<String>,
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

    // =========================================================================
    // Loop detection fields (to prevent infinite tight loops)
    // =========================================================================
    /// Loop detection: last effect executed (for detecting repeats).
    #[serde(default)]
    pub last_effect_kind: Option<String>,

    /// Loop detection: count of consecutive identical effects.
    #[serde(default)]
    pub consecutive_same_effect_count: u32,

    /// Maximum consecutive identical effects before triggering recovery.
    #[serde(default = "default_max_consecutive_same_effect")]
    pub max_consecutive_same_effect: u32,
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

const fn default_max_consecutive_same_effect() -> u32 {
    20
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
            xsd_retry_session_reuse_pending: false,
            last_xsd_error: None,
            last_review_xsd_error: None,
            last_fix_xsd_error: None,
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
            // Loop detection fields
            last_effect_kind: None,
            consecutive_same_effect_count: 0,
            max_consecutive_same_effect: default_max_consecutive_same_effect(),
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
        // Preserve configured limits, reset everything else including loop detection counters.
        // The struct initialization below explicitly preserves max_* fields,
        // then the spread operator ..Self::default() resets ALL other fields
        // (including loop detection fields: last_effect_kind -> None,
        // consecutive_same_effect_count -> 0). This is intentional during
        // loop recovery to break the tight loop cycle and start fresh.
        Self {
            max_xsd_retry_count: self.max_xsd_retry_count,
            max_same_agent_retry_count: self.max_same_agent_retry_count,
            max_continue_count: self.max_continue_count,
            max_fix_continue_count: self.max_fix_continue_count,
            max_consecutive_same_effect: self.max_consecutive_same_effect,
            ..Self::default()
        }
    }
}
