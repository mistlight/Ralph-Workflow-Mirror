//! XSD validation for commit message XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format.
//!
//! The validation is implemented as a lightweight custom validator that
//! checks the XML structure against the defined schema without requiring
//! heavy external dependencies.

use crate::files::llm_output_extraction::commit::is_conventional_commit_subject;

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

    // Check for XML declaration (optional, so we skip it if present)
    let content = if content.starts_with("<?xml") {
        if let Some(end) = content.find("?>") {
            &content[end + 2..]
        } else {
            return Err(XsdValidationError {
                error_type: XsdErrorType::MalformedXml,
                element_path: "xml".to_string(),
                expected: "valid XML declaration ending with ?>".to_string(),
                found: "unclosed XML declaration".to_string(),
                suggestion: "Ensure XML declaration is properly closed with ?>".to_string(),
            });
        }
    } else {
        content
    };

    let content = content.trim();

    // Check for <ralph-commit> root element
    if !content.starts_with("<ralph-commit>") {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-commit".to_string(),
            expected: "<ralph-commit> as root element".to_string(),
            found: if content.is_empty() {
                "empty content".to_string()
            } else if content.len() < 50 {
                content.to_string()
            } else {
                format!("{}...", &content[..50])
            },
            suggestion: "Wrap your commit message in <ralph-commit> tags".to_string(),
        });
    }

    if !content.ends_with("</ralph-commit>") {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-commit".to_string(),
            expected: "closing </ralph-commit> tag".to_string(),
            found: "missing closing tag".to_string(),
            suggestion: "Add </ralph-commit> at the end of your commit message".to_string(),
        });
    }

    // Extract content between root tags
    let root_start = "<ralph-commit>".len();
    let root_end = content.len() - "</ralph-commit>".len();
    let commit_content = &content[root_start..root_end];

    // Parse required and optional elements
    let mut subject = None;
    let mut body = None;
    let mut body_summary = None;
    let mut body_details = None;
    let mut body_footer = None;

    // Parse elements in order
    let mut remaining = commit_content.trim();

    while !remaining.is_empty() {
        // Try to parse each element type
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-subject") {
            if subject.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-subject".to_string(),
                    expected: "only one <ralph-subject> element".to_string(),
                    found: "duplicate <ralph-subject> element".to_string(),
                    suggestion: "Include only one <ralph-subject> element in your commit message"
                        .to_string(),
                });
            }
            subject = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-subject");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-body") {
            // Check if detailed elements already exist
            if body_summary.is_some() || body_details.is_some() || body_footer.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body".to_string(),
                    expected: "use either <ralph-body> OR detailed tags, not both".to_string(),
                    found: "mixed simple and detailed body elements".to_string(),
                    suggestion: "Use either <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format, not both".to_string(),
                });
            }
            if body.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body".to_string(),
                    expected: "only one <ralph-body> element".to_string(),
                    found: "duplicate <ralph-body> element".to_string(),
                    suggestion: "Include only one <ralph-body> element".to_string(),
                });
            }
            body = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-body");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-body-summary") {
            // Check if simple body already exists
            if body.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body-summary".to_string(),
                    expected: "use either <ralph-body> OR detailed tags, not both".to_string(),
                    found: "mixed simple and detailed body elements".to_string(),
                    suggestion: "Use either <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format, not both".to_string(),
                });
            }
            if body_summary.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body-summary".to_string(),
                    expected: "only one <ralph-body-summary> element".to_string(),
                    found: "duplicate <ralph-body-summary> element".to_string(),
                    suggestion: "Include only one <ralph-body-summary> element".to_string(),
                });
            }
            body_summary = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-body-summary");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-body-details") {
            // Check if simple body already exists
            if body.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body-details".to_string(),
                    expected: "use either <ralph-body> OR detailed tags, not both".to_string(),
                    found: "mixed simple and detailed body elements".to_string(),
                    suggestion: "Use either <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format, not both".to_string(),
                });
            }
            if body_details.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body-details".to_string(),
                    expected: "only one <ralph-body-details> element".to_string(),
                    found: "duplicate <ralph-body-details> element".to_string(),
                    suggestion: "Include only one <ralph-body-details> element".to_string(),
                });
            }
            body_details = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-body-details");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-body-footer") {
            // Check if simple body already exists
            if body.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body-footer".to_string(),
                    expected: "use either <ralph-body> OR detailed tags, not both".to_string(),
                    found: "mixed simple and detailed body elements".to_string(),
                    suggestion: "Use either <ralph-body> for simple body OR <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer> for detailed format, not both".to_string(),
                });
            }
            if body_footer.is_some() {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: "ralph-body-footer".to_string(),
                    expected: "only one <ralph-body-footer> element".to_string(),
                    found: "duplicate <ralph-body-footer> element".to_string(),
                    suggestion: "Include only one <ralph-body-footer> element".to_string(),
                });
            }
            body_footer = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-body-footer");
            continue;
        }

        // If we get here, there's unexpected content
        let first_fifty = if remaining.len() > 50 {
            format!("{}...", &remaining[..50])
        } else {
            remaining.to_string()
        };

        // Try to identify what the unexpected content is
        if remaining.starts_with('<') {
            if let Some(tag_end) = remaining.find('>') {
                let potential_tag = &remaining[..tag_end + 1];
                return Err(XsdValidationError {
                    error_type: XsdErrorType::UnexpectedElement,
                    element_path: potential_tag.to_string(),
                    expected: "only valid commit message tags".to_string(),
                    found: format!("unexpected tag: {potential_tag}"),
                    suggestion: format!("Remove the {potential_tag} tag. Valid tags are: <ralph-subject>, <ralph-body>, <ralph-body-summary>, <ralph-body-details>, <ralph-body-footer>"),
                });
            }
        }

        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "content".to_string(),
            expected: "only XML tags".to_string(),
            found: first_fifty,
            suggestion:
                "Remove any text outside of XML tags. All content must be within appropriate tags."
                    .to_string(),
        });
    }

    // Validate required element
    let subject = subject.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-subject".to_string(),
        expected: "<ralph-subject> element (required)".to_string(),
        found: "no <ralph-subject> found".to_string(),
        suggestion:
            "Add <ralph-subject>type(scope): description</ralph-subject> with your commit subject"
                .to_string(),
    })?;

    // Validate subject content
    let subject = subject.trim();
    if subject.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-subject".to_string(),
            expected: "non-empty subject line".to_string(),
            found: "empty subject".to_string(),
            suggestion: "The <ralph-subject> tag must contain a non-empty commit subject like 'feat: add feature'".to_string(),
        });
    }

    // Validate conventional commit format
    if !is_conventional_commit_subject(subject) {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-subject".to_string(),
            expected: "conventional commit format (type: description or type(scope): description)".to_string(),
            found: subject.to_string(),
            suggestion: "Use conventional commit format: feat|fix|docs|style|refactor|perf|test|build|ci|chore: description. Example: <ralph-subject>feat(api): add user authentication</ralph-subject>".to_string(),
        });
    }

    Ok(CommitMessageElements {
        subject: subject.to_string(),
        body: body.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        body_summary: body_summary
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        body_details: body_details
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        body_footer: body_footer
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    })
}

