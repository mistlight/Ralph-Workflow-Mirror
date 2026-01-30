//! Configuration initialization handlers.
//!
//! This module handles the `--init` and `--init-global` flags for creating
//! default unified configuration files and PROMPT.md from templates.
//!
//! # Dependency Injection
//!
//! All init handlers accept a [`ConfigEnvironment`] for path resolution, enabling
//! tests to inject custom paths without relying on environment variables.
//!
//! For convenience, wrapper functions without the resolver parameter are provided
//! that use [`RealConfigEnvironment`] internally.

use crate::config::{ConfigEnvironment, RealConfigEnvironment};
use crate::logger::Colors;
use crate::templates::{get_template, list_templates, ALL_TEMPLATES};
use std::io::IsTerminal;
use std::path::Path;

/// Minimum similarity threshold for suggesting alternatives (0-100 percentage).
const MIN_SIMILARITY_PERCENT: u32 = 40;

/// Handle the `--init-global` flag with a custom path resolver.
///
/// Creates a unified config file at the path determined by the resolver.
/// This is the recommended way to configure Ralph globally.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
/// * `resolver` - Path resolver for determining config file location
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or an error if config creation failed.
pub fn handle_init_global_with<R: ConfigEnvironment>(
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    let global_path = env
        .unified_config_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory (no home directory)"))?;

    // Check if config already exists using the environment
    if env.file_exists(&global_path) {
        println!(
            "{}Unified config already exists:{} {}",
            colors.yellow(),
            colors.reset(),
            global_path.display()
        );
        println!("Edit the file to customize, or delete it to regenerate from defaults.");
        println!();
        println!("Next steps:");
        println!("  1. Create a PROMPT.md for your task:");
        println!("       ralph --init <work-guide>");
        println!("       ralph --list-work-guides  # Show all Work Guides");
        println!("  2. Or run ralph directly with default settings:");
        println!("       ralph \"your commit message\"");
        return Ok(true);
    }

    // Create config using the environment's file operations
    env.write_file(&global_path, crate::config::unified::DEFAULT_UNIFIED_CONFIG)
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create config file {}: {}",
                global_path.display(),
                e
            )
        })?;

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
    println!();
    println!("Next steps:");
    println!("  1. Create a PROMPT.md for your task:");
    println!("       ralph --init <work-guide>");
    println!("       ralph --list-work-guides  # Show all Work Guides");
    println!("  2. Or run ralph directly with default settings:");
    println!("       ralph \"your commit message\"");
    Ok(true)
}

/// Handle the `--init-global` flag using the default path resolver.
///
/// Creates a unified config file at `~/.config/ralph-workflow.toml` if it doesn't exist.
/// This is a convenience wrapper that uses [`RealConfigEnvironment`] internally.
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
    handle_init_global_with(colors, &RealConfigEnvironment)
}

/// Prompt the user to confirm overwriting an existing PROMPT.md.
///
/// Returns `true` if the user confirms, `false` otherwise.
///
/// Requires stdin to be a terminal and at least one output stream (stdout/stderr)
/// to be a terminal so prompts are visible.
fn can_prompt_user() -> bool {
    prompt_output_target().is_some()
}

#[derive(Clone, Copy)]
enum PromptOutputTarget {
    Stdout,
    Stderr,
}

fn prompt_output_target() -> Option<PromptOutputTarget> {
    if !std::io::stdin().is_terminal() {
        return None;
    }

    if std::io::stdout().is_terminal() {
        return Some(PromptOutputTarget::Stdout);
    }
    if std::io::stderr().is_terminal() {
        return Some(PromptOutputTarget::Stderr);
    }

    None
}

fn with_prompt_writer<T>(
    target: PromptOutputTarget,
    f: impl FnOnce(&mut dyn std::io::Write) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    use std::io;

    match target {
        PromptOutputTarget::Stdout => {
            let mut out = io::stdout().lock();
            f(&mut out)
        }
        PromptOutputTarget::Stderr => {
            let mut err = io::stderr().lock();
            f(&mut err)
        }
    }
}

