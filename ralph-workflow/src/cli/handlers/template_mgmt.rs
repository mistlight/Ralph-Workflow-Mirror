//! Template management CLI handler.
//!
//! Provides commands for:
//! - Initializing user templates directory
//! - Listing all templates with metadata
//! - Showing template content and variables
//! - Validating templates for syntax errors
//! - Extracting variables from templates
//! - Rendering templates for testing

use std::collections::HashMap;
use std::fs;

use crate::cli::args::TemplateCommands;
use crate::logger::Colors;
use crate::prompts::partials::get_shared_partials;
use crate::prompts::template_catalog;
use crate::prompts::template_registry::TemplateRegistry;
use crate::prompts::{
    extract_metadata, extract_partials, extract_variables, validate_template, Template,
};

/// Get all available templates as a map of name -> (content, description).
fn get_all_templates() -> HashMap<String, (String, String)> {
    template_catalog::get_templates_map()
}

/// Handle template validation command.
pub fn handle_template_validate(colors: Colors) {
    println!("{}Validating templates...{}", colors.bold(), colors.reset());
    println!();

    let templates = get_all_templates();
    let partials_set: std::collections::HashSet<String> =
        get_shared_partials().keys().cloned().collect();

    let mut total_errors = 0;
    let mut total_warnings = 0;

    for (name, (content, _)) in {
        let mut items: Vec<_> = templates.iter().collect();
        items.sort_by(|a, b| a.0.cmp(b.0));
        items
    } {
        let result = validate_template(content, &partials_set);

        if result.is_valid {
            println!(
                "{}✓{} {}{}{}",
                colors.green(),
                colors.reset(),
                colors.cyan(),
                name,
                colors.reset()
            );
        } else {
            println!(
                "{}✗{} {}{}{}",
                colors.red(),
                colors.reset(),
                colors.cyan(),
                name,
                colors.reset()
            );
        }

        for error in &result.errors {
            println!(
                "  {}error:{} {}",
                colors.red(),
                colors.reset(),
                format_error(error)
            );
            total_errors += 1;
        }

        for warning in &result.warnings {
            println!(
                "  {}warning:{} {}",
                colors.yellow(),
                colors.reset(),
                format_warning(warning)
            );
            total_warnings += 1;
        }

        if !result.variables.is_empty() {
            let var_names: Vec<&str> = result.variables.iter().map(|v| v.name.as_str()).collect();
            println!(
                "  {}variables:{} {}",
                colors.dim(),
                colors.reset(),
                var_names.join(", ")
            );
        }

        if !result.partials.is_empty() {
            println!(
                "  {}partials:{} {}",
                colors.dim(),
                colors.reset(),
                result.partials.join(", ")
            );
        }
    }

    println!();
    if total_errors == 0 {
        println!(
            "{}All templates validated successfully!{}",
            colors.green(),
            colors.reset()
        );
        if total_warnings > 0 {
            println!("{total_warnings} warnings");
        }
    } else {
        println!(
            "{}Validation failed with {} error(s){}",
            colors.red(),
            total_errors,
            colors.reset()
        );
        if total_warnings > 0 {
            println!("{total_warnings} warnings");
        }
        std::process::exit(1);
    }
}

