//! Common utilities shared across phase modules.

use crate::agents::AgentRole;
use crate::phases::PhaseContext;

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
