//! Rendered prompt validation.
//!
//! Validates that rendered prompts have no unresolved template placeholders.

use super::template_types::RenderedPromptError;

/// Validate that a rendered prompt has no unresolved placeholders.
///
/// # Deprecation Notice
///
/// **This function is deprecated.** Use substitution log-based validation instead,
/// which tracks what was actually substituted during rendering rather than
/// scanning for `{{}}` patterns in output. Regex scanning causes false positives
/// when substituted values contain `{{}}` (e.g., JSX code, React components).
///
/// **Replacement approach:**
/// ```ignore
/// // Old (deprecated):
/// let rendered = template.render(&variables)?;
/// validate_no_unresolved_placeholders(&rendered)?;
///
/// // New (correct):
/// let rendered = template.render_with_log("template_name", &variables, &partials)?;
/// if !rendered.log.is_complete() {
///     return Err(...); // Use unsubstituted list from log
/// }
/// ```
///
/// See `SubstitutionLog::is_complete()` for the replacement approach.
///
/// # Why This is Deprecated
///
/// This validation uses regex scanning which has false positives when the
/// rendered prompt legitimately contains literal `{{...}}` patterns, such as:
/// - JSX/React code: `style={{ zIndex: 0 }}`
/// - Code examples that demonstrate templates
/// - Documentation that explains template syntax
///
/// The original comment said "this is an intentional trade-off" but it's actually
/// a bug - we're parsing DATA as CODE. The correct approach is to track substitutions
/// during rendering (like SQL prepared statements) rather than scanning output.
#[deprecated(
    since = "0.7.3",
    note = "Use SubstitutionLog::is_complete() instead. Regex scanning causes false positives."
)]
#[allow(deprecated)]
pub fn validate_no_unresolved_placeholders(rendered: &str) -> Result<(), RenderedPromptError> {
    validate_no_unresolved_placeholders_with_ignored_content(rendered, &[])
}

/// Validate that a rendered prompt has no unresolved placeholders, ignoring known content.
///
/// # Deprecation Notice
///
/// **This function is deprecated.** The `ignored_content` parameter was a workaround
/// for the fundamental flaw in regex-based validation. Use substitution log-based
/// validation instead via `SubstitutionLog::is_complete()`.
///
/// See deprecation notice on `validate_no_unresolved_placeholders` for details.
#[deprecated(
    since = "0.7.3",
    note = "Use SubstitutionLog::is_complete() instead. Regex scanning causes false positives."
)]
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
