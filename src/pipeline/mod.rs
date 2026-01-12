//! Pipeline execution logic for the Ralph agent orchestrator.
//!
//! This module contains the core pipeline execution infrastructure:
//! - Types for tracking pipeline statistics and RAII cleanup
//! - Model flag resolution utilities
//! - Command execution helpers with fault-tolerant fallback chains

#![deny(unsafe_code)]

mod model_flag;
mod runner;
mod types;

#[cfg(test)]
pub(crate) use model_flag::resolve_model_with_provider;
pub(crate) use runner::{run_with_fallback, PipelineRuntime};
pub(crate) use types::{AgentPhaseGuard, Stats};

#[cfg(test)]
pub(crate) use runner::{run_with_prompt, PromptCommand};

#[cfg(test)]
mod tests;
