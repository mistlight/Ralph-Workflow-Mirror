//! Events for AwaitingDevFix phase.
//!
//! This phase handles pipeline failure remediation by invoking the development
//! agent to diagnose and fix the root cause before termination.

use crate::agents::AgentRole;
use serde::{Deserialize, Serialize};

// Import PipelinePhase from the parent event module (defined in mod.rs)
use crate::reducer::event::PipelinePhase;

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
    /// Recovery attempt initiated at a specific escalation level.
    ///
    /// Emitted when the dev-fix completes and the pipeline is ready to retry.
    /// The escalation level determines the recovery strategy.
    RecoveryAttempted {
        /// The recovery escalation level being attempted (1-4).
        level: u32,
        /// Number of recovery attempts so far for this failure.
        attempt_count: u32,
    },
    /// Recovery escalated to a higher level.
    ///
    /// Emitted when a recovery attempt fails and we escalate to a more
    /// aggressive recovery strategy (e.g., from retry → phase reset).
    RecoveryEscalated {
        /// Previous escalation level.
        from_level: u32,
        /// New escalation level.
        to_level: u32,
        /// Reason for escalation.
        reason: String,
    },
    /// Recovery succeeded - pipeline can resume normal operation.
    ///
    /// Emitted when a recovery attempt successfully fixes the issue
    /// (e.g., the retry succeeds, or the reset phase completes).
    RecoverySucceeded {
        /// The escalation level that succeeded.
        level: u32,
        /// Total attempts before success.
        total_attempts: u32,
    },
}
