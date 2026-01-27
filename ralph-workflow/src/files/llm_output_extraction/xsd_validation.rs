//! XSD validation for commit message XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format.
//!
//! Uses quick_xml for robust XML parsing with proper whitespace handling.

use crate::files::llm_output_extraction::commit::is_conventional_commit_subject;
use crate::files::llm_output_extraction::xml_helpers::{
    create_reader, duplicate_element_error, format_content_preview, malformed_xml_error,
    missing_required_error, read_text_until_end, skip_to_end, text_outside_tags_error,
    unexpected_element_error,
};
use quick_xml::events::Event;

/// Example of a valid commit message XML for error messages.
const EXAMPLE_COMMIT_XML: &str = r#"<ralph-commit>
<ralph-subject>feat(api): add user authentication</ralph-subject>
<ralph-body>Implements JWT-based authentication for the API.</ralph-body>
</ralph-commit>"#;

/// Validate XML content against the XSD schema.
///
/// This function validates that the XML content conforms to the expected
/// commit message format defined in commit_message.xsd:
///
/// ```xml
/// <ralph-commit>
///   <ralph-subject>type(scope): description</ralph-subject>
///   <ralph-body>Optional body text</ralph-body>
///   <ralph-body-summary>Optional summary</ralph-body-summary>
///   <ralph-body-details>Optional details</ralph-body-details>
///   <ralph-body-footer>Optional footer</ralph-body-footer>
/// </ralph-commit>
/// ```
///
/// # Arguments
///
/// * `xml_content` - The XML content to validate
///
/// # Returns
///
/// * `Ok(CommitMessageElements)` if the XML is valid and contains all required elements
/// * `Err(XsdValidationError)` if the XML is invalid or doesn't conform to the schema
///
/// # Examples
///
/// ```
/// use ralph_workflow::files::llm_output_extraction::xsd_validation::validate_xml_against_xsd;
///
/// let xml = r#"<ralph-commit>
/// <ralph-subject>feat: add new feature</ralph-subject>
/// </ralph-commit>"#;
/// let result = validate_xml_against_xsd(xml);
/// assert!(result.is_ok());
/// ```
pub(crate) fn validate_xml_against_xsd(
    xml_content: &str,
) -> Result<CommitMessageElements, XsdValidationError> {
    let content = xml_content.trim();
    let mut reader = create_reader(content);
    let mut buf = Vec::new();

    // Find the root element
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"ralph-commit" => break,
            Ok(Event::Start(e)) => {
                let name_bytes = e.name();
                let tag_name = String::from_utf8_lossy(name_bytes.as_ref());
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MissingRequiredElement,
                    element_path: "ralph-commit".to_string(),
                    expected: "<ralph-commit> as root element".to_string(),
                    found: format!("<{}> (wrong root element)", tag_name),
                    suggestion: "Use <ralph-commit> as the root element.".to_string(),
                    example: Some(EXAMPLE_COMMIT_XML.into()),
                });
            }
            Ok(Event::Text(_)) => {
                // Text before root element - continue to find root or reach EOF
                // EOF will give a more informative "missing root element" error
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MissingRequiredElement,
                    element_path: "ralph-commit".to_string(),
                    expected: "<ralph-commit> as root element".to_string(),
                    found: format_content_preview(content),
                    suggestion:
                        "Wrap your commit message in <ralph-commit>...</ralph-commit> tags."
                            .to_string(),
                    example: Some(EXAMPLE_COMMIT_XML.into()),
                });
            }
            Ok(_) => {} // Skip XML declaration, comments, etc.
            Err(e) => return Err(malformed_xml_error(e)),
        }
        buf.clear();
    }

    // Parse child elements
    let mut subject: Option<String> = None;
    let mut body: Option<String> = None;
    let mut body_summary: Option<String> = None;
    let mut body_details: Option<String> = None;
    let mut body_footer: Option<String> = None;

    const VALID_TAGS: [&str; 5] = [
        "ralph-subject",
        "ralph-body",
        "ralph-body-summary",
        "ralph-body-details",
        "ralph-body-footer",
    ];

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                match e.name().as_ref() {
                    b"ralph-subject" => {
                        if subject.is_some() {
                            return Err(duplicate_element_error("ralph-subject", "ralph-commit"));
                        }
                        subject = Some(read_text_until_end(&mut reader, b"ralph-subject")?);
                    }
                    b"ralph-body" => {
                        // Check for mixed body types
                        if body_summary.is_some() || body_details.is_some() || body_footer.is_some()
                        {
                            return Err(XsdValidationError {
                                error_type: XsdErrorType::UnexpectedElement,
                                element_path: "ralph-commit/ralph-body".to_string(),
                                expected:
                                    "either <ralph-body> OR detailed tags, not both".to_string(),
                                found: "mixed simple and detailed body elements".to_string(),
                                suggestion: "Use <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format.".to_string(),
                                example: Some(EXAMPLE_COMMIT_XML.into()),
                            });
                        }
                        if body.is_some() {
                            return Err(duplicate_element_error("ralph-body", "ralph-commit"));
                        }
                        body = Some(read_text_until_end(&mut reader, b"ralph-body")?);
                    }
                    b"ralph-body-summary" => {
                        if body.is_some() {
                            return Err(XsdValidationError {
                                error_type: XsdErrorType::UnexpectedElement,
                                element_path: "ralph-commit/ralph-body-summary".to_string(),
                                expected:
                                    "either <ralph-body> OR detailed tags, not both".to_string(),
                                found: "mixed simple and detailed body elements".to_string(),
                                suggestion: "Use <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format.".to_string(),
                                example: Some(EXAMPLE_COMMIT_XML.into()),
                            });
                        }
                        if body_summary.is_some() {
                            return Err(duplicate_element_error(
                                "ralph-body-summary",
                                "ralph-commit",
                            ));
                        }
                        body_summary =
                            Some(read_text_until_end(&mut reader, b"ralph-body-summary")?);
                    }
                    b"ralph-body-details" => {
                        if body.is_some() {
                            return Err(XsdValidationError {
                                error_type: XsdErrorType::UnexpectedElement,
                                element_path: "ralph-commit/ralph-body-details".to_string(),
                                expected:
                                    "either <ralph-body> OR detailed tags, not both".to_string(),
                                found: "mixed simple and detailed body elements".to_string(),
                                suggestion: "Use <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format.".to_string(),
                                example: Some(EXAMPLE_COMMIT_XML.into()),
                            });
                        }
                        if body_details.is_some() {
                            return Err(duplicate_element_error(
                                "ralph-body-details",
                                "ralph-commit",
                            ));
                        }
                        body_details =
                            Some(read_text_until_end(&mut reader, b"ralph-body-details")?);
                    }
                    b"ralph-body-footer" => {
                        if body.is_some() {
                            return Err(XsdValidationError {
                                error_type: XsdErrorType::UnexpectedElement,
                                element_path: "ralph-commit/ralph-body-footer".to_string(),
                                expected:
                                    "either <ralph-body> OR detailed tags, not both".to_string(),
                                found: "mixed simple and detailed body elements".to_string(),
                                suggestion: "Use <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format.".to_string(),
                                example: Some(EXAMPLE_COMMIT_XML.into()),
                            });
                        }
                        if body_footer.is_some() {
                            return Err(duplicate_element_error(
                                "ralph-body-footer",
                                "ralph-commit",
                            ));
                        }
                        body_footer = Some(read_text_until_end(&mut reader, b"ralph-body-footer")?);
                    }
                    other => {
                        // Skip unknown element but report error
                        let _ = skip_to_end(&mut reader, other);
                        return Err(unexpected_element_error(other, &VALID_TAGS, "ralph-commit"));
                    }
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Err(text_outside_tags_error(trimmed, "ralph-commit"));
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-commit" => break,
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-commit".to_string(),
                    expected: "closing </ralph-commit> tag".to_string(),
                    found: "end of content without closing tag".to_string(),
                    suggestion: "Add </ralph-commit> at the end of your commit message."
                        .to_string(),
                    example: Some(EXAMPLE_COMMIT_XML.into()),
                });
            }
            Ok(_) => {} // Skip comments, etc.
            Err(e) => return Err(malformed_xml_error(e)),
        }
    }

    // Validate required element: subject
    let subject = subject.ok_or_else(|| {
        missing_required_error("ralph-subject", "ralph-commit", Some(EXAMPLE_COMMIT_XML))
    })?;

    // Validate subject content
    let subject = subject.trim();
    if subject.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-subject".to_string(),
            expected: "non-empty subject line".to_string(),
            found: "empty subject".to_string(),
            suggestion:
                "The <ralph-subject> must contain a non-empty commit subject like 'feat: add feature'."
                    .to_string(),
            example: Some(EXAMPLE_COMMIT_XML.into()),
        });
    }

    // Validate conventional commit format
    if !is_conventional_commit_subject(subject) {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-subject".to_string(),
            expected: "conventional commit format (type: description or type(scope): description)"
                .to_string(),
            found: subject.to_string(),
            suggestion:
                "Use conventional commit format: type(scope): description. Valid types: feat, fix, docs, style, refactor, perf, test, build, ci, chore."
                    .to_string(),
            example: Some(EXAMPLE_COMMIT_XML.into()),
        });
    }

    Ok(CommitMessageElements {
        subject: subject.to_string(),
        body: body.filter(|s| !s.is_empty()),
        body_summary: body_summary.filter(|s| !s.is_empty()),
        body_details: body_details.filter(|s| !s.is_empty()),
        body_footer: body_footer.filter(|s| !s.is_empty()),
    })
}

