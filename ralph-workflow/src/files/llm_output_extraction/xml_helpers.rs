//! Shared quick_xml helper utilities for XSD validation.
//!
//! This module provides common parsing functions used across all XSD validators
//! to ensure consistent XML handling with proper whitespace management.
//!
//! # Illegal Character Validation
//!
//! All XML content must be validated for illegal XML 1.0 characters
//! BEFORE quick_xml parsing. This ensures clear, actionable error messages
//! rather than cryptic parser errors.
//!
//! Validation flow:
//! 1. check_for_illegal_xml_characters() - scans for illegal chars
//! 2. create_reader() - creates quick_xml reader
//! 3. XSD validation - validates structure and content
//!
//! # Code Block Content
//!
//! Code blocks containing special characters (`<`, `>`, `&`) MUST use CDATA sections:
//!
//! ```xml
//! <code-block language="rust"><![CDATA[
//! if a < b && c > d {
//!     println!("hello");
//! }
//! ]]></code-block>
//! ```
//!
//! The parser handles CDATA correctly - content is preserved exactly as written.

use crate::common::truncate_text;
use crate::files::llm_output_extraction::xsd_validation::{XsdErrorType, XsdValidationError};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Check for illegal XML 1.0 characters in content.
///
/// XML 1.0 specification allows only specific characters:
/// - Tab (0x09), Line Feed (0x0A), Carriage Return (0x0D)
/// - Characters from 0x20 to 0xD7FF
/// - Characters from 0xE000 to 0xFFFD
/// - Characters from 0x10000 to 0x10FFFF
///
/// This function detects illegal characters and returns a detailed error
/// with position, context, and actionable suggestions.
pub fn check_for_illegal_xml_characters(content: &str) -> Result<(), XsdValidationError> {
    for (byte_index, ch) in content.char_indices() {
        let is_illegal = match ch as u32 {
            0x00 => true,            // NUL byte
            0x01..=0x08 => true,     // Control characters
            0x0B | 0x0C => true,     // Vertical tab, form feed
            0x0E..=0x1F => true,     // Other control characters
            0xD800..=0xDFFF => true, // UTF-16 surrogates
            0xFFFE | 0xFFFF => true, // Non-characters
            _ => false,
        };
        if is_illegal {
            return Err(illegal_character_error(ch, byte_index, content));
        }
    }
    Ok(())
}

/// Create a detailed error message for an illegal XML character.
///
/// The error includes:
/// - Character description (e.g., "NUL (null byte)" or "control character 0x0B")
/// - Position in the content
/// - Surrounding context (up to 100 characters)
/// - Actionable suggestion for fixing
fn illegal_character_error(ch: char, byte_index: usize, content: &str) -> XsdValidationError {
    let char_display = match ch {
        '\0' => "NUL (null byte)".to_string(),
        '\u{0001}'..='\u{001F}' => format!("control character 0x{:02X}", ch as u32),
        _ => format!("0x{:04X}", ch as u32),
    };

    // Extract context around the error position (50 chars before, 50 chars after)
    let context_start = byte_index.saturating_sub(50);
    let context_end = (byte_index + 50).min(content.len());
    let context = &content[context_start..context_end];
    let preview = truncate_text(context, 100);

    // Provide specific suggestions based on character type
    let suggestion = if ch == '\0' {
        format!(
            "NUL byte found at position {}. Common causes:\n\
             - Intended to use non-breaking space (\\u00A0) but wrote \\u0000 instead\n\
             - Binary data mixed into text content\n\
             - Incorrect escape sequence\n\n\
             Near: {}",
            byte_index, preview
        )
    } else {
        format!(
            "Illegal character {} found at position {}. Options to fix:\n\
             - Remove the illegal character\n\
             - Use CDATA section if this is code: <![CDATA[your content]]>\n\
             - Replace with a valid character\n\n\
             Near: {}",
            char_display, byte_index, preview
        )
    };

    XsdValidationError {
        error_type: XsdErrorType::MalformedXml,
        element_path: "xml".to_string(),
        expected: "valid XML 1.0 content (no illegal control characters)".to_string(),
        found: format!(
            "illegal character {} at byte position {}",
            char_display, byte_index
        ),
        suggestion,
        example: None,
    }
}

/// Create a configured quick_xml reader with whitespace trimming enabled.
///
/// The reader is configured with `trim_text(true)` which automatically
/// handles whitespace between XML elements - solving the spacing issues
/// that caused validation failures with manual string parsing.
pub fn create_reader(content: &str) -> Reader<&[u8]> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);
    reader
}

