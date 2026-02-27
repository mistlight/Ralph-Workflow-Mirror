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

#[test]
fn test_format_for_ai_retry_illegal_character_emphasis() {
    // Create an error that represents an illegal character (NUL byte)
    let error = XsdValidationError {
        error_type: XsdErrorType::MalformedXml,
        element_path: "xml".to_string(),
        expected: "valid XML 1.0 content (no illegal control characters)".to_string(),
        found: "illegal character NUL (null byte) at byte position 42".to_string(),
        suggestion: "NUL byte found at position 42. Common causes:\n\
                         - Intended to use non-breaking space (\\u00A0) but wrote \\u0000 instead\n\
                         Near: ...git\0diff..."
            .to_string(),
        example: None,
    };

    let formatted = error.format_for_ai_retry();

    // Verify the formatted output emphasizes illegal character issue
    assert!(
        formatted.contains("ILLEGAL CHARACTER"),
        "Should emphasize illegal character in heading"
    );
    assert!(
        formatted.contains("CRITICAL") || formatted.contains("FIX REQUIRED"),
        "Should indicate critical fix required"
    );
    assert!(
        formatted.contains("NUL"),
        "Should identify the specific character"
    );
    assert!(
        formatted.contains("\\u00A0") || formatted.contains("non-breaking space"),
        "Should mention common NBSP typo"
    );
}

#[test]
fn test_format_for_ai_retry_illegal_character_includes_fix_marker() {
    let error = XsdValidationError {
        error_type: XsdErrorType::MalformedXml,
        element_path: "xml".to_string(),
        expected: "valid XML 1.0 content (no illegal control characters)".to_string(),
        found: "illegal character NUL (null byte) at byte position 42".to_string(),
        suggestion: "NUL byte found at position 42. Common causes:\n\
                         - Intended to use non-breaking space (\\u00A0) but wrote \\u0000 instead\n\
                         Near: ...git\0diff..."
            .to_string(),
        example: None,
    };

    let formatted = error.format_for_ai_retry();

    assert!(
        formatted.contains("How to fix") || formatted.contains("Fix:"),
        "Illegal character errors should include a fix marker, got:\n{formatted}"
    );
}

#[test]
fn test_format_for_ai_retry_generic_malformed_xml() {
    // Create a generic malformed XML error (not illegal character)
    let error = XsdValidationError {
        error_type: XsdErrorType::MalformedXml,
        element_path: "ralph-issues".to_string(),
        expected: "well-formed XML".to_string(),
        found: "parse error: unclosed tag".to_string(),
        suggestion: "Ensure all tags are properly closed".to_string(),
        example: None,
    };

    let formatted = error.format_for_ai_retry();

    // Verify this uses standard formatting (not the enhanced illegal character formatting)
    assert!(
        formatted.contains("MALFORMED XML"),
        "Should use standard malformed XML heading"
    );
    assert!(
        !formatted.contains("ILLEGAL CHARACTER"),
        "Should NOT use illegal character emphasis for generic errors"
    );
}
