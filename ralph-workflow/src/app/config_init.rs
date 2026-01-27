//! Configuration loading and agent registry initialization.
//!
//! This module handles:
//! - Loading configuration from the unified config file (~/.config/ralph-workflow.toml)
//! - Applying environment variable and CLI overrides
//! - Selecting default agents from fallback chains
//! - Loading agent registry data from unified config
//! - Fetching and caching OpenCode API catalog for dynamic provider/model resolution
//!
//! # Dependency Injection
//!
//! The [`initialize_config_with`] function accepts both a [`CatalogLoader`] and a
//! [`ConfigEnvironment`] for full dependency injection. This enables testing without
//! network calls or environment variable dependencies.

use crate::agents::opencode_api::{CatalogLoader, RealCatalogLoader};
use crate::agents::{
    global_agents_config_path, validation as agent_validation, AgentRegistry, AgentRole,
    ConfigSource,
};
use crate::cli::{
    apply_args_to_config, handle_extended_help, handle_generate_completion,
    handle_init_global_with, handle_init_legacy, handle_init_prompt_with, handle_list_work_guides,
    handle_smart_init_with, Args,
};
use crate::config::{
    loader, unified_config_path, Config, ConfigEnvironment, RealConfigEnvironment, UnifiedConfig,
};
use crate::git_helpers::get_repo_root;
use crate::logger::Colors;
use crate::logger::Logger;
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
/// 4. Handles --list-work-guides, --init-prompt, --init/--init-global (unified), and --init-legacy if set
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
    colors: Colors,
    logger: &Logger,
) -> anyhow::Result<Option<ConfigInitResult>> {
    initialize_config_with(
        args,
        colors,
        logger,
        &RealCatalogLoader,
        &RealConfigEnvironment,
    )
}

/// Initializes configuration and agent registry with a custom catalog loader.
///
/// This is the same as [`initialize_config`] but accepts a custom [`CatalogLoader`]
/// for dependency injection. This is primarily useful for testing.
#[deprecated(since = "0.6.0", note = "Use initialize_config_with instead")]
pub fn initialize_config_with_loader<L: CatalogLoader>(
    args: &Args,
    colors: Colors,
    logger: &Logger,
    catalog_loader: &L,
) -> anyhow::Result<Option<ConfigInitResult>> {
    initialize_config_with(args, colors, logger, catalog_loader, &RealConfigEnvironment)
}

/// Initializes configuration and agent registry with full dependency injection.
///
/// This is the same as [`initialize_config`] but accepts both a [`CatalogLoader`]
/// and a [`ConfigEnvironment`] for full dependency injection. This enables testing
/// without network calls or environment variable dependencies.
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments
/// * `colors` - Color configuration for output
/// * `logger` - Logger for info/warning messages
/// * `catalog_loader` - Loader for the OpenCode API catalog
/// * `path_resolver` - Resolver for configuration file paths
///
/// # Returns
///
/// Returns `Ok(Some(result))` on success, `Ok(None)` if an early exit was triggered
/// (e.g., --init, --init-prompt, --list-templates), or an error if initialization fails.
pub fn initialize_config_with<L: CatalogLoader, P: ConfigEnvironment>(
    args: &Args,
    colors: Colors,
    logger: &Logger,
    catalog_loader: &L,
    path_resolver: &P,
) -> anyhow::Result<Option<ConfigInitResult>> {
    // Load configuration from unified config file (with env overrides)
    // Uses the provided path_resolver for filesystem operations instead of std::fs directly
    let (mut config, unified, warnings) =
        loader::load_config_from_path_with_env(args.config.as_deref(), path_resolver);

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

    // Handle --generate-completion flag: generate shell completion script and exit
    if let Some(shell) = args.completion.generate_completion {
        if handle_generate_completion(shell) {
            return Ok(None);
        }
    }

    // Handle --extended-help / --man flag: display extended help and exit.
    // If combined with --list-work-guides, show both to reduce surprises.
    if args.recovery.extended_help {
        handle_extended_help();
        if args.work_guide_list.list_work_guides {
            println!();
            handle_list_work_guides(colors);
        }
        return Ok(None);
    }

    // Handle --list-work-guides / --list-templates flag: display available Work Guides and exit
    if args.work_guide_list.list_work_guides && handle_list_work_guides(colors) {
        return Ok(None);
    }

    // Handle --init-prompt flag: create PROMPT.md from template and exit
    if let Some(ref template_name) = args.init_prompt {
        if handle_init_prompt_with(
            template_name,
            args.unified_init.force_init,
            colors,
            path_resolver,
        )? {
            return Ok(None);
        }
    }

    // Handle smart --init flag: intelligently determine what to initialize
    if args.unified_init.init.is_some()
        && handle_smart_init_with(
            args.unified_init.init.as_deref(),
            args.unified_init.force_init,
            colors,
            path_resolver,
        )?
    {
        return Ok(None);
    }

    // Handle --init-config flag: explicit config creation and exit
    if args.unified_init.init_config && handle_init_global_with(colors, path_resolver)? {
        return Ok(None);
    }

    // Handle --init-global flag: create unified config if it doesn't exist and exit
    if args.unified_init.init_global && handle_init_global_with(colors, path_resolver)? {
        return Ok(None);
    }

    // Handle --init-legacy flag: legacy per-repo agents.toml creation and exit
    if args.legacy_init.init_legacy {
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
    let (registry, config_sources) =
        load_agent_registry(unified.as_ref(), config_path.as_path(), catalog_loader)?;

    // Apply default agents from fallback chains
    apply_default_agents(&mut config, &registry);

    Ok(Some(ConfigInitResult {
        config,
        registry,
        config_path,
        config_sources,
    }))
}

fn load_agent_registry<L: CatalogLoader>(
    unified: Option<&UnifiedConfig>,
    config_path: &std::path::Path,
    catalog_loader: &L,
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

    // Load OpenCode API catalog if there are any opencode/* references
    setup_opencode_catalog(&mut registry, unified, catalog_loader)?;

    Ok((registry, sources))
}

/// Setup OpenCode API catalog for dynamic provider/model resolution.
///
/// This function:
/// 1. Checks if there are any `opencode/*` references in the configured agent chains
/// 2. If yes, fetches/loads the cached OpenCode API catalog
/// 3. Sets the catalog on the registry for dynamic agent resolution
/// 4. Validates all opencode/* references and reports errors with suggestions
fn setup_opencode_catalog<L: CatalogLoader>(
    registry: &mut AgentRegistry,
    unified: Option<&UnifiedConfig>,
    catalog_loader: &L,
) -> anyhow::Result<()> {
    // Collect fallback config from unified config or registry defaults
    let fallback = unified
        .and_then(|u| u.agent_chain.as_ref())
        .cloned()
        .unwrap_or_else(|| registry.fallback_config().clone());

    // Check if there are any opencode/* references
    let opencode_refs = agent_validation::get_opencode_refs(&fallback);
    if opencode_refs.is_empty() {
        // No opencode references, skip catalog loading
        return Ok(());
    }

    // Load the API catalog using the injected loader
    let catalog = catalog_loader.load().map_err(|e| {
        anyhow::anyhow!(
            "Failed to load OpenCode API catalog. \
            This is required for the following agent references: {opencode_refs:?}. \
            Error: {e}"
        )
    })?;

    // Set the catalog on the registry for dynamic resolution
    registry.set_opencode_catalog(catalog.clone());

    // Validate all opencode/* references
    agent_validation::validate_opencode_agents(&fallback, &catalog)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
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
