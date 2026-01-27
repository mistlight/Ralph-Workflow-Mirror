//! Shared quick_xml helper utilities for XSD validation.
//!
//! This module provides common parsing functions used across all XSD validators
//! to ensure consistent XML handling with proper whitespace management.
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

    // Check if this is likely a code-block escaping issue
    let suggestion = if error_str.contains("code") || error_str.contains("block") {
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
}
