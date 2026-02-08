//! PROMPT.md creation and smart init orchestration.
//!
//! Handles creating PROMPT.md from Work Guide templates and smart `--init` logic
//! that infers user intent based on existing files.

use crate::config::ConfigEnvironment;
use crate::logger::Colors;
use crate::templates::{get_template, list_templates};
use std::path::Path;

use super::global::handle_init_global_with;

// Import helpers from parent module
use super::super::{
    can_prompt_user, find_similar_templates, print_common_work_guides, prompt_for_template,
    prompt_overwrite_confirmation,
};

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

/// Create PROMPT.md from a template at the specified path.
pub fn create_prompt_from_template<R: ConfigEnvironment>(
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
pub fn handle_init_template_arg_at_path_with_env<R: ConfigEnvironment>(
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
///
/// Dispatches to the appropriate handler based on which files exist.
pub fn handle_init_state_inference_with_env<R: ConfigEnvironment>(
    config_path: &std::path::Path,
    prompt_path: &Path,
    config_exists: bool,
    prompt_exists: bool,
    force: bool,
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    match (config_exists, prompt_exists) {
        (false, false) => super::global::handle_init_none_exist_with_env(config_path, colors, env),
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

/// Handle --init when only config exists (no PROMPT.md), using the provided environment.
pub fn handle_init_only_config_exists_with_env<R: ConfigEnvironment>(
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
pub fn handle_init_only_prompt_exists_with_env<R: ConfigEnvironment>(
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
