//! Configuration initialization handlers.
//!
//! This module handles the `--init`, `--init-global`, legacy `--init-legacy` flags,
//! and `--init-prompt` flag for creating default agent configuration files
//! and PROMPT.md from templates.

use crate::agents::{AgentsConfigFile, ConfigInitResult};
use crate::config::{unified_config_path, UnifiedConfig, UnifiedConfigInitResult};
use crate::logger::Colors;
use crate::templates::{get_template, list_templates, ALL_TEMPLATES};
use std::fs;
use std::path::Path;

/// Minimum similarity threshold for suggesting alternatives (0-100 percentage).
const MIN_SIMILARITY_PERCENT: u32 = 40;

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
    let Some(template) = get_template(template_name) else {
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
/// Lists all available PROMPT.md templates with descriptions, organized by category.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `true` if the flag was handled (program should exit after).
pub fn handle_list_templates(colors: Colors) -> bool {
    println!("PROMPT.md Task Templates (use: ralph --init <template>)");
    println!();

    // Common templates (most frequently used)
    println!("{}Common Templates:{}", colors.bold(), colors.reset());
    println!("  {}quick{}  Quick/small changes (typos, minor fixes)", colors.cyan(), colors.reset());
    println!("  {}bug-fix{}  Bug fix with investigation guidance", colors.cyan(), colors.reset());
    println!("  {}feature-spec{}  Comprehensive product specification", colors.cyan(), colors.reset());
    println!("  {}refactor{}  Code refactoring with behavior preservation", colors.cyan(), colors.reset());
    println!();

    // Testing and documentation
    println!("{}Testing & Documentation:{}", colors.bold(), colors.reset());
    println!("  {}test{}  Test writing with edge case considerations", colors.cyan(), colors.reset());
    println!("  {}docs{}  Documentation update with completeness checklist", colors.cyan(), colors.reset());
    println!();

    // Specialized development
    println!("{}Specialized Development:{}", colors.bold(), colors.reset());
    println!("  {}cli-tool{}  CLI tool with argument parsing and completion", colors.cyan(), colors.reset());
    println!("  {}web-api{}  REST/HTTP API with error handling", colors.cyan(), colors.reset());
    println!("  {}ui-component{}  UI component with accessibility and responsive design", colors.cyan(), colors.reset());
    println!();

    // Advanced/Infrastructure
    println!("{}Advanced & Infrastructure:{}", colors.bold(), colors.reset());
    println!("  {}performance-optimization{}  Performance optimization with benchmarking", colors.cyan(), colors.reset());
    println!("  {}security-audit{}  Security audit with OWASP Top 10 coverage", colors.cyan(), colors.reset());
    println!("  {}api-integration{}  API integration with retry logic and resilience", colors.cyan(), colors.reset());
    println!("  {}database-migration{}  Database migration with zero-downtime strategies", colors.cyan(), colors.reset());
    println!("  {}dependency-update{}  Dependency update with breaking change handling", colors.cyan(), colors.reset());
    println!("  {}data-pipeline{}  Data pipeline with ETL and monitoring", colors.cyan(), colors.reset());
    println!();

    println!("Usage: ralph --init <template>");
    println!("       ralph --init-prompt <template>");
    println!();
    println!("Example:");
    println!("  ralph --init bug-fix              # Create bug fix template");
    println!("  ralph --init feature-spec         # Create feature spec template");
    println!("  ralph --init quick                # Create quick change template");
    println!();
    println!("{}Tip:{}", colors.yellow(), colors.reset());
    println!("  Use --init without a value to auto-detect what you need.");
    println!("  Run ralph --help to understand the difference between Task Templates");
    println!("  (for PROMPT.md) and System Prompts (backend AI configuration).");

    true
}

/// Handle the smart `--init` flag.
///
/// This function intelligently determines what the user wants to initialize:
/// - If a value is provided and matches a known template name → create PROMPT.md
/// - If config doesn't exist and no template specified → create config
/// - If config exists but PROMPT.md doesn't → prompt to create PROMPT.md
/// - If both exist → show helpful message about what's already set up
///
/// # Arguments
///
/// * `template_arg` - Optional template name from `--init=TEMPLATE`
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or `Ok(false)` if not handled, or an error if initialization failed.
pub fn handle_smart_init(template_arg: Option<&str>, colors: Colors) -> anyhow::Result<bool> {
    let config_path = crate::config::unified_config_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory (no home directory)"))?;
    let prompt_path = Path::new("PROMPT.md");

    let config_exists = config_path.exists();
    let prompt_exists = prompt_path.exists();

    // If a template name is provided, treat it as --init-prompt
    if let Some(template_name) = template_arg {
        return handle_init_template_arg(template_name, colors);
    }

    // No template provided - use smart inference based on current state
    handle_init_state_inference(
        &config_path,
        prompt_path,
        config_exists,
        prompt_exists,
        colors,
    )
}

/// Calculate Levenshtein distance between two strings.
///
/// Returns the minimum number of single-character edits (insertions, deletions,
/// or substitutions) required to change one string into the other.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let b_len = b_chars.len();

    // Use two rows to save memory
    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row = vec![0; b_len + 1];

    for (i, a_char) in a_chars.iter().enumerate() {
        curr_row[0] = i + 1;

        for (j, b_char) in b_chars.iter().enumerate() {
            let cost = usize::from(a_char != b_char);
            curr_row[j + 1] = std::cmp::min(
                std::cmp::min(
                    curr_row[j] + 1,     // deletion
                    prev_row[j + 1] + 1, // insertion
                ),
                prev_row[j] + cost, // substitution
            );
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[b_len]
}

/// Calculate similarity score as a percentage (0-100).
///
/// This avoids floating point comparison issues in tests.
fn similarity_percentage(a: &str, b: &str) -> u32 {
    if a == b {
        return 100;
    }
    if a.is_empty() || b.is_empty() {
        return 0;
    }

    let max_len = a.len().max(b.len());
    let distance = levenshtein_distance(a, b);

    if max_len == 0 {
        return 100;
    }

    // Calculate percentage without floating point
    // (100 * (max_len - distance)) / max_len
    let diff = max_len.saturating_sub(distance);
    // The division result is guaranteed to fit in u32 since it's ≤ 100
    u32::try_from((100 * diff) / max_len).unwrap_or(0)
}

/// Find the best matching template names using fuzzy matching.
///
/// Returns templates that are similar to the input within the threshold.
fn find_similar_templates(input: &str) -> Vec<(&'static str, u32)> {
    let input_lower = input.to_lowercase();
    let mut matches: Vec<(&'static str, u32)> = ALL_TEMPLATES
        .iter()
        .map(|t| {
            let name = t.name();
            let sim = similarity_percentage(&input_lower, &name.to_lowercase());
            (name, sim)
        })
        .filter(|(_, sim)| *sim >= MIN_SIMILARITY_PERCENT)
        .collect();

    // Sort by similarity (highest first)
    matches.sort_by(|a, b| b.1.cmp(&a.1));

    // Return top 3 matches
    matches.truncate(3);
    matches
}

/// Handle --init when a template name is provided.
fn handle_init_template_arg(template_name: &str, colors: Colors) -> anyhow::Result<bool> {
    if get_template(template_name).is_some() {
        return handle_init_prompt(template_name, colors);
    }

    // Unknown value - show helpful error with suggestions
    println!(
        "{}Unknown template: '{}'{}",
        colors.red(),
        template_name,
        colors.reset()
    );
    println!();

    // Try to find similar template names
    let similar = find_similar_templates(template_name);
    if !similar.is_empty() {
        println!("{}Did you mean?{}", colors.yellow(), colors.reset());
        for (name, score) in similar {
            println!(
                "  {}{}{}  ({}% similar)",
                colors.cyan(),
                name,
                colors.reset(),
                score
            );
        }
        println!();
    }

    println!("Available templates:");
    for (name, description) in list_templates() {
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
    println!("Usage: ralph --init=<template>");
    println!("       ralph --init          # Smart init (infers intent)");
    Ok(true)
}

/// Handle --init with smart inference based on current state.
fn handle_init_state_inference(
    config_path: &std::path::Path,
    prompt_path: &Path,
    config_exists: bool,
    prompt_exists: bool,
    colors: Colors,
) -> anyhow::Result<bool> {
    match (config_exists, prompt_exists) {
        (false, false) => handle_init_none_exist(config_path, colors),
        (true, false) => Ok(handle_init_only_config_exists(config_path, colors)),
        (false, true) => handle_init_only_prompt_exists(colors),
        (true, true) => Ok(handle_init_both_exist(config_path, prompt_path, colors)),
    }
}

/// Handle --init when neither config nor PROMPT.md exists.
fn handle_init_none_exist(_config_path: &std::path::Path, colors: Colors) -> anyhow::Result<bool> {
    println!(
        "{}No config found. Creating unified config...{}",
        colors.dim(),
        colors.reset()
    );
    println!();
    handle_init_global(colors)?;
    Ok(true)
}

/// Handle --init when only config exists (no PROMPT.md).
fn handle_init_only_config_exists(config_path: &std::path::Path, colors: Colors) -> bool {
    println!(
        "{}Config found at:{} {}",
        colors.green(),
        colors.reset(),
        config_path.display()
    );
    println!(
        "{}PROMPT.md not found in current directory.{}",
        colors.yellow(),
        colors.reset()
    );
    println!();
    println!("Create a PROMPT.md from a template to get started:");
    println!();

    for (name, description) in list_templates() {
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
    println!("Usage: ralph --init <template>");
    println!("       ralph --init-prompt <template>");
    println!();
    println!("Example:");
    println!("  ralph --init bug-fix");
    println!("  ralph --init feature-spec");
    true
}

/// Handle --init when only PROMPT.md exists (no config).
fn handle_init_only_prompt_exists(colors: Colors) -> anyhow::Result<bool> {
    println!(
        "{}PROMPT.md found in current directory.{}",
        colors.green(),
        colors.reset()
    );
    println!(
        "{}No config found. Creating unified config...{}",
        colors.dim(),
        colors.reset()
    );
    println!();
    handle_init_global(colors)?;
    Ok(true)
}

/// Handle --init when both config and PROMPT.md exist.
fn handle_init_both_exist(
    config_path: &std::path::Path,
    prompt_path: &Path,
    colors: Colors,
) -> bool {
    println!("{}Setup complete!{}", colors.green(), colors.reset());
    println!();
    println!(
        "  Config: {}{}{}",
        colors.dim(),
        config_path.display(),
        colors.reset()
    );
    println!(
        "  PROMPT: {}{}{}",
        colors.dim(),
        prompt_path.display(),
        colors.reset()
    );
    println!();
    println!("You're ready to run Ralph:");
    println!("  ralph \"your commit message\"");
    println!();
    println!("Other commands:");
    println!("  ralph --list-templates    # Show all PROMPT.md templates");
    println!("  ralph --init=<template>    # Create new PROMPT.md from template");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_smart_init_with_valid_template() {
        // When a valid template name is provided, it should delegate to handle_init_prompt
        let colors = Colors::new();
        let result = handle_smart_init(Some("bug-fix"), colors);

        // We expect this to return Ok(true) since it handles the init
        // The actual test would need to mock file system operations
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_smart_init_with_invalid_template() {
        // When an invalid template name is provided, it should show an error
        let colors = Colors::new();
        let result = handle_smart_init(Some("nonexistent-template"), colors);

        // Should still return Ok(true) since it handled the request (showed error)
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_smart_init_no_arg() {
        // When no argument is provided, it should check the current state
        let colors = Colors::new();
        let result = handle_smart_init(None, colors);

        // Should return Ok(something) depending on the state of config/PROMPT.md
        assert!(result.is_ok());
    }

    #[test]
    fn test_template_name_validation() {
        // Test that we can validate template names
        assert!(get_template("bug-fix").is_some());
        assert!(get_template("feature-spec").is_some());
        assert!(get_template("refactor").is_some());
        assert!(get_template("test").is_some());
        assert!(get_template("docs").is_some());
        assert!(get_template("quick").is_some());

        // Invalid template names
        assert!(get_template("invalid").is_none());
        assert!(get_template("").is_none());
    }

    #[test]
    fn test_levenshtein_distance() {
        // Exact match
        assert_eq!(levenshtein_distance("test", "test"), 0);

        // One edit
        assert_eq!(levenshtein_distance("test", "tast"), 1);
        assert_eq!(levenshtein_distance("test", "tests"), 1);
        assert_eq!(levenshtein_distance("test", "est"), 1);

        // Two edits
        assert_eq!(levenshtein_distance("test", "taste"), 2);
        assert_eq!(levenshtein_distance("test", "best"), 1);

        // Completely different
        assert_eq!(levenshtein_distance("abc", "xyz"), 3);
    }

    #[test]
    fn test_similarity() {
        // Exact match
        assert_eq!(similarity_percentage("test", "test"), 100);

        // Similar strings - should be high similarity
        assert!(similarity_percentage("bug-fix", "bugfix") > 80);
        assert!(similarity_percentage("feature-spec", "feature") > 50);

        // Different strings - should be low similarity
        assert!(similarity_percentage("test", "xyz") < 50);

        // Empty strings
        assert_eq!(similarity_percentage("", ""), 100);
        assert_eq!(similarity_percentage("test", ""), 0);
        assert_eq!(similarity_percentage("", "test"), 0);
    }

    #[test]
    fn test_find_similar_templates() {
        // Find similar to "bugfix" (missing hyphen)
        let similar = find_similar_templates("bugfix");
        assert!(!similar.is_empty());
        assert!(similar.iter().any(|(name, _)| *name == "bug-fix"));

        // Find similar to "feature" (should match feature-spec)
        let similar = find_similar_templates("feature");
        assert!(!similar.is_empty());
        assert!(similar.iter().any(|(name, _)| name.contains("feature")));

        // Very different string should return empty or low similarity
        let similar = find_similar_templates("xyzabc");
        // Either empty or all matches have low similarity
        assert!(similar.is_empty() || similar.iter().all(|(_, sim)| *sim < 50));
    }
}
