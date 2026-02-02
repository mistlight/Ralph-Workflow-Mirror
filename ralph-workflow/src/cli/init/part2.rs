// Smart init logic and fuzzy matching utilities.

/// Handle the smart `--init` flag with a custom path resolver.
///
/// This function intelligently determines what the user wants to initialize:
/// - If a value is provided and matches a known template name -> create PROMPT.md
/// - If config doesn't exist and no template specified -> create config
/// - If config exists but PROMPT.md doesn't -> prompt to create PROMPT.md
/// - If both exist -> show helpful message about what's already set up
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
    // The division result is guaranteed to fit in u32 since it's <= 100
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
