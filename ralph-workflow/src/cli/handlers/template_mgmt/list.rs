/// Handle template list command.
pub fn handle_template_list(colors: Colors) {
    handle_template_list_impl(colors, false);
}

/// Handle template list all command (including deprecated).
pub fn handle_template_list_all(colors: Colors) {
    handle_template_list_impl(colors, true);
}

/// Implementation of template list command.
fn handle_template_list_impl(colors: Colors, include_deprecated: bool) {
    let all_templates = get_all_templates();
    let filtered_templates: Vec<_> = all_templates
        .iter()
        .filter(|(name, _)| {
            if include_deprecated {
                return true;
            }
            // Check if this is a deprecated template by looking at the catalog
            if let Some(meta) = template_catalog::get_template_metadata(name) {
                !meta.deprecated
            } else {
                true
            }
        })
        .map(|(name, (content, desc))| {
            // For deprecated templates, use their content which points to consolidated versions
            (name, content, desc)
        })
        .collect();

    let header = if include_deprecated {
        "All Templates (including deprecated):"
    } else {
        "Active Templates:"
    };

    println!("{}{}{}", colors.bold(), header, colors.reset());
    println!();

    for (name, _, description) in {
        let mut items: Vec<_> = filtered_templates.clone();
        items.sort_by(|a, b| a.0.cmp(b.0));
        items
    } {
        // Show deprecated marker in the list
        let is_deprecated = if let Some(meta) = template_catalog::get_template_metadata(name) {
            meta.deprecated
        } else {
            false
        };

        let deprecated_marker = if is_deprecated {
            format!("{} [DEPRECATED]{}", colors.yellow(), colors.reset())
        } else {
            String::new()
        };

        println!(
            "  {}{}{}{}  {}{}{}",
            colors.cyan(),
            name,
            colors.reset(),
            deprecated_marker,
            colors.dim(),
            description,
            colors.reset()
        );
    }

    println!();
    if include_deprecated {
        let deprecated_count = filtered_templates
            .iter()
            .filter(|(name, _, _)| {
                if let Some(meta) = template_catalog::get_template_metadata(name) {
                    meta.deprecated
                } else {
                    false
                }
            })
            .count();

        println!(
            "Total: {} templates ({} active, {} deprecated)",
            filtered_templates.len(),
            filtered_templates.len() - deprecated_count,
            deprecated_count
        );
        println!();
        println!("{}Tip:{}", colors.yellow(), colors.reset());
        println!("  Edit templates in ~/.config/ralph/templates/");
        println!("  Deprecated templates are kept for backward compatibility.");
        println!(
            "  Use {}--list{} to show only active templates.",
            colors.bold(),
            colors.reset()
        );
    } else {
        println!("Total: {} active templates", filtered_templates.len());
        println!();
        println!("{}Tip:{}", colors.yellow(), colors.reset());
        println!("  Edit templates in ~/.config/ralph/templates/");
        println!(
            "  Use {}--list-all{} to include deprecated templates",
            colors.bold(),
            colors.reset()
        );
    }
}
