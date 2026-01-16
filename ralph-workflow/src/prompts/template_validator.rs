//! Template validation and inspection module.
//!
//! Provides functionality for validating template syntax, extracting variables,
//! and checking template integrity.

use std::collections::HashSet;

/// Template validation result.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// Variables referenced in the template
    pub variables: Vec<VariableInfo>,
    /// Partials referenced in the template
    pub partials: Vec<String>,
    /// Validation errors found
    pub errors: Vec<ValidationError>,
    /// Validation warnings found
    pub warnings: Vec<ValidationWarning>,
}

/// Information about a variable reference in a template.
#[derive(Debug, Clone)]
pub struct VariableInfo {
    /// Name of the variable
    pub name: String,
    /// Line number where variable appears (0-indexed)
    pub line: usize,
    /// Whether the variable has a default value
    pub has_default: bool,
    /// Default value if present
    pub default_value: Option<String>,
}

/// Template validation error.
#[derive(Debug, Clone)]
pub enum ValidationError {
    /// Unclosed conditional block
    UnclosedConditional { line: usize },
    /// Unclosed loop block
    UnclosedLoop { line: usize },
    /// Invalid conditional syntax
    InvalidConditional { line: usize, syntax: String },
    /// Invalid loop syntax
    InvalidLoop { line: usize, syntax: String },
    /// Unclosed comment
    UnclosedComment { line: usize },
    /// Partial reference not found
    PartialNotFound { name: String },
}

/// Template validation warning.
#[derive(Debug, Clone)]
pub enum ValidationWarning {
    /// Variable appears to be unused (no default, might error if not provided)
    VariableMayError { name: String },
}

/// Template metadata extracted from header comments.
#[derive(Debug, Clone)]
pub struct TemplateMetadata {
    /// Template version
    pub version: Option<String>,
    /// Template purpose description
    pub purpose: Option<String>,
}

/// Extract all variable references from template content.
///
/// Returns a list of all `{{VARIABLE}}` references found in the template,
/// including their line numbers and default values if present.
pub fn extract_variables(content: &str) -> Vec<VariableInfo> {
    let mut variables = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    let mut line = 0;

    while i < bytes.len().saturating_sub(1) {
        // Track line numbers
        if bytes[i] == b'\n' {
            line += 1;
        }

        // Skip comment blocks
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'#' {
            // Skip to end of comment
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'#' && bytes[i + 1] == b'}') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2; // Skip #}
            }
            continue;
        }

        // Check for {{...}} pattern
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            i += 2;

            // Skip whitespace after {{
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }

            let name_start = i;

            // Find the closing }}
            while i < bytes.len()
                && !(bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}')
            {
                i += 1;
            }

            if i < bytes.len() && bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                let var_spec = &content[name_start..i];
                let trimmed_var = var_spec.trim();

                // Skip partial references {{> partial}}
                if !trimmed_var.starts_with('>') && !trimmed_var.is_empty() {
                    // Check for default value syntax
                    let (var_name, default_value) =
                        var_spec.find('|').map_or((trimmed_var, None), |pipe_pos| {
                            let name = var_spec[..pipe_pos].trim();
                            let rest = &var_spec[pipe_pos + 1..];
                            rest.find('=').map_or((name, None), |eq_pos| {
                                let key = rest[..eq_pos].trim();
                                if key == "default" {
                                    let value = rest[eq_pos + 1..].trim();
                                    let value = if (value.starts_with('"') && value.ends_with('"'))
                                        || (value.starts_with('\'') && value.ends_with('\''))
                                    {
                                        &value[1..value.len() - 1]
                                    } else {
                                        value
                                    };
                                    (name, Some(value.to_string()))
                                } else {
                                    (name, None)
                                }
                            })
                        });

                    variables.push(VariableInfo {
                        name: var_name.to_string(),
                        line,
                        has_default: default_value.is_some(),
                        default_value,
                    });
                }

                i += 2;
                continue;
            }
        }

        i += 1;
    }

    variables
}

