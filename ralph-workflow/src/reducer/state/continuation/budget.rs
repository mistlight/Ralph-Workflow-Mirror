//! Budget tracking logic for continuation attempts.
//!
//! Provides methods for tracking and checking budget exhaustion for:
//! - XSD retries
//! - Same-agent retries
//! - Development continuations
//! - Fix continuations

use super::super::{ArtifactType, DevelopmentStatus, FixStatus, SameAgentRetryReason};
use super::state::ContinuationState;

impl ContinuationState {
    /// Set the current artifact type being processed.
    pub fn with_artifact(&self, artifact: ArtifactType) -> Self {
        // Reset XSD retry state when switching artifacts, preserve everything else
        Self {
            current_artifact: Some(artifact),
            xsd_retry_count: 0,
            xsd_retry_pending: false,
            xsd_retry_session_reuse_pending: false,
            last_xsd_error: None,
            last_review_xsd_error: None,
            last_fix_xsd_error: None,
            ..self.clone()
        }
    }

    /// Mark XSD validation as failed, triggering a retry.
    ///
    /// For XSD retry, we want to re-invoke the same agent in the same session when possible,
    /// to keep retries deterministic and to preserve provider-side context.
    pub fn trigger_xsd_retry(&self) -> Self {
        Self {
            xsd_retry_pending: true,
            xsd_retry_count: self.xsd_retry_count + 1,
            xsd_retry_session_reuse_pending: true,
            ..self.clone()
        }
    }

    /// Clear XSD retry pending flag after starting retry.
    pub fn clear_xsd_retry_pending(&self) -> Self {
        Self {
            xsd_retry_pending: false,
            last_xsd_error: None,
            last_review_xsd_error: None,
            last_fix_xsd_error: None,
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
            xsd_retry_session_reuse_pending: false,
            last_xsd_error: None,
            last_review_xsd_error: None,
            last_fix_xsd_error: None,
            // Reset same-agent retry state for new continuation attempt
            same_agent_retry_count: 0,
            same_agent_retry_pending: false,
            same_agent_retry_reason: None,
            // Set continue_pending to trigger continuation in orchestration
            continue_pending: true,
            // Preserve artifact type and limits
            current_artifact: self.current_artifact,
            max_xsd_retry_count: self.max_xsd_retry_count,
            max_same_agent_retry_count: self.max_same_agent_retry_count,
            max_continue_count: self.max_continue_count,
            // Preserve fix continuation fields
            fix_status: self.fix_status,
            fix_previous_summary: self.fix_previous_summary.clone(),
            fix_continuation_attempt: self.fix_continuation_attempt,
            fix_continue_pending: self.fix_continue_pending,
            max_fix_continue_count: self.max_fix_continue_count,
            // Preserve loop detection fields
            last_effect_kind: self.last_effect_kind.clone(),
            consecutive_same_effect_count: self.consecutive_same_effect_count,
            max_consecutive_same_effect: self.max_consecutive_same_effect,
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
            xsd_retry_session_reuse_pending: false,
            last_xsd_error: None,
            last_review_xsd_error: None,
            last_fix_xsd_error: None,
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
