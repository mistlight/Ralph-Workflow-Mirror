//! Integration tests for review agent XML validation.
//!
//! This module tests the XML extraction and XSD validation behavior for
//! the review agent's output (issues list).
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All integration tests MUST follow the style guide defined in
//! **[INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Before writing, modifying, or debugging any integration test, you MUST read
//! that document. Key principles:
//!
//! - Test **observable behavior**, not implementation details
//! - Mock only at **architectural boundaries** (filesystem, network, external APIs)
//! - Use `with_default_timeout()` wrapper for all tests
//! - NEVER use `cfg!(test)` branches in production code

use crate::test_timeout::with_default_timeout;

/// Test that valid issues XML passes validation.
///
/// This verifies that when the review agent produces valid XML with
/// issues, the validation succeeds and extracts the issue list.
#[test]
fn test_review_xml_valid_issues() {
    with_default_timeout(|| {
        // Setup: Create valid XML with issues (simple text descriptions)
        let xml = r#"<ralph-issues>
<ralph-issue>Variable unused in src/main.rs</ralph-issue>
<ralph-issue>Missing error handling in src/utils.rs</ralph-issue>
</ralph-issues>"#;

        // Execute: Validate the XML through the public API
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify OBSERVABLE behavior (validation passes)
        assert!(result.is_ok(), "Valid XML should pass validation");

        let elements = result.unwrap();
        assert_eq!(elements.issues.len(), 2, "Should extract 2 issues");
        assert!(
            elements.no_issues_found.is_none(),
            "Should not have no_issues_found when issues exist"
        );

        // Verify first issue content
        assert_eq!(
            elements.issues[0], "Variable unused in src/main.rs",
            "Should extract first issue"
        );

        // Verify second issue content
        assert_eq!(
            elements.issues[1], "Missing error handling in src/utils.rs",
            "Should extract second issue"
        );
    });
}

/// Test that valid no_issues_found XML passes validation.
///
/// This verifies that when the review agent produces valid XML with
/// no_issues_found element, the validation succeeds and identifies
/// that no issues were found.
#[test]
fn test_review_xml_valid_no_issues_found() {
    with_default_timeout(|| {
        // Setup: Create valid XML with no_issues_found
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>No issues found during review</ralph-no-issues-found>
</ralph-issues>"#;

        // Execute: Validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation passes
        assert!(
            result.is_ok(),
            "Valid no_issues_found XML should pass validation"
        );

        let elements = result.unwrap();
        assert!(elements.issues.is_empty(), "Should have no issues");
        assert_eq!(
            elements.no_issues_found,
            Some("No issues found during review".to_string()),
            "Should extract no_issues_found message"
        );
    });
}

/// Test that invalid XML format produces specific XSD validation error.
///
/// This verifies that when the review agent produces XML that fails
/// XSD validation, a specific error message is produced that can be fed
/// back to the agent for retry.
#[test]
fn test_review_xml_missing_root_element_provides_specific_error() {
    with_default_timeout(|| {
        // Setup: Create content without proper XML tags
        let content = r#"Some random text without proper XML tags"#;

        // Execute: Try to validate the content
        let result = ralph_workflow::validate_issues_xml(content);

        // Assert: Verify validation fails with specific error
        assert!(
            result.is_err(),
            "Missing root element should fail validation"
        );

        let error = result.unwrap_err();
        assert_eq!(
            error.element_path, "ralph-issues",
            "Error should identify missing root element"
        );
        assert!(
            error.expected.contains("ralph-issues"),
            "Error should indicate expected root element"
        );
        assert!(
            error.suggestion.contains("ralph-issues"),
            "Error should provide actionable suggestion"
        );
    });
}

/// Test that empty issues list produces specific error.
///
/// This verifies that when the review agent produces an issues element
/// with no actual issues and no no_issues_found element, validation fails.
#[test]
fn test_review_xml_empty_issues_list_produces_error() {
    with_default_timeout(|| {
        // Setup: Create XML with empty issues list (no issues, no no_issues_found)
        let xml = r#"<ralph-issues>
</ralph-issues>"#;

        // Execute: Try to validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation fails
        assert!(result.is_err(), "Empty issues list should fail validation");

        let error = result.unwrap_err();
        // Should indicate that either issues or no-issues-found is expected
        assert!(
            error.expected.contains("ralph-issue") || error.expected.contains("at least"),
            "Error should indicate what's expected, got: {}",
            error.expected
        );
    });
}

/// Test that XML extraction works from markdown code fence wrapped content.
///
/// This verifies that issues XML can be extracted even when wrapped
/// in markdown code fences, which is a common AI output pattern.
#[test]
fn test_review_xml_extraction_from_markdown_fence() {
    with_default_timeout(|| {
        // Setup: Create content with XML wrapped in markdown fence
        let content = r#"Here's my review:

```xml
<ralph-issues>
<ralph-issue>Variable unused</ralph-issue>
</ralph-issues>
```

That's all."#;

        // Execute: Extract XML from the content
        let extracted = ralph_workflow::extract_issues_xml(content);

        // Assert: Verify XML is extracted and validates
        assert!(
            extracted.is_some(),
            "Should extract XML from markdown fence"
        );

        let xml = extracted.unwrap();
        let result = ralph_workflow::validate_issues_xml(&xml);
        assert!(result.is_ok(), "Extracted XML should validate");
    });
}

