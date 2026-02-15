//! Commit Message Extraction Functions
//!
//! This module provides utilities for extracting commit messages from AI agent output
//! using XML format with XSD validation.

use super::cleaning::{final_escape_sequence_cleanup, unescape_json_strings_aggressive};
use super::xml_extraction::extract_xml_commit;
use super::xsd_validation::validate_xml_against_xsd;
use crate::common::truncate_text;

/// Result of commit message extraction.
///
/// This struct wraps a successfully extracted commit message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitExtractionResult(String);

impl CommitExtractionResult {
    /// Create a new extraction result with the given message.
    pub fn new(message: String) -> Self {
        Self(message)
    }

    /// Convert into the inner message string with final escape sequence cleanup.
    ///
    /// This applies the final rendering step to ensure no escape sequences leak through
    /// to the actual commit message.
    pub fn into_message(self) -> String {
        render_final_commit_message(&self.0)
    }
}

/// Try to extract a commit message from the XML format, with a trace string for debugging.
///
/// This uses flexible XML extraction (direct tags, fenced blocks, escaped JSON strings, embedded
/// text) and validates the resulting XML against the commit XSD.
///
/// Returns: (message, skip_reason, trace_detail)
/// - message: Some(msg) if commit message found
/// - skip_reason: Some(reason) if AI determined no commit needed
/// - trace_detail: Diagnostic string explaining extraction result
pub fn try_extract_xml_commit_with_trace(
    content: &str,
) -> (Option<String>, Option<String>, String) {
    // Try flexible XML extraction that handles various AI embedding patterns.
    // If extraction fails, use the raw content directly - XSD validation will
    // provide a clear error message explaining what's wrong (e.g., missing
    // <ralph-commit> root element) that can be sent back to the AI for retry.
    let (xml_block, extraction_pattern) = match extract_xml_commit(content) {
        Some(xml) => {
            // Detect which extraction pattern was used for logging
            let pattern = if content.trim().starts_with("<ralph-commit>") {
                "direct XML"
            } else if content.contains("```xml") || content.contains("```\n<ralph-commit>") {
                "markdown code fence"
            } else if content.contains("{\"result\":") || content.contains("\"result\":") {
                "JSON string"
            } else {
                "embedded search"
            };
            (xml, pattern)
        }
        None => {
            // No XML tags found - use raw content and let XSD validation
            // produce an informative error for the AI to retry
            (content.to_string(), "raw content (no XML tags found)")
        }
    };

    // Run XSD validation - this will catch both malformed XML and missing elements
    let xsd_result = validate_xml_against_xsd(&xml_block);

    match xsd_result {
        Ok(elements) => {
            // Check for skip first
            if let Some(reason) = elements.skip_reason {
                return (
                    None,
                    Some(reason.clone()),
                    format!(
                        "Found <ralph-skip> via {}, reason: '{}'",
                        extraction_pattern, reason
                    ),
                );
            }

            // Format the commit message using parsed elements
            let body = elements.format_body();
            let message = if body.is_empty() {
                elements.subject.clone()
            } else {
                format!("{}\n\n{}", elements.subject, body)
            };

            // Determine body presence for logging
            let has_body = message.lines().count() > 1;

            // Use character-based truncation for UTF-8 safety
            let message_preview = {
                let escaped = message.replace('\n', "\\n");
                truncate_text(&escaped, 83) // ~80 chars + "..."
            };

            (
                Some(message.clone()),
                None,
                format!(
                    "Found <ralph-commit> via {}, XSD validation passed, body={}, message: '{}'",
                    extraction_pattern,
                    if has_body { "present" } else { "absent" },
                    message_preview
                ),
            )
        }
        Err(e) => {
            // XSD validation failed - return error with details for AI retry
            let error_msg = e.format_for_ai_retry();
            (None, None, format!("XSD validation failed: {}", error_msg))
        }
    }
}

