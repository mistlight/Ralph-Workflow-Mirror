//! Template engine for rendering prompt templates.
//!
//! This module provides a template variable replacement system for prompt templates
//! with support for variables, partials, comments, conditionals, loops, and defaults.
//!
//! ## Syntax
//!
//! - **Variables**: `{{VARIABLE}}` or `{{ VARIABLE }}` - replaced with values
//! - **Default values**: `{{VARIABLE|default="value"}}` - uses value if VARIABLE is missing
//! - **Conditionals**: `{% if VARIABLE %}...{% endif %}` - include content if VARIABLE is truthy
//! - **Negation**: `{% if !VARIABLE %}...{% endif %}` - include content if VARIABLE is falsy
//! - **Loops**: `{% for item in ITEMS %}...{% endfor %}` - iterate over comma-separated values
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
}

impl Template {
    /// Create a template from a string.
    ///
    /// Comments (`{# ... #}`) are stripped during creation.
    /// All features are enabled by default: variables, conditionals, loops, and defaults.
    pub fn new(content: &str) -> Self {
        // Strip comments first
        let content = Self::strip_comments(content);
        Self { content }
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

    /// Process conditionals in the content based on variable values.
    ///
    /// Supports:
    /// - `{% if VARIABLE %}...{% endif %}` - show content if VARIABLE is truthy
    /// - `{% if !VARIABLE %}...{% endif %}` - show content if VARIABLE is falsy
    ///
    /// A variable is considered "truthy" if it exists and is non-empty.
    fn process_conditionals(content: &str, variables: &HashMap<&str, String>) -> String {
        let mut result = content.to_string();

        // Find all {% if ... %} blocks
        while let Some(start) = result.find("{% if ") {
            // Find the end of the if condition
            let if_end_start = start + 6; // "{% if " is 6 chars
            let if_end = match result[if_end_start..].find("%}") {
                Some(pos) => if_end_start + pos + 2,
                None => {
                    // Unclosed if tag - skip it
                    result = result[start + 1..].to_string();
                    continue;
                }
            };

            // Extract the condition
            let condition = result[if_end_start..if_end - 2].trim().to_string();

            // Find the matching {% endif %}
            let endif_start = match result[if_end..].find("{% endif %}") {
                Some(pos) => if_end + pos,
                None => {
                    // Unclosed if block - skip it
                    result = result[start + 1..].to_string();
                    continue;
                }
            };

            let endif_end = endif_start + 11; // "{% endif %}" is 11 chars

            // Extract the content inside the if block
            let block_content = result[if_end..endif_start].to_string();

            // Evaluate the condition
            let should_show = Self::evaluate_condition(&condition, variables);

            // Replace the entire if block with the content or empty string
            let replacement = if should_show {
                block_content
            } else {
                String::new()
            };
            result.replace_range(start..endif_end, &replacement);
        }

        result
    }

    /// Evaluate a conditional expression.
    ///
    /// Supports:
    /// - `VARIABLE` - true if variable exists and is non-empty
    /// - `!VARIABLE` - true if variable doesn't exist or is empty
    fn evaluate_condition(condition: &str, variables: &HashMap<&str, String>) -> bool {
        let condition = condition.trim();

        // Check for negation
        if let Some(rest) = condition.strip_prefix('!') {
            let var_name = rest.trim();
            let value = variables.get(var_name);
            return value.map_or(true, |v| v.is_empty());
        }

        // Normal condition - check if variable exists and is non-empty
        let value = variables.get(condition);
        value.is_some_and(|v| !v.is_empty())
    }

    /// Process loops in the content based on variable values.
    ///
    /// Supports:
    /// - `{% for item in ITEMS %}...{% endfor %}` - iterate over comma-separated ITEMS
    ///
    /// The loop variable is available for use in the block content.
    fn process_loops(content: &str, variables: &HashMap<&str, String>) -> String {
        let mut result = content.to_string();

        // Find all {% for ... %} blocks
        while let Some(start) = result.find("{% for ") {
            // Find the end of the for condition
            let for_end_start = start + 7; // "{% for " is 7 chars
            let for_end = match result[for_end_start..].find("%}") {
                Some(pos) => for_end_start + pos + 2,
                None => {
                    // Unclosed for tag - skip it
                    result = result[start + 1..].to_string();
                    continue;
                }
            };

            // Parse "item in ITEMS"
            let condition = result[for_end_start..for_end - 2].trim();
            let parts: Vec<&str> = condition.split(" in ").collect();
            if parts.len() != 2 {
                // Invalid for syntax - skip it
                result = result[start + 1..].to_string();
                continue;
            }

            let loop_var = parts[0].trim().to_string();
            let list_var = parts[1].trim();

            // Find the matching {% endfor %}
            let endfor_start = match result[for_end..].find("{% endfor %}") {
                Some(pos) => for_end + pos,
                None => {
                    // Unclosed for block - skip it
                    result = result[start + 1..].to_string();
                    continue;
                }
            };

            let endfor_end = endfor_start + 12; // "{% endfor %}" is 12 chars

            // Extract the template inside the for block
            let block_template = result[for_end..endfor_start].to_string();

            // Get the list of values
            let items: Vec<String> = variables.get(list_var).map_or(Vec::new(), |v| {
                if v.is_empty() {
                    Vec::new()
                } else {
                    // Split by comma and trim each item
                    v.split(',').map(|s| s.trim().to_string()).collect()
                }
            });

            // Build the loop output
            let mut loop_output = String::new();
            for item in items {
                // Create a temporary variable map with the loop variable
                let mut loop_vars: HashMap<&str, String> = variables.clone();
                loop_vars.insert(&loop_var, item);

                // Process conditionals first with loop variables
                let processed = Self::process_conditionals(&block_template, &loop_vars);

                // Then substitute variables
                let (processed, _missing) = Self::substitute_variables(&processed, &loop_vars);
                loop_output.push_str(&processed);
            }

            // Replace the entire for block with the loop output
            result.replace_range(start..endfor_end, &loop_output);
        }

        result
    }

    /// Substitute variables in content (simple version without partials or conditionals).
    /// Returns (result, missing_vars) where missing_vars is a list of variable names
    /// that were referenced but not found (and had no default).
    fn substitute_variables(
        content: &str,
        variables: &HashMap<&str, String>,
    ) -> (String, Vec<String>) {
        let mut result = content.to_string();
        let mut missing_vars = Vec::new();

        // Find all {{...}} patterns
        let mut replacements = Vec::new();
        let mut i = 0;
        let bytes = content.as_bytes();
        while i < bytes.len().saturating_sub(1) {
            if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                let start = i;
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

                if i < bytes.len()
                    && bytes[i] == b'}'
                    && i + 1 < bytes.len()
                    && bytes[i + 1] == b'}'
                {
                    let end = i + 2;
                    let var_spec = &content[name_start..i];

                    // Check for partial reference {{> partial}} - skip it
                    if var_spec.trim().starts_with('>') {
                        i = end;
                        continue;
                    }

                    // Skip if variable name is empty or whitespace only
                    let trimmed_var = var_spec.trim();
                    if trimmed_var.is_empty() {
                        i = end;
                        continue;
                    }

                    // Check for default value syntax: {{VAR|default="value"}}
                    let (var_name, default_value) = if let Some(pipe_pos) = var_spec.find('|') {
                        let name = var_spec[..pipe_pos].trim();
                        let rest = &var_spec[pipe_pos + 1..];
                        // Parse default="value"
                        if let Some(eq_pos) = rest.find('=') {
                            let key = rest[..eq_pos].trim();
                            if key == "default" {
                                let value = rest[eq_pos + 1..].trim();
                                // Remove quotes if present (both single and double)
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
                        } else {
                            (trimmed_var, None)
                        }
                    } else {
                        (trimmed_var, None)
                    };

                    // Look up the variable
                    let (replacement, should_replace) = if let Some(value) = variables.get(var_name)
                    {
                        if !value.is_empty() {
                            (value.clone(), true)
                        } else if let Some(default) = &default_value {
                            (default.clone(), true)
                        } else {
                            // Variable exists but is empty, and no default - keep placeholder
                            (String::new(), false)
                        }
                    } else if let Some(default) = &default_value {
                        (default.clone(), true)
                    } else {
                        // Variable not found and no default - track as missing
                        missing_vars.push(var_name.to_string());
                        (String::new(), false)
                    };

                    if should_replace {
                        replacements.push((start, end, replacement));
                    }
                    i = end;
                    continue;
                }
            }
            i += 1;
        }

        // Apply replacements in reverse order to maintain correct positions
        for (start, end, replacement) in replacements.into_iter().rev() {
            result.replace_range(start..end, &replacement);
        }

        (result, missing_vars)
    }

    /// Render the template with the provided variables.
    pub fn render(&self, variables: &HashMap<&str, String>) -> Result<String, TemplateError> {
        // Process loops first (they may generate new variable references)
        let mut result = Self::process_loops(&self.content, variables);

        // Process conditionals
        result = Self::process_conditionals(&result, variables);

        // Substitute variables (with default values)
        let (result_after_sub, missing_vars) = Self::substitute_variables(&result, variables);

        // Check for missing variables
        if let Some(first_missing) = missing_vars.first() {
            return Err(TemplateError::MissingVariable(first_missing.clone()));
        }

        Ok(result_after_sub)
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

        // Now substitute variables in the result (using the new method that handles defaults)
        let (result_after_sub, missing_vars) = Self::substitute_variables(&result, variables);

        // Check for missing variables
        if let Some(first_missing) = missing_vars.first() {
            return Err(TemplateError::MissingVariable(first_missing.clone()));
        }

        Ok(result_after_sub)
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
        let variables = HashMap::from([("NAME", "".to_string())]);
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
        let variables = HashMap::from([("NAME", "".to_string())]);
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
        let variables = HashMap::from([("ITEMS", "".to_string())]);
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
}
