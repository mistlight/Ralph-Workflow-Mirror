//! Configuration initialization handlers.
//!
//! This module handles the `--init`, `--init-global`, legacy `--init-legacy` flags,
//! and `--init-prompt` flag for creating default agent configuration files
//! and PROMPT.md from templates.

use crate::agents::{AgentsConfigFile, ConfigInitResult};
use crate::colors::Colors;
use crate::config::{unified_config_path, UnifiedConfig, UnifiedConfigInitResult};
use crate::templates::{get_template, list_templates, PromptTemplate, TemplateCategory, ALL_TEMPLATES};
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

/// Handle the `--init-prompt` or `--prompt` flag without a template argument.
///
/// Shows helpful template suggestions when no template is specified.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after).
pub fn handle_init_prompt_noarg(colors: &Colors) -> anyhow::Result<bool> {
    println!("{}No template specified.{} Here are some common templates to get you started:\n",
        colors.yellow(),
        colors.reset()
    );

    println!("{}Most Common Templates:{}",
        colors.bold(),
        colors.reset()
    );
    println!();

    // Show most common general templates
    let common_templates = [
        PromptTemplate::FeatureSpec,
        PromptTemplate::BugFix,
        PromptTemplate::Quick,
    ];

    for template in common_templates {
        println!(
            "  {}{}{}  {}{}{}",
            colors.cyan(),
            template.name(),
            colors.reset(),
            colors.dim(),
            template.description(),
            colors.reset()
        );
    }

    println!();
    println!("{}{}Popular Language-Specific Templates:{}",
        colors.bold(),
        colors.green(),
        colors.reset()
    );
    println!();

    // Show some popular language-specific templates
    let popular_language_templates = [
        PromptTemplate::RustFeature,
        PromptTemplate::TypeScriptFeature,
        PromptTemplate::RubyOnRails,
    ];

    for template in popular_language_templates {
        println!(
            "  {}{}{}  {}{}{}",
            colors.cyan(),
            template.name(),
            colors.reset(),
            colors.dim(),
            template.description(),
            colors.reset()
        );
    }

    println!();
    println!("{}All Available Templates:{}",
        colors.bold(),
        colors.reset()
    );
    println!();

    // Group templates by category
    let mut general_templates = Vec::new();
    let mut language_specific_templates = Vec::new();

    for template in ALL_TEMPLATES {
        match template.category() {
            TemplateCategory::General => general_templates.push(template),
            TemplateCategory::LanguageSpecific => language_specific_templates.push(template),
        }
    }

    // Display General templates
    println!("  {}General:{}",
        colors.yellow(),
        colors.reset()
    );
    for template in general_templates {
        println!(
            "    {}{}{}  {}{}{}",
            colors.cyan(),
            template.name(),
            colors.reset(),
            colors.dim(),
            template.description(),
            colors.reset()
        );
    }

    println!();
    println!("  {}Language-Specific:{}",
        colors.green(),
        colors.reset()
    );
    for template in language_specific_templates {
        println!(
            "    {}{}{}  {}{}{}",
            colors.cyan(),
            template.name(),
            colors.reset(),
            colors.dim(),
            template.description(),
            colors.reset()
        );
    }

    println!();
    println!("{}Usage:{}",
        colors.bold(),
        colors.reset()
    );
    println!("  ralph --init-prompt <template>");
    println!("  ralph --prompt <template>");
    println!("  ralph -P <template>");
    println!();
    println!("{}Examples:{}",
        colors.bold(),
        colors.reset()
    );
    println!("  ralph --init-prompt feature-spec       # Create comprehensive spec template");
    println!("  ralph --prompt bug-fix                # Create bug fix template");
    println!("  ralph -P rust-feature                 # Create Rust-specific feature template");
    println!("  ralph -P typescript-feature           # Create TypeScript-specific feature template");
    println!();
    println!("{}Tip:{} Use --list-templates to see all available templates with descriptions.",
        colors.bold(),
        colors.reset()
    );

    Ok(true)
}

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
pub fn handle_init_prompt(template_name: &str, colors: &Colors) -> anyhow::Result<bool> {
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
    let template = match get_template(template_name) {
        Some(t) => t,
        None => {
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
        }
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
/// Lists all available PROMPT.md templates with descriptions, grouped by category.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after).
pub fn handle_list_templates(colors: &Colors) -> anyhow::Result<bool> {
    println!("Available PROMPT.md templates:");
    println!();

    // Group templates by category
    let mut general_templates = Vec::new();
    let mut language_specific_templates = Vec::new();

    for template in ALL_TEMPLATES {
        match template.category() {
            TemplateCategory::General => general_templates.push(template),
            TemplateCategory::LanguageSpecific => language_specific_templates.push(template),
        }
    }

    // Display General templates
    println!("{}{}General Templates:{}",
        colors.bold(),
        colors.yellow(),
        colors.reset()
    );
    println!();
    for template in general_templates {
        println!(
            "  {}{}{}  {}{}{}",
            colors.cyan(),
            template.name(),
            colors.reset(),
            colors.dim(),
            template.description(),
            colors.reset()
        );
    }

    println!();
    println!("{}{}Language-Specific Templates:{}",
        colors.bold(),
        colors.green(),
        colors.reset()
    );
    println!();
    for template in language_specific_templates {
        println!(
            "  {}{}{}  {}{}{}",
            colors.cyan(),
            template.name(),
            colors.reset(),
            colors.dim(),
            template.description(),
            colors.reset()
        );
    }

    println!();
    println!("{}Usage:{}",
        colors.bold(),
        colors.reset()
    );
    println!("  ralph --init-prompt <template>");
    println!("  ralph --prompt <template>");
    println!("  ralph -P <template>");
    println!();
    println!("{}Examples:{}",
        colors.bold(),
        colors.reset()
    );
    println!("  ralph --init-prompt feature-spec       # Create comprehensive spec template");
    println!("  ralph --prompt bug-fix                # Create bug fix template");
    println!("  ralph -P rust-feature                 # Create Rust-specific feature template");
    println!("  ralph -P typescript-feature           # Create TypeScript-specific feature template");

    Ok(true)
}
