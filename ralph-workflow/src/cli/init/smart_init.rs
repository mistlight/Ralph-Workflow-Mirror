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
/// * `env` - Path resolver for determining config file locations
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
