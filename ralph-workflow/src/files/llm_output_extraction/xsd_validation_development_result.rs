//! XSD validation for development result XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format for development results.

use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;

/// Validate development result XML content against the XSD schema.
///
/// This function validates that the XML content conforms to the expected
/// development result format defined in development_result.xsd:
///
/// ```xml
/// <ralph-development-result>
///   <ralph-status>completed|partial|failed</ralph-status>
///   <ralph-summary>Brief summary of what was done</ralph-summary>
///   <ralph-files-changed>Optional list of files modified</ralph-files-changed>
///   <ralph-next-steps>Optional next steps</ralph-next-steps>
/// </ralph-development-result>
/// ```
///
/// # Arguments
///
/// * `xml_content` - The XML content to validate
///
/// # Returns
///
/// * `Ok(DevelopmentResultElements)` if the XML is valid and contains all required elements
/// * `Err(XsdValidationError)` if the XML is invalid or doesn't conform to the schema
pub fn validate_development_result_xml(
    xml_content: &str,
) -> Result<DevelopmentResultElements, XsdValidationError> {
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

    // Check for <ralph-development-result> root element
    if !content.starts_with("<ralph-development-result>") {
        return Err(XsdValidationError {
            error_type:
                crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-development-result".to_string(),
            expected: "<ralph-development-result> as root element".to_string(),
            found: if content.is_empty() {
                "empty content".to_string()
            } else if content.len() < 50 {
                content.to_string()
            } else {
                format!("{}...", &content[..50])
            },
            suggestion: "Wrap your development result in <ralph-development-result> tags"
                .to_string(),
        });
    }

    if !content.ends_with("</ralph-development-result>") {
        return Err(XsdValidationError {
            error_type:
                crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-development-result".to_string(),
            expected: "closing </ralph-development-result> tag".to_string(),
            found: "missing closing tag".to_string(),
            suggestion: "Add </ralph-development-result> at the end of your development result"
                .to_string(),
        });
    }

    // Extract content between root tags
    let root_start = "<ralph-development-result>".len();
    let root_end = content.len() - "</ralph-development-result>".len();
    let development_result_content = &content[root_start..root_end];

    // Parse required and optional elements
    let mut status = None;
    let mut summary = None;
    let mut files_changed = None;
    let mut next_steps = None;

    // Parse elements in order
    let mut remaining = development_result_content.trim();

    while !remaining.is_empty() {
        // Try to parse ralph-status element
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-status") {
            if status.is_some() {
                return Err(XsdValidationError {
                    error_type:
                        crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-status".to_string(),
                    expected: "only one <ralph-status> element".to_string(),
                    found: "duplicate <ralph-status> element".to_string(),
                    suggestion:
                        "Include only one <ralph-status> element in your development result"
                            .to_string(),
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
                    error_type:
                        crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
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

        // Try to parse ralph-files-changed element
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-files-changed") {
            if files_changed.is_some() {
                return Err(XsdValidationError {
                    error_type:
                        crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-files-changed".to_string(),
                    expected: "only one <ralph-files-changed> element".to_string(),
                    found: "duplicate <ralph-files-changed> element".to_string(),
                    suggestion: "Include only one <ralph-files-changed> element (optional)".to_string(),
                });
            }
            files_changed = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-files-changed");
            continue;
        }

        // Try to parse ralph-next-steps element
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-next-steps") {
            if next_steps.is_some() {
                return Err(XsdValidationError {
                    error_type:
                        crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-next-steps".to_string(),
                    expected: "only one <ralph-next-steps> element".to_string(),
                    found: "duplicate <ralph-next-steps> element".to_string(),
                    suggestion: "Include only one <ralph-next-steps> element (optional)".to_string(),
                });
            }
            next_steps = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-next-steps");
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
                    error_type:
                        crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: potential_tag.to_string(),
                    expected: "only valid development result tags".to_string(),
                    found: format!("unexpected tag: {potential_tag}"),
                    suggestion: "Remove the unexpected tag. Valid tags are: <ralph-status>, <ralph-summary>, <ralph-files-changed>, <ralph-next-steps>".to_string(),
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

    // Validate required elements
    let status = status.ok_or_else(|| XsdValidationError {
        error_type:
            crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
        element_path: "ralph-status".to_string(),
        expected: "<ralph-status> element (required)".to_string(),
        found: "no <ralph-status> found".to_string(),
        suggestion:
            "Add <ralph-status> with one of: completed, partial, failed".to_string(),
    })?;

    let summary = summary.ok_or_else(|| XsdValidationError {
        error_type:
            crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
        element_path: "ralph-summary".to_string(),
        expected: "<ralph-summary> element (required)".to_string(),
        found: "no <ralph-summary> found".to_string(),
        suggestion: "Add <ralph-summary> with a brief description of what was done".to_string(),
    })?;

    // Validate status content is not empty
    let status = status.trim();
    if status.is_empty() {
        return Err(XsdValidationError {
            error_type:
                crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "ralph-status".to_string(),
            expected: "non-empty status".to_string(),
            found: "empty status".to_string(),
            suggestion: "The <ralph-status> tag must contain one of: completed, partial, failed"
                .to_string(),
        });
    }

    // Validate summary content is not empty
    let summary = summary.trim();
    if summary.is_empty() {
        return Err(XsdValidationError {
            error_type:
                crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "ralph-summary".to_string(),
            expected: "non-empty summary".to_string(),
            found: "empty summary".to_string(),
            suggestion: "The <ralph-summary> tag must contain a description of what was done"
                .to_string(),
        });
    }

    // Validate status is one of the allowed values
    let valid_statuses = ["completed", "partial", "failed"];
    if !valid_statuses.contains(&status) {
        return Err(XsdValidationError {
            error_type:
                crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "ralph-status".to_string(),
            expected: "one of: completed, partial, failed".to_string(),
            found: status.to_string(),
            suggestion:
                "The <ralph-status> tag must contain exactly one of: completed, partial, failed"
                    .to_string(),
        });
    }

    Ok(DevelopmentResultElements {
        status: status.to_string(),
        summary: summary.to_string(),
        files_changed: files_changed
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        next_steps: next_steps
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

/// Parsed development result elements from valid XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelopmentResultElements {
    /// The development status (required)
    /// Valid values: completed, partial, failed
    pub status: String,
    /// Summary of what was done (required)
    pub summary: String,
    /// Optional list of files changed
    pub files_changed: Option<String>,
    /// Optional next steps
    pub next_steps: Option<String>,
}

impl DevelopmentResultElements {
    /// Returns true if the work is completed.
    pub fn is_completed(&self) -> bool {
        self.status == "completed"
    }

    /// Returns true if the work is partially done.
    pub fn is_partial(&self) -> bool {
        self.status == "partial"
    }

    /// Returns true if the work failed.
    pub fn is_failed(&self) -> bool {
        self.status == "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_completed() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Fixed all bugs</ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "completed");
        assert!(elements.is_completed());
        assert!(!elements.is_partial());
        assert!(!elements.is_failed());
    }

    #[test]
    fn test_validate_valid_partial() {
        let xml = r#"<ralph-development-result>
<ralph-status>partial</ralph-status>
<ralph-summary>Started fixing bugs</ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "partial");
        assert!(elements.is_partial());
    }

    #[test]
    fn test_validate_valid_failed() {
        let xml = r#"<ralph-development-result>
<ralph-status>failed</ralph-status>
<ralph-summary>Could not complete the task</ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "failed");
        assert!(elements.is_failed());
    }

    #[test]
    fn test_validate_valid_with_all_optional_fields() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Implemented feature X</ralph-summary>
<ralph-files-changed>- src/main.rs
- src/utils.rs</ralph-files-changed>
<ralph-next-steps>Continue with testing</ralph-next-steps>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.status, "completed");
        assert_eq!(elements.summary, "Implemented feature X");
        assert_eq!(
            elements.files_changed,
            Some("- src/main.rs\n- src/utils.rs".to_string())
        );
        assert_eq!(
            elements.next_steps,
            Some("Continue with testing".to_string())
        );
    }

    #[test]
    fn test_validate_missing_root_element() {
        let xml = r#"Some random text without proper XML tags"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-development-result");
    }

    #[test]
    fn test_validate_missing_status() {
        let xml = r#"<ralph-development-result>
<ralph-summary>No status</ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-status");
    }

    #[test]
    fn test_validate_missing_summary() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-summary");
    }

    #[test]
    fn test_validate_invalid_status() {
        let xml = r#"<ralph-development-result>
<ralph-status>invalid_status_value</ralph-status>
<ralph-summary>Test</ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.expected.contains("completed"));
    }

    #[test]
    fn test_validate_empty_status() {
        let xml = r#"<ralph-development-result>
<ralph-status>   </ralph-status>
<ralph-summary>Test</ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_empty_summary() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>   </ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_duplicate_status() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-status>partial</ralph-status>
<ralph-summary>Test</ralph-summary>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_unexpected_element() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Test</ralph-summary>
<ralph-unknown>value</ralph-unknown>
</ralph-development-result>"#;

        let result = validate_development_result_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.element_path.contains("ralph-unknown"));
    }
}
