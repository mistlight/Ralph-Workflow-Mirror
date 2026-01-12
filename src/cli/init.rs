//! Configuration initialization handlers.
//!
//! This module handles the `--init`, `--init-global`, and legacy `--init-legacy` flags,
//! which create default agent configuration files.

use crate::agents::{AgentsConfigFile, ConfigInitResult};
use crate::colors::Colors;
use crate::config::{unified_config_path, UnifiedConfig, UnifiedConfigInitResult};
use std::path::Path;

/// Handle the `--init-global` flag.
///
/// Creates a unified config file at `~/.config/ralph-workflow.toml` if it doesn't exist.
/// This is the recommended way to configure Ralph globally.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or an error if config creation failed.
pub fn handle_init_global(colors: &Colors) -> anyhow::Result<bool> {
    let global_path = unified_config_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory (no home directory)"))?;

    match UnifiedConfig::ensure_config_exists() {
        Ok(UnifiedConfigInitResult::Created) => {
            println!(
                "{}Created unified config: {}{}{}\n",
                colors.green(),
                colors.bold(),
                global_path.display(),
                colors.reset()
            );
            println!("This is the primary configuration file for Ralph.");
            println!();
            println!("Features:");
            println!("  - General settings (verbosity, iterations, etc.)");
            println!("  - CCS aliases for Claude Code Switch integration");
            println!("  - Custom agent definitions");
            println!("  - Agent chain configuration with fallbacks");
            println!();
            println!("Environment variables (RALPH_*) override these settings.");
            Ok(true)
        }
        Ok(UnifiedConfigInitResult::AlreadyExists) => {
            println!(
                "{}Unified config already exists:{} {}",
                colors.yellow(),
                colors.reset(),
                global_path.display()
            );
            println!("Edit the file to customize, or delete it to regenerate from defaults.");
            Ok(true)
        }
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create config file {}: {}",
            global_path.display(),
            e
        )),
    }
}

/// Handle the legacy `--init-legacy` flag.
///
/// Creates a local agents.toml file at the specified path if it doesn't exist.
pub fn handle_init_legacy(colors: &Colors, agents_config_path: &Path) -> anyhow::Result<bool> {
    match AgentsConfigFile::ensure_config_exists(agents_config_path) {
        Ok(ConfigInitResult::Created) => {
            println!(
                "{}Created {}{}{}\n",
                colors.green(),
                colors.bold(),
                agents_config_path.display(),
                colors.reset()
            );
            println!("Edit the file to customize agent configurations, then run ralph again.");
            println!("Or run ralph now to use the default settings.");
            Ok(true)
        }
        Ok(ConfigInitResult::AlreadyExists) => {
            println!(
                "{}Config file already exists:{} {}",
                colors.yellow(),
                colors.reset(),
                agents_config_path.display()
            );
            println!("Edit the file to customize, or delete it to regenerate from defaults.");
            Ok(true)
        }
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create config file {}: {}",
            agents_config_path.display(),
            e
        )),
    }
}

// NOTE: legacy per-repo agents.toml creation is handled by `--init-legacy` only.
