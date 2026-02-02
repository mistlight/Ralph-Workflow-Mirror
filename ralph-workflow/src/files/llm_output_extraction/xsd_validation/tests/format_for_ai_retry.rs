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