/// Handle template list command.
pub fn handle_template_list(colors: Colors) {
    let templates = get_all_templates();

    println!("{}Available Templates:{}", colors.bold(), colors.reset());
    println!();

    for (name, (_, description)) in {
        let mut items: Vec<_> = templates.iter().collect();
        items.sort_by(|a, b| a.0.cmp(b.0));
        items
    } {
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
    println!("Total: {} templates", templates.len());
}

/// Handle template show command.
pub fn handle_template_show(name: &str, colors: Colors) -> anyhow::Result<()> {
    let templates = get_all_templates();

    let (content, description) = templates
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Template '{name}' not found"))?;

    println!(
        "{}Template: {}{}{}{}",
        colors.bold(),
        colors.cyan(),
        name,
        colors.reset(),
        colors.reset()
    );
    println!(
        "{}Description: {}{}{}",
        colors.dim(),
        description,
        colors.reset(),
        colors.reset()
    );
    println!();

    // Show metadata
    let metadata = extract_metadata(content);
    if let Some(version) = metadata.version {
        println!(
            "{}Version: {}{}{}",
            colors.dim(),
            version,
            colors.reset(),
            colors.reset()
        );
    }
    if let Some(purpose) = metadata.purpose {
        println!(
            "{}Purpose: {}{}{}",
            colors.dim(),
            purpose,
            colors.reset(),
            colors.reset()
        );
    }

    println!();
    println!("{}Variables:{}", colors.bold(), colors.reset());

    let variables = extract_variables(content);
    if variables.is_empty() {
        println!("  (none)");
    } else {
        for var in &variables {
            if var.has_default {
                println!(
                    "  {}{}{} = {}{}{}",
                    colors.cyan(),
                    var.name,
                    colors.reset(),
                    colors.green(),
                    var.default_value.as_deref().unwrap_or(""),
                    colors.reset()
                );
            } else {
                println!("  {}{}{}", colors.cyan(), var.name, colors.reset());
            }
        }
    }

    println!();
    println!("{}Partials:{}", colors.bold(), colors.reset());

    let partials = extract_partials(content);
    if partials.is_empty() {
        println!("  (none)");
    } else {
        for partial in &partials {
            println!("  {}{}{}", colors.cyan(), partial, colors.reset());
        }
    }

    println!();
    println!("{}Content:{}", colors.bold(), colors.reset());
    println!("{}", colors.dim());
    for line in content.lines().take(50) {
        println!("{line}");
    }
    if content.lines().count() > 50 {
        println!("... ({} more lines)", content.lines().count() - 50);
    }
    println!("{}", colors.reset());

    Ok(())
}

/// Handle template variables command.
pub fn handle_template_variables(name: &str, colors: Colors) -> anyhow::Result<()> {
    let templates = get_all_templates();

    let (content, _) = templates
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Template '{name}' not found"))?;

    let variables = extract_variables(content);

    println!(
        "{}Variables in '{}':{}",
        colors.bold(),
        name,
        colors.reset()
    );
    println!();

    if variables.is_empty() {
        println!("  (no variables found)");
    } else {
        for var in &variables {
            let default = if var.has_default {
                format!(
                    " = {}{}{}",
                    colors.green(),
                    var.default_value.as_deref().unwrap_or(""),
                    colors.reset()
                )
            } else {
                String::new()
            };
            println!(
                "  {}{}{}{}  {}line {}{}",
                colors.cyan(),
                var.name,
                colors.reset(),
                default,
                colors.dim(),
                var.line,
                colors.reset()
            );
        }
    }

    println!();
    println!("Total: {} variable(s)", variables.len());

    Ok(())
}

/// Handle template render command.
pub fn handle_template_render(name: &str, colors: Colors) -> anyhow::Result<()> {
    let templates = get_all_templates();

    let (content, _) = templates
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Template '{name}' not found"))?;

    // Get variables from environment or command line
    let mut variables = HashMap::new();

    // For now, just use some example variables for testing
    // In a full implementation, this would parse --var KEY=VALUE arguments
    variables.insert("PROMPT".to_string(), "Example prompt content".to_string());
    variables.insert("PLAN".to_string(), "Example plan content".to_string());
    variables.insert("DIFF".to_string(), "+ example line".to_string());

    println!(
        "{}Rendering template '{}'...{}",
        colors.bold(),
        name,
        colors.reset()
    );
    println!();

    let partials = get_shared_partials();
    let template = Template::new(content);

    match template.render_with_partials(
        &variables
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect(),
        &partials,
    ) {
        Ok(rendered) => {
            println!("{}", colors.dim());
            println!("{rendered}");
            println!("{}", colors.reset());
        }
        Err(e) => {
            println!(
                "{}Render error: {}{}{}",
                colors.red(),
                e,
                colors.reset(),
                colors.reset()
            );
            println!();
            println!("{}Tip:{}", colors.yellow(), colors.reset());
            println!("  Use --template-variables to see which variables are required.");
        }
    }

    Ok(())
}

/// Format a validation error for display.
fn format_error(error: &crate::prompts::ValidationError) -> String {
    match error {
        crate::prompts::ValidationError::UnclosedConditional { line } => {
            format!("unclosed conditional block on line {line}")
        }
        crate::prompts::ValidationError::UnclosedLoop { line } => {
            format!("unclosed loop block on line {line}")
        }
        crate::prompts::ValidationError::InvalidConditional { line, syntax } => {
            format!("invalid conditional syntax on line {line}: '{syntax}'")
        }
        crate::prompts::ValidationError::InvalidLoop { line, syntax } => {
            format!("invalid loop syntax on line {line}: '{syntax}'")
        }
        crate::prompts::ValidationError::UnclosedComment { line } => {
            format!("unclosed comment on line {line}")
        }
        crate::prompts::ValidationError::PartialNotFound { name } => {
            format!("partial not found: '{name}'")
        }
    }
}

/// Format a validation warning for display.
fn format_warning(warning: &crate::prompts::ValidationWarning) -> String {
    match warning {
        crate::prompts::ValidationWarning::VariableMayError { name } => {
            format!("variable '{name}' may cause error if not provided")
        }
    }
}

/// Handle template initialization command.
///
/// Creates the user templates directory and copies all default templates.
fn handle_template_init(force: bool, colors: Colors) -> anyhow::Result<()> {
    let templates_dir = TemplateRegistry::default_user_templates_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory for templates"))?;

    // Create a registry instance to validate the directory structure
    let registry = TemplateRegistry::new(Some(templates_dir.clone()));

    // Check if we're using user templates or embedded templates
    let source = registry.template_source("commit_message_xml");
    let has_user = registry.has_user_template("commit_message_xml");

    // Use the variables to avoid dead code warnings
    let _ = (source, has_user);

    println!(
        "{}Initializing user templates directory...{}",
        colors.bold(),
        colors.reset()
    );
    println!(
        "  Location: {}{}{}",
        colors.cyan(),
        templates_dir.display(),
        colors.reset()
    );
    println!();

    // Check if directory already exists
    if templates_dir.exists() {
        if force {
            println!(
                "{}Warning: {}Directory already exists. Overwriting...{}",
                colors.yellow(),
                colors.reset(),
                colors.reset()
            );
        } else {
            println!(
                "{}Error: {}Directory already exists. Use --force to overwrite.{}",
                colors.red(),
                colors.reset(),
                colors.reset()
            );
            println!();
            println!("To reinitialize with defaults, run:");
            println!("  ralph --template-init --force");
            return Err(anyhow::anyhow!("Templates directory already exists"));
        }
    }

    // Create directory structure
    fs::create_dir_all(&templates_dir)?;

    let shared_dir = templates_dir.join("shared");
    fs::create_dir_all(&shared_dir)?;

    let reviewer_dir = templates_dir.join("reviewer");
    fs::create_dir_all(&reviewer_dir)?;

    // Copy all templates from the embedded templates
    let templates = get_all_templates();
    let mut copied = 0;
    let mut skipped = 0;

    for (name, (content, _)) in &templates {
        let target_path = if name.starts_with("reviewer/") {
            let parts: Vec<&str> = name.split('/').collect();
            if parts.len() == 2 {
                templates_dir
                    .join("reviewer")
                    .join(format!("{}.txt", parts[1]))
            } else {
                continue;
            }
        } else {
            templates_dir.join(format!("{name}.txt"))
        };

        // Skip if file exists and not forcing
        if target_path.exists() && !force {
            skipped += 1;
            continue;
        }

        fs::write(&target_path, content)?;
        copied += 1;
    }

    // Copy shared partials
    let partials = get_shared_partials();
    for (name, content) in &partials {
        let target_path = templates_dir.join(format!("{name}.txt"));
        if target_path.exists() && !force {
            skipped += 1;
            continue;
        }
        fs::write(&target_path, content)?;
        copied += 1;
    }

    println!(
        "{}Successfully initialized user templates!{}",
        colors.green(),
        colors.reset()
    );
    println!();
    println!("  {copied} templates copied");
    if skipped > 0 {
        println!("  {skipped} templates skipped (already exists)");
    }
    println!();
    println!("You can now edit templates in:");
    println!("  {}", templates_dir.display());
    println!();
    println!("Changes to user templates will override the built-in templates.");

    Ok(())
}

/// Handle all template commands.
pub fn handle_template_commands(commands: &TemplateCommands, colors: Colors) -> anyhow::Result<()> {
    if commands.init_templates_enabled() {
        handle_template_init(commands.force, colors)?;
    } else if commands.validate {
        handle_template_validate(colors);
    } else if let Some(ref name) = commands.show {
        handle_template_show(name, colors)?;
    } else if commands.list {
        handle_template_list(colors);
    } else if let Some(ref name) = commands.variables {
        handle_template_variables(name, colors)?;
    } else if let Some(ref name) = commands.render {
        handle_template_render(name, colors)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_templates_not_empty() {
        let templates = get_all_templates();
        assert!(!templates.is_empty());
        assert!(templates.contains_key("developer_iteration"));
        assert!(templates.contains_key("commit_message_xml"));
    }

    #[test]
    fn test_template_show_valid() {
        let colors = Colors::new();
        let result = handle_template_show("developer_iteration", colors);
        assert!(result.is_ok());
    }

    #[test]
    fn test_template_show_invalid() {
        let colors = Colors::new();
        let result = handle_template_show("nonexistent", colors);
        assert!(result.is_err());
    }

    #[test]
    fn test_template_variables() {
        let colors = Colors::new();
        let result = handle_template_variables("developer_iteration", colors);
        assert!(result.is_ok());
    }
}
