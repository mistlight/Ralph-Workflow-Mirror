//! Configuration loading and agent registry initialization.
//!
//! This module handles:
//! - Loading configuration from environment and CLI arguments
//! - Initializing the agent registry with merged configs (global + local)
//! - Selecting default agents from fallback chains
//! - Resolving the agents.toml config path relative to repo root

use crate::agents::{AgentRegistry, AgentRole, ConfigSource};
use crate::cli::{
    apply_args_to_config, ensure_config_or_create, handle_init, handle_init_global, Args,
};
use crate::colors::Colors;
use crate::config::Config;
use crate::git_helpers::get_repo_root;
use crate::utils::Logger;
use std::path::PathBuf;

/// Result of configuration initialization.
pub struct ConfigInitResult {
    /// The loaded configuration with CLI args applied.
    pub config: Config,
    /// The agent registry with merged configs.
    pub registry: AgentRegistry,
    /// The resolved path to agents.toml.
    pub agents_config_path: PathBuf,
    /// Sources from which agent configs were loaded.
    pub config_sources: Vec<ConfigSource>,
}

/// Initializes configuration and agent registry.
///
/// This function performs the following steps:
/// 1. Loads config from environment variables
/// 2. Applies CLI arguments to config
/// 3. Handles --init-global and --init flags if set
/// 4. Resolves agents.toml path relative to repo root
/// 5. Loads agent registry with merged configs (global + local)
/// 6. Selects default agents from fallback chains
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments
/// * `colors` - Color configuration for output
/// * `logger` - Logger for info/warning messages
///
/// # Returns
///
/// Returns `Ok(Some(result))` on success, `Ok(None)` if an early exit was triggered
/// (e.g., --init), or an error if initialization fails.
pub fn initialize_config(
    args: &Args,
    colors: &Colors,
    logger: &mut Logger,
) -> anyhow::Result<Option<ConfigInitResult>> {
    // Load configuration
    let mut config = Config::from_env().with_commit_msg(args.commit_msg.clone());

    // Apply CLI arguments to config
    apply_args_to_config(args, &mut config, colors);

    // Handle --init-global flag: create global agents.toml if it doesn't exist and exit
    if args.init_global && handle_init_global(colors)? {
        return Ok(None);
    }

    // Resolve config path relative to repo root (for git worktree support)
    let repo_root_for_config = get_repo_root().ok();
    let agents_config_path = if config.agents_config_path.is_relative() {
        repo_root_for_config
            .as_ref()
            .map(|root| root.join(&config.agents_config_path))
            .unwrap_or_else(|| config.agents_config_path.clone())
    } else {
        config.agents_config_path.clone()
    };

    // Handle --init flag: create agents.toml if it doesn't exist and exit
    if args.init && handle_init(colors, &agents_config_path)? {
        return Ok(None);
    }

    // Check if agents.toml exists; if not, create it and prompt user
    if ensure_config_or_create(colors, &agents_config_path, logger)? {
        return Ok(None);
    }

    // Initialize agent registry with merged configs (global + local)
    let (registry, config_sources) = load_agent_registry(&agents_config_path, colors, logger)?;

    // Log if no config files were found (but we still have built-in defaults)
    if config_sources.is_empty() {
        logger.info(&format!(
            "Using built-in agent defaults {}(no agents.toml found){}",
            colors.dim(),
            colors.reset()
        ));
    }

    // Apply default agents from fallback chains
    apply_default_agents(&mut config, &registry);

    Ok(Some(ConfigInitResult {
        config,
        registry,
        agents_config_path,
        config_sources,
    }))
}

/// Loads the agent registry with merged configs from global and local sources.
///
/// Priority: built-in defaults < global config < local config
fn load_agent_registry(
    agents_config_path: &PathBuf,
    colors: &Colors,
    logger: &mut Logger,
) -> anyhow::Result<(AgentRegistry, Vec<ConfigSource>)> {
    match AgentRegistry::with_merged_configs(agents_config_path) {
        Ok((registry, sources, warnings)) => {
            for warning in warnings {
                logger.warn(&warning);
            }
            // Log which configs were loaded
            if !sources.is_empty() {
                for source in &sources {
                    logger.info(&format!(
                        "Loaded {} agents from {}{}{}",
                        source.agents_loaded,
                        colors.cyan(),
                        source.path.display(),
                        colors.reset()
                    ));
                }
            }
            Ok((registry, sources))
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to load agents config from {}: {}, using defaults",
                agents_config_path.display(),
                e
            ));
            let registry = AgentRegistry::new().map_err(|defaults_err| {
                anyhow::anyhow!(
                    "Failed to load built-in default agents config (examples/agents.toml): {}",
                    defaults_err
                )
            })?;
            Ok((registry, Vec::new()))
        }
    }
}

/// Applies default agent selection from fallback chains.
///
/// If no agent was explicitly selected via CLI/env/preset, uses the first entry
/// from the agent_chain configuration.
fn apply_default_agents(config: &mut Config, registry: &AgentRegistry) {
    if config.developer_agent.is_none() {
        config.developer_agent = registry
            .fallback_config()
            .get_fallbacks(AgentRole::Developer)
            .first()
            .cloned();
    }
    if config.reviewer_agent.is_none() {
        config.reviewer_agent = registry
            .fallback_config()
            .get_fallbacks(AgentRole::Reviewer)
            .first()
            .cloned();
    }
}
