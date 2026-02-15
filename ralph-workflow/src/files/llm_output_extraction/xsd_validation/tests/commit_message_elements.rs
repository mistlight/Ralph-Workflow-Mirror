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
        skip_reason: None,
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
        skip_reason: None,
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
        skip_reason: None,
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