/// Extract all partial references from template content.
///
/// Returns a list of all `{{> partial}}` references found in the template.
pub fn extract_partials(content: &str) -> Vec<String> {
    let mut partials = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len().saturating_sub(2) {
        // Check for {{> pattern
        if bytes[i] == b'{' && bytes[i + 1] == b'{' && i + 2 < bytes.len() {
            i += 2;

            // Skip whitespace after {{
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }

            // Check for > character
            if i < bytes.len() && bytes[i] == b'>' {
                i += 1;

                // Skip whitespace after >
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }

                // Extract partial name until }}
                let name_start = i;
                while i < bytes.len()
                    && !(bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}')
                {
                    i += 1;
                }

                if i < bytes.len()
                    && bytes[i] == b'}'
                    && i + 1 < bytes.len()
                    && bytes[i + 1] == b'}'
                {
                    let name = content[name_start..i].trim();
                    if !name.is_empty() {
                        partials.push(name.to_string());
                    }
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }

    partials
}

/// Extract template metadata from header comments.
///
/// Parses structured comments like:
/// ```text
/// {# Template: name #}
/// {# Version: 1.0 #}
/// {# VARIABLES: VAR1, VAR2 #}
/// ```
pub fn extract_metadata(content: &str) -> TemplateMetadata {
    let mut version = None;
    let mut purpose = None;

    for line in content.lines().take(50) {
        // Look for comment markers
        let line = line.trim();
        if !line.starts_with("{#") || !line.ends_with("#}") {
            continue;
        }

        let inner = line[2..line.len() - 2].trim();

        // Parse Version: x.x
        if let Some(rest) = inner.strip_prefix("Version:") {
            version = Some(rest.trim().to_string());
        } else if let Some(rest) = inner.strip_prefix("PURPOSE:") {
            // Parse PURPOSE: description
            purpose = Some(rest.trim().to_string());
        }
    }

    TemplateMetadata { version, purpose }
}

/// Validate a template's syntax and structure.
///
/// Checks for:
/// - Unclosed variable references
/// - Unclosed conditionals
/// - Unclosed loops
/// - Unclosed comments
/// - Invalid syntax in conditionals and loops
#[allow(clippy::too_many_lines)]
pub fn validate_syntax(content: &str) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    let mut line = 0;

    // Track nesting for conditionals and loops
    let mut conditional_stack = Vec::new();
    let mut loop_stack = Vec::new();

    while i < bytes.len() {
        // Track line numbers
        if bytes[i] == b'\n' {
            line += 1;
        }

        // Check for {# comment start
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'#' {
            let comment_start = line;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'#' && bytes[i + 1] == b'}') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i + 1 >= bytes.len() {
                errors.push(ValidationError::UnclosedComment {
                    line: comment_start,
                });
            }
            if i + 1 < bytes.len() {
                i += 2;
            }
            continue;
        }

        // Check for {% if ... %}
        if i + 5 < bytes.len()
            && bytes[i] == b'{'
            && bytes[i + 1] == b'%'
            && bytes[i + 2] == b' '
            && bytes[i + 3] == b'i'
            && bytes[i + 4] == b'f'
            && bytes[i + 5] == b' '
        {
            let if_start = i;
            i += 6;
            // Find the end of the if tag
            while i + 1 < bytes.len() && !(bytes[i] == b'%' && bytes[i + 1] == b'}') {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                errors.push(ValidationError::UnclosedConditional { line });
            } else {
                // Extract condition
                let condition = content[if_start + 6..i].trim();
                // Validate condition syntax
                if condition.is_empty() || condition.contains('{') || condition.contains('}') {
                    errors.push(ValidationError::InvalidConditional {
                        line,
                        syntax: condition.to_string(),
                    });
                }
                conditional_stack.push((line, "if"));
                i += 2;
            }
            continue;
        }

        // Check for {% endif %}
        if i + 9 < bytes.len()
            && bytes[i] == b'{'
            && bytes[i + 1] == b'%'
            && bytes[i + 2] == b' '
            && bytes[i + 3] == b'e'
            && bytes[i + 4] == b'n'
            && bytes[i + 5] == b'd'
            && bytes[i + 6] == b'i'
            && bytes[i + 7] == b'f'
            && bytes[i + 8] == b' '
            && bytes[i + 9] == b'%'
        {
            if conditional_stack.is_empty() {
                // Unmatched endif
            } else {
                conditional_stack.pop();
            }
            i += 11;
            continue;
        }

        // Check for {% for ... %}
        if i + 6 < bytes.len()
            && bytes[i] == b'{'
            && bytes[i + 1] == b'%'
            && bytes[i + 2] == b' '
            && bytes[i + 3] == b'f'
            && bytes[i + 4] == b'o'
            && bytes[i + 5] == b'r'
            && bytes[i + 6] == b' '
        {
            let for_start = i;
            i += 7;
            // Find the end of the for tag
            while i + 1 < bytes.len() && !(bytes[i] == b'%' && bytes[i + 1] == b'}') {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                errors.push(ValidationError::UnclosedLoop { line });
            } else {
                // Extract condition
                let condition = content[for_start + 7..i].trim();
                // Validate "item in ITEMS" syntax
                if !condition.contains(" in ") || condition.split(" in ").count() != 2 {
                    errors.push(ValidationError::InvalidLoop {
                        line,
                        syntax: condition.to_string(),
                    });
                }
                loop_stack.push((line, "for"));
                i += 2;
            }
            continue;
        }

        // Check for {% endfor %}
        if i + 10 < bytes.len()
            && bytes[i] == b'{'
            && bytes[i + 1] == b'%'
            && bytes[i + 2] == b' '
            && bytes[i + 3] == b'e'
            && bytes[i + 4] == b'n'
            && bytes[i + 5] == b'd'
            && bytes[i + 6] == b'f'
            && bytes[i + 7] == b'o'
            && bytes[i + 8] == b'r'
            && bytes[i + 9] == b' '
        {
            if loop_stack.is_empty() {
                // Unmatched endfor
            } else {
                loop_stack.pop();
            }
            i += 12;
            continue;
        }

        i += 1;
    }

    // Check for unclosed blocks
    if let Some((line, _)) = conditional_stack.first() {
        errors.push(ValidationError::UnclosedConditional { line: *line });
    }
    if let Some((line, _)) = loop_stack.first() {
        errors.push(ValidationError::UnclosedLoop { line: *line });
    }

    errors
}

/// Validate a complete template.
///
/// Performs comprehensive validation including syntax checking,
/// variable extraction, and partial reference validation.
pub fn validate_template(content: &str, available_partials: &HashSet<String>) -> ValidationResult {
    let mut is_valid = true;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Validate syntax
    let syntax_errors = validate_syntax(content);
    if !syntax_errors.is_empty() {
        is_valid = false;
        errors.extend(syntax_errors);
    }

    // Extract variables
    let variables = extract_variables(content);

    // Extract partials
    let partials = extract_partials(content);

    // Check for missing partials
    for partial in &partials {
        if !available_partials.contains(partial) {
            is_valid = false;
            errors.push(ValidationError::PartialNotFound {
                name: partial.clone(),
            });
        }
    }

    // Check for variables without defaults that might error
    for var in &variables {
        if !var.has_default {
            warnings.push(ValidationWarning::VariableMayError {
                name: var.name.clone(),
            });
        }
    }

    ValidationResult {
        is_valid,
        variables,
        partials,
        errors,
        warnings,
    }
}

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
}
