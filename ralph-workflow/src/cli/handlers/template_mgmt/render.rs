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
