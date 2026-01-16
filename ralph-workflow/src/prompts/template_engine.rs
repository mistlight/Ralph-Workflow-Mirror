//! Template engine for rendering prompt templates.
//!
//! This module provides a simple template variable replacement system for
//! prompt templates using `{{VARIABLE}}` syntax for placeholders like `{{DIFF}}`,
//! `{{GUIDELINES}}`, etc.

use std::collections::HashMap;

/// Error type for template operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// Required variable not provided.
    MissingVariable(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVariable(name) => write!(f, "Missing required variable: {{{{ {name} }}}}"),
        }
    }
}

impl std::error::Error for TemplateError {}

/// A simple template engine for prompt templates.
///
/// Templates use `{{VARIABLE}}` syntax for placeholders. Variables are replaced
/// with the provided values.
///
/// # Example
///
/// ```ignore
/// let template = Template::from_str("Review this diff:\n```diff\n{{DIFF}}\n```")?;
/// let variables = HashMap::from([("DIFF", "+ new line")]);
/// let rendered = template.render(&variables)?;
/// ```
#[derive(Debug, Clone)]
pub struct Template {
    /// The raw template content.
    content: String,
    /// Full placeholder text found in the template (e.g., "{{DIFF}}", "{{ VALUE }}").
    placeholders: Vec<String>,
    /// Trimmed variable names for lookup (e.g., "DIFF", "VALUE").
    variables: Vec<String>,
}

impl Template {
    /// Create a template from a string.
    pub fn from_str(content: String) -> Self {
        let (placeholders, variables) = Self::extract_variables(&content);
        Self {
            content,
            placeholders,
            variables,
        }
    }

    /// Extract all variable names and placeholder text from the template content.
    /// Returns `(placeholders, trimmed_variable_names)`.
    fn extract_variables(content: &str) -> (Vec<String>, Vec<String>) {
        let mut placeholders = Vec::new();
        let mut variables = Vec::new();
        let bytes = content.as_bytes();

        let mut i = 0;
        while i < bytes.len().saturating_sub(1) {
            if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                let start = i;
                i += 2;
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
                    let end = i + 2;
                    let placeholder = &content[start..end];
                    let name = &content[name_start..i];

                    if !name.trim().is_empty() {
                        placeholders.push(placeholder.to_string());
                        variables.push(name.trim().to_string());
                    }
                    i = end;
                }
            }
            i += 1;
        }

        (placeholders, variables)
    }

    /// Render the template with the provided variables.
    pub fn render(&self, variables: &HashMap<&str, String>) -> Result<String, TemplateError> {
        let mut result = self.content.clone();

        for (placeholder, var) in self.placeholders.iter().zip(&self.variables) {
            if let Some(value) = variables.get(var.as_str()) {
                result = result.replace(placeholder, value);
            } else {
                return Err(TemplateError::MissingVariable(var.clone()));
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template() {
        let template = Template::from_str("Hello {{NAME}}, your score is {{SCORE}}.".to_string());
        let variables = HashMap::from([("NAME", "Alice".to_string()), ("SCORE", "42".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Alice, your score is 42.");
    }

    #[test]
    fn test_missing_variable() {
        let template = Template::from_str("Hello {{NAME}}.".to_string());
        let variables = HashMap::new();
        let result = template.render(&variables);
        assert_eq!(
            result,
            Err(TemplateError::MissingVariable("NAME".to_string()))
        );
    }

    #[test]
    fn test_no_variables() {
        let template = Template::from_str("Just plain text.".to_string());
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "Just plain text.");
    }

    #[test]
    fn test_multiline_template() {
        let template = Template::from_str("Review this:\n{{DIFF}}\nEnd of review.".to_string());
        let variables = HashMap::from([("DIFF", "+ new line".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Review this:\n+ new line\nEnd of review.");
    }

    #[test]
    fn test_whitespace_in_variables() {
        let template = Template::from_str("Value: {{ VALUE }}.".to_string());
        let variables = HashMap::from([("VALUE", "42".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Value: 42.");
    }

    #[test]
    fn test_unclosed_opening_braces() {
        // Unclosed {{ should be ignored (no placeholder extracted)
        let template = Template::from_str("Hello {{NAME and some text".to_string());
        let rendered = template.render(&HashMap::new()).unwrap();
        // The unclosed braces are treated as literal text
        assert_eq!(rendered, "Hello {{NAME and some text");
    }

    #[test]
    fn test_empty_variable_name() {
        // Empty variable name {{}} should be ignored (no placeholder extracted)
        let template = Template::from_str("Value: {{}}.".to_string());
        let rendered = template.render(&HashMap::new()).unwrap();
        // Empty placeholder is treated as literal text
        assert_eq!(rendered, "Value: {{}}.");
    }

    #[test]
    fn test_whitespace_only_variable_name() {
        // Whitespace-only variable name {{   }} should be ignored
        let template = Template::from_str("Value: {{   }}.".to_string());
        let rendered = template.render(&HashMap::new()).unwrap();
        // Whitespace-only placeholder is treated as literal text
        assert_eq!(rendered, "Value: {{   }}.");
    }

    #[test]
    fn test_multiple_unclosed_braces() {
        // Multiple unclosed {{ should all be ignored
        let template = Template::from_str("{{A text {{B text".to_string());
        let rendered = template.render(&HashMap::new()).unwrap();
        assert_eq!(rendered, "{{A text {{B text");
    }

    #[test]
    fn test_partial_closing_brace() {
        // Single closing brace without the second should not close the placeholder
        let template = Template::from_str("Hello {{NAME}} and {{VAR}} text".to_string());
        let variables = HashMap::from([("NAME", "Alice".to_string()), ("VAR", "Bob".to_string())]);
        let rendered = template.render(&variables).unwrap();
        assert_eq!(rendered, "Hello Alice and Bob text");
    }
}
