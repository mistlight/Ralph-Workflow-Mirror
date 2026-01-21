//! XSD validation for issues XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format for review issues.
//!
//! Uses quick_xml for robust XML parsing with proper whitespace handling.

use crate::files::llm_output_extraction::xml_helpers::{
    create_reader, duplicate_element_error, malformed_xml_error, read_text_until_end, skip_to_end,
    text_outside_tags_error, unexpected_element_error,
};
use crate::files::llm_output_extraction::xsd_validation::{XsdErrorType, XsdValidationError};
use quick_xml::events::Event;

/// Example of valid issues XML with issues.
const EXAMPLE_ISSUES_XML: &str = r#"<ralph-issues>
<ralph-issue>Missing error handling in API endpoint</ralph-issue>
<ralph-issue>Variable shadowing in loop construct</ralph-issue>
</ralph-issues>"#;

/// Example of valid issues XML with no issues.
const EXAMPLE_NO_ISSUES_XML: &str = r#"<ralph-issues>
<ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
</ralph-issues>"#;

/// Validate issues XML content against the XSD schema.
///
/// This function validates that the XML content conforms to the expected
/// issues format defined in issues.xsd:
///
/// ```xml
/// <ralph-issues>
///   <ralph-issue>First issue description</ralph-issue>
///   <ralph-issue>Second issue description</ralph-issue>
///   ...
/// </ralph-issues>
/// ```
///
/// OR for no issues:
///
/// ```xml
/// <ralph-issues>
///   <ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
/// </ralph-issues>
/// ```
///
/// # Arguments
///
/// * `xml_content` - The XML content to validate
///
/// # Returns
///
/// * `Ok(IssuesElements)` if the XML is valid and contains all required elements
/// * `Err(XsdValidationError)` if the XML is invalid or doesn't conform to the schema
pub fn validate_issues_xml(xml_content: &str) -> Result<IssuesElements, XsdValidationError> {
    let content = xml_content.trim();
    let mut reader = create_reader(content);
    let mut buf = Vec::new();

    // Find the root element
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"ralph-issues" => break,
            Ok(Event::Start(e)) => {
                let name_bytes = e.name();
                let tag_name = String::from_utf8_lossy(name_bytes.as_ref());
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MissingRequiredElement,
                    element_path: "ralph-issues".to_string(),
                    expected: "<ralph-issues> as root element".to_string(),
                    found: format!("<{}> (wrong root element)", tag_name),
                    suggestion: "Use <ralph-issues> as the root element.".to_string(),
                    example: Some(EXAMPLE_ISSUES_XML.into()),
                });
            }
            Ok(Event::Text(_)) => {
                // Text before root element - continue to EOF error which is more informative
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MissingRequiredElement,
                    element_path: "ralph-issues".to_string(),
                    expected: "<ralph-issues> as root element".to_string(),
                    found: if content.is_empty() {
                        "empty content".to_string()
                    } else if content.len() <= 60 {
                        content.to_string()
                    } else {
                        format!("{}...", &content[..60])
                    },
                    suggestion: "Wrap your issues in <ralph-issues>...</ralph-issues> tags."
                        .to_string(),
                    example: Some(EXAMPLE_ISSUES_XML.into()),
                });
            }
            Ok(_) => {} // Skip XML declaration, comments, etc.
            Err(e) => return Err(malformed_xml_error(e)),
        }
        buf.clear();
    }

    // Parse child elements
    let mut issues: Vec<String> = Vec::new();
    let mut no_issues_found: Option<String> = None;

    const VALID_TAGS: [&str; 2] = ["ralph-issue", "ralph-no-issues-found"];

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                match e.name().as_ref() {
                    b"ralph-issue" => {
                        // Cannot mix issues and no-issues-found
                        if no_issues_found.is_some() {
                            return Err(XsdValidationError {
                                error_type: XsdErrorType::UnexpectedElement,
                                element_path: "ralph-issues/ralph-issue".to_string(),
                                expected: "either <ralph-issue> elements OR <ralph-no-issues-found>, not both".to_string(),
                                found: "mixed issues and no-issues-found".to_string(),
                                suggestion: "Use <ralph-issue> when issues exist, or <ralph-no-issues-found> when no issues exist.".to_string(),
                                example: Some(EXAMPLE_ISSUES_XML.into()),
                            });
                        }
                        let issue_text = read_text_until_end(&mut reader, b"ralph-issue")?;
                        issues.push(issue_text);
                    }
                    b"ralph-no-issues-found" => {
                        // Cannot mix issues and no-issues-found
                        if !issues.is_empty() {
                            return Err(XsdValidationError {
                                error_type: XsdErrorType::UnexpectedElement,
                                element_path: "ralph-issues/ralph-no-issues-found".to_string(),
                                expected: "either <ralph-issue> elements OR <ralph-no-issues-found>, not both".to_string(),
                                found: "mixed issues and no-issues-found".to_string(),
                                suggestion: "Use <ralph-issue> when issues exist, or <ralph-no-issues-found> when no issues exist.".to_string(),
                                example: Some(EXAMPLE_NO_ISSUES_XML.into()),
                            });
                        }
                        if no_issues_found.is_some() {
                            return Err(duplicate_element_error(
                                "ralph-no-issues-found",
                                "ralph-issues",
                            ));
                        }
                        no_issues_found =
                            Some(read_text_until_end(&mut reader, b"ralph-no-issues-found")?);
                    }
                    other => {
                        let _ = skip_to_end(&mut reader, other);
                        return Err(unexpected_element_error(other, &VALID_TAGS, "ralph-issues"));
                    }
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Err(text_outside_tags_error(trimmed, "ralph-issues"));
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-issues" => break,
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-issues".to_string(),
                    expected: "closing </ralph-issues> tag".to_string(),
                    found: "end of content without closing tag".to_string(),
                    suggestion: "Add </ralph-issues> at the end.".to_string(),
                    example: Some(EXAMPLE_ISSUES_XML.into()),
                });
            }
            Ok(_) => {} // Skip comments, etc.
            Err(e) => return Err(malformed_xml_error(e)),
        }
    }

    // Filter out empty issues
    let filtered_issues: Vec<String> = issues.into_iter().filter(|s| !s.is_empty()).collect();
    let filtered_no_issues = no_issues_found.filter(|s| !s.is_empty());

    // Must have either issues or no-issues-found
    if filtered_issues.is_empty() && filtered_no_issues.is_none() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-issues".to_string(),
            expected: "at least one <ralph-issue> element OR <ralph-no-issues-found>".to_string(),
            found: "empty <ralph-issues> element".to_string(),
            suggestion:
                "Add <ralph-issue> elements for issues found, or <ralph-no-issues-found> if no issues exist."
                    .to_string(),
            example: Some(EXAMPLE_ISSUES_XML.into()),
        });
    }

    Ok(IssuesElements {
        issues: filtered_issues,
        no_issues_found: filtered_no_issues,
    })
}

