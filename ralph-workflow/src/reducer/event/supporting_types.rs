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
