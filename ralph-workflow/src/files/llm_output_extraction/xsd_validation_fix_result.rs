//! XSD validation for fix result XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format for fix results.
//!
//! Uses `quick_xml` for robust XML parsing with proper whitespace handling.

use crate::files::llm_output_extraction::xml_helpers::{
    create_reader, duplicate_element_error, format_content_preview, malformed_xml_error,
    missing_required_error, read_text_until_end, skip_to_end, text_outside_tags_error,
    unexpected_element_error,
};
use crate::files::llm_output_extraction::xsd_validation::{XsdErrorType, XsdValidationError};
use quick_xml::events::Event;

/// Example of a valid fix result XML for error messages.
const EXAMPLE_FIX_RESULT_XML: &str = r"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
<ralph-summary>Fixed all 3 issues found during review</ralph-summary>
</ralph-fix-result>";

/// Valid status values for fix results.
const VALID_STATUSES: [&str; 3] = ["all_issues_addressed", "issues_remain", "no_issues_found"];

/// Validate fix result XML content against the XSD schema.
///
/// This function validates that the XML content conforms to the expected
/// fix result format defined in `fix_result.xsd`:
///
/// ```xml
/// <ralph-fix-result>
///   <ralph-status>all_issues_addressed|issues_remain|no_issues_found</ralph-status>
///   <ralph-summary>Optional summary of fixes applied</ralph-summary>
/// </ralph-fix-result>
/// ```
///
/// # Arguments
///
/// * `xml_content` - The XML content to validate
///
/// # Returns
///
/// * `Ok(FixResultElements)` if the XML is valid and contains all required elements
/// * `Err(XsdValidationError)` if the XML is invalid or doesn't conform to the schema
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn validate_fix_result_xml(xml_content: &str) -> Result<FixResultElements, XsdValidationError> {
    let content = xml_content.trim();

    // Check for illegal XML characters BEFORE parsing
    use crate::files::llm_output_extraction::xml_helpers::check_for_illegal_xml_characters;
    check_for_illegal_xml_characters(content)?;

    let mut reader = create_reader(content);
    let mut buf = Vec::new();

    // Find the root element
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"ralph-fix-result" => break,
            Ok(Event::Start(e)) => {
                let name_bytes = e.name();
                let tag_name = String::from_utf8_lossy(name_bytes.as_ref());
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MissingRequiredElement,
                    element_path: "ralph-fix-result".to_string(),
                    expected: "<ralph-fix-result> as root element".to_string(),
                    found: format!("<{tag_name}> (wrong root element)"),
                    suggestion: "Use <ralph-fix-result> as the root element.".to_string(),
                    example: Some(EXAMPLE_FIX_RESULT_XML.into()),
                });
            }
            Ok(Event::Text(_)) => {
                // Text before root element - continue to find root or reach EOF
                // EOF will give a more informative "missing root element" error
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MissingRequiredElement,
                    element_path: "ralph-fix-result".to_string(),
                    expected: "<ralph-fix-result> as root element".to_string(),
                    found: format_content_preview(content),
                    suggestion:
                        "Wrap your result in <ralph-fix-result>...</ralph-fix-result> tags."
                            .to_string(),
                    example: Some(EXAMPLE_FIX_RESULT_XML.into()),
                });
            }
            Ok(_) => {} // Skip XML declaration, comments, etc.
            Err(e) => return Err(malformed_xml_error(e)),
        }
        buf.clear();
    }

    // Parse child elements
    let mut status: Option<String> = None;
    let mut summary: Option<String> = None;

    const VALID_TAGS: [&str; 2] = ["ralph-status", "ralph-summary"];

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"ralph-status" => {
                    if status.is_some() {
                        return Err(duplicate_element_error("ralph-status", "ralph-fix-result"));
                    }
                    status = Some(read_text_until_end(&mut reader, b"ralph-status")?);
                }
                b"ralph-summary" => {
                    if summary.is_some() {
                        return Err(duplicate_element_error("ralph-summary", "ralph-fix-result"));
                    }
                    summary = Some(read_text_until_end(&mut reader, b"ralph-summary")?);
                }
                other => {
                    let _ = skip_to_end(&mut reader, other);
                    return Err(unexpected_element_error(
                        other,
                        &VALID_TAGS,
                        "ralph-fix-result",
                    ));
                }
            },
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Err(text_outside_tags_error(trimmed, "ralph-fix-result"));
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-fix-result" => break,
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-fix-result".to_string(),
                    expected: "closing </ralph-fix-result> tag".to_string(),
                    found: "end of content without closing tag".to_string(),
                    suggestion: "Add </ralph-fix-result> at the end.".to_string(),
                    example: Some(EXAMPLE_FIX_RESULT_XML.into()),
                });
            }
            Ok(_) => {} // Skip comments, etc.
            Err(e) => return Err(malformed_xml_error(e)),
        }
    }

    // Validate required element: status
    let status = status.ok_or_else(|| {
        missing_required_error(
            "ralph-status",
            "ralph-fix-result",
            Some(EXAMPLE_FIX_RESULT_XML),
        )
    })?;

    // Validate status content
    if status.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-status".to_string(),
            expected: "non-empty status value".to_string(),
            found: "empty status".to_string(),
            suggestion: format!(
                "The <ralph-status> must contain one of: {}",
                VALID_STATUSES.join(", ")
            ),
            example: Some(EXAMPLE_FIX_RESULT_XML.into()),
        });
    }

    // Validate status is one of the allowed values
    if !VALID_STATUSES.contains(&status.as_str()) {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-status".to_string(),
            expected: format!("one of: {}", VALID_STATUSES.join(", ")),
            found: status.clone(),
            suggestion: format!(
                "Change <ralph-status>{}</ralph-status> to use a valid value: {}",
                status,
                VALID_STATUSES.join(", ")
            ),
            example: Some(EXAMPLE_FIX_RESULT_XML.into()),
        });
    }

    Ok(FixResultElements {
        status,
        summary: summary.filter(|s| !s.is_empty()),
    })
}