/// Read text content until the closing tag, trimming whitespace.
///
/// This handles XML text nodes properly, including entity unescaping
/// (e.g., `&lt;` -> `<`, `&amp;` -> `&`) and CDATA sections.
pub fn read_text_until_end(
    reader: &mut Reader<&[u8]>,
    end_tag: &[u8],
) -> Result<String, XsdValidationError> {
    let mut buf = Vec::new();
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                text.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::CData(e)) => {
                // CDATA content is preserved exactly as-is
                text.push_str(&String::from_utf8_lossy(&e));
            }
            Ok(Event::End(e)) if e.name().as_ref() == end_tag => {
                break;
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: format!("closing </{}>", String::from_utf8_lossy(end_tag)),
                    found: "unexpected end of file".to_string(),
                    suggestion: format!(
                        "Ensure the <{}> element has a matching closing tag.",
                        String::from_utf8_lossy(end_tag)
                    ),
                    example: None,
                });
            }
            Ok(_) => {} // Skip comments, processing instructions, nested elements
            Err(e) => {
                return Err(make_parse_error(end_tag, e));
            }
        }
        buf.clear();
    }

    Ok(text.trim().to_string())
}

/// Skip all content until the closing tag of the current element.
///
/// This properly handles nested elements with the same tag name by tracking depth.
pub fn skip_to_end(reader: &mut Reader<&[u8]>, end_tag: &[u8]) -> Result<(), XsdValidationError> {
    let mut buf = Vec::new();
    let mut depth = 1;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == end_tag => {
                depth += 1;
            }
            Ok(Event::End(e)) if e.name().as_ref() == end_tag => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: format!("closing </{}>", String::from_utf8_lossy(end_tag)),
                    found: "unexpected end of file".to_string(),
                    suggestion: "Check that all XML elements are properly closed.".to_string(),
                    example: None,
                });
            }
            Ok(_) => {}
            Err(e) => {
                return Err(make_parse_error(end_tag, e));
            }
        }
        buf.clear();
    }

    Ok(())
}

/// Create an error for unexpected element.
pub fn unexpected_element_error(
    found_tag: &[u8],
    valid_tags: &[&str],
    parent_element: &str,
) -> XsdValidationError {
    let found_name = String::from_utf8_lossy(found_tag);
    let valid_list = valid_tags.join(", ");

    XsdValidationError {
        error_type: XsdErrorType::UnexpectedElement,
        element_path: format!("{}/{}", parent_element, found_name),
        expected: format!("one of: {}", valid_list),
        found: format!("<{}>", found_name),
        suggestion: format!(
            "Remove <{}> or replace with a valid element. Valid elements inside <{}>: {}",
            found_name, parent_element, valid_list
        ),
        example: None,
    }
}

/// Create an error for missing required element.
pub fn missing_required_error(
    element_name: &str,
    parent_element: &str,
    example: Option<&str>,
) -> XsdValidationError {
    XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: format!("{}/{}", parent_element, element_name),
        expected: format!("<{}> element (required)", element_name),
        found: format!("no <{}> found", element_name),
        suggestion: format!(
            "Add <{0}>value</{0}> inside <{1}>.",
            element_name, parent_element
        ),
        example: example.map(|s| s.into()),
    }
}

/// Create an error for duplicate element.
pub fn duplicate_element_error(element_name: &str, parent_element: &str) -> XsdValidationError {
    XsdValidationError {
        error_type: XsdErrorType::UnexpectedElement,
        element_path: format!("{}/{}", parent_element, element_name),
        expected: format!("only one <{}> element", element_name),
        found: format!("duplicate <{}> element", element_name),
        suggestion: format!(
            "Remove the duplicate <{}>. Only one is allowed.",
            element_name
        ),
        example: None,
    }
}

/// Create an error for text content outside of tags.
pub fn text_outside_tags_error(text: &str, parent_element: &str) -> XsdValidationError {
    // Use truncate_text for UTF-8 safe truncation (53 chars = ~50 visible + "...")
    let display_text = truncate_text(text, 53);

    XsdValidationError {
        error_type: XsdErrorType::InvalidContent,
        element_path: parent_element.to_string(),
        expected: "only XML elements (no loose text)".to_string(),
        found: format!("text content: {:?}", display_text),
        suggestion: "Remove any text that is not inside a child element tag.".to_string(),
        example: None,
    }
}

/// Format content for display in error messages (UTF-8 safe truncation).
///
/// Returns "empty content" for empty strings, the full content if <= 60 chars,
/// or a truncated preview with "..." for longer content.
pub fn format_content_preview(content: &str) -> String {
    if content.is_empty() {
        "empty content".to_string()
    } else {
        // truncate_text handles the ellipsis and UTF-8 char boundaries
        truncate_text(content, 63)
    }
}