fn prompt_overwrite_confirmation(prompt_path: &Path, colors: Colors) -> anyhow::Result<bool> {
    use std::io;

    let Some(target) = prompt_output_target() else {
        return Ok(false);
    };

    with_prompt_writer(target, |w| {
        writeln!(
            w,
            "{}PROMPT.md already exists:{} {}",
            colors.yellow(),
            colors.reset(),
            prompt_path.display()
        )?;
        write!(w, "Do you want to overwrite it? [y/N]: ")?;
        w.flush()?;
        Ok(())
    })?;

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(0) => return Ok(false),
        Ok(_) => {}
        Err(_) => return Ok(false),
    }

    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}

/// Print a short list of common Work Guides.
///
/// Shows the most commonly used Work Guides with a note to use --list-work-guides for more.
fn print_common_work_guides(colors: Colors) {
    println!("{}Common Work Guides:{}", colors.bold(), colors.reset());
    println!(
        "  {}quick{}           Quick/small changes (typos, minor fixes)",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  {}bug-fix{}         Bug fix with investigation guidance",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  {}feature-spec{}    Comprehensive product specification",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  {}refactor{}        Code refactoring with behavior preservation",
        colors.cyan(),
        colors.reset()
    );
    println!();
    println!(
        "Use {}--list-work-guides{} for the complete list of Work Guides.",
        colors.cyan(),
        colors.reset()
    );
    println!();
}

/// Print a template category section.
///
/// Helper function to reduce the length of `handle_list_work_guides`.
fn print_template_category(category_name: &str, templates: &[(&str, &str)], colors: Colors) {
    println!("{}{}:{}", colors.bold(), category_name, colors.reset());
    for (name, description) in templates {
        println!(
            "  {}{}{}  {}",
            colors.cyan(),
            name,
            colors.reset(),
            description
        );
    }
    println!();
}

/// Handle the `--list-work-guides` (or `--list-templates`) flag.
///
/// Lists all available PROMPT.md Work Guides with descriptions, organized by category.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `true` if the flag was handled (program should exit after).
pub fn handle_list_work_guides(colors: Colors) -> bool {
    println!("PROMPT.md Work Guides (use: ralph --init <work-guide>)");
    println!();

    // Common templates (most frequently used)
    print_template_category(
        "Common Templates",
        &[
            ("quick", "Quick/small changes (typos, minor fixes)"),
            ("bug-fix", "Bug fix with investigation guidance"),
            ("feature-spec", "Comprehensive product specification"),
            ("refactor", "Code refactoring with behavior preservation"),
        ],
        colors,
    );

    // Testing and documentation
    print_template_category(
        "Testing & Documentation",
        &[
            ("test", "Test writing with edge case considerations"),
            ("docs", "Documentation update with completeness checklist"),
            ("code-review", "Structured code review for pull requests"),
        ],
        colors,
    );

    // Specialized development
    print_template_category(
        "Specialized Development",
        &[
            ("cli-tool", "CLI tool with argument parsing and completion"),
            ("web-api", "REST/HTTP API with error handling"),
            (
                "ui-component",
                "UI component with accessibility and responsive design",
            ),
            ("onboarding", "Learn a new codebase efficiently"),
        ],
        colors,
    );

    // Advanced/Infrastructure
    print_template_category(
        "Advanced & Infrastructure",
        &[
            (
                "performance-optimization",
                "Performance optimization with benchmarking",
            ),
            (
                "security-audit",
                "Security audit with OWASP Top 10 coverage",
            ),
            (
                "api-integration",
                "API integration with retry logic and resilience",
            ),
            (
                "database-migration",
                "Database migration with zero-downtime strategies",
            ),
            (
                "dependency-update",
                "Dependency update with breaking change handling",
            ),
            ("data-pipeline", "Data pipeline with ETL and monitoring"),
        ],
        colors,
    );

    // Maintenance
    print_template_category(
        "Maintenance & Operations",
        &[
            (
                "debug-triage",
                "Systematic issue investigation and diagnosis",
            ),
            (
                "tech-debt",
                "Technical debt refactoring with prioritization",
            ),
            (
                "release",
                "Release preparation with versioning and changelog",
            ),
        ],
        colors,
    );

    println!("Usage: ralph --init <work-guide>");
    println!();
    println!("Example:");
    println!("  ralph --init bug-fix              # Create bug fix Work Guide");
    println!("  ralph --init feature-spec         # Create feature spec Work Guide");
    println!("  ralph --init quick                # Create quick change Work Guide");
    println!();
    println!("{}Tip:{}", colors.yellow(), colors.reset());
    println!("  Use --init without a value to auto-detect what you need.");
    println!("  Use --force-overwrite to overwrite an existing PROMPT.md.");
    println!("  Run ralph --extended-help to learn about Work Guides vs Agent Prompts.");

    true
}

