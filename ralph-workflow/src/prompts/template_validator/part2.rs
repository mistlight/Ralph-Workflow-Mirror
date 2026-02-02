// ============================================================================
// Tests for template validation
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_variable() {
        let content = "Hello {{NAME}}";
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "NAME");
        assert!(!vars[0].has_default);
    }

    #[test]
    fn test_extract_variable_with_whitespace() {
        let content = "Value: {{ VALUE }}";
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "VALUE");
    }

    #[test]
    fn test_extract_variable_with_default() {
        let content = "Hello {{NAME|default=\"Guest\"}}";
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "NAME");
        assert!(vars[0].has_default);
        assert_eq!(vars[0].default_value, Some("Guest".to_string()));
    }

    #[test]
    fn test_extract_variable_with_default_single_quotes() {
        let content = "Hello {{NAME|default='Guest'}}";
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].default_value, Some("Guest".to_string()));
    }

    #[test]
    fn test_extract_partials() {
        let content = "{{> shared/_header}}\nContent";
        let partials = extract_partials(content);
        assert_eq!(partials.len(), 1);
        assert_eq!(partials[0], "shared/_header");
    }

    #[test]
    fn test_extract_multiple_partials() {
        let content = "{{> header}}\n{{> footer}}";
        let partials = extract_partials(content);
        assert_eq!(partials.len(), 2);
    }

    #[test]
    fn test_validate_syntax_valid() {
        let content = "Hello {{NAME}}";
        let errors = validate_syntax(content);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_syntax_unclosed_comment() {
        let content = "Hello {# unclosed comment\nworld";
        let errors = validate_syntax(content);
        assert!(!errors.is_empty());
        assert!(matches!(errors[0], ValidationError::UnclosedComment { .. }));
    }

    #[test]
    fn test_validate_conditional_valid() {
        let content = "{% if NAME %}Hello{% endif %}";
        let errors = validate_syntax(content);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_loop_valid() {
        let content = "{% for item in ITEMS %}{{item}}{% endfor %}";
        let errors = validate_syntax(content);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_loop_invalid_syntax() {
        let content = "{% for item ITEMS %}{{item}}{% endfor %}";
        let errors = validate_syntax(content);
        assert!(!errors.is_empty());
        assert!(matches!(errors[0], ValidationError::InvalidLoop { .. }));
    }

    #[test]
    fn test_extract_metadata() {
        let content = r"{# Template: test.txt #}
{# Version: 1.0 #}
{# PURPOSE: Test template #}
{# VARIABLES: {{NAME}}, {{AGE}} #}
Content here";

        let metadata = extract_metadata(content);
        assert_eq!(metadata.version, Some("1.0".to_string()));
        assert_eq!(metadata.purpose, Some("Test template".to_string()));
    }

    #[test]
    fn test_validate_template_complete() {
        let content = "Hello {{NAME|default=\"Guest\"}}";
        let partials = HashSet::new();
        let result = validate_template(content, &partials);

        assert!(result.is_valid);
        assert_eq!(result.variables.len(), 1);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_template_with_missing_partial() {
        let content = "{{> missing_partial}}";
        let partials = HashSet::new();
        let result = validate_template(content, &partials);

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_skip_variables_in_comments() {
        let content = "{# This is a comment with {{VARIABLE}} #}\nHello {{NAME}}";
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "NAME");
    }

    #[test]
    fn test_skip_partials_in_variable_extraction() {
        let content = "{{> partial}}\n{{NAME}}";
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "NAME");
    }

    #[test]
    fn test_extract_variables_from_conditional() {
        let content = "{% if NAME %}Hello {{NAME}}{% endif %}";
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 1); // Only NAME in output is extracted
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_pass() {
        let rendered = "Hello John, your order 12345 is ready.";
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_fail() {
        let rendered = "Hello {{NAME}}, your order {{ORDER_ID}} is ready.";
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.unresolved_placeholders.len(), 2);
        assert!(err
            .unresolved_placeholders
            .contains(&"{{NAME}}".to_string()));
        assert!(err
            .unresolved_placeholders
            .contains(&"{{ORDER_ID}}".to_string()));
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_empty() {
        let rendered = "";
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_with_default() {
        // Variables with defaults are still considered unresolved if present in output
        let rendered = "Hello {{NAME|default='Guest'}}";
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_ignored_content_does_not_mask_outside() {
        let rendered = "Intro {{MISSING}}\nDIFF:\n{{MISSING}}";
        let ignored = ["DIFF:\n{{MISSING}}"];
        let result = validate_no_unresolved_placeholders_with_ignored_content(rendered, &ignored);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.unresolved_placeholders.len(), 1);
        assert!(err
            .unresolved_placeholders
            .contains(&"{{MISSING}}".to_string()));
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_unclosed() {
        // Unclosed patterns like "{{VAR" should also be detected
        let rendered = "Hello {{NAME";
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.unresolved_placeholders.len(), 1);
        assert!(err.unresolved_placeholders[0].contains("{{NAME"));
        assert!(err.unresolved_placeholders[0].contains("unclosed"));
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_multiline_unclosed() {
        // Unclosed on one line, properly closed on next - both should be detected
        let rendered = "Line 1 {{UNCLOSED\nLine 2 {{CLOSED}}";
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should have both: the closed pattern and the unclosed pattern
        assert_eq!(err.unresolved_placeholders.len(), 2);
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_with_nested_braces() {
        // Complex pattern with nested braces in default value
        let rendered = r#"Value: {{VAR|default="{}"}}"#;
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should detect the full pattern including nested braces
        assert!(
            err.unresolved_placeholders
                .iter()
                .any(|p| p.contains("VAR")),
            "Should detect placeholder with nested braces, got: {:?}",
            err.unresolved_placeholders
        );
    }

    #[test]
    fn test_validate_no_unresolved_placeholders_triple_braces() {
        // Triple braces (raw output in some template engines)
        let rendered = "Value: {{{RAW}}}";
        let result = validate_no_unresolved_placeholders(rendered);
        assert!(result.is_err());
        // Should detect the inner {{RAW}} at minimum
    }
}