/// Test that XML extraction works from JSON string escaped content.
///
/// This verifies that issues XML can be extracted even when
/// JSON-escaped as a string, which can happen in some output formats.
#[test]
fn test_review_xml_extraction_from_json_string() {
    with_default_timeout(|| {
        // Setup: Create JSON with escaped XML string
        let content = r#"{"type":"result","result":"<ralph-issues>\n<ralph-issue>Variable unused<\/ralph-issue>\n<\/ralph-issues>"}"#;

        // Execute: Extract XML from the JSON content
        let extracted = ralph_workflow::extract_issues_xml(content);

        // Assert: Verify XML is extracted and validates
        assert!(extracted.is_some(), "Should extract XML from JSON string");

        let xml = extracted.unwrap();
        let result = ralph_workflow::validate_issues_xml(&xml);
        assert!(result.is_ok(), "Extracted XML should validate");
    });
}

/// Test that XML is formatted nicely for display.
///
/// This verifies that valid XML is formatted in a user-friendly way
/// rather than displayed as raw XML.
#[test]
fn test_review_xml_formatted_for_display() {
    with_default_timeout(|| {
        // Setup: Create valid XML
        let xml = r#"<ralph-issues>
<ralph-issue>Variable unused in src/main.rs</ralph-issue>
</ralph-issues>"#;

        // Execute: Format the XML for display
        let formatted = ralph_workflow::format_xml_for_display(xml);

        // Assert: Verify output is formatted nicely (pretty-printed XML)
        assert!(
            formatted.contains("Variable unused"),
            "Should include issue description"
        );
        assert!(
            formatted.contains("src/main.rs"),
            "Should include file reference"
        );
        // format_xml_for_display returns pretty-printed XML (with indentation)
        assert!(
            formatted.contains("<ralph-"),
            "Should include XML tags (pretty-printed format)"
        );
    });
}

/// Test that multiple issues are all extracted.
///
/// This verifies that when the review agent produces multiple issues,
/// all of them are extracted and validated.
#[test]
fn test_review_xml_multiple_issues_all_extracted() {
    with_default_timeout(|| {
        // Setup: Create XML with multiple issues
        let xml = r#"<ralph-issues>
<ralph-issue>Error 1</ralph-issue>
<ralph-issue>Warning 1</ralph-issue>
<ralph-issue>Info 1</ralph-issue>
<ralph-issue>Note 1</ralph-issue>
</ralph-issues>"#;

        // Execute: Validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify all issues are extracted
        assert!(
            result.is_ok(),
            "Valid XML with multiple issues should pass validation"
        );

        let elements = result.unwrap();
        assert_eq!(elements.issues.len(), 4, "Should extract all 4 issues");

        // Verify each issue was extracted correctly
        assert_eq!(elements.issues[0], "Error 1");
        assert_eq!(elements.issues[1], "Warning 1");
        assert_eq!(elements.issues[2], "Info 1");
        assert_eq!(elements.issues[3], "Note 1");
    });
}

/// Test that issues and no_issues_found cannot coexist.
///
/// This verifies that when both issues and no_issues_found are present,
/// validation produces an appropriate error.
#[test]
fn test_review_xml_issues_and_no_issues_found_cannot_coexist() {
    with_default_timeout(|| {
        // Setup: Create XML with both issues and no_issues_found
        let xml = r#"<ralph-issues>
<ralph-issue>Some issue</ralph-issue>
<ralph-no-issues-found>No issues found</ralph-no-issues-found>
</ralph-issues>"#;

        // Execute: Try to validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation fails
        assert!(
            result.is_err(),
            "Should not allow both issues and no_issues_found"
        );

        let error = result.unwrap_err();
        assert!(
            error.expected.contains("either") || error.expected.contains("OR"),
            "Error should indicate mutual exclusivity"
        );
    });
}

/// Test that duplicate no_issues_found produces specific error.
///
/// This verifies that when the review agent includes multiple no_issues_found
/// elements, validation produces an appropriate error.
#[test]
fn test_review_xml_duplicate_no_issues_found_produces_error() {
    with_default_timeout(|| {
        // Setup: Create XML with duplicate no_issues_found
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>No issues</ralph-no-issues-found>
<ralph-no-issues-found>Also no issues</ralph-no-issues-found>
</ralph-issues>"#;

        // Execute: Try to validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation fails
        assert!(
            result.is_err(),
            "Duplicate no_issues_found should fail validation"
        );

        let error = result.unwrap_err();
        assert_eq!(
            error.element_path, "ralph-issues/ralph-no-issues-found",
            "Error should identify the duplicated element"
        );
        assert!(
            error.found.contains("duplicate") || error.expected.contains("only one"),
            "Error should indicate this is a duplicate"
        );
    });
}

