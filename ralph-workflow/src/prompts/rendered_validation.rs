//! Rendered prompt validation.
//!
//! Validates that rendered prompts have no unresolved template placeholders.

use super::template_types::RenderedPromptError;

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
///
/// # Trade-offs
///
/// This validation uses a simple regex that may have false positives if the
/// rendered prompt legitimately contains literal `{{...}}` patterns, such as:
/// - Documentation that explains template syntax
/// - Code examples that demonstrate Handlebars/Jinja templates
///
/// This is an intentional trade-off: it's safer to reject rare legitimate content
/// than to allow prompts with unresolved template variables to reach agents.
/// If templates need to include literal `{{...}}` patterns as content, they
/// should be escaped appropriately during rendering (e.g., using `\{{` or `{{{{`).
pub fn validate_no_unresolved_placeholders(rendered: &str) -> Result<(), RenderedPromptError> {
    validate_no_unresolved_placeholders_with_ignored_content(rendered, &[])
}

/// Validate that a rendered prompt has no unresolved placeholders, ignoring known content.
///
/// This variant allows callers to provide trusted content (e.g., diff/plan text)
/// that may contain literal `{{...}}` patterns. Any placeholder that appears
/// inside one of the ignored content strings will be skipped.
pub fn validate_no_unresolved_placeholders_with_ignored_content(
    rendered: &str,
    ignored_content: &[&str],
) -> Result<(), RenderedPromptError> {
    struct UnresolvedPlaceholder {
        display: String,
        start: usize,
        end: usize,
    }

    // Use a regex to catch ANY remaining {{...}} patterns, including:
    // - Normal variables: {{VAR}}
    // - Variables with defaults: {{VAR|default="x"}}
    // - Variables with nested braces in defaults: {{VAR|default="{}"}}
    // - Triple braces: {{{VAR}}} (will match {{VAR}} inside)
    // - Malformed/unclosed patterns: {{VAR (detected separately below)
    //
    // The pattern uses non-greedy matching (.*?) to capture everything between
    // {{ and the first occurrence of }}. This correctly handles nested single
    // braces like `default="{}"` by continuing until the closing `}}`.
    //
    // This is more robust than extract_variables() which parses template syntax
    // and may miss malformed patterns that indicate rendering failures.
    let closed_re = regex::Regex::new(r"\{\{.*?\}\}").expect("regex should be valid");
    let mut unresolved: Vec<UnresolvedPlaceholder> = closed_re
        .find_iter(rendered)
        .map(|m| UnresolvedPlaceholder {
            display: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        })
        .collect();

    // Also check for unclosed {{ patterns that never close.
    // This catches malformed templates like "Hello {{VAR" where the closing }} is missing.
    // We look for {{ that is NOT followed by a matching }} on the same line.
    let unclosed_re = regex::Regex::new(r"\{\{[^}]*$").expect("regex should be valid");
    let mut offset = 0;
    for line in rendered.split_inclusive('\n') {
        let line_trimmed = line.strip_suffix('\n').unwrap_or(line);
        // Check if line has {{ without matching }}
        if let Some(m) = unclosed_re.find(line_trimmed) {
            let raw = m.as_str().to_string();
            unresolved.push(UnresolvedPlaceholder {
                display: format!("{} (unclosed)", raw),
                start: offset + m.start(),
                end: offset + m.end(),
            });
        }
        offset += line.len();
    }

    if !ignored_content.is_empty() {
        let mut ignored_ranges = Vec::new();
        for content in ignored_content {
            if content.is_empty() {
                continue;
            }
            let mut search_start = 0;
            while let Some(pos) = rendered[search_start..].find(content) {
                let range_start = search_start + pos;
                let range_end = range_start + content.len();
                ignored_ranges.push((range_start, range_end));
                search_start = range_end;
            }
        }
        unresolved.retain(|placeholder| {
            !ignored_ranges
                .iter()
                .any(|(start, end)| placeholder.start >= *start && placeholder.end <= *end)
        });
    }

    if unresolved.is_empty() {
        Ok(())
    } else {
        Err(RenderedPromptError {
            unresolved_placeholders: unresolved
                .into_iter()
                .map(|placeholder| placeholder.display)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
