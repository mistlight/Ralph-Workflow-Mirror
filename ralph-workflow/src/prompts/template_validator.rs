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

/// Error type for rendered prompt validation failures.
///
/// Returned when a rendered prompt still contains unresolved template
/// placeholders, indicating missing variables or template rendering failures.
#[derive(Debug, Clone)]
pub struct RenderedPromptError {
    /// Placeholder patterns that remain unresolved in the rendered output.
    pub unresolved_placeholders: Vec<String>,
}

impl std::fmt::Display for RenderedPromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Rendered prompt contains unresolved placeholders: {}",
            self.unresolved_placeholders.join(", ")
        )
    }
}

impl std::error::Error for RenderedPromptError {}

/// Validate that a rendered prompt has no unresolved placeholders.
///
/// This should be called AFTER template rendering to ensure no `{{...}}`
/// patterns remain in the output. Unresolved placeholders indicate either
/// missing template variables or template rendering failures.
///
/// Per the reducer fallback spec, Ralph must validate templates before
/// invoking an agent and emit `TEMPLATE_VARIABLES_INVALID` if validation fails.
///
/// # Arguments
///
/// * `rendered` - The rendered prompt string to validate
///
/// # Returns
///
/// * `Ok(())` if no unresolved placeholders are found
/// * `Err(RenderedPromptError)` with the list of unresolved placeholders
pub fn validate_no_unresolved_placeholders(rendered: &str) -> Result<(), RenderedPromptError> {
    // Use a simple regex to catch ANY remaining {{...}} patterns, including:
    // - Normal variables: {{VAR}}
    // - Variables with defaults: {{VAR|default="x"}}
    // - Triple braces: {{{VAR}}} (will match {{VAR}} inside)
    // - Malformed/unclosed patterns: {{VAR (detected separately below)
    //
    // This is more robust than extract_variables() which parses template syntax
    // and may miss malformed patterns that indicate rendering failures.
    let closed_re = regex::Regex::new(r"\{\{[^}]*\}\}").expect("regex should be valid");
    let mut unresolved: Vec<String> = closed_re
        .find_iter(rendered)
        .map(|m| m.as_str().to_string())
        .collect();

    // Also check for unclosed {{ patterns that never close.
    // This catches malformed templates like "Hello {{VAR" where the closing }} is missing.
    // We look for {{ that is NOT followed by a matching }} on the same line.
    let unclosed_re = regex::Regex::new(r"\{\{[^}]*$").expect("regex should be valid");
    for line in rendered.lines() {
        // Check if line has {{ without matching }}
        if let Some(m) = unclosed_re.find(line) {
            unresolved.push(format!("{} (unclosed)", m.as_str()));
        }
    }

    if unresolved.is_empty() {
        Ok(())
    } else {
        Err(RenderedPromptError {
            unresolved_placeholders: unresolved,
        })
    }
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
pub fn validate_syntax(content: &str) -> Vec<ValidationError> {
    let bytes = content.as_bytes();
    SyntaxValidator::new(content).validate(bytes)
}

/// Helper struct for template syntax validation.
struct SyntaxValidator<'a> {
    content: &'a str,
    errors: Vec<ValidationError>,
    line: usize,
    i: usize,
    conditional_stack: Vec<(usize, &'static str)>,
    loop_stack: Vec<(usize, &'static str)>,
}

impl<'a> SyntaxValidator<'a> {
    const fn new(content: &'a str) -> Self {
        Self {
            content,
            errors: Vec::new(),
            line: 0,
            i: 0,
            conditional_stack: Vec::new(),
            loop_stack: Vec::new(),
        }
    }

    fn validate(mut self, bytes: &[u8]) -> Vec<ValidationError> {
        while self.i < bytes.len() {
            self.track_newlines(bytes);
            if self.try_skip_comment(bytes) {
                continue;
            }
            if self.try_parse_conditional(bytes) {
                continue;
            }
            if self.try_parse_loop(bytes) {
                continue;
            }
            self.i += 1;
        }
        self.check_unclosed_blocks();
        self.errors
    }

    fn track_newlines(&mut self, bytes: &[u8]) {
        if bytes[self.i] == b'\n' {
            self.line += 1;
        }
    }

    fn try_skip_comment(&mut self, bytes: &[u8]) -> bool {
        if self.i + 1 < bytes.len() && bytes[self.i] == b'{' && bytes[self.i + 1] == b'#' {
            let comment_start = self.line;
            self.i += 2;
            while self.i + 1 < bytes.len() && !(bytes[self.i] == b'#' && bytes[self.i + 1] == b'}')
            {
                if bytes[self.i] == b'\n' {
                    self.line += 1;
                }
                self.i += 1;
            }
            if self.i + 1 >= bytes.len() {
                self.errors.push(ValidationError::UnclosedComment {
                    line: comment_start,
                });
            }
            if self.i + 1 < bytes.len() {
                self.i += 2;
            }
            true
        } else {
            false
        }
    }

    fn try_parse_conditional(&mut self, bytes: &[u8]) -> bool {
        // Check for {% if ... %}
        if self.i + 5 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'i'
            && bytes[self.i + 4] == b'f'
            && bytes[self.i + 5] == b' '
        {
            let if_start = self.i;
            self.i += 6;
            while self.i + 1 < bytes.len() && !(bytes[self.i] == b'%' && bytes[self.i + 1] == b'}')
            {
                self.i += 1;
            }
            if self.i + 1 >= bytes.len() {
                self.errors
                    .push(ValidationError::UnclosedConditional { line: self.line });
            } else {
                let condition = self.content[if_start + 6..self.i].trim();
                if condition.is_empty() || condition.contains('{') || condition.contains('}') {
                    self.errors.push(ValidationError::InvalidConditional {
                        line: self.line,
                        syntax: condition.to_string(),
                    });
                }
                self.conditional_stack.push((self.line, "if"));
                self.i += 2;
            }
            return true;
        }

        // Check for {% endif %}
        if self.i + 9 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'e'
            && bytes[self.i + 4] == b'n'
            && bytes[self.i + 5] == b'd'
            && bytes[self.i + 6] == b'i'
            && bytes[self.i + 7] == b'f'
            && bytes[self.i + 8] == b' '
            && bytes[self.i + 9] == b'%'
        {
            self.conditional_stack.pop();
            self.i += 11;
            return true;
        }

        false
    }

    fn try_parse_loop(&mut self, bytes: &[u8]) -> bool {
        // Check for {% for ... %}
        if self.i + 6 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'f'
            && bytes[self.i + 4] == b'o'
            && bytes[self.i + 5] == b'r'
            && bytes[self.i + 6] == b' '
        {
            let for_start = self.i;
            self.i += 7;
            while self.i + 1 < bytes.len() && !(bytes[self.i] == b'%' && bytes[self.i + 1] == b'}')
            {
                self.i += 1;
            }
            if self.i + 1 >= bytes.len() {
                self.errors
                    .push(ValidationError::UnclosedLoop { line: self.line });
            } else {
                let condition = self.content[for_start + 7..self.i].trim();
                if !condition.contains(" in ") || condition.split(" in ").count() != 2 {
                    self.errors.push(ValidationError::InvalidLoop {
                        line: self.line,
                        syntax: condition.to_string(),
                    });
                }
                self.loop_stack.push((self.line, "for"));
                self.i += 2;
            }
            return true;
        }

        // Check for {% endfor %}
        if self.i + 10 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'e'
            && bytes[self.i + 4] == b'n'
            && bytes[self.i + 5] == b'd'
            && bytes[self.i + 6] == b'f'
            && bytes[self.i + 7] == b'o'
            && bytes[self.i + 8] == b'r'
            && bytes[self.i + 9] == b' '
        {
            self.loop_stack.pop();
            self.i += 12;
            return true;
        }

        false
    }

    fn check_unclosed_blocks(&mut self) {
        if let Some((line, _)) = self.conditional_stack.first() {
            self.errors
                .push(ValidationError::UnclosedConditional { line: *line });
        }
        if let Some((line, _)) = self.loop_stack.first() {
            self.errors
                .push(ValidationError::UnclosedLoop { line: *line });
        }
    }
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
}
