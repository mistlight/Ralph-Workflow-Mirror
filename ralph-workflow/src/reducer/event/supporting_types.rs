//! Supporting types for event definitions.
//!
//! This module contains enums and types that support event definitions
//! but are not events themselves.

use serde::{Deserialize, Serialize};

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

/// Whether a timed-out agent produced any output before the connection was cut.
///
/// Carried in `AgentEvent::TimedOut` so the reducer can apply different retry
/// policies for complete silences versus interrupted-but-working invocations.
///
/// # Serde Backward Compatibility
///
/// Old checkpoints did not carry `output_kind`; the field uses an explicit default
/// function (`default_timeout_output_kind`) that defaults to `PartialOutput` to
/// preserve pre-feature retry behavior (same-agent retry, not immediate switch).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeoutOutputKind {
    /// Agent produced no output at all — likely overloaded or unavailable.
    NoOutput,
    /// Agent produced partial output before timing out — likely a connectivity issue.
    PartialOutput,
}

/// Default function for serde backward compatibility.
///
/// Old checkpoints did not carry `output_kind`; default to `PartialOutput`
/// to preserve pre-feature retry behavior (same-agent retry, not immediate switch).
#[must_use]
pub const fn default_timeout_output_kind() -> TimeoutOutputKind {
    TimeoutOutputKind::PartialOutput
}
