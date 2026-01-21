//! XSD validation for plan XML format.
//!
//! This module provides validation of XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format for development plans.

use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;

/// Validate plan XML content against the XSD schema.
///
/// This function validates that the XML content conforms to the expected
/// plan format defined in plan.xsd:
///
/// ```xml
/// <ralph-plan>
///   <ralph-summary>One paragraph explaining what will be done</ralph-summary>
///   <ralph-implementation-steps>Numbered, actionable steps</ralph-implementation-steps>
///   <ralph-critical-files>List of key files (optional)</ralph-critical-files>
///   <ralph-risks-mitigations>Challenges and mitigations (optional)</ralph-risks-mitigations>
///   <ralph-verification-strategy>How to verify acceptance checks (optional)</ralph-verification-strategy>
/// </ralph-plan>
/// ```
///
/// # Arguments
///
/// * `xml_content` - The XML content to validate
///
/// # Returns
///
/// * `Ok(PlanElements)` if the XML is valid and contains all required elements
/// * `Err(XsdValidationError)` if the XML is invalid or doesn't conform to the schema
pub fn validate_plan_xml(xml_content: &str) -> Result<PlanElements, XsdValidationError> {
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

    // Check for <ralph-plan> root element
    if !content.starts_with("<ralph-plan>") {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-plan".to_string(),
            expected: "<ralph-plan> as root element".to_string(),
            found: if content.is_empty() {
                "empty content".to_string()
            } else if content.len() < 50 {
                content.to_string()
            } else {
                format!("{}...", &content[..50])
            },
            suggestion: "Wrap your plan in <ralph-plan> tags".to_string(),
        });
    }

    if !content.ends_with("</ralph-plan>") {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
            element_path: "ralph-plan".to_string(),
            expected: "closing </ralph-plan> tag".to_string(),
            found: "missing closing tag".to_string(),
            suggestion: "Add </ralph-plan> at the end of your plan".to_string(),
        });
    }

    // Extract content between root tags
    let root_start = "<ralph-plan>".len();
    let root_end = content.len() - "</ralph-plan>".len();
    let plan_content = &content[root_start..root_end];

    // Parse required and optional elements
    let mut summary = None;
    let mut implementation_steps = None;
    let mut critical_files = None;
    let mut risks_mitigations = None;
    let mut verification_strategy = None;

    // Parse elements in order
    let mut remaining = plan_content.trim();

    while !remaining.is_empty() {
        // Try to parse each element type
        if let Some(tag_content) = extract_tag_content(remaining, "ralph-summary") {
            if summary.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-summary".to_string(),
                    expected: "only one <ralph-summary> element".to_string(),
                    found: "duplicate <ralph-summary> element".to_string(),
                    suggestion: "Include only one <ralph-summary> element in your plan".to_string(),
                });
            }
            summary = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-summary");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-implementation-steps") {
            if implementation_steps.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-implementation-steps".to_string(),
                    expected: "only one <ralph-implementation-steps> element".to_string(),
                    found: "duplicate <ralph-implementation-steps> element".to_string(),
                    suggestion: "Include only one <ralph-implementation-steps> element".to_string(),
                });
            }
            implementation_steps = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-implementation-steps");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-critical-files") {
            if critical_files.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-critical-files".to_string(),
                    expected: "only one <ralph-critical-files> element".to_string(),
                    found: "duplicate <ralph-critical-files> element".to_string(),
                    suggestion: "Include only one <ralph-critical-files> element".to_string(),
                });
            }
            critical_files = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-critical-files");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-risks-mitigations") {
            if risks_mitigations.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-risks-mitigations".to_string(),
                    expected: "only one <ralph-risks-mitigations> element".to_string(),
                    found: "duplicate <ralph-risks-mitigations> element".to_string(),
                    suggestion: "Include only one <ralph-risks-mitigations> element".to_string(),
                });
            }
            risks_mitigations = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-risks-mitigations");
            continue;
        }

        if let Some(tag_content) = extract_tag_content(remaining, "ralph-verification-strategy") {
            if verification_strategy.is_some() {
                return Err(XsdValidationError {
                    error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::UnexpectedElement,
                    element_path: "ralph-verification-strategy".to_string(),
                    expected: "only one <ralph-verification-strategy> element".to_string(),
                    found: "duplicate <ralph-verification-strategy> element".to_string(),
                    suggestion: "Include only one <ralph-verification-strategy> element".to_string(),
                });
            }
            verification_strategy = Some(tag_content);
            remaining = advance_past_tag(remaining, "ralph-verification-strategy");
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
                    expected: "only valid plan tags".to_string(),
                    found: format!("unexpected tag: {potential_tag}"),
                    suggestion: "Remove the unexpected tag. Valid tags are: <ralph-summary>, <ralph-implementation-steps>, <ralph-critical-files>, <ralph-risks-mitigations>, <ralph-verification-strategy>".to_string(),
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
    let summary = summary.ok_or_else(|| XsdValidationError {
        error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
        element_path: "ralph-summary".to_string(),
        expected: "<ralph-summary> element (required)".to_string(),
        found: "no <ralph-summary> found".to_string(),
        suggestion: "Add <ralph-summary> with a one-paragraph explanation of what will be done and why".to_string(),
    })?;

    let implementation_steps = implementation_steps.ok_or_else(|| XsdValidationError {
        error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::MissingRequiredElement,
        element_path: "ralph-implementation-steps".to_string(),
        expected: "<ralph-implementation-steps> element (required)".to_string(),
        found: "no <ralph-implementation-steps> found".to_string(),
        suggestion: "Add <ralph-implementation-steps> with numbered, actionable steps for implementation".to_string(),
    })?;

    // Validate content is not empty
    let summary = summary.trim();
    if summary.is_empty() {
        return Err(XsdValidationError {
            error_type:
                crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "ralph-summary".to_string(),
            expected: "non-empty summary".to_string(),
            found: "empty summary".to_string(),
            suggestion: "The <ralph-summary> tag must contain a non-empty explanation of the plan"
                .to_string(),
        });
    }

    let implementation_steps = implementation_steps.trim();
    if implementation_steps.is_empty() {
        return Err(XsdValidationError {
            error_type: crate::files::llm_output_extraction::xsd_validation::XsdErrorType::InvalidContent,
            element_path: "ralph-implementation-steps".to_string(),
            expected: "non-empty implementation steps".to_string(),
            found: "empty implementation steps".to_string(),
            suggestion: "The <ralph-implementation-steps> tag must contain non-empty steps for implementation".to_string(),
        });
    }

    Ok(PlanElements {
        summary: summary.to_string(),
        implementation_steps: implementation_steps.to_string(),
        critical_files: critical_files
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        risks_mitigations: risks_mitigations
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        verification_strategy: verification_strategy
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

/// Parsed plan elements from valid XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanElements {
    /// The plan summary (required)
    pub summary: String,
    /// Implementation steps (required)
    pub implementation_steps: String,
    /// Critical files (optional)
    pub critical_files: Option<String>,
    /// Risks and mitigations (optional)
    pub risks_mitigations: Option<String>,
    /// Verification strategy (optional)
    pub verification_strategy: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_minimal_plan() {
        let xml = r#"<ralph-plan>
<ralph-summary>This is a summary</ralph-summary>
<ralph-implementation-steps>1. First step</ralph-implementation-steps>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.summary, "This is a summary");
        assert_eq!(elements.implementation_steps, "1. First step");
        assert!(elements.critical_files.is_none());
    }

    #[test]
    fn test_validate_valid_full_plan() {
        let xml = r#"<ralph-plan>
<ralph-summary>Summary of the plan</ralph-summary>
<ralph-implementation-steps>1. Step one
2. Step two</ralph-implementation-steps>
<ralph-critical-files>file1.rs, file2.rs</ralph-critical-files>
<ralph-risks-mitigations>Risk: something</ralph-risks-mitigations>
<ralph-verification-strategy>Run tests</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert_eq!(elements.summary, "Summary of the plan");
        assert!(elements.critical_files.is_some());
        assert!(elements.risks_mitigations.is_some());
        assert!(elements.verification_strategy.is_some());
    }

    #[test]
    fn test_validate_missing_root_element() {
        let xml = r#"Some random text without proper XML tags"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-plan");
    }

    #[test]
    fn test_validate_missing_summary() {
        let xml = r#"<ralph-plan>
<ralph-implementation-steps>1. First step</ralph-implementation-steps>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-summary");
    }

    #[test]
    fn test_validate_missing_implementation_steps() {
        let xml = r#"<ralph-plan>
<ralph-summary>Summary</ralph-summary>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.element_path, "ralph-implementation-steps");
    }

    #[test]
    fn test_validate_empty_summary() {
        let xml = r#"<ralph-plan>
<ralph-summary>   </ralph-summary>
<ralph-implementation-steps>1. Step</ralph-implementation-steps>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_duplicate_summary() {
        let xml = r#"<ralph-plan>
<ralph-summary>First summary</ralph-summary>
<ralph-summary>Second summary</ralph-summary>
<ralph-implementation-steps>1. Step</ralph-implementation-steps>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
    }
}