/// Handle the smart `--init` flag with a custom path resolver.
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
/// * `force` - If true, overwrite existing PROMPT.md without prompting
/// * `colors` - Terminal color configuration for output
/// * `resolver` - Path resolver for determining config file locations
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or `Ok(false)` if not handled, or an error if initialization failed.
pub fn handle_smart_init_with<R: ConfigEnvironment>(
    template_arg: Option<&str>,
    force: bool,
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    let config_path = env
        .unified_config_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory (no home directory)"))?;
    let prompt_path = env.prompt_path();
    handle_smart_init_at_paths_with_env(
        template_arg,
        force,
        colors,
        &config_path,
        &prompt_path,
        env,
    )
}

/// Handle the smart `--init` flag using the default path resolver.
///
/// This is a convenience wrapper that uses [`RealConfigEnvironment`] internally.
///
/// # Arguments
///
/// * `template_arg` - Optional template name from `--init=TEMPLATE`
/// * `force` - If true, overwrite existing PROMPT.md without prompting
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or `Ok(false)` if not handled, or an error if initialization failed.
pub fn handle_smart_init(
    template_arg: Option<&str>,
    force: bool,
    colors: Colors,
) -> anyhow::Result<bool> {
    handle_smart_init_with(template_arg, force, colors, &RealConfigEnvironment)
}

fn handle_smart_init_at_paths_with_env<R: ConfigEnvironment>(
    template_arg: Option<&str>,
    force: bool,
    colors: Colors,
    config_path: &std::path::Path,
    prompt_path: &Path,
    env: &R,
) -> anyhow::Result<bool> {
    let config_exists = env.file_exists(config_path);
    let prompt_exists = env.file_exists(prompt_path);

    // If a template name is provided (non-empty), treat it as --init <template>
    if let Some(template_name) = template_arg {
        if !template_name.is_empty() {
            return handle_init_template_arg_at_path_with_env(
                template_name,
                prompt_path,
                force,
                colors,
                env,
            );
        }
        // Empty string means --init was used without a value, fall through to smart inference
    }

    // No template provided - use smart inference based on current state
    handle_init_state_inference_with_env(
        config_path,
        prompt_path,
        config_exists,
        prompt_exists,
        force,
        colors,
        env,
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

/// Prompt the user to select a template interactively.
///
/// Returns `Some(template_name)` if the user selected a template,
/// or `None` if the user declined or entered invalid input.
fn prompt_for_template(colors: Colors) -> Option<String> {
    use std::io;

    let target = prompt_output_target()?;
    if with_prompt_writer(target, |w| {
        let _ = writeln!(
            w,
            "PROMPT.md contains your task specification for the AI agents."
        );
        let _ = write!(w, "Would you like to create one from a Work Guide? [Y/n]: ");
        w.flush()?;
        Ok(())
    })
    .is_err()
    {
        return None;
    };

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }

    let response = input.trim().to_lowercase();
    if response == "n" || response == "no" || response == "skip" {
        return None;
    }

    // Show available templates
    let templates: Vec<(&str, &str)> = list_templates();
    if with_prompt_writer(target, |w| {
        let _ = writeln!(w);
        let _ = writeln!(w, "Available Work Guides:");

        for (i, (name, description)) in templates.iter().enumerate() {
            let _ = writeln!(
                w,
                "  {}{}{}  {}{}{}",
                colors.cyan(),
                name,
                colors.reset(),
                colors.dim(),
                description,
                colors.reset()
            );
            if (i + 1) % 5 == 0 {
                let _ = writeln!(w); // Group templates in sets of 5 for readability
            }
        }

        let _ = writeln!(w);
        let _ = writeln!(w, "Common choices:");
        let _ = writeln!(
            w,
            "  {}quick{}           - Quick/small changes (typos, minor fixes)",
            colors.cyan(),
            colors.reset()
        );
        let _ = writeln!(
            w,
            "  {}bug-fix{}         - Bug fix with investigation guidance",
            colors.cyan(),
            colors.reset()
        );
        let _ = writeln!(
            w,
            "  {}feature-spec{}    - Product specification",
            colors.cyan(),
            colors.reset()
        );
        let _ = writeln!(w);
        let _ = write!(w, "Enter Work Guide name (or press Enter to use 'quick'): ");
        w.flush()?;
        Ok(())
    })
    .is_err()
    {
        return None;
    };

    let mut template_input = String::new();
    match io::stdin().read_line(&mut template_input) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }

    let template_name = template_input.trim();
    if template_name.is_empty() {
        // Default to 'quick' template
        return Some("quick".to_string());
    }

    // Validate the template exists
    if get_template(template_name).is_some() {
        Some(template_name.to_string())
    } else {
        let _ = with_prompt_writer(target, |w| {
            writeln!(
                w,
                "{}Unknown Work Guide: '{}'{}",
                colors.red(),
                template_name,
                colors.reset()
            )?;
            writeln!(
                w,
                "Run 'ralph --list-work-guides' to see all available Work Guides."
            )?;
            Ok(())
        });
        None
    }
}

