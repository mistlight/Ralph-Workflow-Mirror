//! Local configuration file creation.
//!
//! Handles `--init-local-config` flag to create a local config file at
//! `.agent/ralph-workflow.toml` in the current directory.

use crate::config::{ConfigEnvironment, RealConfigEnvironment};
use crate::logger::Colors;

/// Local config template with minimal override examples.
const LOCAL_CONFIG_TEMPLATE: &str = r#"# Local Ralph configuration (.agent/ralph-workflow.toml)
# Overrides ~/.config/ralph-workflow.toml for this project.
# Only include settings you want to override.
# Run `ralph --check-config` to validate and see effective settings.

[general]
# Project-specific iteration limits
# developer_iters = 5
# reviewer_reviews = 2

# Project-specific context levels
# developer_context = 1
# reviewer_context = 0

# [agent_chain]
# Project-specific agent chains
# developer = ["claude"]
# reviewer = ["claude"]
"#;

/// Handle the `--init-local-config` flag with a custom path resolver.
///
/// Creates a local config file at `.agent/ralph-workflow.toml` in the current directory.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
/// * `env` - Path resolver for determining config file location
/// * `force` - Whether to overwrite existing config file
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or an error if config creation failed.
pub fn handle_init_local_config_with<R: ConfigEnvironment>(
    colors: Colors,
    env: &R,
    force: bool,
) -> anyhow::Result<bool> {
    let local_path = env
        .local_config_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine local config path"))?;

    // Check if config already exists
    if env.file_exists(&local_path) && !force {
        println!(
            "{}Local config already exists:{} {}",
            colors.yellow(),
            colors.reset(),
            local_path.display()
        );
        println!("Use --force-overwrite to replace it, or edit the existing file.");
        println!();
        println!("Run `ralph --check-config` to see effective configuration.");
        return Ok(true);
    }

    // Create config using the environment's file operations
    env.write_file(&local_path, LOCAL_CONFIG_TEMPLATE)
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create local config file {}: {}",
                local_path.display(),
                e
            )
        })?;

    // Try to show absolute path, fall back to the path as-is if canonicalization fails
    let display_path = local_path
        .canonicalize()
        .unwrap_or_else(|_| local_path.clone());

    println!(
        "{}Created{} {}",
        colors.green(),
        colors.reset(),
        display_path.display()
    );
    println!();
    println!(
        "This local config will override your global settings (~/.config/ralph-workflow.toml)."
    );
    println!("Edit the file to customize Ralph for this project.");
    println!();
    println!("Tip: Run `ralph --check-config` to validate your configuration.");

    Ok(true)
}

/// Handle the `--init-local-config` flag using the default path resolver.
///
/// Convenience wrapper that uses [`RealConfigEnvironment`] internally.
pub fn handle_init_local_config(colors: Colors, force: bool) -> anyhow::Result<bool> {
    handle_init_local_config_with(colors, &RealConfigEnvironment, force)
}