/// Test that unexpected elements produce specific error.
///
/// This verifies that when the review agent includes unknown elements,
/// a specific error identifies the problem and lists valid options.
#[test]
fn test_review_xml_unexpected_element_provides_valid_options() {
    with_default_timeout(|| {
        // Setup: Create XML with unexpected element
        let xml = r#"<ralph-issues>
<ralph-unknown-field>Some value</ralph-unknown-field>
</ralph-issues>"#;

        // Execute: Try to validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation fails with specific error about valid tags
        assert!(result.is_err(), "Unexpected element should fail validation");

        let error = result.unwrap_err();
        assert!(
            error.element_path.contains("ralph-unknown-field"),
            "Error should identify the unexpected element"
        );
        assert!(
            error.suggestion.contains("ralph-issue")
                && error.suggestion.contains("ralph-no-issues-found"),
            "Error should list valid element names"
        );
    });
}

/// Test that missing closing tag produces specific error.
///
/// This verifies that when the review agent's XML is missing the closing tag,
/// a specific error identifies the problem.
#[test]
fn test_review_xml_missing_closing_tag_produces_error() {
    with_default_timeout(|| {
        // Setup: Create XML without closing tag
        let xml = r#"<ralph-issues>
<ralph-issue>Some issue</ralph-issue>
"#;

        // Execute: Try to validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation fails
        assert!(
            result.is_err(),
            "Missing closing tag should fail validation"
        );

        let error = result.unwrap_err();
        assert_eq!(
            error.element_path, "ralph-issues",
            "Error should identify root element"
        );
        assert!(
            error.expected.contains("closing") && error.expected.contains("</ralph-issues>"),
            "Error should indicate missing closing tag"
        );
    });
}

/// Test XSD validation error messages include all required information.
///
/// This verifies that XSD validation errors contain the information needed
/// to provide useful feedback to the AI agent for retry.
#[test]
fn test_review_xsd_error_contains_all_required_information() {
    with_default_timeout(|| {
        // Setup: Create invalid XML (missing root element)
        let xml = r#"Random text without proper XML"#;

        // Execute: Try to validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify error contains all required fields
        assert!(result.is_err(), "Invalid XML should fail validation");

        let error = result.unwrap_err();

        // Verify error has element_path (identifies where the error is)
        assert!(
            !error.element_path.is_empty(),
            "Error should have element_path"
        );

        // Verify error has expected (what was expected)
        assert!(
            !error.expected.is_empty(),
            "Error should have expected field"
        );

        // Verify error has found (what was actually found)
        assert!(!error.found.is_empty(), "Error should have found field");

        // Verify error has suggestion (how to fix it)
        assert!(!error.suggestion.is_empty(), "Error should have suggestion");

        // Verify format_for_ai_retry produces a complete message
        let formatted = error.format_for_ai_retry();
        assert!(
            formatted.contains(&error.element_path),
            "Formatted error should include element_path"
        );
        assert!(
            formatted.contains(&error.expected),
            "Formatted error should include expected"
        );
        assert!(
            formatted.contains(&error.found),
            "Formatted error should include found"
        );
    });
}

/// Test that whitespace-only issues are filtered.
///
/// This verifies that when the review agent includes issues with only
/// whitespace, they are filtered out.
#[test]
fn test_review_xml_whitespace_only_issues_are_filtered() {
    with_default_timeout(|| {
        // Setup: Create XML with whitespace-only issues
        let xml = r#"<ralph-issues>
<ralph-issue>   </ralph-issue>
<ralph-issue>Actual issue</ralph-issue>
<ralph-issue>  </ralph-issue>
</ralph-issues>"#;

        // Execute: Validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation passes and whitespace issues are filtered
        assert!(
            result.is_ok(),
            "Whitespace filtering should still pass validation"
        );

        let elements = result.unwrap();
        assert_eq!(elements.issues.len(), 1, "Should only have non-empty issue");
        assert_eq!(
            elements.issues[0], "Actual issue",
            "Should keep actual issue"
        );
    });
}

/// Test that whitespace-only no_issues_found is filtered.
///
/// This verifies that when the review agent includes no_issues_found with only
/// whitespace, it's treated as missing and validation fails (since we need either
/// issues or no_issues_found).
#[test]
fn test_review_xml_whitespace_only_no_issues_found_is_filtered() {
    with_default_timeout(|| {
        // Setup: Create XML with whitespace-only no_issues_found
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>   </ralph-no-issues-found>
</ralph-issues>"#;

        // Execute: Try to validate the XML
        let result = ralph_workflow::validate_issues_xml(xml);

        // Assert: Verify validation fails (whitespace is filtered to None, leaving empty issues)
        assert!(
            result.is_err(),
            "Whitespace-only no_issues_found should be filtered and fail validation"
        );
    });
}