/// Create a minimal default PROMPT.md content.
fn create_minimal_prompt_md() -> String {
    "# Task Description

Describe what you want the AI agents to implement.

## Example

\"Fix the typo in the README file\"

## Context

Provide any relevant context about the task:
- What problem are you trying to solve?
- What are the acceptance criteria?
- Are there any specific requirements or constraints?

## Notes

- This is a minimal PROMPT.md created by `ralph --init`
- You can edit this file directly or use `ralph --init <work-guide>` to start from a Work Guide
- Run `ralph --list-work-guides` to see all available Work Guides
"
    .to_string()
}

/// Handle --init when both config and PROMPT.md exist.
fn handle_init_both_exist(
    config_path: &std::path::Path,
    prompt_path: &Path,
    force: bool,
    colors: Colors,
) -> bool {
    // If force is set, show that they can use --force-overwrite to overwrite
    if force {
        println!(
            "{}Note:{} --force-overwrite has no effect when not specifying a Work Guide.",
            colors.yellow(),
            colors.reset()
        );
        println!("Use: ralph --init <work-guide> --force-overwrite  to overwrite PROMPT.md");
        println!();
    }

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
    println!("  ralph --list-work-guides   # Show all Work Guides");
    println!("  ralph --init <work-guide> --force-overwrite  # Overwrite PROMPT.md");
    true
}

// ============================================================================
// Environment-aware versions of init handlers
// ============================================================================
// These versions accept a ConfigEnvironment for dependency injection,
// enabling tests to use in-memory file storage instead of real filesystem.

/// Create PROMPT.md from a template at the specified path.
fn create_prompt_from_template<R: ConfigEnvironment>(
    template_name: &str,
    prompt_path: &Path,
    force: bool,
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    // Validate the template exists first, before any file operations
    let Some(template) = get_template(template_name) else {
        println!(
            "{}Unknown Work Guide: '{}'{}",
            colors.red(),
            template_name,
            colors.reset()
        );
        println!();
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
        println!("Commonly used Work Guides:");
        print_common_work_guides(colors);
        println!("Usage: ralph --init <work-guide>");
        return Ok(true);
    };

    let content = template.content();

    // Check if file exists using the environment
    let file_exists = env.file_exists(prompt_path);

    if force || !file_exists {
        // Write file using the environment
        env.write_file(prompt_path, content)?;
    } else {
        // File exists and not forcing - check if we can prompt
        if can_prompt_user() {
            if !prompt_overwrite_confirmation(prompt_path, colors)? {
                return Ok(true);
            }
            env.write_file(prompt_path, content)?;
        } else {
            return Err(anyhow::anyhow!(
                "PROMPT.md already exists: {}\nRefusing to overwrite in non-interactive mode. Use --force-overwrite to overwrite, or delete/backup the existing file.",
                prompt_path.display()
            ));
        }
    }

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
    println!("Tip: Use --list-work-guides to see all available Work Guides.");

    Ok(true)
}

