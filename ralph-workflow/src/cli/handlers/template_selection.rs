//! Interactive template selection module.
//!
//! Provides functionality for prompting users to select a PROMPT.md template
//! when one doesn't exist and interactive mode is enabled.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::logger::Colors;
use crate::templates::{get_template, list_templates};

/// Result of interactive template selection.
///
/// * `Some(template_name)` - User selected a template
/// * `None` - User declined or input was not a terminal
pub type TemplateSelectionResult = Option<String>;

/// Prompt the user to select a template when PROMPT.md is missing.
///
/// This function:
/// 1. Displays a message that PROMPT.md is missing
/// 2. Asks if the user wants to create one from a template
/// 3. If yes, displays available templates
/// 4. Prompts for template selection (with default to feature-spec)
/// 5. Returns the selected template name or None if declined
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// * `Some(template_name)` - User selected a template
/// * `None` - User declined, input was not a terminal, or input errored/ended
pub fn prompt_template_selection(colors: Colors) -> TemplateSelectionResult {
    // Interactive prompts require both stdin and stdout to be terminals.
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return None;
    }

    println!();
    println!("{}PROMPT.md not found.{}", colors.yellow(), colors.reset());
    println!();
    println!("PROMPT.md contains your task specification for the AI agents.");
    print!("Would you like to create one from a template? [Y/n]: ");
    if io::stdout().flush().is_err() {
        return None;
    }

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(0) | Err(_) => return None, // EOF
        Ok(_) => {}
    }

    let response = input.trim().to_lowercase();

    // User declined (explicit 'n' or 'no')
    if response == "n" || response == "no" || response == "skip" {
        return None;
    }

    // Empty or 'y'/'yes' means yes - proceed to template selection
    println!();
    println!("Available templates:");

    let templates = list_templates();

    for (name, description) in &templates {
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

    // Prompt for template selection with default to feature-spec
    print!(
        "Select template {}[default: feature-spec]{}: ",
        colors.dim(),
        colors.reset()
    );
    if io::stdout().flush().is_err() {
        return None;
    }

    let mut template_input = String::new();
    match io::stdin().read_line(&mut template_input) {
        Ok(0) | Err(_) => return None, // EOF
        Ok(_) => {}
    }

    let template_name = template_input.trim();

    // Empty input defaults to feature-spec
    let selected = if template_name.is_empty() {
        "feature-spec"
    } else {
        template_name
    };

    // Validate the template exists
    if get_template(selected).is_none() {
        println!(
            "{}Unknown template: '{}'. Using feature-spec as default.{}",
            colors.yellow(),
            selected,
            colors.reset()
        );
        return Some("feature-spec".to_string());
    }

    Some(selected.to_string())
}

/// Create PROMPT.md from the selected template.
///
/// # Arguments
///
/// * `template_name` - The name of the template to use
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// * `Ok(())` - File created successfully
/// * `Err(e)` - Failed to create file
pub fn create_prompt_from_template(template_name: &str, colors: Colors) -> anyhow::Result<()> {
    let prompt_path = Path::new("PROMPT.md");

    // Check if PROMPT.md already exists (shouldn't happen in our flow, but safety check)
    if prompt_path.exists() {
        println!(
            "{}PROMPT.md already exists. Skipping creation.{}",
            colors.yellow(),
            colors.reset()
        );
        return Ok(());
    }

    // Get the template
    let Some(template) = get_template(template_name) else {
        return Err(anyhow::anyhow!("Template '{template_name}' not found"));
    };

    // Write the template content to PROMPT.md
    let content = template.content();
    fs::write(prompt_path, content)?;

    println!();
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
    println!("  2. Run ralph again with your commit message");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_template_by_name() {
        // Verify all our template names are valid
        assert!(get_template("feature-spec").is_some());
        assert!(get_template("bug-fix").is_some());
        assert!(get_template("refactor").is_some());
        assert!(get_template("test").is_some());
        assert!(get_template("docs").is_some());
        assert!(get_template("quick").is_some());
        assert!(get_template("nonexistent").is_none());
    }

    #[test]
    fn test_template_has_required_content() {
        // All templates should have Goal and Acceptance sections
        for (name, _) in list_templates() {
            if let Some(template) = get_template(name) {
                let content = template.content();
                assert!(
                    content.contains("## Goal"),
                    "Template {name} missing Goal section"
                );
                assert!(
                    content.contains("Acceptance") || content.contains("## Acceptance Checks"),
                    "Template {name} missing Acceptance section"
                );
            }
        }
    }
}