/// Parsed commit message elements from valid XML.
///
/// This struct contains all the elements that were successfully
/// extracted and validated from the XML content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommitMessageElements {
    /// The commit subject line (required)
    /// Format: type(scope): description
    pub subject: String,
    /// Optional simple body content (mutually exclusive with detailed elements)
    pub body: Option<String>,
    /// Optional body summary (for detailed format)
    pub body_summary: Option<String>,
    /// Optional body details (for detailed format)
    pub body_details: Option<String>,
    /// Optional body footer (for detailed format)
    pub body_footer: Option<String>,
}

impl CommitMessageElements {
    /// Format all body elements into a single body string.
    ///
    /// Combines the simple body or detailed elements into a formatted
    /// commit message body string suitable for git commit.
    pub(crate) fn format_body(&self) -> String {
        // If simple body exists, use it directly
        if let Some(ref body) = self.body {
            return body.clone();
        }

        // Otherwise, combine detailed elements
        let mut parts = Vec::new();

        if let Some(ref summary) = self.body_summary {
            parts.push(summary.trim());
        }

        if let Some(ref details) = self.body_details {
            parts.push(details.trim());
        }

        if let Some(ref footer) = self.body_footer {
            parts.push(footer.trim());
        }

        if parts.is_empty() {
            String::new()
        } else {
            parts.join("\n\n")
        }
    }
}

