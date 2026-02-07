// Configuration file generation logic.
//
// This file is included via include!() macro from the parent init.rs module.
// Contains handlers for creating config files and PROMPT.md.

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

    println!(
        "{}Created{} {}",
        colors.green(),
        colors.reset(),
        local_path.display()
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
pub fn handle_init_local_config(colors: Colors, force: bool) -> anyhow::Result<bool> {
    handle_init_local_config_with(colors, &RealConfigEnvironment, force)
}

/// Handle the `--check-config` flag with a custom environment.
///
/// Validates all config files and displays effective merged settings.
/// Returns error (non-zero exit) if validation fails.
pub fn handle_check_config_with<R: ConfigEnvironment>(
    colors: Colors,
    env: &R,
    verbose: bool,
) -> anyhow::Result<bool> {
    use crate::config::loader::{
        load_config_from_path_with_env, ConfigLoadWithValidationError,
    };

    println!(
        "{}Checking configuration...{}",
        colors.dim(),
        colors.reset()
    );
    println!();

    // Load and merge configs (this performs validation)
    let (config, merged_unified, warnings) = match load_config_from_path_with_env(None, env) {
        Ok(result) => result,
        Err(ConfigLoadWithValidationError::ValidationErrors(errors)) => {
            // Validation failed - display errors and exit with non-zero status
            println!("{}Validation errors found:{}", colors.red(), colors.reset());
            println!();

            // Group errors by file for clearer presentation
            let mut global_errors: Vec<_> = Vec::new();
            let mut local_errors: Vec<_> = Vec::new();
            let mut other_errors: Vec<_> = Vec::new();

            for error in &errors {
                let path_str = error.file().to_string_lossy();
                if path_str.contains(".config") {
                    global_errors.push(error);
                } else if path_str.contains(".agent") {
                    local_errors.push(error);
                } else {
                    other_errors.push(error);
                }
            }

            if !global_errors.is_empty() {
                println!("{}~/.config/ralph-workflow.toml:{}", colors.yellow(), colors.reset());
                for error in global_errors {
                    print_config_error(colors, error);
                }
                println!();
            }

            if !local_errors.is_empty() {
                println!("{}.agent/ralph-workflow.toml:{}", colors.yellow(), colors.reset());
                for error in local_errors {
                    print_config_error(colors, error);
                }
                println!();
            }

            if !other_errors.is_empty() {
                for error in other_errors {
                    println!("{}{}:{}", colors.yellow(), error.file().display(), colors.reset());
                    print_config_error(colors, error);
                    println!();
                }
            }

            println!(
                "{}Fix these errors and try again.{}",
                colors.red(),
                colors.reset()
            );

            // Return error to indicate non-zero exit
            return Err(anyhow::anyhow!("Configuration validation failed"));
        }
        Err(ConfigLoadWithValidationError::Io(e)) => {
            return Err(anyhow::anyhow!("Failed to read config file: {}", e));
        }
    };

    // Show warnings (deprecation warnings, etc.)
    if !warnings.is_empty() {
        println!("{}Warnings:{}", colors.yellow(), colors.reset());
        for warning in &warnings {
            println!("  {}", warning);
        }
        println!();
    }

    // Show which config files are active
    let global_path = env.unified_config_path();
    let local_path = env.local_config_path();

    println!("{}Configuration sources:{}", colors.cyan(), colors.reset());

    if let Some(path) = global_path {
        let exists = env.file_exists(&path);
        println!(
            "  Global: {} {}",
            path.display(),
            if exists {
                format!("{}(active){}", colors.green(), colors.reset())
            } else {
                format!("{}(not found){}", colors.dim(), colors.reset())
            }
        );
    }

    if let Some(path) = local_path {
        let exists = env.file_exists(&path);
        println!(
            "  Local:  {} {}",
            path.display(),
            if exists {
                format!("{}(active){}", colors.green(), colors.reset())
            } else {
                format!("{}(not found){}", colors.dim(), colors.reset())
            }
        );
    }

    println!();

    // Show effective settings
    println!("{}Effective settings:{}", colors.cyan(), colors.reset());
    println!("  Verbosity: {}", config.verbosity as u8);
    println!("  Developer iterations: {}", config.developer_iters);
    println!("  Reviewer reviews: {}", config.reviewer_reviews);
    println!("  Interactive: {}", config.behavior.interactive);
    println!("  Isolation mode: {}", config.isolation_mode);

    if verbose {
        println!();
        println!(
            "{}Full merged configuration:{}",
            colors.cyan(),
            colors.reset()
        );
        if let Some(unified) = merged_unified {
            let toml_str = toml::to_string_pretty(&unified)
                .unwrap_or_else(|_| "Error serializing config".to_string());
            println!("{}", toml_str);
        }
    }

    println!();
    println!("{}Configuration valid{}", colors.green(), colors.reset());

    Ok(true)
}

/// Print a single config validation error with appropriate formatting.
fn print_config_error(colors: Colors, error: &crate::config::validation::ConfigValidationError) {
    use crate::config::validation::ConfigValidationError;
    match error {
        ConfigValidationError::TomlSyntax { error, .. } => {
            println!("  {}TOML syntax error:{}", colors.red(), colors.reset());
            println!("    {}", error);
        }
        ConfigValidationError::UnknownKey {
            key,
            suggestion,
            ..
        } => {
            println!("  {}Unknown key '{}'{}", colors.red(), key, colors.reset());
            if let Some(s) = suggestion {
                println!("    {}Did you mean '{}'?{}", colors.dim(), s, colors.reset());
            }
        }
        ConfigValidationError::InvalidValue { key, message, .. } => {
            println!("  {}Invalid value for '{}'{}", colors.red(), key, colors.reset());
            println!("    {}", message);
        }
    }
}

/// Handle the `--check-config` flag using the default environment.
pub fn handle_check_config(colors: Colors, verbose: bool) -> anyhow::Result<bool> {
    handle_check_config_with(colors, &RealConfigEnvironment, verbose)
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