/// Handle --init with template argument using the provided environment.
fn handle_init_template_arg_at_path_with_env<R: ConfigEnvironment>(
    template_name: &str,
    prompt_path: &Path,
    force: bool,
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    if get_template(template_name).is_some() {
        return create_prompt_from_template(template_name, prompt_path, force, colors, env);
    }

    // Unknown value - show helpful error with suggestions
    println!(
        "{}Unknown Work Guide: '{}'{}",
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

    println!("Commonly used Work Guides:");
    print_common_work_guides(colors);
    println!("Usage: ralph --init=<work-guide>");
    println!("       ralph --init            # Smart init (infers intent)");
    Ok(true)
}

/// Handle --init with smart inference using the provided environment.
fn handle_init_state_inference_with_env<R: ConfigEnvironment>(
    config_path: &std::path::Path,
    prompt_path: &Path,
    config_exists: bool,
    prompt_exists: bool,
    force: bool,
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    match (config_exists, prompt_exists) {
        (false, false) => handle_init_none_exist_with_env(config_path, colors, env),
        (true, false) => Ok(handle_init_only_config_exists_with_env(
            config_path,
            prompt_path,
            force,
            colors,
            env,
        )),
        (false, true) => handle_init_only_prompt_exists_with_env(colors, env),
        (true, true) => Ok(handle_init_both_exist(
            config_path,
            prompt_path,
            force,
            colors,
        )),
    }
}

/// Handle --init when neither config nor PROMPT.md exists, using the provided environment.
fn handle_init_none_exist_with_env<R: ConfigEnvironment>(
    _config_path: &std::path::Path,
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    println!(
        "{}No config found. Creating unified config...{}",
        colors.dim(),
        colors.reset()
    );
    println!();
    handle_init_global_with(colors, env)?;
    Ok(true)
}

/// Handle --init when only config exists (no PROMPT.md), using the provided environment.
fn handle_init_only_config_exists_with_env<R: ConfigEnvironment>(
    config_path: &std::path::Path,
    prompt_path: &Path,
    force: bool,
    colors: Colors,
    env: &R,
) -> bool {
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

    // Show common Work Guides inline
    print_common_work_guides(colors);

    // Check if we're in a TTY for interactive prompting
    if can_prompt_user() {
        // Interactive mode: prompt for template selection
        if let Some(template_name) = prompt_for_template(colors) {
            match create_prompt_from_template(&template_name, prompt_path, force, colors, env) {
                Ok(_) => return true,
                Err(e) => {
                    println!(
                        "{}Failed to create PROMPT.md: {}{}",
                        colors.red(),
                        e,
                        colors.reset()
                    );
                    return true;
                }
            }
        }
        // User declined or entered invalid input, fall through to show usage
    } else {
        // Non-interactive mode: create a minimal default PROMPT.md
        let default_content = create_minimal_prompt_md();

        // Check if file exists using the environment
        if env.file_exists(prompt_path) {
            println!(
                "{}PROMPT.md already exists:{} {}",
                colors.yellow(),
                colors.reset(),
                prompt_path.display()
            );
            println!("Use --force-overwrite to overwrite, or delete/backup the existing file.");
            return true;
        }

        // Write file using the environment
        match env.write_file(prompt_path, &default_content) {
            Ok(()) => {
                println!(
                    "{}Created minimal PROMPT.md{}",
                    colors.green(),
                    colors.reset()
                );
                println!();
                println!("Next steps:");
                println!("  1. Edit PROMPT.md with your task details");
                println!("  2. Run: ralph \"your commit message\"");
                println!();
                println!("Tip: Use ralph --list-work-guides to see all available Work Guides.");
                return true;
            }
            Err(e) => {
                println!(
                    "{}Failed to create PROMPT.md: {}{}",
                    colors.red(),
                    e,
                    colors.reset()
                );
                return true;
            }
        }
    }

    // Show template list if we didn't create PROMPT.md
    println!("Create a PROMPT.md from a Work Guide to get started:");
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
    println!("Usage: ralph --init <work-guide>");
    println!();
    println!("Example:");
    println!("  ralph --init bug-fix");
    println!("  ralph --init feature-spec");
    true
}

/// Handle --init when only PROMPT.md exists (no config), using the provided environment.
fn handle_init_only_prompt_exists_with_env<R: ConfigEnvironment>(
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
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
    handle_init_global_with(colors, env)?;
    Ok(true)
}

/// Handle the `--extended-help` / `--man` flag.
///
/// Displays comprehensive help including shell completion, all presets,
/// troubleshooting information, and the difference between Work Guides and Agent Prompts.
pub fn handle_extended_help() {
    println!(
        r#"RALPH EXTENDED HELP
═══════════════════════════════════════════════════════════════════════════════

Ralph is a PROMPT-driven multi-agent orchestrator for git repos. It runs a
developer agent for code implementation, then a reviewer agent for quality
assurance, automatically staging and committing the final result.

═══════════════════════════════════════════════════════════════════════════════
GETTING STARTED
═══════════════════════════════════════════════════════════════════════════════

  1. Initialize config:
       ralph --init                      # Smart init (infers what you need)

  2. Create a PROMPT.md from a Work Guide:
       ralph --init feature-spec         # Or: bug-fix, refactor, quick, etc.

  3. Edit PROMPT.md with your task details

  4. Run Ralph:
       ralph "fix: my bug description"   # Commit message for the final commit

═══════════════════════════════════════════════════════════════════════════════
WORK GUIDES VS AGENT PROMPTS
═══════════════════════════════════════════════════════════════════════════════

  Ralph has two types of templates - understanding the difference is key:

  1. WORK GUIDES (for PROMPT.md - YOUR task descriptions)
     ─────────────────────────────────────────────────────
     These are templates for describing YOUR work to the AI.
     You fill them in with your specific task requirements.

     Examples: quick, bug-fix, feature-spec, refactor, test, docs

     Commands:
       ralph --init <work-guide>      Create PROMPT.md from a Work Guide
       ralph --list-work-guides       Show all available Work Guides

  2. AGENT PROMPTS (backend AI behavior configuration)
     ─────────────────────────────────────────────────────
     These configure HOW the AI agents behave (internal system prompts).
     You probably don't need to touch these unless customizing agent behavior.

     Commands:
       ralph --init-system-prompts    Create default Agent Prompts
       ralph --list                   Show Agent Prompt templates
       ralph --show <name>            Show a specific Agent Prompt

═══════════════════════════════════════════════════════════════════════════════
PRESET MODES
═══════════════════════════════════════════════════════════════════════════════

  Pick how thorough the AI should be:

    -Q  Quick:      1 dev iteration  + 1 review   (typos, small fixes)
    -U  Rapid:      2 dev iterations + 1 review   (minor changes)
    -S  Standard:   5 dev iterations + 2 reviews  (default for most tasks)
    -T  Thorough:  10 dev iterations + 5 reviews  (complex features)
    -L  Long:      15 dev iterations + 10 reviews (most thorough)

  Custom iterations:
    ralph -D 3 -R 2 "feat: feature"   # 3 dev iterations, 2 review cycles
    ralph -D 10 -R 0 "feat: no review"  # Skip review phase entirely

═══════════════════════════════════════════════════════════════════════════════
COMMON OPTIONS
═══════════════════════════════════════════════════════════════════════════════

  Iterations:
    -D N, --developer-iters N   Set developer iterations
    -R N, --reviewer-reviews N  Set review cycles (0 = skip review)

  Agents:
    -a AGENT, --developer-agent AGENT   Pick developer agent
    -r AGENT, --reviewer-agent AGENT    Pick reviewer agent

  Verbosity:
    -q, --quiet          Quiet mode (minimal output)
    -f, --full           Full output (no truncation)
    -v N, --verbosity N  Set verbosity (0-4)

  Other:
    -d, --diagnose       Show system info and agent status

═══════════════════════════════════════════════════════════════════════════════
ADVANCED OPTIONS
═══════════════════════════════════════════════════════════════════════════════

  These options are hidden from the main --help to reduce clutter.

  Initialization:
    --force-overwrite            Overwrite PROMPT.md without prompting
    -i, --interactive            Prompt for PROMPT.md if missing

  Git Control:
    --with-rebase                Enable automatic rebase to main branch (disabled by default)
    --rebase-only                Only rebase, then exit (no pipeline)
    --git-user-name <name>       Override git user name for commits
    --git-user-email <email>     Override git user email for commits

  Recovery:
    --resume                     Resume from last checkpoint
    --dry-run                    Validate setup without running agents

  Agent Prompt Management:
    --init-system-prompts        Create default Agent Prompt templates
    --list                       List all Agent Prompt templates
    --show <name>                Show Agent Prompt content
    --validate                   Validate Agent Prompt templates
    --variables <name>           Extract variables from template
    --render <name>              Test render a template

  Debugging:
    --show-streaming-metrics     Show JSON streaming quality metrics
    -c PATH, --config PATH       Use specific config file

═══════════════════════════════════════════════════════════════════════════════
SHELL COMPLETION
═══════════════════════════════════════════════════════════════════════════════

  Enable tab-completion for faster command entry:

    Bash:
      ralph --generate-completion=bash > ~/.local/share/bash-completion/completions/ralph

    Zsh:
      ralph --generate-completion=zsh > ~/.zsh/completion/_ralph

    Fish:
      ralph --generate-completion=fish > ~/.config/fish/completions/ralph.fish

  Then restart your shell or source the file.

═══════════════════════════════════════════════════════════════════════════════
TROUBLESHOOTING
═══════════════════════════════════════════════════════════════════════════════

  Common issues:

    "PROMPT.md not found"
      → Run: ralph --init <work-guide>  (e.g., ralph --init bug-fix)

    "No agents available"
      → Run: ralph -d  (diagnose) to check agent status
      → Ensure at least one agent is installed (claude, codex, opencode)

    "Config file not found"
      → Run: ralph --init  to create ~/.config/ralph-workflow.toml

    Resume after interruption:
      → Run: ralph --resume  to continue from last checkpoint

    Validate setup without running:
      → Run: ralph --dry-run

═══════════════════════════════════════════════════════════════════════════════
EXAMPLES
═══════════════════════════════════════════════════════════════════════════════

    ralph "fix: typo"                 Run with default settings
    ralph -Q "fix: small bug"         Quick mode for tiny fixes
    ralph -U "feat: add button"       Rapid mode for minor features
    ralph -a claude "fix: bug"        Use specific agent
    ralph --list-work-guides          See all Work Guides
    ralph --init bug-fix              Create PROMPT.md from a Work Guide
    ralph --init bug-fix --force-overwrite  Overwrite existing PROMPT.md

═══════════════════════════════════════════════════════════════════════════════
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemoryConfigEnvironment;

    /// Create a test environment with typical paths configured.
    fn test_env() -> MemoryConfigEnvironment {
        MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
    }

    #[test]
    fn test_handle_smart_init_with_valid_template_creates_prompt_md() {
        let env = test_env();
        let colors = Colors::new();

        let result = handle_smart_init_with(Some("quick"), false, colors, &env).unwrap();
        assert!(result);

        // Check prompt was created at the environment's prompt path
        let prompt_path = env.prompt_path();
        assert!(env.file_exists(&prompt_path));

        let template = get_template("quick").unwrap();
        let content = env.read_file(&prompt_path).unwrap();
        assert_eq!(content, template.content());
    }

    #[test]
    fn test_handle_smart_init_with_invalid_template_does_not_create_prompt_md() {
        let env = test_env();
        let colors = Colors::new();

        let result =
            handle_smart_init_with(Some("nonexistent-template"), false, colors, &env).unwrap();
        assert!(result);

        // Prompt should not be created for invalid template
        let prompt_path = env.prompt_path();
        assert!(!env.file_exists(&prompt_path));
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