/// Detailed XSD validation error for reporting to AI agent.
///
/// This error type provides comprehensive information about what went wrong
/// during validation, making it suitable for generating retry prompts that
/// guide the AI agent toward producing valid output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XsdValidationError {
    /// The type of validation error that occurred
    pub(crate) error_type: XsdErrorType,
    /// The path to the element that failed validation
    pub element_path: String,
    /// What was expected at this location
    pub expected: String,
    /// What was actually found
    pub found: String,
    /// Suggestion for fixing the error
    pub suggestion: String,
    /// Optional concrete example of valid XML (boxed to reduce struct size)
    pub example: Option<Box<str>>,
}

impl XsdValidationError {
    /// Format this error for display in logs or retry prompts.
    pub fn format_for_display(&self) -> String {
        let example_section = self
            .example
            .as_ref()
            .map(|ex| format!("\n  Example:\n{}", ex))
            .unwrap_or_default();

        format!(
            "XSD Validation Error [{}]: {}\n  Element: {}\n  Expected: {}\n  Found: {}\n  Suggestion: {}{}",
            self.error_type,
            self.error_type.description(),
            self.element_path,
            self.expected,
            self.found,
            self.suggestion,
            example_section
        )
    }

    /// Format this error as a concise message for AI retry prompt.
    ///
    /// This provides an actionable, human-readable error message that guides
    /// the AI agent toward producing valid XML output.
    pub fn format_for_ai_retry(&self) -> String {
        let example_section = self
            .example
            .as_ref()
            .map(|ex| format!("\n\nExample of correct format:\n{}", ex))
            .unwrap_or_default();

        match self.error_type {
            XsdErrorType::MissingRequiredElement => {
                format!(
                    "MISSING REQUIRED ELEMENT: '{}' is required but was not found.\n\n\
                     What was expected: {}\n\
                     What was found: {}\n\n\
                     How to fix: {}{}",
                    self.element_path, self.expected, self.found, self.suggestion, example_section
                )
            }
            XsdErrorType::UnexpectedElement => {
                format!(
                    "UNEXPECTED ELEMENT: Found '{}' which is not allowed.\n\n\
                     What was expected: {}\n\
                     What was found: {}\n\n\
                     How to fix: {}{}",
                    self.element_path, self.expected, self.found, self.suggestion, example_section
                )
            }
            XsdErrorType::InvalidContent => {
                format!(
                    "INVALID CONTENT: The content of '{}' does not meet requirements.\n\n\
                     What was expected: {}\n\
                     What was found: {}\n\n\
                     How to fix: {}{}",
                    self.element_path, self.expected, self.found, self.suggestion, example_section
                )
            }
            XsdErrorType::MalformedXml => {
                format!(
                    "MALFORMED XML: The XML structure is invalid.\n\n\
                     What was expected: {}\n\
                     What was found: {}\n\n\
                     How to fix: {}{}",
                    self.expected, self.found, self.suggestion, example_section
                )
            }
        }
    }
}