/// Create an error for malformed XML.
pub fn malformed_xml_error(error: quick_xml::Error) -> XsdValidationError {
    let error_str = error.to_string();

    // Check if this is likely an illegal character issue (even though we pre-validate)
    // This can catch cases where quick_xml detects invalid UTF-8 or other encoding issues
    let suggestion =
        if error_str.contains("invalid character") || error_str.contains("Invalid character") {
            "Invalid character detected in XML content. Common causes:\n\
         - Illegal control characters (NUL, etc.) in text\n\
         - Invalid UTF-8 encoding\n\
         - Use CDATA for code blocks with special characters: <![CDATA[your code]]>"
                .to_string()
        } else if error_str.contains("code") || error_str.contains("block") {
            "Code blocks with special characters (<, >, &) MUST use CDATA sections:\n\
         <code-block><![CDATA[\n\
           if a < b && c > d { ... }\n\
         ]]></code-block>"
                .to_string()
        } else {
            "Check that all XML tags are properly opened and closed. \
         For code with special characters, use CDATA: <![CDATA[your code]]>"
                .to_string()
        };

    XsdValidationError {
        error_type: XsdErrorType::MalformedXml,
        element_path: "xml".to_string(),
        expected: "well-formed XML".to_string(),
        found: format!("parse error: {}", error),
        suggestion,
        example: None,
    }
}

