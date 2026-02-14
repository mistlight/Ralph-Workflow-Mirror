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
    pub fn with_artifact(mut self, artifact: ArtifactType) -> Self {
        // Reset XSD retry state when switching artifacts, preserve everything else
        self.current_artifact = Some(artifact);
        self.xsd_retry_count = 0;
        self.xsd_retry_pending = false;
        self.xsd_retry_session_reuse_pending = false;
        self.last_xsd_error = None;
        self.last_review_xsd_error = None;
        self.last_fix_xsd_error = None;
        self
    }

    /// Mark XSD validation as failed, triggering a retry.
    ///
    /// For XSD retry, we want to re-invoke the same agent in the same session when possible,
    /// to keep retries deterministic and to preserve provider-side context.
    pub fn trigger_xsd_retry(mut self) -> Self {
        self.xsd_retry_pending = true;
        self.xsd_retry_count += 1;
        self.xsd_retry_session_reuse_pending = true;
        self
    }

    /// Clear XSD retry pending flag after starting retry.
    pub fn clear_xsd_retry_pending(mut self) -> Self {
        self.xsd_retry_pending = false;
        self.last_xsd_error = None;
        self.last_review_xsd_error = None;
        self.last_fix_xsd_error = None;
        self
    }

    /// Check if XSD retries are exhausted.
    pub fn xsd_retries_exhausted(&self) -> bool {
        self.xsd_retry_count >= self.max_xsd_retry_count
    }

    /// Mark a same-agent retry as pending for a transient invocation failure.
    pub fn trigger_same_agent_retry(mut self, reason: SameAgentRetryReason) -> Self {
        self.same_agent_retry_pending = true;
        self.same_agent_retry_count += 1;
        self.same_agent_retry_reason = Some(reason);
        self
    }

    /// Clear same-agent retry pending flag after starting retry.
    pub fn clear_same_agent_retry_pending(mut self) -> Self {
        self.same_agent_retry_pending = false;
        self.same_agent_retry_reason = None;
        self
    }

    /// Check if same-agent retries are exhausted.
    pub fn same_agent_retries_exhausted(&self) -> bool {
        self.same_agent_retry_count >= self.max_same_agent_retry_count
    }

    /// Mark continuation as pending (output valid but work incomplete).
    pub fn trigger_continue(mut self) -> Self {
        self.continue_pending = true;
        self
    }

    /// Clear continue pending flag after starting continuation.
    pub fn clear_continue_pending(mut self) -> Self {
        self.continue_pending = false;
        self
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
        mut self,
        status: DevelopmentStatus,
        summary: String,
        files_changed: Option<Vec<String>>,
        next_steps: Option<String>,
    ) -> Self {
        self.previous_status = Some(status);
        self.previous_summary = Some(summary);
        self.previous_files_changed = files_changed;
        self.previous_next_steps = next_steps;
        self.continuation_attempt += 1;
        self.invalid_output_attempts = 0;
        self.context_write_pending = true;
        self.context_cleanup_pending = false;
        // Reset XSD retry count for new continuation attempt
        self.xsd_retry_count = 0;
        self.xsd_retry_pending = false;
        self.xsd_retry_session_reuse_pending = false;
        self.last_xsd_error = None;
        self.last_review_xsd_error = None;
        self.last_fix_xsd_error = None;
        // Reset same-agent retry state for new continuation attempt
        self.same_agent_retry_count = 0;
        self.same_agent_retry_pending = false;
        self.same_agent_retry_reason = None;
        // Set continue_pending to trigger continuation in orchestration
        self.continue_pending = true;
        // Fix continuation fields and loop detection already preserved
        self
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
    pub fn trigger_fix_continuation(mut self, status: FixStatus, summary: Option<String>) -> Self {
        self.fix_status = Some(status);
        self.fix_previous_summary = summary;
        self.fix_continuation_attempt += 1;
        self.fix_continue_pending = true;
        // Reset XSD retry state for new continuation
        self.xsd_retry_count = 0;
        self.xsd_retry_pending = false;
        self.xsd_retry_session_reuse_pending = false;
        self.last_xsd_error = None;
        self.last_review_xsd_error = None;
        self.last_fix_xsd_error = None;
        // Reset invalid output attempts for new continuation
        self.invalid_output_attempts = 0;
        // Clear other pending flags
        self.context_write_pending = false;
        self.context_cleanup_pending = false;
        self.continue_pending = false;
        self
    }

    /// Clear fix continuation pending flag after starting continuation.
    pub fn clear_fix_continue_pending(mut self) -> Self {
        self.fix_continue_pending = false;
        self
    }

    /// Reset fix continuation state (e.g., when entering a new review pass).
    pub fn reset_fix_continuation(mut self) -> Self {
        self.fix_status = None;
        self.fix_previous_summary = None;
        self.fix_continuation_attempt = 0;
        self.fix_continue_pending = false;
        self
    }
}