/// Check if a string is a valid conventional commit subject line.
pub fn is_conventional_commit_subject(subject: &str) -> bool {
    let valid_types = [
        "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
    ];

    // Find the colon
    let Some(colon_pos) = subject.find(':') else {
        return false;
    };

    let prefix = &subject[..colon_pos];

    // Extract type (before optional scope and !)
    let type_end = prefix
        .find('(')
        .unwrap_or_else(|| prefix.find('!').unwrap_or(prefix.len()));
    let commit_type = &prefix[..type_end];

    valid_types.contains(&commit_type)
}

// =========================================================================
// Final Commit Message Rendering
// =========================================================================

/// Render the final commit message with all cleanup applied.
///
/// This is the final step before returning a commit message for use in git commit.
/// It applies:
/// 1. Escape sequence cleanup (aggressive unescaping)
/// 2. Final whitespace cleanup
///
/// # Arguments
///
/// * `message` - The commit message to render
///
/// # Returns
///
/// The fully rendered commit message with all escape sequences properly handled.
pub fn render_final_commit_message(message: &str) -> String {
    let mut result = message.to_string();

    // Step 1: Apply final escape sequence cleanup
    // This handles any escape sequences that leaked through the pipeline
    result = final_escape_sequence_cleanup(&result);

    // Step 2: Try aggressive unescaping if there are still escape sequences
    if result.contains("\\n") || result.contains("\\t") || result.contains("\\r") {
        result = unescape_json_strings_aggressive(&result);
    }

    // Step 3: Final whitespace cleanup
    result = result
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Tests for CommitExtractionResult
    // =========================================================================

    #[test]
    fn test_commit_extraction_result_into_message() {
        let result = CommitExtractionResult::new("feat: add feature".to_string());
        assert_eq!(result.into_message(), "feat: add feature");
    }

    // =========================================================================
    // Tests for render_final_commit_message
    // =========================================================================

    #[test]
    fn test_render_final_commit_message_with_literal_escapes() {
        // Test that render_final_commit_message cleans up escape sequences
        // Note: whitespace cleanup removes blank lines
        let input = "feat: add feature\n\\n\\nBody with literal escapes";
        let result = render_final_commit_message(input);
        assert_eq!(result, "feat: add feature\nBody with literal escapes");
    }

    #[test]
    fn test_render_final_commit_message_already_clean() {
        // Test that already-clean messages pass through (whitespace cleanup applied)
        let input = "feat: add feature\n\nBody text here";
        let result = render_final_commit_message(input);
        assert_eq!(result, "feat: add feature\nBody text here");
    }

    #[test]
    fn test_render_final_commit_message_with_tabs() {
        // Test that tab escapes are properly handled
        let input = "feat: add feature\\n\\t- item 1\\n\\t- item 2";
        let result = render_final_commit_message(input);
        // Tabs are stripped by whitespace cleanup (trim() removes leading whitespace)
        assert_eq!(result, "feat: add feature\n- item 1\n- item 2");
    }

    #[test]
    fn test_render_final_commit_message_with_carriage_returns() {
        // Test that carriage return escapes are properly handled
        let input = "feat: add feature\\r\\nBody text";
        let result = render_final_commit_message(input);
        // Carriage returns are converted, but whitespace cleanup removes extra blank lines
        assert_eq!(result, "feat: add feature\nBody text");
    }

    #[test]
    fn test_render_final_commit_message_whitespace_cleanup() {
        // Test that trailing empty lines are removed
        let input = "feat: add feature\n\nBody text\n\n\n  \n  ";
        let result = render_final_commit_message(input);
        assert_eq!(result, "feat: add feature\nBody text");
    }

    #[test]
    fn test_render_final_commit_message_mixed_escape_sequences() {
        // Test handling of mixed escape sequences
        let input = "feat: add feature\\n\\nDetails:\\r\\n\\t- item 1\\n\\t- item 2";
        let result = render_final_commit_message(input);
        // Carriage returns normalized to newlines, tabs stripped by trim, blank lines removed
        assert_eq!(result, "feat: add feature\nDetails:\n- item 1\n- item 2");
    }

    // =========================================================================
    // Tests for is_conventional_commit_subject
    // =========================================================================

    #[test]
    fn test_conventional_commit_subject_valid() {
        assert!(is_conventional_commit_subject("feat: add feature"));
        assert!(is_conventional_commit_subject("fix: resolve bug"));
        assert!(is_conventional_commit_subject("docs: update readme"));
        assert!(is_conventional_commit_subject(
            "refactor(core): simplify logic"
        ));
        assert!(is_conventional_commit_subject("feat!: breaking change"));
        assert!(is_conventional_commit_subject("fix(api)!: breaking fix"));
    }

    #[test]
    fn test_conventional_commit_subject_invalid() {
        assert!(!is_conventional_commit_subject("invalid: not a type"));
        assert!(!is_conventional_commit_subject("no colon here"));
        assert!(!is_conventional_commit_subject(""));
        assert!(!is_conventional_commit_subject("Feature: capitalize"));
    }

    // =========================================================================
    // Tests for XML extraction (try_extract_xml_commit_with_trace)
    // =========================================================================

    #[test]
    fn test_xml_extract_basic_subject_only() {
        // Test basic XML extraction with subject only
        let content = r"<ralph-commit>
<ralph-subject>feat: add new feature</ralph-subject>
</ralph-commit>";
        let (result, skip, reason) = try_extract_xml_commit_with_trace(content);
        assert!(
            result.is_some(),
            "Should extract from basic XML. Reason: {}",
            reason
        );
        assert!(skip.is_none());
        assert_eq!(result.unwrap(), "feat: add new feature");
    }

    #[test]
    fn test_xml_extract_with_body() {
        // Test XML extraction with subject and body
        let content = r"<ralph-commit>
<ralph-subject>feat(auth): add OAuth2 login flow</ralph-subject>
<ralph-body>Implement Google and GitHub OAuth providers.
Add session management for OAuth tokens.</ralph-body>
</ralph-commit>";
        let (result, skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_some(), "Should extract from XML with body");
        assert!(skip.is_none());
        let msg = result.unwrap();
        assert!(msg.starts_with("feat(auth): add OAuth2 login flow"));
        assert!(msg.contains("Implement Google and GitHub OAuth providers"));
        assert!(msg.contains("Add session management"));
    }

    #[test]
    fn test_xml_extract_with_empty_body() {
        // Test XML extraction with empty body tags
        let content = r"<ralph-commit>
<ralph-subject>fix: resolve bug</ralph-subject>
<ralph-body></ralph-body>
</ralph-commit>";
        let (result, skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_some(), "Should extract even with empty body");
        assert!(skip.is_none());
        // Empty body should be treated as no body
        assert_eq!(result.unwrap(), "fix: resolve bug");
    }

    #[test]
    fn test_xml_extract_ignores_preamble() {
        // Test that content before <ralph-commit> is ignored
        let content = r"Here is the commit message based on my analysis:

Looking at the diff, I can see...

<ralph-commit>
<ralph-subject>refactor: simplify logic</ralph-subject>
</ralph-commit>

That's all!";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_some(), "Should ignore preamble and extract XML");
        assert_eq!(result.unwrap(), "refactor: simplify logic");
    }

    #[test]
    fn test_xml_extract_fails_missing_tags() {
        // Test that extraction fails when tags are missing
        let content = "Just some text without XML tags";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_none(), "Should fail when XML tags are missing");
    }

    #[test]
    fn test_xml_extract_fails_invalid_commit_type() {
        // Test that extraction fails for invalid conventional commit types
        let content = r"<ralph-commit>
<ralph-subject>invalid: not a real type</ralph-subject>
</ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_none(), "Should reject invalid commit type");
    }

    #[test]
    fn test_xml_extract_fails_missing_subject() {
        // Test that extraction fails when subject is missing
        let content = r"<ralph-commit>
<ralph-body>Just a body, no subject</ralph-body>
</ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_none(), "Should fail when subject is missing");
    }

    #[test]
    fn test_xml_extract_fails_empty_subject() {
        // Test that extraction fails when subject is empty
        let content = r"<ralph-commit>
<ralph-subject></ralph-subject>
</ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_none(), "Should fail when subject is empty");
    }

    #[test]
    fn test_xml_extract_handles_whitespace_in_subject() {
        // Test that whitespace around subject is trimmed
        let content = r"<ralph-commit>
<ralph-subject>   docs: update readme   </ralph-subject>
</ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_some(), "Should handle whitespace in subject");
        assert_eq!(result.unwrap(), "docs: update readme");
    }

    #[test]
    fn test_xml_extract_with_breaking_change() {
        // Test XML extraction with breaking change indicator
        let content = r"<ralph-commit>
<ralph-subject>feat!: drop Python 3.7 support</ralph-subject>
<ralph-body>BREAKING CHANGE: Minimum Python version is now 3.8.</ralph-body>
</ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_some(), "Should handle breaking change indicator");
        let msg = result.unwrap();
        assert!(msg.starts_with("feat!:"));
        assert!(msg.contains("BREAKING CHANGE"));
    }

    #[test]
    fn test_xml_extract_with_scope() {
        // Test XML extraction with scope
        let content = r"<ralph-commit>
<ralph-subject>test(parser): add coverage for edge cases</ralph-subject>
</ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_some(), "Should handle scope in subject");
        assert_eq!(result.unwrap(), "test(parser): add coverage for edge cases");
    }

    #[test]
    fn test_xml_extract_body_preserves_newlines() {
        // Test that newlines in body are preserved
        let content = r"<ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
<ralph-body>Line 1
Line 2
Line 3</ralph-body>
</ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_some(), "Should preserve newlines in body");
        let msg = result.unwrap();
        assert!(msg.contains("Line 1\nLine 2\nLine 3"));
    }

    #[test]
    fn test_xml_extract_fails_malformed_tags() {
        // Test that extraction fails for malformed tags (end before start)
        let content = r"</ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
<ralph-commit>";
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(result.is_none(), "Should fail for malformed tags");
    }

    #[test]
    fn test_xml_extract_handles_markdown_code_fence() {
        // Test that XML inside markdown code fence is extracted
        let content = r"```xml
<ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
</ralph-commit>
```";
        // The XML extractor looks for tags directly, so this should still work
        // since the tags are present in the content
        let (result, _skip, _) = try_extract_xml_commit_with_trace(content);
        assert!(
            result.is_some(),
            "Should extract from XML even inside code fence"
        );
    }

    #[test]
    fn test_xml_extract_with_thinking_preamble() {
        // Test that thinking preamble is ignored
        let log_content = r"[Claude] Thinking: Looking at this diff, I need to analyze...

<ralph-commit>
<ralph-subject>feat(pipeline): add recovery mechanism</ralph-subject>
<ralph-body>When commit validation fails, attempt to salvage valid message.</ralph-body>
</ralph-commit>";

        let (result, _skip, _reason) = try_extract_xml_commit_with_trace(log_content);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.starts_with("feat(pipeline):"));
    }

    // Test that validates XSD functionality using the integrated validation
    #[test]
    fn test_xsd_validation_integrated_in_extraction() {
        // The XSD validation is called within try_extract_xml_commit_with_trace
        // This test ensures that path is exercised
        let xml = r#"Some text before
<ralph-commit>
<ralph-subject>fix: resolve bug</ralph-subject>
</ralph-commit>
Some text after"#;
        let (msg, _skip, trace) = try_extract_xml_commit_with_trace(xml);
        assert!(msg.is_some(), "Should extract valid message");
        // The trace should contain XSD validation result
        assert!(trace.contains("XSD"), "Trace should mention XSD validation");
    }
}