impl std::fmt::Display for XsdValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_for_display())
    }
}

impl std::error::Error for XsdValidationError {}

/// Type of XSD validation error.
///
/// Each variant represents a different category of validation failure,
/// allowing for targeted error messages and retry strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XsdErrorType {
    /// A required element is missing from the XML
    MissingRequiredElement,
    /// An unexpected element was found
    UnexpectedElement,
    /// Element content is invalid
    InvalidContent,
    /// The XML is malformed
    MalformedXml,
}

impl XsdErrorType {
    /// Get a human-readable description of this error type.
    pub const fn description(self) -> &'static str {
        match self {
            Self::MissingRequiredElement => "Missing required element",
            Self::UnexpectedElement => "Unexpected element",
            Self::InvalidContent => "Invalid content",
            Self::MalformedXml => "Malformed XML",
        }
    }
}

impl std::fmt::Display for XsdErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Tests for format_for_ai_retry()
    // ============================================================================

    #[test]
    fn test_format_for_ai_retry_missing_required_element() {
        let error = XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-subject".to_string(),
            expected: "<ralph-subject> element (required)".to_string(),
            found: "no <ralph-subject> found".to_string(),
            suggestion: "Add <ralph-subject>type(scope): description</ralph-subject>".to_string(),
            example: None,
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("MISSING REQUIRED ELEMENT"));
        assert!(formatted.contains("'ralph-subject' is required"));
        assert!(formatted.contains("Add <ralph-subject>"));
    }

    #[test]
    fn test_format_for_ai_retry_with_example() {
        let error = XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-subject".to_string(),
            expected: "<ralph-subject> element (required)".to_string(),
            found: "no <ralph-subject> found".to_string(),
            suggestion: "Add the required element".to_string(),
            example: Some(
                "<ralph-commit><ralph-subject>feat: example</ralph-subject></ralph-commit>".into(),
            ),
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("Example of correct format:"));
        assert!(formatted.contains("feat: example"));
    }

    #[test]
    fn test_format_for_ai_retry_unexpected_element() {
        let error = XsdValidationError {
            error_type: XsdErrorType::UnexpectedElement,
            element_path: "<unknown-tag>".to_string(),
            expected: "only valid commit message tags".to_string(),
            found: "unexpected tag: <unknown-tag>".to_string(),
            suggestion: "Remove the <unknown-tag> tag".to_string(),
            example: None,
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("UNEXPECTED ELEMENT"));
        assert!(formatted.contains("<unknown-tag>"));
        assert!(formatted.contains("not allowed"));
    }

    #[test]
    fn test_format_for_ai_retry_invalid_content() {
        let error = XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-subject".to_string(),
            expected: "conventional commit format".to_string(),
            found: "bad subject".to_string(),
            suggestion: "Use conventional commit format".to_string(),
            example: None,
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("INVALID CONTENT"));
        assert!(formatted.contains("ralph-subject"));
        assert!(formatted.contains("conventional commit format"));
    }

    #[test]
    fn test_format_for_ai_retry_malformed_xml() {
        let error = XsdValidationError {
            error_type: XsdErrorType::MalformedXml,
            element_path: "xml".to_string(),
            expected: "valid XML declaration ending with ?>".to_string(),
            found: "unclosed XML declaration".to_string(),
            suggestion: "Ensure XML declaration is properly closed".to_string(),
            example: None,
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("MALFORMED XML"));
        assert!(formatted.contains("XML structure is invalid"));
        assert!(formatted.contains("properly closed"));
    }

    // ============================================================================
    // Tests for validate_xml_against_xsd()
    // ============================================================================

    #[test]
    fn test_validate_valid_minimal_xml() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add new feature</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.subject, "feat: add new feature");
        assert!(elements.body.is_none());
        assert!(elements.body_summary.is_none());
    }

    #[test]
    fn test_validate_valid_xml_with_body() {
        let xml = r#"<ralph-commit>
<ralph-subject>fix(api): resolve null pointer</ralph-subject>
<ralph-body>This fixes the null pointer issue in the API handler.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.subject, "fix(api): resolve null pointer");
        assert_eq!(
            elements.body,
            Some("This fixes the null pointer issue in the API handler.".to_string())
        );
    }

    #[test]
    fn test_validate_valid_xml_with_detailed_body() {
        let xml = r#"<ralph-commit>
<ralph-subject>docs: update API documentation</ralph-subject>
<ralph-body-summary>Updated the API documentation to reflect recent changes.</ralph-body-summary>
<ralph-body-details>- Added new endpoints
- Updated request/response examples
- Fixed typos in authentication section</ralph-body-details>
<ralph-body-footer>Closes #123</ralph-body-footer>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.subject, "docs: update API documentation");
        assert!(elements.body.is_none());
        assert_eq!(
            elements.body_summary,
            Some("Updated the API documentation to reflect recent changes.".to_string())
        );
        assert!(elements.body_details.is_some());
        assert_eq!(elements.body_footer, Some("Closes #123".to_string()));
    }

    #[test]
    fn test_validate_with_xml_declaration() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ralph-commit>
