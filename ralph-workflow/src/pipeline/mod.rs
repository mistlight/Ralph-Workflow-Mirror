//! Pipeline Execution Module
//!
//! This module contains the core pipeline execution infrastructure:
//! - Types for tracking pipeline statistics and RAII cleanup
//! - Model flag resolution utilities
//! - Command execution helpers with fault-tolerant fallback chains
//!
//! # Module Structure
//!
//! - [`model_flag`] - Model flag resolution and provider detection
//! - [`runner`] - Pipeline runtime and command execution with fallback
//! - [`types`] - Pipeline statistics tracking and RAII guards

#![deny(unsafe_code)]

mod model_flag;
mod runner;
mod types;

#[cfg(test)]
pub use model_flag::resolve_model_with_provider;
pub use runner::{run_with_fallback, PipelineRuntime};
pub use types::{AgentPhaseGuard, Stats};

#[cfg(test)]
pub use runner::{run_with_prompt, PromptCommand};

#[cfg(test)]
mod tests;
