//! Cloud types for progress reporting.

use serde::{Deserialize, Serialize};

/// Progress update payload for cloud API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Current pipeline phase
    pub phase: String,
    /// Previous phase (for transition tracking)
    pub previous_phase: Option<String>,
    /// Development iteration (1-based)
    pub iteration: Option<u32>,
    /// Total iterations configured
    pub total_iterations: Option<u32>,
    /// Review pass (1-based)
    pub review_pass: Option<u32>,
    /// Total review passes configured
    pub total_review_passes: Option<u32>,
    /// Human-readable status message
    pub message: String,
    /// Structured event type for programmatic consumption
    pub event_type: ProgressEventType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEventType {
    PipelineStarted,
    PhaseTransition { from: Option<String>, to: String },
    IterationStarted { iteration: u32 },
    IterationCompleted { iteration: u32, status: String },
    ReviewPassStarted { pass: u32 },
    ReviewPassCompleted { pass: u32, issues_found: bool },
    AgentInvoked { role: String, agent: String },
    AgentCompleted { role: String, duration_ms: u64 },
    CheckpointSaved,
    CommitCreated { sha: String },
    PushCompleted { remote: String, branch: String },
    PipelineCompleted { success: bool },
    PipelineInterrupted { reason: String },
    Heartbeat,
}

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("HTTP error {0}: {1}")]
    HttpError(u16, String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