<ralph-subject>test: add unit tests</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_missing_root_element() {
        let xml = r#"Some random text without proper XML tags"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(
            error.error_type,
            XsdErrorType::MissingRequiredElement
        ));
        assert_eq!(error.element_path, "ralph-commit");
    }

    #[test]
    fn test_validate_missing_closing_tag() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error.error_type, XsdErrorType::MalformedXml));
    }

    #[test]
    fn test_validate_missing_subject() {
        let xml = r#"<ralph-commit>
<ralph-body>Some body text</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(
            error.error_type,
            XsdErrorType::MissingRequiredElement
        ));
        assert!(error.element_path.contains("ralph-subject"));
    }

    #[test]
    fn test_validate_empty_subject() {
        let xml = r#"<ralph-commit>
<ralph-subject>   </ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error.error_type, XsdErrorType::InvalidContent));
        assert_eq!(error.element_path, "ralph-subject");
    }

    #[test]
    fn test_validate_invalid_conventional_commit_format() {
        let xml = r#"<ralph-commit>
<ralph-subject>This is not a conventional commit</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error.error_type, XsdErrorType::InvalidContent));
        assert!(error.suggestion.contains("conventional commit format"));
    }

    #[test]
    fn test_validate_duplicate_subject() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: first subject</ralph-subject>
<ralph-subject>feat: second subject</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error.error_type, XsdErrorType::UnexpectedElement));
        assert!(error.found.contains("duplicate"));
    }

    #[test]
    fn test_validate_mixed_simple_and_detailed_body() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
