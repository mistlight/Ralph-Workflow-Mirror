//! Template engine for rendering prompt templates.
//!
//! This module provides a simple template variable replacement system for
//! prompt templates using `{{VARIABLE}}` syntax for placeholders like `{{DIFF}}`,
//! `{{GUIDELINES}}`, etc.
//!
//! ## Syntax
//!
//! - **Variables**: `{{VARIABLE}}` or `{{ VARIABLE }}` - replaced with values
//! - **Comments**: `{# comment #}` - stripped from output, useful for documentation

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
/// with the provided values. Comments using `{# comment #}` syntax are stripped.
///
/// # Example
///
/// ```ignore
/// let template = Template::new("Review this diff:\n{# This is a doc comment #}\n```diff\n{{DIFF}}\n```");
/// let variables = HashMap::from([("DIFF", "+ new line")]);
/// let rendered = template.render(&variables)?;
/// // Comments are stripped from the output
/// ```
#[derive(Debug, Clone)]
pub struct Template {
    /// The template content with comments stripped.
    content: String,
    /// Full placeholder text found in the template (e.g., "{{DIFF}}", "{{ VALUE }}").
    placeholders: Vec<String>,
    /// Trimmed variable names for lookup (e.g., "DIFF", "VALUE").
    variables: Vec<String>,
}

impl Template {
    /// Create a template from a string.
    ///
    /// Comments (`{# ... #}`) are stripped during creation.
    pub fn new(content: &str) -> Self {
        // Strip comments first
        let content = Self::strip_comments(content);
        let (placeholders, variables) = Self::extract_variables(&content);
        Self {
            content,
            placeholders,
            variables,
        }
    }

    /// Strip `{# comment #}` style comments from the content.
    ///
    /// Comments can span multiple lines. Handles line-only comments that leave
    /// empty lines behind by collapsing them.
    fn strip_comments(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let bytes = content.as_bytes();

        let mut i = 0;
        while i < bytes.len() {
            // Check for {# comment start
            if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'#' {
                // Find the end of the comment (#})
                let comment_start = i;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'#' && bytes[i + 1] == b'}') {
                    i += 1;
                }
                if i + 1 < bytes.len() && bytes[i] == b'#' && bytes[i + 1] == b'}' {
                    i += 2;
                    // Skip trailing whitespace on the same line if comment was on its own line
                    // Check if we're at the end of a line (or there's only whitespace until newline)
                    let whitespace_start = i;
                    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                        i += 1;
                    }
                    // If we hit a newline after whitespace, skip it too (comment was full line)
                    if i < bytes.len() && bytes[i] == b'\n' {
                        // Check if the line before the comment was also empty
                        let was_line_start = result.is_empty() || result.ends_with('\n');
                        if was_line_start {
                            // Comment was on its own line - skip the newline
                            i += 1;
                        } else {
                            // Comment was at end of a content line - restore whitespace position
                            i = whitespace_start;
                        }
                    } else if i < bytes.len() {
                        // Not a newline - restore whitespace position
                        i = whitespace_start;
                    }
                    continue;
                }
                // Unclosed comment - treat as literal text
                result.push_str(&content[comment_start..i]);
            } else {
                result.push(bytes[i] as char);
                i += 1;
            }
        }

        result
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
}
