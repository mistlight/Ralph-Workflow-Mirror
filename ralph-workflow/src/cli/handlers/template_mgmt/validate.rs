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