<ralph-body>Simple body</ralph-body>
<ralph-body-summary>Detailed summary</ralph-body-summary>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error.error_type, XsdErrorType::UnexpectedElement));
        assert!(error.suggestion.contains("<ralph-body>"));
    }

    #[test]
    fn test_validate_whitespace_handling() {
        let xml = r#"
  <ralph-commit>

    <ralph-subject>feat: add feature</ralph-subject>

    <ralph-body>
      Body with whitespace
    </ralph-body>

  </ralph-commit>
"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.subject, "feat: add feature");
        assert!(elements
            .body
            .as_ref()
            .unwrap()
            .contains("Body with whitespace"));
    }

    #[test]
    fn test_validate_with_escaped_newlines_in_content() {
        // This tests that quick_xml properly handles whitespace between elements
        // which was the original issue with literal \n characters
        let xml = "<ralph-commit>\n<ralph-subject>feat: test</ralph-subject>\n</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
    }

    // ============================================================================
    // Tests for CommitMessageElements::format_body()
    // ============================================================================

    #[test]
    fn test_format_body_with_simple_body() {
        let elements = CommitMessageElements {
            subject: "feat: test".to_string(),
            body: Some("Simple body text".to_string()),
            body_summary: None,
            body_details: None,
            body_footer: None,
        };

        assert_eq!(elements.format_body(), "Simple body text");
    }

    #[test]
    fn test_format_body_with_detailed_elements() {
        let elements = CommitMessageElements {
            subject: "feat: test".to_string(),
            body: None,
            body_summary: Some("Summary line".to_string()),
            body_details: Some("Detailed explanation".to_string()),
            body_footer: Some("Footer text".to_string()),
        };

        let formatted = elements.format_body();
        assert!(formatted.contains("Summary line"));
        assert!(formatted.contains("Detailed explanation"));
        assert!(formatted.contains("Footer text"));
    }

    #[test]
    fn test_format_body_empty_when_no_body() {
        let elements = CommitMessageElements {
            subject: "feat: test".to_string(),
            body: None,
            body_summary: None,
            body_details: None,
            body_footer: None,
        };

        assert_eq!(elements.format_body(), "");
    }

    // ============================================================================
    // Tests for XsdErrorType::description()
    // ============================================================================

    #[test]
    fn test_error_type_descriptions() {
        assert_eq!(
            XsdErrorType::MissingRequiredElement.description(),
            "Missing required element"
        );
        assert_eq!(
            XsdErrorType::UnexpectedElement.description(),
            "Unexpected element"
        );
        assert_eq!(
            XsdErrorType::InvalidContent.description(),
            "Invalid content"
        );
        assert_eq!(XsdErrorType::MalformedXml.description(), "Malformed XML");
    }

    // ============================================================================
    // Tests for <code> element support
    // ============================================================================

    #[test]
    fn test_validate_subject_with_code_element() {
        // XSD allows <code> elements for escaping special characters
        let xml = r#"<ralph-commit>
<ralph-subject>fix: handle <code>a &lt; b</code> comparison</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        // Text from both outside and inside <code> should be collected
        assert!(elements.subject.contains("fix: handle"));
        assert!(elements.subject.contains("a < b"));
        assert!(elements.subject.contains("comparison"));
    }

    #[test]
    fn test_validate_body_with_code_element() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add generic support</ralph-subject>
