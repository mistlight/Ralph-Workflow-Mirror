//! Template engine for rendering prompt templates.
//!
//! This module provides a template variable replacement system for prompt templates
//! with support for variables, partials, and comments.
//!
//! ## Syntax
//!
//! - **Variables**: `{{VARIABLE}}` or `{{ VARIABLE }}` - replaced with values
//! - **Partials**: `{{> partial_name}}` or `{{> partial/path}}` - includes another template
//! - **Comments**: `{# comment #}` - stripped from output, useful for documentation
//!
//! ## Partials System
//!
//! Partials allow sharing common template sections across multiple templates.
//! When a partial is referenced, it's looked up from the provided partials map
//! and recursively rendered with the same variables.
//!
//! Example partial include:
//! ```text
//! {{> shared/_critical_header}}
//! ```
//!
//! The partials system:
//! - Detects and prevents circular references
//! - Provides clear error messages for missing partials
//! - Supports hierarchical naming (dot notation or path-style)

use std::collections::HashMap;

/// Error type for template operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// Required variable not provided.
    MissingVariable(String),
    /// Referenced partial not found in partials map.
    PartialNotFound(String),
    /// Circular reference detected in partial includes.
    CircularReference(Vec<String>),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVariable(name) => write!(f, "Missing required variable: {{{{ {name} }}}}"),
            Self::PartialNotFound(name) => {
                write!(f, "Partial not found: '{{> {name}}}'")
            }
            Self::CircularReference(chain) => {
                write!(f, "Circular reference detected in partials: ")?;
                let mut sep = "";
                for partial in chain {
                    write!(f, "{sep}{{{{> {partial}}}}}")?;
                    sep = " -> ";
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for TemplateError {}

/// A simple template engine for prompt templates.
///
/// Templates use `{{VARIABLE}}` syntax for placeholders and `{{> partial}}` for
/// including shared templates. Variables are replaced with the provided values.
/// Comments using `{# comment #}` syntax are stripped.
///
/// # Example
///
/// ```ignore
/// let partials = HashMap::from([("header", "Common Header\n")]);
/// let template = Template::new("{{> header}}\nReview this diff:\n{{DIFF}}");
/// let variables = HashMap::from([("DIFF", "+ new line")]);
/// let rendered = template.render_with_partials(&variables, &partials)?;
/// ```
#[derive(Debug, Clone)]
pub struct Template {
    /// The template content with comments and partials processed.
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
    /// Note: Partial syntax {{> partial}} is excluded from variable extraction.
    fn extract_variables(content: &str) -> (Vec<String>, Vec<String>) {
        let mut placeholders = Vec::new();
        let mut variables = Vec::new();
        let bytes = content.as_bytes();

        let mut i = 0;
        while i < bytes.len().saturating_sub(1) {
            if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                let start = i;
                i += 2;

                // Skip whitespace after {{
                let whitespace_start = i;
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }

                // Check if this is a partial reference {{> partial}}
                if i < bytes.len() && bytes[i] == b'>' {
                    // Skip past the partial reference - don't extract it as a variable
                    i = whitespace_start; // Reset to skip the entire {{> ... }} pattern
                    while i < bytes.len()
                        && !(bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}')
                    {
                        i += 1;
                    }
                    if i < bytes.len() && bytes[i] == b'}' {
                        i += 2;
                    }
                    continue;
                }

                // Reset i to after {{ for normal variable extraction
                i = whitespace_start;
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

    /// Render the template with variables and partials support.
    ///
    /// Partials are processed recursively, with the same variables passed to each partial.
    /// Circular references are detected and reported with a clear error.
    pub fn render_with_partials(
        &self,
        variables: &HashMap<&str, String>,
        partials: &HashMap<String, String>,
    ) -> Result<String, TemplateError> {
        self.render_with_partials_recursive(variables, partials, &mut Vec::new())
    }

    /// Internal recursive rendering with circular reference detection.
    /// `visited` is a Vec that tracks the order of partials visited for proper error reporting.
    fn render_with_partials_recursive(
        &self,
        variables: &HashMap<&str, String>,
        partials: &HashMap<String, String>,
        visited: &mut Vec<String>,
    ) -> Result<String, TemplateError> {
        // First, extract and resolve all partials in this template
        let mut result = self.content.clone();

        // Find all {{> partial}} references
        let partial_refs = Self::extract_partials(&result);

        // Process partials in reverse order to maintain correct positions when replacing
        for (full_match, partial_name) in partial_refs.into_iter().rev() {
            // Check for circular reference
            if visited.contains(&partial_name) {
                let mut chain = visited.clone();
                chain.push(partial_name.clone());
                return Err(TemplateError::CircularReference(chain));
            }

            // Look up the partial content
            let partial_content = partials
                .get(&partial_name)
                .ok_or_else(|| TemplateError::PartialNotFound(partial_name.clone()))?;

            // Create a template from the partial and render it recursively
            let partial_template = Self::new(partial_content);
            visited.push(partial_name.clone());
            let rendered_partial =
                partial_template.render_with_partials_recursive(variables, partials, visited)?;
            visited.pop();

            // Replace the partial reference with rendered content
            result = result.replace(&full_match, &rendered_partial);
        }

        // Now substitute variables in the result
        for (placeholder, var) in self.placeholders.iter().zip(&self.variables) {
            if let Some(value) = variables.get(var.as_str()) {
                result = result.replace(placeholder, value);
            } else {
                return Err(TemplateError::MissingVariable(var.clone()));
            }
        }

        Ok(result)
    }

    /// Extract all partial references from template content.
    ///
    /// Returns Vec of (`full_match`, `partial_name`) tuples in order of appearance.
    fn extract_partials(content: &str) -> Vec<(String, String)> {
        let mut partials = Vec::new();
        let bytes = content.as_bytes();

        let mut i = 0;
        while i < bytes.len().saturating_sub(2) {
            // Check for {{> pattern
            if bytes[i] == b'{' && bytes[i + 1] == b'{' && i + 2 < bytes.len() {
                let start = i;
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
                        let end = i + 2;
                        let full_match = &content[start..end];
                        let name = &content[name_start..i];

                        let partial_name = name.trim().to_string();
                        if !partial_name.is_empty() {
                            partials.push((full_match.to_string(), partial_name));
                        }
                        i = end;
                        continue;
                    }
                }
            }
            i += 1;
        }

        partials
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
    fn test_partial_extraction() {
        let content = "Start\n{{> partial1}}\nMiddle\n{{>partial_2}}\nEnd";
        let partials = Template::extract_partials(content);
        assert_eq!(partials.len(), 2);
        assert_eq!(partials[0].0, "{{> partial1}}");
        assert_eq!(partials[0].1, "partial1");
        assert_eq!(partials[1].0, "{{>partial_2}}");
        assert_eq!(partials[1].1, "partial_2");
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
}
