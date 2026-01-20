//! XSD validation for issues XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format for review issues.

use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;

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

    // Check for <ralph-issues> root element
    if !content.starts_with("<ralph-issues>") {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-issues".to_string(),
            expected: "<ralph-issues> as root element".to_string(),
            found: if content.is_empty() {
                "empty content".to_string()
            } else if content.len() < 50 {
                content.to_string()
            } else {
                format!("{}...", &content[..50])
            },
            suggestion: "Wrap your issues in <ralph-issues> tags".to_string(),
        });
    }

    if !content.ends_with("</ralph-issues>") {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-issues".to_string(),
            expected: "closing </ralph-issues> tag".to_string(),
            found: "missing closing tag".to_string(),
            suggestion: "Add </ralph-issues> at the end of your issues".to_string(),
        });
    }

    // Extract content between root tags
    let root_start = "<ralph-issues>".len();
    let root_end = content.len() - "</ralph-issues>".len();
    let issues_content = &content[root_start..root_end];

    // Parse elements
    let mut issues = Vec::new();
    let mut no_issues_found = None;

    // Parse elements in order
    let mut remaining = issues_content.trim();

    while !remaining.is_empty() {
        // Try to parse ralph-issue elements
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-issue") {
            // Cannot mix issues and no-issues-found
            if no_issues_found.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-issue".to_string(),
                    expected: "either <ralph-issue> elements OR <ralph-no-issues-found>, not both".to_string(),
                    found: "mixed issues and no-issues-found".to_string(),
                    suggestion: "Use either <ralph-issue> elements when issues exist, or <ralph-no-issues-found> when no issues exist, not both".to_string(),
                });
            }
            issues.push(tag_content);
            remaining = advance_past_tag(remaining, "ralph-issue");
            continue;
        }

        // Try to parse ralph-no-issues-found element
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-no-issues-found") {
            // Cannot mix issues and no-issues-found
            if !issues.is_empty() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-no-issues-found".to_string(),
                    expected: "either <ralph-issue> elements OR <ralph-no-issues-found>, not both".to_string(),
                    found: "mixed issues and no-issues-found".to_string(),
                    suggestion: "Use either <ralph-issue> elements when issues exist, or <ralph-no-issues-found> when no issues exist, not both".to_string(),
                });
            }
            if no_issues_found.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-no-issues-found".to_string(),
                    expected: "only one <ralph-no-issues-found> element".to_string(),
                    found: "duplicate <ralph-no-issues-found> element".to_string(),
                    suggestion: "Include only one <ralph-no-issues-found> element".to_string(),
                });
            }
            no_issues_found = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-no-issues-found");
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
                    expected: "only valid issues tags".to_string(),
                    found: format!("unexpected tag: {potential_tag}"),
                    suggestion: "Remove the unexpected tag. Valid tags are: <ralph-issue>, <ralph-no-issues-found>".to_string(),
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

    // Must have either issues or no-issues-found
    if issues.is_empty() && no_issues_found.is_none() {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-issues".to_string(),
            expected: "at least one <ralph-issue> element OR <ralph-no-issues-found>".to_string(),
            found: "no issues or no-issues-found element".to_string(),
            suggestion: "Add either <ralph-issue> elements for issues found, or <ralph-no-issues-found> if no issues exist".to_string(),
        });
    }

    Ok(IssuesElements {
        issues: issues
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        no_issues_found: no_issues_found
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
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty() && self.no_issues_found.is_some()
    }

    /// Returns the number of issues.
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
        assert!(error.suggestion.contains("not both"));
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
}
