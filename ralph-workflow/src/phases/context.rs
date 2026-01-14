//! Phase execution context.
//!
//! This module defines the shared context that is passed to each phase
//! of the pipeline. It contains references to configuration, registry,
//! logging utilities, and runtime state that all phases need access to.

use crate::agents::AgentRegistry;
use crate::colors::Colors;
use crate::config::Config;
use crate::guidelines::ReviewGuidelines;
use crate::pipeline::Stats;
use crate::timer::Timer;
use crate::logger::Logger;

/// Shared context for all pipeline phases.
///
/// This struct holds references to all the shared state that phases need
/// to access. It is passed by mutable reference to each phase function.
pub struct PhaseContext<'a> {
    /// Configuration settings for the pipeline.
    pub config: &'a Config,
    /// Agent registry for looking up agent configurations.
    pub registry: &'a AgentRegistry,
    /// Logger for output and diagnostics.
    pub logger: &'a Logger,
    /// Terminal color configuration.
    pub colors: &'a Colors,
    /// Timer for tracking elapsed time.
    pub timer: &'a mut Timer,
    /// Statistics for tracking pipeline progress.
    pub stats: &'a mut Stats,
    /// Name of the developer agent.
    pub developer_agent: &'a str,
    /// Name of the reviewer agent.
    pub reviewer_agent: &'a str,
    /// Review guidelines based on detected project stack.
    pub review_guidelines: Option<&'a ReviewGuidelines>,
}

impl<'a> PhaseContext<'a> {}
