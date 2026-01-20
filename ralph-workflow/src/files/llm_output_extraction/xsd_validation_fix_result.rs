//! XSD validation for fix result XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format for fix results.

use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;

/// Validate fix result XML content against the XSD schema.
///
/// This function validates that the XML content conforms to the expected
/// fix result format defined in fix_result.xsd:
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
pub fn validate_fix_result_xml(xml_content: &str) -> Result<FixResultElements, XsdValidationError> {
    let content = xml_content.trim();

    // Check for XML declaration (optional, so we skip it if present)
    let content = if content.starts_with("<?xml") {
        if let Some(end) = content.find("?>") {
            &content[end + 2..]
        } else {
            return Err(XsdValidationError {
                error_type:
                    crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MalformedXml,
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

    // Check for <ralph-fix-result> root element
    if !content.starts_with("<ralph-fix-result>") {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-fix-result".to_string(),
            expected: "<ralph-fix-result> as root element".to_string(),
            found: if content.is_empty() {
                "empty content".to_string()
            } else if content.len() < 50 {
                content.to_string()
            } else {
                format!("{}...", &content[..50])
            },
            suggestion: "Wrap your fix result in <ralph-fix-result> tags".to_string(),
        });
    }

    if !content.ends_with("</ralph-fix-result>") {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-fix-result".to_string(),
            expected: "closing </ralph-fix-result> tag".to_string(),
            found: "missing closing tag".to_string(),
            suggestion: "Add </ralph-fix-result> at the end of your fix result".to_string(),
        });
    }

    // Extract content between root tags
    let root_start = "<ralph-fix-result>".len();
    let root_end = content.len() - "</ralph-fix-result>".len();
    let fix_result_content = &content[root_start..root_end];

    // Parse required and optional elements
    let mut status = None;
    let mut summary = None;

    // Parse elements in order
    let mut remaining = fix_result_content.trim();

    while !remaining.is_empty() {
        // Try to parse ralph-status element
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-status") {
            if status.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-status".to_string(),
                    expected: "only one <ralph-status> element".to_string(),
                    found: "duplicate <ralph-status> element".to_string(),
                    suggestion: "Include only one <ralph-status> element in your fix result".to_string(),
                });
            }
            status = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-status");
            continue;
        }

        // Try to parse ralph-summary element
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-summary") {
            if summary.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-summary".to_string(),
                    expected: "only one <ralph-summary> element".to_string(),
                    found: "duplicate <ralph-summary> element".to_string(),
                    suggestion: "Include only one <ralph-summary> element".to_string(),
                });
            }
            summary = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-summary");
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
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: potential_tag.to_string(),
                    expected: "only valid fix result tags".to_string(),
                    found: format!("unexpected tag: {potential_tag}"),
                    suggestion: "Remove the unexpected tag. Valid tags are: <ralph-status>, <ralph-summary>".to_string(),
                });
            }
        }

        return Err(XsdValidationError {
            error_type:
                crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "content".to_string(),
            expected: "only XML tags".to_string(),
            found: first_fifty,
            suggestion:
                "Remove any text outside of XML tags. All content must be within appropriate tags."
                    .to_string(),
        });
    }

    // Validate required element
    let status = status.ok_or_else(|| XsdValidationError {
        error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
        element_path: "ralph-status".to_string(),
        expected: "<ralph-status> element (required)".to_string(),
        found: "no <ralph-status> found".to_string(),
        suggestion: "Add <ralph-status> with one of: all_issues_addressed, issues_remain, no_issues_found".to_string(),
    })?;

    // Validate status content is not empty
    let status = status.trim();
    if status.is_empty() {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "ralph-status".to_string(),
            expected: "non-empty status".to_string(),
            found: "empty status".to_string(),
            suggestion: "The <ralph-status> tag must contain one of: all_issues_addressed, issues_remain, no_issues_found".to_string(),
        });
    }

    // Validate status is one of the allowed values
    let valid_statuses = ["all_issues_addressed", "issues_remain", "no_issues_found"];
    if !valid_statuses.contains(&status) {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "ralph-status".to_string(),
            expected: "one of: all_issues_addressed, issues_remain, no_issues_found".to_string(),
            found: status.to_string(),
            suggestion: "The <ralph-status> tag must contain exactly one of: all_issues_addressed, issues_remain, no_issues_found".to_string(),
        });
    }

    Ok(FixResultElements {
        status: status.to_string(),
        summary: summary
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    })
}

/// Extract content from an XML-style tag.
fn extract_tag_content(content: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{tag_name}>");
    let close_tag = format!("</{tag_name}>");

    let content_trimmed = content.trim_start();
    if !content_trimmed.starts_with(&open_tag) {
        return None;
    }

    let open_pos = content.len() - content_trimmed.len();
    let content_after_open = &content[open_pos + open_tag.len()..];

    let close_pos = content_after_open.find(&close_tag)?;
    let inner = &content_after_open[..close_pos];
    Some(inner.to_string())
}

/// Advance the content pointer past the specified tag.
fn advance_past_tag<'a>(content: &'a str, tag_name: &str) -> &'a str {
    let close_tag = format!("</{tag_name}>");
    let trimmed = content.trim_start();

    if let Some(pos) = trimmed.find(&close_tag) {
        let after_close = &trimmed[pos + close_tag.len()..];
        after_close.trim_start()
    } else {
        &content[content.len()..]
    }
}

/// Parsed fix result elements from valid XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixResultElements {
    /// The fix status (required)
    /// Valid values: all_issues_addressed, issues_remain, no_issues_found
    pub status: String,
    /// Optional summary of fixes applied
    pub summary: Option<String>,
}

impl FixResultElements {
    /// Returns true if all issues have been addressed.
    pub fn is_complete(&self) -> bool {
        self.status == "all_issues_addressed"
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
        assert_eq!(error.element_path, "ralph-status");
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
}