/// Extract content from an XML-style tag.
///
/// Returns `Some(content)` if the tag is found, `None` otherwise.
fn extract_tag_content(content: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{tag_name}>");
    let close_tag = format!("</{tag_name}>");

    // Find opening tag (must be at start or after whitespace)
    let content_trimmed = content.trim_start();
    if !content_trimmed.starts_with(&open_tag) {
        return None;
    }

    // Adjust for any leading whitespace
    let open_pos = content.len() - content_trimmed.len();
    let content_after_open = &content[open_pos + open_tag.len()..];

    // Find closing tag
    let close_pos = content_after_open.find(&close_tag)?;

    let inner = &content_after_open[..close_pos];
    Some(inner.to_string())
}

/// Advance the content pointer past the specified tag.
fn advance_past_tag<'a>(content: &'a str, tag_name: &str) -> &'a str {
    let close_tag = format!("</{tag_name}>");

    // Trim leading whitespace first
    let trimmed = content.trim_start();

    if let Some(pos) = trimmed.find(&close_tag) {
        let after_close = &trimmed[pos + close_tag.len()..];
        after_close.trim_start()
    } else {
        // Return empty string slice with same lifetime as content
        &content[content.len()..]
    }
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
}

impl XsdValidationError {
    /// Format this error for display in logs or retry prompts.
    pub fn format_for_display(&self) -> String {
        format!(
            "XSD Validation Error [{}]: {}\n  Element: {}\n  Expected: {}\n  Found: {}\n  Suggestion: {}",
            self.error_type,
            self.error_type.description(),
            self.element_path,
            self.expected,
            self.found,
            self.suggestion
        )
    }

    /// Format this error as a concise message for AI retry prompt.
    ///
    /// This provides an actionable, human-readable error message that guides
    /// the AI agent toward producing valid XML output.
    pub fn format_for_ai_retry(&self) -> String {
        match self.error_type {
            XsdErrorType::MissingRequiredElement => {
                format!(
                    "MISSING REQUIRED ELEMENT: '{}' is required but was not found.\n\n\
                     What was expected: {}\n\n\
                     How to fix: {}",
                    self.element_path, self.expected, self.suggestion
                )
            }
            XsdErrorType::UnexpectedElement => {
                format!(
                    "UNEXPECTED ELEMENT: Found '{}' which is not allowed.\n\n\
                     What was expected: {}\n\
                     What was found: {}\n\n\
                     How to fix: {}",
                    self.element_path, self.expected, self.found, self.suggestion
                )
            }
            XsdErrorType::InvalidContent => {
                format!(
                    "INVALID CONTENT: The content of '{}' does not meet requirements.\n\n\
                     What was expected: {}\n\
                     What was found: {}\n\n\
                     How to fix: {}",
                    self.element_path, self.expected, self.found, self.suggestion
                )
            }
            XsdErrorType::MalformedXml => {
                format!(
                    "MALFORMED XML: The XML structure is invalid.\n\n\
                     What was expected: {}\n\
                     What was found: {}\n\n\
                     How to fix: {}",
                    self.expected, self.found, self.suggestion
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
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("MISSING REQUIRED ELEMENT"));
        assert!(formatted.contains("'ralph-subject' is required"));
        assert!(formatted.contains("Add <ralph-subject>"));
    }

    #[test]
    fn test_format_for_ai_retry_unexpected_element() {
        let error = XsdValidationError {
            error_type: XsdErrorType::UnexpectedElement,
            element_path: "<unknown-tag>".to_string(),
            expected: "only valid commit message tags".to_string(),
            found: "unexpected tag: <unknown-tag>".to_string(),
            suggestion: "Remove the <unknown-tag> tag".to_string(),
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
        assert!(matches!(
            error.error_type,
            XsdErrorType::MissingRequiredElement
        ));
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
        assert_eq!(error.element_path, "ralph-subject");
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
        assert!(error
            .suggestion
            .contains("<ralph-body> for simple body OR <ralph-body-summary>"));
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
}
