// Tests for XSD validation module.

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
            example: None,
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("MISSING REQUIRED ELEMENT"));
        assert!(formatted.contains("'ralph-subject' is required"));
        assert!(formatted.contains("Add <ralph-subject>"));
    }

    #[test]
    fn test_format_for_ai_retry_with_example() {
        let error = XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-subject".to_string(),
            expected: "<ralph-subject> element (required)".to_string(),
            found: "no <ralph-subject> found".to_string(),
            suggestion: "Add the required element".to_string(),
            example: Some(
                "<ralph-commit><ralph-subject>feat: example</ralph-subject></ralph-commit>".into(),
            ),
        };

        let formatted = error.format_for_ai_retry();
        assert!(formatted.contains("Example of correct format:"));
        assert!(formatted.contains("feat: example"));
    }

    #[test]
    fn test_format_for_ai_retry_unexpected_element() {
        let error = XsdValidationError {
            error_type: XsdErrorType::UnexpectedElement,
            element_path: "<unknown-tag>".to_string(),
            expected: "only valid commit message tags".to_string(),
            found: "unexpected tag: <unknown-tag>".to_string(),
            suggestion: "Remove the <unknown-tag> tag".to_string(),
            example: None,
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
            example: None,
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
            example: None,
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
        assert!(matches!(error.error_type, XsdErrorType::MalformedXml));
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
        assert!(error.element_path.contains("ralph-subject"));
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
        assert!(error.suggestion.contains("<ralph-body>"));
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

    #[test]
    fn test_validate_with_escaped_newlines_in_content() {
        // This tests that quick_xml properly handles whitespace between elements
        // which was the original issue with literal \n characters
        let xml = "<ralph-commit>\n<ralph-subject>feat: test</ralph-subject>\n</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
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

    // ============================================================================
    // Tests for <code> element support
    // ============================================================================

    #[test]
    fn test_validate_subject_with_code_element() {
        // XSD allows <code> elements for escaping special characters
        let xml = r#"<ralph-commit>
<ralph-subject>fix: handle <code>a &lt; b</code> comparison</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        // Text from both outside and inside <code> should be collected
        assert!(elements.subject.contains("fix: handle"));
        assert!(elements.subject.contains("a < b"));
        assert!(elements.subject.contains("comparison"));
    }

    #[test]
    fn test_validate_body_with_code_element() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add generic support</ralph-subject>
<ralph-body>Added <code>HashMap&lt;K, V&gt;</code> support to the parser.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("HashMap<K, V>"));
    }

    #[test]
    fn test_validate_detailed_body_with_code_elements() {
        let xml = r#"<ralph-commit>
<ralph-subject>refactor: improve type handling</ralph-subject>
<ralph-body-summary>Refactored <code>Option&lt;T&gt;</code> handling</ralph-body-summary>
<ralph-body-details>Changed <code>if a &lt; b</code> to <code>if a &gt; b</code></ralph-body-details>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(result.is_ok());
        let elements = result.unwrap();
        assert!(elements.body_summary.unwrap().contains("Option<T>"));
        let details = elements.body_details.unwrap();
        assert!(details.contains("if a < b"));
        assert!(details.contains("if a > b"));
    }

    // =========================================================================
    // REALISTIC LLM OUTPUT TESTS FOR COMMIT MESSAGES
    // These test actual patterns that LLMs produce when following the prompts
    // =========================================================================

    #[test]
    fn test_llm_commit_with_generic_type_in_subject() {
        // LLM correctly escapes generic type in commit subject
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add <code>Result&lt;T, E&gt;</code> support</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped generic should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.subject.contains("Result<T, E>"));
    }

    #[test]
    fn test_llm_commit_with_comparison_in_body() {
        // LLM correctly escapes comparison in commit body
        let xml = r#"<ralph-commit>
<ralph-subject>fix: correct boundary check</ralph-subject>
<ralph-body>The condition <code>count &lt; 0</code> was incorrect. Changed to <code>count &lt;= 0</code> to handle zero case.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped comparisons should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("count < 0"));
        assert!(body.contains("count <= 0"));
    }

    #[test]
    fn test_llm_commit_with_logical_operators_in_body() {
        // LLM correctly escapes logical operators
        let xml = r#"<ralph-commit>
<ralph-subject>refactor: simplify condition</ralph-subject>
<ralph-body>Simplified <code>a &amp;&amp; b || c</code> to use helper function.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped logical operators should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.body.unwrap().contains("a && b || c"));
    }

    #[test]
    fn test_llm_commit_with_detailed_body_describing_code() {
        // LLM uses detailed body format with code references
        let xml = r#"<ralph-commit>
<ralph-subject>feat(parser): add generic parsing</ralph-subject>
<ralph-body-summary>Added generic <code>parse&lt;T&gt;</code> function.</ralph-body-summary>
<ralph-body-details>- Supports any type implementing <code>FromStr</code>
- Returns <code>Result&lt;T, ParseError&gt;</code>
- Handles cases where <code>input.len() &gt; 0</code></ralph-body-details>
<ralph-body-footer>Closes #456</ralph-body-footer>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with detailed body should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.body_summary.unwrap().contains("parse<T>"));
        let details = elements.body_details.unwrap();
        assert!(details.contains("Result<T, ParseError>"));
        assert!(details.contains("input.len() > 0"));
    }

    #[test]
    fn test_llm_commit_with_html_reference_in_body() {
        // LLM describes HTML-related changes
        let xml = r#"<ralph-commit>
<ralph-subject>fix(ui): correct template rendering</ralph-subject>
<ralph-body>Fixed the <code>&lt;div class="container"&gt;</code> element that was not rendering correctly when <code>count &gt; 10</code>.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with HTML reference should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("<div class=\"container\">"));
        assert!(body.contains("count > 10"));
    }

    #[test]
    fn test_llm_mistake_unescaped_generic_in_subject_fails() {
        // LLM forgets to escape generic in subject - this SHOULD fail
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add Vec<String> support</ralph-subject>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_err(),
            "Unescaped generic in subject should fail: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_llm_mistake_unescaped_comparison_in_body_fails() {
        // LLM forgets to escape comparison - this SHOULD fail
        let xml = r#"<ralph-commit>
<ralph-subject>fix: correct comparison</ralph-subject>
<ralph-body>Changed a < b to a <= b.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_err(),
            "Unescaped comparison in body should fail: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_llm_uses_cdata_in_body() {
        // LLM uses CDATA for complex code reference (valid alternative)
        let xml = r#"<ralph-commit>
<ralph-subject>fix: handle edge case</ralph-subject>
<ralph-body>Fixed the case where <code><![CDATA[a < b && c > d]]></code> fails.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "CDATA in body should be valid: {:?}",
            result
        );
        let elements = result.unwrap();
        assert!(elements.body.unwrap().contains("a < b && c > d"));
    }

    #[test]
    fn test_llm_commit_realistic_refactor_message() {
        // A realistic refactor commit message an LLM might produce
        let xml = r#"<ralph-commit>
<ralph-subject>refactor(api): extract validation logic</ralph-subject>
<ralph-body>Extracted the validation logic from <code>handle_request&lt;T&gt;</code> into a separate
<code>validate&lt;T: Validate&gt;</code> function. This improves testability and allows
reuse across endpoints that check <code>input.size() &lt; MAX_SIZE</code>.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Realistic refactor commit should parse: {:?}",
            result
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("handle_request<T>"));
        assert!(body.contains("validate<T: Validate>"));
        assert!(body.contains("input.size() < MAX_SIZE"));
    }
}