/// Parsed issues elements from valid XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuesElements {
    /// List of issues (if any)
    pub issues: Vec<String>,
    /// No issues found message (if no issues)
    pub no_issues_found: Option<String>,
}

impl IssuesElements {
    /// Returns true if there are no issues.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty() && self.no_issues_found.is_some()
    }

    /// Returns the number of issues.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn issue_count(&self) -> usize {
        self.issues.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_single_issue() {
        let xml = r#"<ralph-issues>
<ralph-issue>First issue description</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.issues.len(), 1);
        assert_eq!(elements.issues[0], "First issue description");
        assert!(elements.no_issues_found.is_none());
    }

    #[test]
    fn test_validate_valid_multiple_issues() {
        let xml = r#"<ralph-issues>
<ralph-issue>First issue</ralph-issue>
<ralph-issue>Second issue</ralph-issue>
<ralph-issue>Third issue</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.issues.len(), 3);
        assert_eq!(elements.issue_count(), 3);
    }

    #[test]
    fn test_validate_valid_no_issues_found() {
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert!(elements.issues.is_empty());
        assert!(elements.no_issues_found.is_some());
        assert!(elements.is_empty());
    }

    #[test]
    fn test_validate_missing_root_element() {
        let xml = r#"Some random text without proper XML tags"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-issues");
    }

    #[test]
    fn test_validate_empty_issues() {
        let xml = r#"<ralph-issues>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.expected.contains("at least one"));
    }

    #[test]
    fn test_validate_mixed_issues_and_no_issues_found() {
        let xml = r#"<ralph-issues>
<ralph-issue>First issue</ralph-issue>
<ralph-no-issues-found>No issues</ralph-no-issues-found>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.suggestion.contains("not both") || error.expected.contains("not both"));
    }

    #[test]
    fn test_validate_duplicate_no_issues_found() {
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>No issues</ralph-no-issues-found>
<ralph-no-issues-found>Also no issues</ralph-no-issues-found>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_whitespace_handling() {
        // This is the key test - quick_xml should handle whitespace between elements
        let xml =
            "  <ralph-issues>  \n  <ralph-issue>Issue text</ralph-issue>  \n  </ralph-issues>  ";

        let result = validate_issues_xml(xml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_xml_declaration() {
        let xml = r#"<?xml version="1.0"?>
<ralph-issues>
<ralph-issue>Issue text</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_issue_with_code_element() {
        // XSD now allows <code> elements for escaping special characters
        let xml = r#"<ralph-issues>
<ralph-issue>Check if <code>a &lt; b</code> is valid</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.issues.len(), 1);
        // The text from both outside and inside <code> should be collected
        assert!(elements.issues[0].contains("Check if"));
        assert!(elements.issues[0].contains("a < b"));
        assert!(elements.issues[0].contains("is valid"));
    }

    #[test]
    fn test_validate_no_issues_with_code_element() {
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>All <code>Record&lt;string, T&gt;</code> types are correct</ralph-no-issues-found>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert!(elements.no_issues_found.is_some());
        let msg = elements.no_issues_found.unwrap();
        assert!(msg.contains("Record<string, T>"));
    }
}
