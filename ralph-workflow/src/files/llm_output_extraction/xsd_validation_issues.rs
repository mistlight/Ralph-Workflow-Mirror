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

    // =========================================================================
    // REALISTIC LLM OUTPUT TESTS
    // These test actual patterns that LLMs produce when following the prompts
    // =========================================================================

    #[test]
    fn test_llm_realistic_issue_with_generic_type_escaped() {
        // LLM correctly escapes generic types per prompt instructions
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/parser.rs:42 - The function <code>parse&lt;T&gt;</code> does not handle empty input.
Suggested fix: Add a check for empty input before parsing.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok(), "Should parse escaped generic: {:?}", result);
        let elements = result.unwrap();
        assert!(elements.issues[0].contains("parse<T>"));
    }

    #[test]
    fn test_llm_realistic_issue_with_comparison_escaped() {
        // LLM correctly escapes comparison operators
        let xml = r#"<ralph-issues>
<ralph-issue>[Medium] src/validate.rs:15 - The condition <code>count &lt; 0</code> should be <code>count &lt;= 0</code>.
Suggested fix: Change the comparison operator.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(
            result.is_ok(),
            "Should parse escaped comparisons: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.issues[0].contains("count < 0"));
        assert!(elements.issues[0].contains("count <= 0"));
    }

    #[test]
    fn test_llm_realistic_issue_with_logical_operators_escaped() {
        // LLM escapes && and || operators
        let xml = r#"<ralph-issues>
<ralph-issue>[Low] src/filter.rs:88 - The expression <code>a &amp;&amp; b || c</code> has ambiguous precedence.
Suggested fix: Add explicit parentheses.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(
            result.is_ok(),
            "Should parse escaped logical operators: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.issues[0].contains("a && b || c"));
    }

    #[test]
    fn test_llm_realistic_issue_with_rust_lifetime() {
        // LLM references Rust lifetime syntax
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/buffer.rs:23 - The lifetime <code>&amp;'a str</code> should match the struct lifetime.
Suggested fix: Ensure lifetime annotations are consistent.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok(), "Should parse lifetime syntax: {:?}", result);
        let elements = result.unwrap();
        assert!(elements.issues[0].contains("&'a str"));
    }

    #[test]
    fn test_llm_realistic_issue_with_html_in_description() {
        // LLM describes HTML-related code
        let xml = r#"<ralph-issues>
<ralph-issue>[Medium] src/template.rs:56 - The HTML template uses <code>&lt;div class="container"&gt;</code> but should use semantic tags.
Suggested fix: Replace with appropriate semantic HTML elements.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok(), "Should parse HTML in code: {:?}", result);
        let elements = result.unwrap();
        assert!(elements.issues[0].contains("<div class=\"container\">"));
    }

    #[test]
    fn test_llm_realistic_no_issues_with_detailed_explanation() {
        // LLM provides detailed explanation when no issues found
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>The implementation correctly handles all edge cases:
- Input validation properly rejects values where <code>x &lt; 0</code>
- The generic <code>Result&lt;T, E&gt;</code> type is used consistently
- Error handling follows the project's established patterns
No issues require attention.</ralph-no-issues-found>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(
            result.is_ok(),
            "Should parse detailed no-issues: {:?}",
            result
        );
        let elements = result.unwrap();
        let msg = elements.no_issues_found.unwrap();
        assert!(msg.contains("x < 0"));
        assert!(msg.contains("Result<T, E>"));
    }

    #[test]
    fn test_llm_realistic_multiple_issues_with_mixed_content() {
        // LLM reports multiple issues with various escaped content
        let xml = r#"<ralph-issues>
<ralph-issue>[Critical] src/auth.rs:12 - SQL injection vulnerability: user input in <code>query &amp;&amp; filter</code> is not sanitized.</ralph-issue>
<ralph-issue>[High] src/api.rs:45 - Missing null check: <code>response.data</code> may be undefined when <code>status &lt; 200</code>.</ralph-issue>
<ralph-issue>[Medium] src/utils.rs:78 - The type <code>Option&lt;Vec&lt;T&gt;&gt;</code> could be simplified to <code>Vec&lt;T&gt;</code> with empty default.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(
            result.is_ok(),
            "Should parse multiple issues with mixed content: {:?}",
            result
        );
        let elements = result.unwrap();
        assert_eq!(elements.issues.len(), 3);
        assert!(elements.issues[0].contains("query && filter"));
        assert!(elements.issues[1].contains("status < 200"));
        assert!(elements.issues[2].contains("Option<Vec<T>>"));
    }

    #[test]
    fn test_llm_mistake_unescaped_less_than_fails() {
        // LLM forgets to escape < - this SHOULD fail
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/compare.rs:10 - The condition a < b is wrong.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(
            result.is_err(),
            "Unescaped < should fail XML parsing: {:?}",
            result
        );
    }

    #[test]
    fn test_llm_mistake_unescaped_generic_fails() {
        // LLM forgets to escape generic type - this SHOULD fail
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/types.rs:5 - The type Vec<String> is incorrect.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(
            result.is_err(),
            "Unescaped generic should fail XML parsing: {:?}",
            result
        );
    }

    #[test]
    fn test_llm_mistake_unescaped_ampersand_fails() {
        // LLM forgets to escape & - this SHOULD fail
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/logic.rs:20 - The expression a && b is wrong.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(
            result.is_err(),
            "Unescaped && should fail XML parsing: {:?}",
            result
        );
    }

    #[test]
    fn test_llm_uses_cdata_for_code_content() {
        // LLM uses CDATA instead of escaping (valid alternative)
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/cmp.rs:10 - The condition <code><![CDATA[a < b && c > d]]></code> has issues.</ralph-issue>
</ralph-issues>"#;

        let result = validate_issues_xml(xml);
        assert!(result.is_ok(), "CDATA should be valid: {:?}", result);
        let elements = result.unwrap();
        assert!(elements.issues[0].contains("a < b && c > d"));
    }
}
