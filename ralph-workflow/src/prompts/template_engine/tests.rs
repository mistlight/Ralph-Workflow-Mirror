// Tests for the template engine.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template() {
        let template = Template::new("Hello {{NAME}}, your score is {{SCORE}}.");
        let variables = HashMap::from([("NAME", "Alice".to_string()), ("SCORE", "42".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Alice, your score is 42.");
    }

    #[test]
    fn test_missing_variable() {
        let template = Template::new("Hello {{NAME}}.");
        let variables = HashMap::new();
        let result = template.render(&variables);
        assert_eq!(
            result,
            Err(TemplateError::MissingVariable("NAME".to_string()))
        );
    }

    #[test]
    fn test_no_variables() {
        let template = Template::new("Just plain text.");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "Just plain text.");
    }

    #[test]
    fn test_multiline_template() {
        let template = Template::new("Review this:\n{{DIFF}}\nEnd of review.");
        let variables = HashMap::from([("DIFF", "+ new line".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Review this:\n+ new line\nEnd of review.");
    }

    #[test]
    fn test_whitespace_in_variables() {
        let template = Template::new("Value: {{ VALUE }}.");
        let variables = HashMap::from([("VALUE", "42".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Value: 42.");
    }

    #[test]
    fn test_unclosed_opening_braces() {
        // Unclosed {{ should be ignored (no placeholder extracted)
        let template = Template::new("Hello {{NAME and some text");
        let rendered = template.render(&HashMap::new()).unwrap();
        // The unclosed braces are treated as literal text
        assert_eq!(rendered, "Hello {{NAME and some text");
    }

    #[test]
    fn test_empty_variable_name() {
        // Empty variable name {{}} should be ignored (no placeholder extracted)
        let template = Template::new("Value: {{}}.");
        let rendered = template.render(&HashMap::new()).unwrap();
        // Empty placeholder is treated as literal text
        assert_eq!(rendered, "Value: {{}}.");
    }

    #[test]
    fn test_whitespace_only_variable_name() {
        // Whitespace-only variable name {{   }} should be ignored
        let template = Template::new("Value: {{   }}.");
        let rendered = template.render(&HashMap::new()).unwrap();
        // Whitespace-only placeholder is treated as literal text
        assert_eq!(rendered, "Value: {{   }}.");
    }

    #[test]
    fn test_multiple_unclosed_braces() {
        // Multiple unclosed {{ should all be ignored
        let template = Template::new("{{A text {{B text");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "{{A text {{B text");
    }

    #[test]
    fn test_partial_closing_brace() {
        // Single closing brace without the second should not close the placeholder
        let template = Template::new("Hello {{NAME}} and {{VAR}} text");
        let variables = HashMap::from([("NAME", "Alice".to_string()), ("VAR", "Bob".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Alice and Bob text");
    }

    // =========================================================================
    // Comment Stripping Tests
    // =========================================================================

    #[test]
    fn test_inline_comment_stripped() {
        let template = Template::new("Hello {# this is a comment #}world.");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "Hello world.");
    }

    #[test]
    fn test_comment_on_own_line_stripped() {
        // Comment on its own line should be completely removed including the line itself
        let template = Template::new("Line 1\n{# This is a comment #}\nLine 2");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "Line 1\nLine 2");
    }

    #[test]
    fn test_multiline_comment() {
        // Multiline comments should be fully stripped
        let template = Template::new("Before{# comment\nspanning\nlines #}After");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "BeforeAfter");
    }

    #[test]
    fn test_comment_at_end_of_content_line() {
        // Comment at end of content line should only remove the comment
        let template = Template::new("Content{# comment #}\nMore");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "Content\nMore");
    }

    #[test]
    fn test_multiple_comments() {
        let template = Template::new("{# first #}A{# second #}B{# third #}");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "AB");
    }

    #[test]
    fn test_comment_with_variable() {
        // Comments should work alongside variables
        let template = Template::new("{# doc comment #}\nHello {{NAME}}!");
        let variables = HashMap::from([("NAME", "World".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello World!");
    }

    #[test]
    fn test_unclosed_comment_preserved() {
        // Unclosed comment should be treated as literal text
        let template = Template::new("Hello {# unclosed comment");
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "Hello {# unclosed comment");
    }

    #[test]
    fn test_comment_documentation_use_case() {
        // Real use case: documentation comments in template
        let content = r"{# Template Version: 1.0 #}
{# This template generates commit messages #}
You are a commit message expert.

{# DIFF variable contains the git diff #}
DIFF:
{{DIFF}}

{# End of template #}
";
        let template = Template::new(content);
        let variables = HashMap::from([("DIFF", "+added line".to_string())]);
        let rendered = template.render(&variables).unwrap();

        // Verify documentation comments are stripped
        assert!(!rendered.contains("Template Version"));
        assert!(!rendered.contains("This template generates"));
        assert!(!rendered.contains("DIFF variable contains"));
        assert!(!rendered.contains("End of template"));

        // Verify content is preserved
        assert!(rendered.contains("You are a commit message expert."));
        assert!(rendered.contains("+added line"));
    }

    // =========================================================================
    // Partials Tests
    // =========================================================================

    #[test]
    fn test_simple_partial_include() {
        let partials = HashMap::from([("header".to_string(), "Common Header".to_string())]);
        let template = Template::new("{{>header}}\nContent here");
        let variables = HashMap::new();
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "Common Header\nContent here");
    }

    #[test]
    fn test_partial_with_whitespace() {
        let partials = HashMap::from([("header".to_string(), "Header".to_string())]);
        let template = Template::new("{{> header}}\nContent");
        let variables = HashMap::new();
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "Header\nContent");
    }

    #[test]
    fn test_partial_with_variables() {
        let partials = HashMap::from([("greeting".to_string(), "Hello {{NAME}}\n".to_string())]);
        let template = Template::new("{{>greeting}}Body content");
        let variables = HashMap::from([("NAME", "World".to_string())]);
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "Hello World\nBody content");
    }

    #[test]
    fn test_multiple_partials() {
        let partials = HashMap::from([
            ("header".to_string(), "=== HEADER ===\n".to_string()),
            ("footer".to_string(), "\n=== FOOTER ===".to_string()),
        ]);
        let template = Template::new("{{>header}}Content{{>footer}}");
        let variables = HashMap::new();
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "=== HEADER ===\nContent\n=== FOOTER ===");
    }

    #[test]
    fn test_nested_partials() {
        let partials = HashMap::from([
            (
                "outer".to_string(),
                "Outer start\n{{>inner}}\nOuter end".to_string(),
            ),
            ("inner".to_string(), "INNER CONTENT".to_string()),
        ]);
        let template = Template::new("{{>outer}}");
        let variables = HashMap::new();
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "Outer start\nINNER CONTENT\nOuter end");
    }

    #[test]
    fn test_partial_not_found() {
        let partials = HashMap::new();
        let template = Template::new("{{>missing_partial}}");
        let variables = HashMap::new();
        let result = template.render_with_partials(&variables, &partials);
        assert_eq!(
            result,
            Err(TemplateError::PartialNotFound(
                "missing_partial".to_string()
            ))
        );
    }

    #[test]
    fn test_circular_reference_detection() {
        let partials = HashMap::from([
            ("a".to_string(), "{{>b}}".to_string()),
            ("b".to_string(), "{{>a}}".to_string()),
        ]);
        let template = Template::new("{{>a}}");
        let variables = HashMap::new();
        let result = template.render_with_partials(&variables, &partials);
        match result {
            Err(TemplateError::CircularReference(chain)) => {
                // Chain should contain a circular reference between a and b
                assert_eq!(chain.len(), 3);
                assert!(chain.contains(&"a".to_string()));
                assert!(chain.contains(&"b".to_string()));
                // First and last elements should be the same (indicating a cycle)
                assert_eq!(chain.first(), chain.last());
            }
            _ => panic!("Expected CircularReference error"),
        }
    }

    #[test]
    fn test_self_referential_partial() {
        let partials = HashMap::from([("loop".to_string(), "{{>loop}}".to_string())]);
        let template = Template::new("{{>loop}}");
        let variables = HashMap::new();
        let result = template.render_with_partials(&variables, &partials);
        match result {
            Err(TemplateError::CircularReference(chain)) => {
                assert_eq!(chain, vec!["loop".to_string(), "loop".to_string()]);
            }
            _ => panic!("Expected CircularReference error"),
        }
    }

    #[test]
    fn test_partial_with_missing_variable() {
        let partials = HashMap::from([("greeting".to_string(), "Hello {{NAME}}".to_string())]);
        let template = Template::new("{{>greeting}}");
        let variables = HashMap::new(); // NAME not provided
        let result = template.render_with_partials(&variables, &partials);
        assert_eq!(
            result,
            Err(TemplateError::MissingVariable("NAME".to_string()))
        );
    }

    #[test]
    fn test_partial_and_main_variables() {
        let partials = HashMap::from([("greeting".to_string(), "Hello {{NAME}}\n".to_string())]);
        let template = Template::new("{{>greeting}}Your score is {{SCORE}}");
        let variables = HashMap::from([("NAME", "Alice".to_string()), ("SCORE", "42".to_string())]);
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "Hello Alice\nYour score is 42");
    }

    #[test]
    fn test_partial_with_comments() {
        let partials = HashMap::from([(
            "header".to_string(),
            "{# This is a header #}Header Content\n".to_string(),
        )]);
        let template = Template::new("{{>header}}Body");
        let variables = HashMap::new();
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "Header Content\nBody");
    }

    #[test]
    fn test_partial_with_path_style_name() {
        let partials = HashMap::from([("shared/_header".to_string(), "Shared Header".to_string())]);
        let template = Template::new("{{> shared/_header}}\nContent");
        let variables = HashMap::new();
        let rendered = template
            .render_with_partials(&variables, &partials)
            .unwrap();
        assert_eq!(rendered, "Shared Header\nContent");
    }

    #[test]
    fn test_backward_compatibility_render_without_partials() {
        // Ensure the original render() method still works
        let template = Template::new("Hello {{NAME}}");
        let variables = HashMap::from([("NAME", "World".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello World");
    }

    #[test]
    fn test_empty_partial_name_ignored() {
        // {{> }} with empty name should be treated as literal text
        let template = Template::new("Before {{> }} After");
        let variables = HashMap::new();
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Before {{> }} After");
    }

    // =========================================================================
    // Conditional Tests
    // =========================================================================

    #[test]
    fn test_conditional_with_true_variable() {
        let template = Template::new("{% if NAME %}Hello {{NAME}}{% endif %}");
        let variables = HashMap::from([("NAME", "World".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello World");
    }

    #[test]
    fn test_conditional_with_false_variable() {
        let template = Template::new("{% if NAME %}Hello {{NAME}}{% endif %}");
        let variables = HashMap::new(); // NAME not provided
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "");
    }

    #[test]
    fn test_conditional_with_empty_variable() {
        let template = Template::new("{% if NAME %}Hello {{NAME}}{% endif %}");
        let variables = HashMap::from([("NAME", String::new())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "");
    }

    #[test]
    fn test_conditional_with_negation_true() {
        let template = Template::new("{% if !NAME %}No name{% endif %}");
        let variables = HashMap::new(); // NAME not provided
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "No name");
    }

    #[test]
    fn test_conditional_with_negation_false() {
        let template = Template::new("{% if !NAME %}No name{% endif %}");
        let variables = HashMap::from([("NAME", "Alice".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "");
    }

    #[test]
    fn test_multiple_conditionals() {
        let template = Template::new(
            "{% if GREETING %}{{GREETING}}{% endif %} {% if NAME %}{{NAME}}{% endif %}",
        );
        let variables = HashMap::from([("NAME", "Bob".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, " Bob");
    }

    #[test]
    fn test_conditional_with_surrounding_content() {
        let template = Template::new("Start {% if SHOW %}shown{% endif %} End");
        let variables = HashMap::from([("SHOW", "yes".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Start shown End");
    }

    // =========================================================================
    // Default Value Tests
    // =========================================================================

    #[test]
    fn test_default_value_with_missing_variable() {
        let template = Template::new("Hello {{NAME|default=\"Guest\"}}");
        let variables = HashMap::new();
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Guest");
    }

    #[test]
    fn test_default_value_with_empty_variable() {
        let template = Template::new("Hello {{NAME|default=\"Guest\"}}");
        let variables = HashMap::from([("NAME", String::new())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Guest");
    }

    #[test]
    fn test_default_value_with_present_variable() {
        let template = Template::new("Hello {{NAME|default=\"Guest\"}}");
        let variables = HashMap::from([("NAME", "Alice".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Alice");
    }

    #[test]
    fn test_default_value_with_single_quotes() {
        let template = Template::new("Hello {{NAME|default='Guest'}}");
        let variables = HashMap::new();
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Guest");
    }

    // =========================================================================
    // Loop Tests
    // =========================================================================

    #[test]
    fn test_loop_with_items() {
        let template = Template::new("{% for item in ITEMS %}{{item}} {% endfor %}");
        let variables = HashMap::from([("ITEMS", "apple,banana,cherry".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "apple banana cherry ");
    }

    #[test]
    fn test_loop_with_empty_list() {
        let template = Template::new("{% for item in ITEMS %}{{item}} {% endfor %}");
        let variables = HashMap::from([("ITEMS", String::new())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "");
    }

    #[test]
    fn test_loop_with_missing_variable() {
        let template = Template::new("{% for item in ITEMS %}{{item}} {% endfor %}");
        let variables = HashMap::new();
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "");
    }

    #[test]
    fn test_loop_with_conditional_inside() {
        let template =
            Template::new("{% for item in ITEMS %}{% if item %}{{item}} {% endif %}{% endfor %}");
        let variables = HashMap::from([("ITEMS", "apple,,cherry".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "apple cherry ");
    }

    // =========================================================================
    // Substitution Log Tests
    // =========================================================================

    #[test]
    fn test_substitution_log_value_provided() {
        let template = Template::new("Hello {{NAME}}");
        let variables = HashMap::from([("NAME", "Alice".to_string())]);

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .unwrap();

        assert_eq!(rendered.content, "Hello Alice");
        assert_eq!(rendered.log.template_name, "test");
        assert_eq!(rendered.log.substituted.len(), 1);
        assert_eq!(rendered.log.substituted[0].name, "NAME");
        assert_eq!(
            rendered.log.substituted[0].source,
            crate::prompts::SubstitutionSource::Value
        );
        assert!(rendered.log.is_complete());
        assert!(rendered.log.unsubstituted.is_empty());
    }

    #[test]
    fn test_substitution_log_default_used() {
        let template = Template::new("Hello {{NAME|default=\"Guest\"}}");
        let variables = HashMap::new();

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .unwrap();

        assert_eq!(rendered.content, "Hello Guest");
        assert_eq!(rendered.log.substituted.len(), 1);
        assert_eq!(rendered.log.substituted[0].name, "NAME");
        assert_eq!(
            rendered.log.substituted[0].source,
            crate::prompts::SubstitutionSource::Default
        );
        assert!(rendered.log.is_complete());
    }

    #[test]
    fn test_substitution_log_empty_with_default() {
        let template = Template::new("Hello {{NAME|default=\"Guest\"}}");
        let variables = HashMap::from([("NAME", "".to_string())]);

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .unwrap();

        assert_eq!(rendered.content, "Hello Guest");
        assert_eq!(
            rendered.log.substituted[0].source,
            crate::prompts::SubstitutionSource::EmptyWithDefault
        );
        assert!(rendered.log.is_complete());
    }

    #[test]
    fn test_substitution_log_truly_missing() {
        let template = Template::new("Hello {{NAME}}");
        let variables = HashMap::new();

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .expect("render_with_log should succeed even when variables are missing");

        assert_eq!(rendered.content, "Hello {{NAME}}");
        assert!(rendered.log.substituted.is_empty());
        assert_eq!(rendered.log.unsubstituted, vec!["NAME".to_string()]);
        assert!(!rendered.log.is_complete());
    }

    #[test]
    fn test_substitution_log_jsx_in_value() {
        let template = Template::new("Code: {{CODE}}");
        let variables = HashMap::from([("CODE", "style={{ zIndex: 0 }}".to_string())]);

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .unwrap();

        assert!(rendered.content.contains("{{ zIndex: 0 }}"));
        assert_eq!(
            rendered.log.substituted[0].source,
            crate::prompts::SubstitutionSource::Value
        );
        assert!(rendered.log.is_complete());
    }

    #[test]
    fn test_defaults_used_helper() {
        let template = Template::new("{{A}} {{B|default=\"x\"}} {{C|default=\"y\"}}");
        let variables = HashMap::from([("A", "a".to_string())]);

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .unwrap();

        let defaults = rendered.log.defaults_used();
        assert_eq!(defaults.len(), 2);
        assert!(defaults.contains(&"B"));
        assert!(defaults.contains(&"C"));
        assert!(!defaults.contains(&"A"));
    }

    #[test]
    fn test_substitution_log_mixed() {
        let template = Template::new("{{A}} {{B|default=\"b\"}} {{C}}");
        let variables = HashMap::from([("A", "a".to_string()), ("C", "c".to_string())]);
        // B will use default

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .unwrap();

        assert_eq!(rendered.content, "a b c");
        assert_eq!(rendered.log.substituted.len(), 3);
        assert!(rendered.log.is_complete());

        // Check specific sources
        let a_entry = rendered
            .log
            .substituted
            .iter()
            .find(|e| e.name == "A")
            .unwrap();
        assert_eq!(a_entry.source, crate::prompts::SubstitutionSource::Value);

        let b_entry = rendered
            .log
            .substituted
            .iter()
            .find(|e| e.name == "B")
            .unwrap();
        assert_eq!(b_entry.source, crate::prompts::SubstitutionSource::Default);

        let c_entry = rendered
            .log
            .substituted
            .iter()
            .find(|e| e.name == "C")
            .unwrap();
        assert_eq!(c_entry.source, crate::prompts::SubstitutionSource::Value);
    }
}
