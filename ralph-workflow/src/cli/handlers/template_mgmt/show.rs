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