/// Parsed fix result elements from valid XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixResultElements {
    /// The fix status (required)
    /// Valid values: `all_issues_addressed`, `issues_remain`, `no_issues_found`
    pub status: String,
    /// Optional summary of fixes applied
    pub summary: Option<String>,
}

impl FixResultElements {
    /// Returns true if all issues have been addressed or no issues were found.
    pub fn is_complete(&self) -> bool {
        self.status == "all_issues_addressed" || self.status == "no_issues_found"
    }

    /// Returns true if issues remain.
    pub fn has_remaining_issues(&self) -> bool {
        self.status == "issues_remain"
    }

    /// Returns true if no issues were found.
    pub fn is_no_issues(&self) -> bool {
        self.status == "no_issues_found"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_all_issues_addressed() {
        let xml = r#"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "all_issues_addressed");
        assert!(elements.is_complete());
        assert!(!elements.has_remaining_issues());
    }

    #[test]
    fn test_validate_valid_issues_remain() {
        let xml = r#"<ralph-fix-result>
<ralph-status>issues_remain</ralph-status>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "issues_remain");
        assert!(elements.has_remaining_issues());
        assert!(!elements.is_complete());
    }

    #[test]
    fn test_validate_valid_no_issues_found() {
        let xml = r#"<ralph-fix-result>
<ralph-status>no_issues_found</ralph-status>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "no_issues_found");
        assert!(elements.is_no_issues());
    }

    #[test]
    fn test_validate_valid_with_summary() {
        let xml = r#"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
<ralph-summary>All reported issues have been fixed</ralph-summary>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "all_issues_addressed");
        assert_eq!(
            elements.summary,
            Some("All reported issues have been fixed".to_string())
        );
    }

    #[test]
    fn test_validate_missing_root_element() {
        let xml = r#"Some random text without proper XML tags"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-fix-result");
    }

    #[test]
    fn test_validate_missing_status() {
        let xml = r#"<ralph-fix-result>
<ralph-summary>No status</ralph-summary>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.element_path.contains("ralph-status"));
    }

    #[test]
    fn test_validate_invalid_status() {
        let xml = r#"<ralph-fix-result>
<ralph-status>invalid_status_value</ralph-status>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.expected.contains("all_issues_addressed"));
    }

    #[test]
    fn test_validate_empty_status() {
        let xml = r#"<ralph-fix-result>
<ralph-status>   </ralph-status>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_duplicate_status() {
        let xml = r#"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
<ralph-status>issues_remain</ralph-status>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_whitespace_handling() {
        // This is the key test - quick_xml should handle whitespace between elements
        let xml = "  <ralph-fix-result>  \n  <ralph-status>all_issues_addressed</ralph-status>  \n  </ralph-fix-result>  ";

        let result = validate_fix_result_xml(xml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_xml_declaration() {
        let xml = r#"<?xml version="1.0"?>
<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
</ralph-fix-result>"#;

        let result = validate_fix_result_xml(xml);
        assert!(result.is_ok());
    }
}
