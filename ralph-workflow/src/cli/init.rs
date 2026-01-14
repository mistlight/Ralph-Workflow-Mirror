//! Configuration initialization handlers.
//!
//! This module handles the `--init`, `--init-global`, legacy `--init-legacy` flags,
//! and `--init-prompt` flag for creating default agent configuration files
//! and PROMPT.md from templates.

use crate::agents::{AgentsConfigFile, ConfigInitResult};
use crate::colors::Colors;
use crate::config::{unified_config_path, UnifiedConfig, UnifiedConfigInitResult};
use crate::templates::{get_template, list_templates};
use std::fs;
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
pub fn handle_init_global(colors: Colors) -> anyhow::Result<bool> {
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
pub fn handle_init_legacy(colors: Colors, agents_config_path: &Path) -> anyhow::Result<bool> {
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

/// Handle the `--init-prompt` flag.
///
/// Creates a PROMPT.md file from the specified template.
///
/// # Arguments
///
/// * `template_name` - The name of the template to use
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or an error if template creation failed.
pub fn handle_init_prompt(template_name: &str, colors: Colors) -> anyhow::Result<bool> {
    let prompt_path = Path::new("PROMPT.md");

    // Check if PROMPT.md already exists
    if prompt_path.exists() {
        println!(
            "{}PROMPT.md already exists:{} {}",
            colors.yellow(),
            colors.reset(),
            prompt_path.display()
        );
        println!("Delete or backup the existing file to create a new one from a template.");
        return Ok(true);
    }

    // Validate the template exists
    let template = if let Some(t) = get_template(template_name) {
        t
    } else {
        println!(
            "{}Unknown template: '{}'{}",
            colors.red(),
            template_name,
            colors.reset()
        );
        println!();
        println!("Available templates:");
        for (name, description) in list_templates() {
            println!(
                "  {}{}{}  {}",
                colors.cyan(),
                name,
                colors.reset(),
                description
            );
        }
        println!();
        println!("Usage: ralph --init-prompt <template>");
        println!("       ralph --list-templates");
        return Ok(true);
    };

    // Write the template content to PROMPT.md
    let content = template.content();
    fs::write(prompt_path, content)?;

    println!(
        "{}Created PROMPT.md from template: {}{}{}",
        colors.green(),
        colors.bold(),
        template_name,
        colors.reset()
    );
    println!();
    println!(
        "Template: {}{}{}  {}",
        colors.cyan(),
        template.name(),
        colors.reset(),
        template.description()
    );
    println!();
    println!("Next steps:");
    println!("  1. Edit PROMPT.md with your task details");
    println!("  2. Run: ralph \"your commit message\"");
    println!();
    println!("Tip: Use --list-templates to see all available templates.");

    Ok(true)
}

/// Handle the `--list-templates` flag.
///
/// Lists all available PROMPT.md templates with descriptions.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after).
pub fn handle_list_templates(colors: Colors) -> anyhow::Result<bool> {
    println!("Available PROMPT.md templates:");
    println!();

    let templates = list_templates();
    let _max_name_len = templates
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0);

    for (name, description) in templates {
        println!(
            "  {}{}{}  {}{}{}",
            colors.cyan(),
            name,
            colors.reset(),
            colors.dim(),
            description,
            colors.reset()
        );
    }

    println!();
    println!("Usage: ralph --init-prompt <template>");
    println!();
    println!("Example:");
    println!("  ralph --init-prompt feature-spec   # Create comprehensive spec template");
    println!("  ralph --init-prompt bug-fix        # Create bug fix template");
    println!("  ralph --init-prompt quick          # Create quick change template");

    Ok(true)
}
