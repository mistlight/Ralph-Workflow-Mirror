//! Shared quick_xml helper utilities for XSD validation.
//!
//! This module provides common parsing functions used across all XSD validators
//! to ensure consistent XML handling with proper whitespace management.
//!
//! # XML Sanitization
//!
//! LLMs often produce XML with unescaped special characters in text content.
//! For example: `<code-block>if a < b && c > d</code-block>`
//!
//! This module provides functions to sanitize such content before parsing:
//! - [`escape_xml_text`]: Escapes `<`, `>`, `&` characters in text
//! - [`sanitize_text_element`]: Sanitizes content within a specific element
//! - [`sanitize_xml_content`]: Sanitizes all known text-containing elements

use crate::files::llm_output_extraction::xsd_validation::{XsdErrorType, XsdValidationError};
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use std::sync::LazyLock;

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
/// (e.g., `&lt;` -> `<`, `&amp;` -> `&`).
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
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: "valid XML content".to_string(),
                    found: format!("XML parse error: {}", e),
                    suggestion: "Check that all XML tags are properly formed and closed."
                        .to_string(),
                    example: None,
                });
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
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("XML parse error: {}", e),
                    suggestion: "Check that all XML tags are properly formed.".to_string(),
                    example: None,
                });
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
    let display_text = if text.len() > 50 {
        format!("{}...", &text[..50])
    } else {
        text.to_string()
    };

    XsdValidationError {
        error_type: XsdErrorType::InvalidContent,
        element_path: parent_element.to_string(),
        expected: "only XML elements (no loose text)".to_string(),
        found: format!("text content: {:?}", display_text),
        suggestion: "Remove any text that is not inside a child element tag.".to_string(),
        example: None,
    }
}