<ralph-body>Added <code>HashMap&lt;K, V&gt;</code> support to the parser.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("HashMap<K, V>"));
    }

    #[test]
    fn test_validate_detailed_body_with_code_elements() {
        let xml = r#"<ralph-commit>
<ralph-subject>refactor: improve type handling</ralph-subject>
<ralph-body-summary>Refactored <code>Option&lt;T&gt;</code> handling</ralph-body-summary>
<ralph-body-details>Changed <code>if a &lt; b</code> to <code>if a &gt; b</code></ralph-body-details>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert!(elements.body_summary.unwrap().contains("Option<T>"));
        let details = elements.body_details.unwrap();
        assert!(details.contains("if a < b"));
        assert!(details.contains("if a > b"));
    }

    // =========================================================================
    // REALISTIC LLM OUTPUT TESTS FOR COMMIT MESSAGES
    // These test actual patterns that LLMs produce when following the prompts
    // =========================================================================

    #[test]
    fn test_llm_commit_with_generic_type_in_subject() {
        // LLM correctly escapes generic type in commit subject
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add <code>Result&lt;T, E&gt;</code> support</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped generic should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.subject.contains("Result<T, E>"));
    }

    #[test]
    fn test_llm_commit_with_comparison_in_body() {
        // LLM correctly escapes comparison in commit body
        let xml = r#"<ralph-commit>
<ralph-subject>fix: correct boundary check</ralph-subject>
<ralph-body>The condition <code>count &lt; 0</code> was incorrect. Changed to <code>count &lt;= 0</code> to handle zero case.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped comparisons should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("count < 0"));
        assert!(body.contains("count <= 0"));
    }

    #[test]
    fn test_llm_commit_with_logical_operators_in_body() {
        // LLM correctly escapes logical operators
        let xml = r#"<ralph-commit>
<ralph-subject>refactor: simplify condition</ralph-subject>
<ralph-body>Simplified <code>a &amp;&amp; b || c</code> to use helper function.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped logical operators should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.body.unwrap().contains("a && b || c"));
    }

    #[test]
    fn test_llm_commit_with_detailed_body_describing_code() {
        // LLM uses detailed body format with code references
        let xml = r#"<ralph-commit>
<ralph-subject>feat(parser): add generic parsing</ralph-subject>
<ralph-body-summary>Added generic <code>parse&lt;T&gt;</code> function.</ralph-body-summary>
<ralph-body-details>- Supports any type implementing <code>FromStr</code>
- Returns <code>Result&lt;T, ParseError&gt;</code>
- Handles cases where <code>input.len() &gt; 0</code></ralph-body-details>
<ralph-body-footer>Closes #456</ralph-body-footer>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with detailed body should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.body_summary.unwrap().contains("parse<T>"));
        let details = elements.body_details.unwrap();
        assert!(details.contains("Result<T, ParseError>"));
        assert!(details.contains("input.len() > 0"));
    }

    #[test]
    fn test_llm_commit_with_html_reference_in_body() {
        // LLM describes HTML-related changes
        let xml = r#"<ralph-commit>
<ralph-subject>fix(ui): correct template rendering</ralph-subject>
<ralph-body>Fixed the <code>&lt;div class="container"&gt;</code> element that was not rendering correctly when <code>count &gt; 10</code>.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with HTML reference should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("<div class=\"container\">"));
        assert!(body.contains("count > 10"));
    }

    #[test]
    fn test_llm_mistake_unescaped_generic_in_subject_fails() {
        // LLM forgets to escape generic in subject - this SHOULD fail
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add Vec<String> support</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_err(),
            "Unescaped generic in subject should fail: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_llm_mistake_unescaped_comparison_in_body_fails() {
        // LLM forgets to escape comparison - this SHOULD fail
        let xml = r#"<ralph-commit>
<ralph-subject>fix: correct comparison</ralph-subject>
<ralph-body>Changed a < b to a <= b.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_err(),
            "Unescaped comparison in body should fail: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_llm_uses_cdata_in_body() {
        // LLM uses CDATA for complex code reference (valid alternative)
        let xml = r#"<ralph-commit>
<ralph-subject>fix: handle edge case</ralph-subject>
<ralph-body>Fixed the case where <code><![CDATA[a < b && c > d]]></code> fails.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "CDATA in body should be valid: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.body.unwrap().contains("a < b && c > d"));
    }

    #[test]
    fn test_llm_commit_realistic_refactor_message() {
        // A realistic refactor commit message an LLM might produce
        let xml = r#"<ralph-commit>
<ralph-subject>refactor(api): extract validation logic</ralph-subject>
<ralph-body>Extracted the validation logic from <code>handle_request&lt;T&gt;</code> into a separate
<code>validate&lt;T: Validate&gt;</code> function. This improves testability and allows
reuse across endpoints that check <code>input.size() &lt; MAX_SIZE</code>.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Realistic refactor commit should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("handle_request<T>"));
        assert!(body.contains("validate<T: Validate>"));
        assert!(body.contains("input.size() < MAX_SIZE"));
    }
}
