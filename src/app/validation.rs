//! Agent validation and chain validation.
//!
//! This module handles validation of agents and agent chains:
//! - Resolving required agent names from config
//! - Validating that agent commands exist in the registry
//! - Enforcing workflow-capable agents (can_commit=true)
//! - Validating agent chain configuration

use crate::agents::AgentRegistry;
use crate::colors::Colors;
use crate::config::Config;
use std::path::Path;

/// Result of agent validation containing the resolved agent names.
pub struct ValidatedAgents {
    /// The validated developer agent name.
    pub developer_agent: String,
    /// The validated reviewer agent name.
    pub reviewer_agent: String,
}

/// Resolves and validates the required agent names from configuration.
///
/// Both developer and reviewer agents must be configured at this point,
/// either via CLI args, environment variables, or agent_chain defaults.
///
/// # Arguments
///
/// * `config` - The pipeline configuration
///
/// # Returns
///
/// Returns the validated agent names or an error if agents are not configured.
pub fn resolve_required_agents(config: &Config) -> anyhow::Result<ValidatedAgents> {
    let developer_agent = config.developer_agent.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "No developer agent configured.\n\
            Set via --developer-agent, RALPH_DEVELOPER_AGENT env, or [agent_chain] in ~/.config/ralph-workflow.toml."
        )
    })?;
    let reviewer_agent = config.reviewer_agent.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "No reviewer agent configured.\n\
            Set via --reviewer-agent, RALPH_REVIEWER_AGENT env, or [agent_chain] in ~/.config/ralph-workflow.toml."
        )
    })?;

    Ok(ValidatedAgents {
        developer_agent,
        reviewer_agent,
    })
}

/// Validates that agent commands exist in the registry.
///
/// Checks that both developer and reviewer agents have valid commands
/// defined either in the config or the registry.
///
/// # Arguments
///
/// * `config` - The pipeline configuration
/// * `registry` - The agent registry
/// * `developer_agent` - Name of the developer agent
/// * `reviewer_agent` - Name of the reviewer agent
/// * `config_path` - Path to the unified config file for error messages
///
/// # Returns
///
/// Returns `Ok(())` if validation passes, or an error with details.
pub fn validate_agent_commands(
    config: &Config,
    registry: &AgentRegistry,
    developer_agent: &str,
    reviewer_agent: &str,
    config_path: &Path,
) -> anyhow::Result<()> {
    // Validate developer command exists
    if config.developer_cmd.is_none() {
        registry.developer_cmd(developer_agent).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown developer agent '{}'. Use --list-agents or define it in {} under [agents].",
                developer_agent,
                config_path.display()
            )
        })?;
    }

    // Validate reviewer command exists
    if config.reviewer_cmd.is_none() {
        registry.reviewer_cmd(reviewer_agent).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown reviewer agent '{}'. Use --list-agents or define it in {} under [agents].",
                reviewer_agent,
                config_path.display()
            )
        })?;
    }

    Ok(())
}

/// Validates that agents are workflow-capable (can_commit=true).
///
/// Agents with can_commit=false are chat-only / non-tool agents and will
/// stall Ralph's workflow. This validation is skipped if a custom command
/// override is provided.
///
/// # Arguments
///
/// * `config` - The pipeline configuration
/// * `registry` - The agent registry
/// * `developer_agent` - Name of the developer agent
/// * `reviewer_agent` - Name of the reviewer agent
/// * `config_path` - Path to the unified config file for error messages
///
/// # Returns
///
/// Returns `Ok(())` if validation passes, or an error with details.
pub fn validate_can_commit(
    config: &Config,
    registry: &AgentRegistry,
    developer_agent: &str,
    reviewer_agent: &str,
    config_path: &Path,
) -> anyhow::Result<()> {
    // Enforce workflow-capable agents unless custom command override provided
    if config.developer_cmd.is_none() {
        if let Some(cfg) = registry.get(developer_agent) {
            if !cfg.can_commit {
                anyhow::bail!(
                    "Developer agent '{}' has can_commit=false and cannot run Ralph's workflow.\n\
                    Fix: choose a different agent (see --list-agents) or set can_commit=true in {} under [agents].",
                    developer_agent,
                    config_path.display()
                );
            }
        }
    }
    if config.reviewer_cmd.is_none() {
        if let Some(cfg) = registry.get(reviewer_agent) {
            if !cfg.can_commit {
                anyhow::bail!(
                    "Reviewer agent '{}' has can_commit=false and cannot run Ralph's workflow.\n\
                    Fix: choose a different agent (see --list-agents) or set can_commit=true in {} under [agents].",
                    reviewer_agent,
                    config_path.display()
                );
            }
        }
    }

    Ok(())
}

/// Validates that agent chains are properly configured.
///
/// Displays an error and exits if the agent chains are not configured.
///
/// # Arguments
///
/// * `registry` - The agent registry
/// * `colors` - Color configuration for output
pub fn validate_agent_chains(registry: &AgentRegistry, colors: &Colors) {
    if let Err(msg) = registry.validate_agent_chains() {
        eprintln!();
        eprintln!(
            "{}{}Error:{} {}",
            colors.bold(),
            colors.red(),
            colors.reset(),
            msg
        );
        eprintln!();
        eprintln!(
            "{}Hint:{} Run 'ralph --init-global' to create ~/.config/ralph-workflow.toml.",
            colors.yellow(),
            colors.reset()
        );
        eprintln!();
        std::process::exit(1);
    }
}
