//! Configuration validation and error display.
//!
//! Handles `--check-config` flag to validate config files and display effective settings.

use crate::config::loader::{load_config_from_path_with_env, ConfigLoadWithValidationError};
use crate::config::validation::ConfigValidationError;
use crate::config::{ConfigEnvironment, RealConfigEnvironment};
use crate::logger::Colors;

/// Handle the `--check-config` flag with a custom environment.
///
/// Validates all config files and displays effective merged settings.
/// Returns error (non-zero exit) if validation fails.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
/// * `env` - Config environment for path resolution and file operations
/// * `verbose` - Whether to display full merged configuration
///
/// # Returns
///
/// Returns `Ok(true)` if validation succeeded, or an error if validation failed.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn handle_check_config_with<R: ConfigEnvironment>(
    colors: Colors,
    env: &R,
    verbose: bool,
) -> anyhow::Result<bool> {
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
                println!(
                    "{}~/.config/ralph-workflow.toml:{}",
                    colors.yellow(),
                    colors.reset()
                );
                for error in global_errors {
                    print_config_error(colors, error);
                }
                println!();
            }

            if !local_errors.is_empty() {
                println!(
                    "{}.agent/ralph-workflow.toml:{}",
                    colors.yellow(),
                    colors.reset()
                );
                for error in local_errors {
                    print_config_error(colors, error);
                }
                println!();
            }

            if !other_errors.is_empty() {
                for error in other_errors {
                    println!(
                        "{}{}:{}",
                        colors.yellow(),
                        error.file().display(),
                        colors.reset()
                    );
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
            return Err(anyhow::anyhow!("Failed to read config file: {e}"));
        }
    };

    // Show warnings (deprecation warnings, etc.)
    if !warnings.is_empty() {
        println!("{}Warnings:{}", colors.yellow(), colors.reset());
        for warning in &warnings {
            println!("  {warning}");
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
            println!("{toml_str}");
        }
    }

    println!();
    println!("{}Configuration valid{}", colors.green(), colors.reset());

    Ok(true)
}

/// Print a single config validation error with appropriate formatting.
fn print_config_error(colors: Colors, error: &ConfigValidationError) {
    match error {
        ConfigValidationError::TomlSyntax { error, .. } => {
            println!("  {}TOML syntax error:{}", colors.red(), colors.reset());
            println!("    {error}");
        }
        ConfigValidationError::UnknownKey {
            key, suggestion, ..
        } => {
            println!("  {}Unknown key '{}'{}", colors.red(), key, colors.reset());
            if let Some(s) = suggestion {
                println!(
                    "    {}Did you mean '{}'?{}",
                    colors.dim(),
                    s,
                    colors.reset()
                );
            }
        }
        ConfigValidationError::InvalidValue { key, message, .. } => {
            println!(
                "  {}Invalid value for '{}'{}",
                colors.red(),
                key,
                colors.reset()
            );
            println!("    {message}");
        }
    }
}

/// Handle the `--check-config` flag using the default environment.
///
/// Convenience wrapper that uses [`RealConfigEnvironment`] internally.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn handle_check_config(colors: Colors, verbose: bool) -> anyhow::Result<bool> {
    handle_check_config_with(colors, &RealConfigEnvironment, verbose)
}
