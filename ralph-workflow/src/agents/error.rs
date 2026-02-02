//! Error classification for agent failures.
//!
//! This module provides error classification logic to determine appropriate
//! recovery strategies when agents fail. Different error types warrant
//! different responses: retry, fallback to another agent, or abort.

#[path = "error/glm_detection.rs"]
mod glm_detection;
#[path = "error/kind.rs"]
mod kind;

pub use glm_detection::{contains_glm_model, is_glm_like_agent};
pub use kind::AgentErrorKind;

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