/// Create an error for malformed XML.
pub fn malformed_xml_error(error: quick_xml::Error) -> XsdValidationError {
    XsdValidationError {
        error_type: XsdErrorType::MalformedXml,
        element_path: "xml".to_string(),
        expected: "well-formed XML".to_string(),
        found: format!("parse error: {}", error),
        suggestion: "Check that all XML tags are properly opened and closed, and that special characters are escaped (< as &lt;, > as &gt;, & as &amp;).".to_string(),
        example: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// XML CONTENT SANITIZATION
// ═══════════════════════════════════════════════════════════════════════════════

/// Regex to match valid XML entities that should NOT be escaped.
///
/// Valid XML entities: `&amp;`, `&lt;`, `&gt;`, `&apos;`, `&quot;`, `&#NN;`, `&#xNN;`
static VALID_XML_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(amp|lt|gt|apos|quot|#\d+|#x[0-9a-fA-F]+);").unwrap());

/// Escape XML special characters in text content.
///
/// This function escapes characters that have special meaning in XML:
/// - `&` → `&amp;` (only if not already part of a valid entity)
/// - `<` → `&lt;`
/// - `>` → `&gt;`
///
/// # Examples
///
/// ```
/// use ralph_workflow::files::llm_output_extraction::xml_helpers::escape_xml_text;
///
/// // Basic escaping
/// assert_eq!(escape_xml_text("a < b"), "a &lt; b");
/// assert_eq!(escape_xml_text("a && b"), "a &amp;&amp; b");
///
/// // Already escaped content is preserved
/// assert_eq!(escape_xml_text("a &lt; b"), "a &lt; b");
/// assert_eq!(escape_xml_text("a &amp; b"), "a &amp; b");
/// ```
pub fn escape_xml_text(text: &str) -> String {
    // Strategy: Replace valid entities with placeholders, escape all &, then restore
    // This avoids needing negative lookahead

    // Step 1: Find all valid entities and their positions
    let entities: Vec<_> = VALID_XML_ENTITY.find_iter(text).collect();

    if entities.is_empty() {
        // No valid entities - simple case, escape everything
        return text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
    }

    // Step 2: Build result by processing segments between entities
    let mut result = String::with_capacity(text.len() * 2);
    let mut last_end = 0;

    for entity in entities {
        // Escape the segment before this entity
        let segment = &text[last_end..entity.start()];
        result.push_str(
            &segment
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;"),
        );

        // Keep the valid entity as-is
        result.push_str(entity.as_str());
        last_end = entity.end();
    }

    // Escape any remaining segment after the last entity
    let segment = &text[last_end..];
    result.push_str(
        &segment
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
    );

    result
}

/// Sanitize text content within a specific XML element.
///
/// This function finds all instances of `<element_name>...</element_name>` (including
/// self-closing and variants with attributes) and escapes any unescaped XML special
/// characters in the content.
///
/// # Arguments
///
/// * `xml` - The XML string to sanitize
/// * `element_name` - The name of the element whose content should be sanitized
///
/// # Examples
///
/// ```
/// use ralph_workflow::files::llm_output_extraction::xml_helpers::sanitize_text_element;
///
/// let xml = r#"<code-block>if a < b</code-block>"#;
/// let result = sanitize_text_element(xml, "code-block");
/// assert_eq!(result, r#"<code-block>if a &lt; b</code-block>"#);
///
/// // With attributes
/// let xml = r#"<code-block language="rust">a && b</code-block>"#;
/// let result = sanitize_text_element(xml, "code-block");
/// assert_eq!(result, r#"<code-block language="rust">a &amp;&amp; b</code-block>"#);
/// ```
pub fn sanitize_text_element(xml: &str, element_name: &str) -> String {
    // Build regex pattern to match element with optional attributes
    // Pattern: <element_name(attributes)?>content</element_name>
    let pattern = format!(
        r"(?s)<{}(\s+[^>]*)?>(.+?)</{}>",
        regex::escape(element_name),
        regex::escape(element_name)
    );

    let re = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return xml.to_string(),
    };

    re.replace_all(xml, |caps: &regex::Captures| {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let content = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Check if content is already a CDATA section - don't escape
        if content.trim().starts_with("<![CDATA[") && content.trim().ends_with("]]>") {
            return format!("<{}{}>{}</{}>", element_name, attrs, content, element_name);
        }

        // Escape the content
        let escaped = escape_xml_text(content);
        format!("<{}{}>{}</{}>", element_name, attrs, escaped, element_name)
    })
    .to_string()
}

/// List of XML elements that contain code which may have unescaped `<`, `>`, `&`.
///
/// Only code-related elements should be sanitized because they legitimately contain
/// source code with comparison operators (`<`, `>`), logical operators (`&&`), etc.
///
/// Other text elements (like `<context>`, `<rationale>`, etc.) should NOT be sanitized
/// because:
/// 1. They shouldn't contain raw `<>` characters - those would break XML structure
/// 2. If an LLM produces `Record<string, string>` in a text element, that's an LLM
///    error that should be caught by validation, not silently "fixed"
/// 3. Sanitizing non-code elements can corrupt valid XML by misinterpreting content
const CODE_ELEMENTS: &[&str] = &["code-block", "code"];

/// Sanitize code elements in an XML string.
///
/// This function applies [`sanitize_text_element`] only to code-related elements
/// (`<code-block>` and `<code>`), ensuring that source code containing `<`, `>`,
/// or `&` characters is properly escaped before parsing.
///
/// # Why only code elements?
///
/// Code elements legitimately contain source code with comparison operators,
/// generics, and logical operators. Other text elements should not contain
/// raw `<>` characters - if they do, it's an LLM error that should fail
/// validation rather than be silently "fixed".
///
/// # Example
///
/// ```
/// use ralph_workflow::files::llm_output_extraction::xml_helpers::sanitize_xml_content;
///
/// let xml = r#"<plan>
///   <code-block>if a < b && c > d</code-block>
///   <context>Some context text</context>
/// </plan>"#;
///
/// let sanitized = sanitize_xml_content(xml);
/// // Code is escaped
/// assert!(sanitized.contains("a &lt; b &amp;&amp; c &gt; d"));
/// // Non-code content is unchanged
/// assert!(sanitized.contains("<context>Some context text</context>"));
/// ```
pub fn sanitize_xml_content(xml: &str) -> String {
    let mut result = xml.to_string();
    for element in CODE_ELEMENTS {
        result = sanitize_text_element(&result, element);
    }
    result
}
