//! Configuration loading and agent registry initialization.
//!
//! This module handles:
//! - Loading configuration from the unified config file (~/.config/ralph-workflow.toml)
//! - Applying environment variable and CLI overrides
//! - Selecting default agents from fallback chains
//! - Loading agent registry data from unified config

use crate::agents::{global_agents_config_path, AgentRegistry, AgentRole, ConfigSource};
use crate::cli::{
    apply_args_to_config, handle_init_global, handle_init_legacy, handle_init_prompt,
    handle_list_templates, Args,
};
use crate::colors::Colors;
use crate::config::{loader, unified_config_path, Config, UnifiedConfig};
use crate::git_helpers::get_repo_root;
use crate::utils::Logger;
use std::path::PathBuf;

/// Result of configuration initialization.
pub struct ConfigInitResult {
    /// The loaded configuration with CLI args applied.
    pub config: Config,
    /// The agent registry with merged configs.
    pub registry: AgentRegistry,
    /// The resolved path to the unified config file (for diagnostics/errors).
    pub config_path: PathBuf,
    /// Sources from which agent configs were loaded.
    pub config_sources: Vec<ConfigSource>,
}

/// Initializes configuration and agent registry.
///
/// This function performs the following steps:
/// 1. Loads config from unified config file (~/.config/ralph-workflow.toml)
/// 2. Applies environment variable overrides
/// 3. Applies CLI arguments to config
/// 4. Handles --list-templates, --init-prompt, --init/--init-global (unified), and --init-legacy if set
/// 5. Loads agent registry from built-ins + unified config
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
/// (e.g., --init, --init-prompt, --list-templates), or an error if initialization fails.
pub fn initialize_config(
    args: &Args,
    colors: &Colors,
    logger: &mut Logger,
) -> anyhow::Result<Option<ConfigInitResult>> {
    // Load configuration from unified config file (with env overrides)
    let (mut config, unified, warnings) = if let Some(config_path) = &args.config {
        loader::load_config_from_path(Some(config_path.as_path()))
    } else {
        loader::load_config()
    };

    // Display any deprecation warnings from config loading
    for warning in warnings {
        logger.warn(&warning);
    }

    let config_path = args
        .config
        .clone()
        .or_else(unified_config_path)
        .unwrap_or_else(|| PathBuf::from("~/.config/ralph-workflow.toml"));

    // Set commit message from CLI
    config = config.with_commit_msg(args.commit_msg.clone());

    // Apply CLI arguments to config
    apply_args_to_config(args, &mut config, colors);

    // Handle --list-templates flag: display available templates and exit
    if args.list_templates && handle_list_templates(colors)? {
        return Ok(None);
    }

    // Handle --init-prompt flag: create PROMPT.md from template and exit
    if let Some(ref template_name) = args.init_prompt {
        if handle_init_prompt(template_name, colors)? {
            return Ok(None);
        }
    }

    // Handle unified init flags: create unified config if it doesn't exist and exit
    if (args.init_global || args.init) && handle_init_global(colors)? {
        return Ok(None);
    }

    // Handle --init-legacy flag: legacy per-repo agents.toml creation and exit
    if args.init_legacy {
        let repo_root = get_repo_root().ok();
        let legacy_path = repo_root.map_or_else(
            || PathBuf::from(".agent/agents.toml"),
            |root| root.join(".agent/agents.toml"),
        );
        if handle_init_legacy(colors, &legacy_path)? {
            return Ok(None);
        }
    }

    // Initialize agent registry with built-in defaults + unified config.
    let (registry, config_sources) = load_agent_registry(&unified, config_path.as_path())?;

    // Apply default agents from fallback chains
    apply_default_agents(&mut config, &registry);

    Ok(Some(ConfigInitResult {
        config,
        registry,
        config_path,
        config_sources,
    }))
}

fn load_agent_registry(
    unified: &Option<UnifiedConfig>,
    config_path: &std::path::Path,
) -> anyhow::Result<(AgentRegistry, Vec<ConfigSource>)> {
    let mut registry = AgentRegistry::new().map_err(|e| {
        anyhow::anyhow!("Failed to load built-in default agents config (examples/agents.toml): {e}")
    })?;

    let mut sources = Vec::new();

    // Backwards compatibility: load legacy agent config files only when unified config
    // isn't present (this matches the deprecation warning behavior in config loader).
    if unified.is_none() {
        if let Some(global_path) = global_agents_config_path() {
            if global_path.exists() {
                let loaded = registry.load_from_file(&global_path).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to load legacy global agent config {}: {}",
                        global_path.display(),
                        e
                    )
                })?;
                sources.push(ConfigSource {
                    path: global_path,
                    agents_loaded: loaded,
                });
            }
        }

        let repo_root = get_repo_root().ok();
        let project_path = repo_root.map_or_else(
            || PathBuf::from(".agent/agents.toml"),
            |root| root.join(".agent/agents.toml"),
        );
        if project_path.exists() {
            let loaded = registry.load_from_file(&project_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to load legacy per-repo agent config {}: {}",
                    project_path.display(),
                    e
                )
            })?;
            sources.push(ConfigSource {
                path: project_path,
                agents_loaded: loaded,
            });
        }
    }

    if let Some(unified_cfg) = unified {
        let loaded = registry.apply_unified_config(unified_cfg);
        if loaded > 0 || unified_cfg.agent_chain.is_some() {
            sources.push(ConfigSource {
                path: config_path.to_path_buf(),
                agents_loaded: loaded,
            });
        }
    }

    Ok((registry, sources))
}

/// Applies default agent selection from fallback chains.
///
/// If no agent was explicitly selected via CLI/env/preset, uses the first entry
/// from the `agent_chain` configuration.
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
