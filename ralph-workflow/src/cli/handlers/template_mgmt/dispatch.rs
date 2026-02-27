/// Handle all template commands.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn handle_template_commands(commands: &TemplateCommands, colors: Colors) -> anyhow::Result<()> {
    if commands.init_templates_enabled() {
        handle_template_init(commands.force, colors)?;
    } else if commands.validate {
        handle_template_validate(colors);
    } else if let Some(ref name) = commands.show {
        handle_template_show(name, colors)?;
    } else if commands.list {
        handle_template_list(colors);
    } else if commands.list_all {
        handle_template_list_all(colors);
    } else if let Some(ref name) = commands.variables {
        handle_template_variables(name, colors)?;
    } else if let Some(ref name) = commands.render {
        handle_template_render(name, colors)?;
    }

    Ok(())
}
