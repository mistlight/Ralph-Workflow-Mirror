//! Phase execution context.
//!
//! This module defines the shared context that is passed to each phase
//! of the pipeline. It contains references to configuration, registry,
//! logging utilities, and runtime state that all phases need access to.

use crate::agents::{AgentRegistry, AgentRole};
use crate::config::Config;
use crate::guidelines::ReviewGuidelines;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::pipeline::Stats;
use crate::pipeline::Timer;

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

impl PhaseContext<'_> {}

/// Get the primary commit agent from the registry.
///
/// This function returns the name of the primary commit agent.
/// If a commit-specific agent is configured, it uses that. Otherwise, it falls back
/// to using the developer agent name (since commit messages should reflect development work).
pub fn get_primary_commit_agent(ctx: &PhaseContext<'_>) -> Option<String> {
    let fallback_config = ctx.registry.fallback_config();

    // First, try to get commit-specific agents
    let commit_agents = fallback_config.get_fallbacks(AgentRole::Commit);
    if !commit_agents.is_empty() {
        // Return the first commit agent as the primary
        return commit_agents.first().cloned();
    }

    // Fallback to using developer agents for commit generation
    let developer_agents = fallback_config.get_fallbacks(AgentRole::Developer);
    if !developer_agents.is_empty() {
        return developer_agents.first().cloned();
    }

    // Last resort: use the current developer agent
    Some(ctx.developer_agent.to_string())
}
