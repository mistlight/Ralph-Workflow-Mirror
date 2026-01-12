//! Configuration initialization handlers.
//!
//! This module handles the `--init` and `--init-global` command-line flags,
//! which create default agent configuration files.

use crate::agents::{global_agents_config_path, AgentsConfigFile, ConfigInitResult};
use crate::colors::Colors;
use std::path::Path;

/// Handle the `--init-global` flag.
///
/// Creates a global agents.toml file at `~/.config/ralph/agents.toml` if it doesn't exist.
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
    let global_path = global_agents_config_path().ok_or_else(|| {
        anyhow::anyhow!("Cannot determine global config directory (no home directory)")
    })?;

    match AgentsConfigFile::ensure_config_exists(&global_path) {
        Ok(ConfigInitResult::Created) => {
            println!(
                "{}Created global config: {}{}{}\n",
                colors.green(),
                colors.bold(),
                global_path.display(),
                colors.reset()
            );
            println!("This config will be loaded for all repositories.");
            println!("Per-repository configs in .agent/agents.toml will override these settings.");
            Ok(true)
        }
        Ok(ConfigInitResult::AlreadyExists) => {
            println!(
                "{}Global config already exists:{} {}",
                colors.yellow(),
                colors.reset(),
                global_path.display()
            );
            println!("Edit the file to customize, or delete it to regenerate from defaults.");
            Ok(true)
        }
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create global config file {}: {}",
            global_path.display(),
            e
        )),
    }
}

/// Handle the `--init` flag.
///
/// Creates a local agents.toml file at the specified path if it doesn't exist.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
/// * `agents_config_path` - Path where the config file should be created
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or an error if config creation failed.
pub fn handle_init(colors: &Colors, agents_config_path: &Path) -> anyhow::Result<bool> {
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

/// Ensure config file exists, creating it with defaults if needed.
///
/// Unlike `handle_init`, this is called during normal operation and doesn't exit.
/// It creates the config file with a helpful message if it doesn't exist.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
/// * `agents_config_path` - Path where the config file should be created
/// * `logger` - Logger for warning messages
///
/// # Returns
///
/// Returns `Ok(true)` if the config was just created (caller may want to exit),
/// `Ok(false)` if config already existed (continue normally),
/// or logs a warning and returns `Ok(false)` if creation failed.
pub fn ensure_config_or_create(
    colors: &Colors,
    agents_config_path: &Path,
    logger: &crate::utils::Logger,
) -> anyhow::Result<bool> {
    match AgentsConfigFile::ensure_config_exists(agents_config_path) {
        Ok(ConfigInitResult::Created) => {
            println!();
            println!(
                "{}{}No agents.toml found - created default configuration:{}",
                colors.bold(),
                colors.yellow(),
                colors.reset()
            );
            println!(
                "  {}{}{}",
                colors.cyan(),
                agents_config_path.display(),
                colors.reset()
            );
            println!();
            println!("{}Options:{}", colors.bold(), colors.reset());
            println!("  1. Edit the file to customize agent settings, then run ralph again");
            println!("  2. Run ralph again now to use the default settings");
            println!();
            Ok(true)
        }
        Ok(ConfigInitResult::AlreadyExists) => {
            // Config exists, continue normally
            Ok(false)
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to create agents config at {}: {}",
                agents_config_path.display(),
                e
            ));
            // Continue with built-in defaults
            Ok(false)
        }
    }
}
