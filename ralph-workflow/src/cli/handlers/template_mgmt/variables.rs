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
