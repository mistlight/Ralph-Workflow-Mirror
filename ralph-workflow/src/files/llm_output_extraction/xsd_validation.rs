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
    /// Optional simple body content
    pub body: Option<String>,
}

/// Detailed XSD validation error for reporting to AI agent.
///
/// This error type provides comprehensive information about what went wrong
/// during validation, making it suitable for generating retry prompts that
/// guide the AI agent toward producing valid output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XsdValidationError {
    /// The type of validation error that occurred
    pub error_type: XsdErrorType,
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
    pub fn format_for_ai_retry(&self) -> String {
        format!(
            "Validation failed for '{}': {}. {}",
            self.element_path,
            self.error_type.description(),
            self.suggestion
        )
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