/// Create a parse error with CDATA suggestion if the element is code-related.
fn make_parse_error(element: &[u8], error: quick_xml::Error) -> XsdValidationError {
    let element_name = String::from_utf8_lossy(element);
    let error_str = error.to_string();

    // Check if this is a code element - suggest CDATA
    let is_code_element = element_name.contains("code");
    let suggestion = if is_code_element {
        format!(
            "The <{}> element contains characters that break XML parsing. \
             Use CDATA to wrap code content:\n\
             <{}><![CDATA[\n  your code with <, >, & here\n]]></{}>",
            element_name, element_name, element_name
        )
    } else {
        "Check that all XML tags are properly formed and closed.".to_string()
    };

    XsdValidationError {
        error_type: XsdErrorType::MalformedXml,
        element_path: element_name.to_string(),
        expected: "valid XML content".to_string(),
        found: format!("parse error: {}", error_str),
        suggestion,
        example: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_reader() {
        // Just test that reader is created and can parse XML
        let mut reader = create_reader("<root>test</root>");
        let mut buf = Vec::new();
        // Reader should parse successfully
        let event = reader.read_event_into(&mut buf);
        assert!(event.is_ok());
    }

    #[test]
    fn test_read_text_until_end_simple() {
        let xml = "<root>hello world</root>";
        let mut reader = create_reader(xml);
        let mut buf = Vec::new();

        // Skip the opening tag
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) if e.name().as_ref() == b"root" => break,
                Ok(Event::Eof) => panic!("Unexpected EOF"),
                _ => {}
            }
            buf.clear();
        }

        let result = read_text_until_end(&mut reader, b"root").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_read_text_until_end_with_cdata() {
        let xml = "<code><![CDATA[a < b && c > d]]></code>";
        let mut reader = create_reader(xml);
        let mut buf = Vec::new();

        // Skip the opening tag
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) if e.name().as_ref() == b"code" => break,
                Ok(Event::Eof) => panic!("Unexpected EOF"),
                _ => {}
            }
            buf.clear();
        }

        let result = read_text_until_end(&mut reader, b"code").unwrap();
        // CDATA content should be preserved with actual < and > characters
        assert_eq!(result, "a < b && c > d");
    }

    #[test]
    fn test_read_text_until_end_with_entities() {
        let xml = "<code>a &lt; b &amp;&amp; c &gt; d</code>";
        let mut reader = create_reader(xml);
        let mut buf = Vec::new();

        // Skip the opening tag
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) if e.name().as_ref() == b"code" => break,
                Ok(Event::Eof) => panic!("Unexpected EOF"),
                _ => {}
            }
            buf.clear();
        }

        let result = read_text_until_end(&mut reader, b"code").unwrap();
        // Entities should be unescaped
        assert_eq!(result, "a < b && c > d");
    }

    #[test]
    fn test_malformed_xml_error_suggests_cdata_for_code() {
        let error = quick_xml::Error::Syntax(quick_xml::errors::SyntaxError::UnclosedTag);
        let result = malformed_xml_error(error);
        // Should return MalformedXml error type
        assert_eq!(result.error_type, XsdErrorType::MalformedXml);
    }

    #[test]
    fn test_make_parse_error_suggests_cdata_for_code_element() {
        let error = quick_xml::Error::Syntax(quick_xml::errors::SyntaxError::UnclosedTag);
        let result = make_parse_error(b"code-block", error);
        // Should suggest CDATA for code-block element
        assert!(result.suggestion.contains("CDATA"));
        assert!(result.suggestion.contains("code-block"));
    }

    #[test]
    fn test_unexpected_element_error() {
        let error = unexpected_element_error(b"foo", &["bar", "baz"], "parent");
        assert_eq!(error.element_path, "parent/foo");
        assert!(error.suggestion.contains("foo"));
    }

    #[test]
    fn test_missing_required_error() {
        let error = missing_required_error("child", "parent", Some("<child>example</child>"));
        assert_eq!(error.element_path, "parent/child");
        assert!(error.example.is_some());
    }

    #[test]
    fn test_duplicate_element_error() {
        let error = duplicate_element_error("child", "parent");
        assert!(error.suggestion.contains("Remove"));
    }

    #[test]
    fn test_text_outside_tags_error() {
        let error = text_outside_tags_error("stray text", "parent");
        assert!(error.found.contains("stray text"));
    }

    #[test]
    fn test_text_outside_tags_error_truncates_long_text() {
        let long_text = "x".repeat(100);
        let error = text_outside_tags_error(&long_text, "parent");
        assert!(error.found.contains("..."));
    }

    // =========================================================================
    // ILLEGAL CHARACTER DETECTION TESTS
    // =========================================================================

    #[test]
    fn test_check_for_illegal_xml_characters_accepts_valid_content() {
        // Valid content with allowed characters
        let valid = "Hello world\nNew line\tTab\rCarriage return";
        assert!(
            check_for_illegal_xml_characters(valid).is_ok(),
            "Valid content should pass"
        );
    }

    #[test]
    fn test_check_for_illegal_xml_characters_accepts_unicode() {
        // Valid Unicode characters
        let valid = "Hello 世界 🌍 Ωμέγα";
        assert!(
            check_for_illegal_xml_characters(valid).is_ok(),
            "Valid Unicode should pass"
        );
    }

    #[test]
    fn test_check_for_illegal_xml_characters_rejects_nul() {
        // NUL byte (the bug from the issue report)
        let invalid = "text\0here";
        let result = check_for_illegal_xml_characters(invalid);
        assert!(result.is_err(), "NUL byte should be rejected");

        let error = result.unwrap_err();
        assert!(
            error.found.contains("NUL") || error.found.contains("0x00"),
            "Error should mention NUL or 0x00, got: {}",
            error.found
        );
        assert!(
            error.suggestion.contains("\\u00A0") || error.suggestion.contains("non-breaking space"),
            "Error should suggest NBSP as common fix, got: {}",
            error.suggestion
        );
    }

    #[test]
    fn test_check_for_illegal_xml_characters_rejects_control_chars() {
        // Various control characters
        let test_cases = vec![
            ("\u{0001}", "0x01"),
            ("\u{0008}", "0x08"),
            ("\u{000B}", "0x0B"), // Vertical tab
            ("\u{000C}", "0x0C"), // Form feed
            ("\u{000E}", "0x0E"),
            ("\u{001F}", "0x1F"),
        ];

        for (invalid_str, expected_code) in test_cases {
            let content = format!("text{}here", invalid_str);
            let result = check_for_illegal_xml_characters(&content);
            assert!(
                result.is_err(),
                "Control character {} should be rejected",
                expected_code
            );

            let error = result.unwrap_err();
            assert!(
                error.found.contains(expected_code) || error.found.contains("control character"),
                "Error should mention control character, got: {}",
                error.found
            );
        }
    }

    #[test]
    fn test_check_for_illegal_xml_characters_provides_context() {
        // Position and context information
        let invalid = "Valid text before\0invalid character after";
        let result = check_for_illegal_xml_characters(invalid);
        assert!(result.is_err());

        let error = result.unwrap_err();
        // Should include surrounding context
        assert!(
            error.suggestion.contains("before") || error.suggestion.contains("after"),
            "Error should include context, got: {}",
            error.suggestion
        );
        // Should mention position
        assert!(
            error.found.contains("position"),
            "Error should mention position, got: {}",
            error.found
        );
    }

    #[test]
    fn test_check_for_illegal_xml_characters_allows_tab_newline_cr() {
        // These control characters ARE allowed in XML
        let valid = "text\twith\ntab\rand\nnewlines";
        assert!(
            check_for_illegal_xml_characters(valid).is_ok(),
            "Tab, LF, CR should be allowed"
        );
    }

    #[test]
    fn test_illegal_character_error_format_is_actionable() {
        // Verify error format is suitable for AI retry
        let invalid = "git\0diff";
        let result = check_for_illegal_xml_characters(invalid);
        assert!(result.is_err());

        let error = result.unwrap_err();
        let formatted = error.format_for_ai_retry();

        // Should contain key information for agent to fix
        assert!(
            formatted.contains("NUL") || formatted.contains("0x00"),
            "Formatted error should mention NUL"
        );
        assert!(
            formatted.contains("How to fix") || formatted.contains("suggestion"),
            "Formatted error should include fix guidance"
        );
    }
}
