//! XML formatter for displaying XML content to users.
//!
//! This module provides pretty-printing of XML content for display purposes.
//! When AI agents return XML, we want to display it in a nice, readable format
//! rather than showing raw XML.

#![allow(clippy::if_same_then_else)]
#![allow(clippy::needless_range_loop)]

/// Format XML content for nice display.
///
/// This function prettifies XML by adding proper indentation and line breaks.
/// If the XML parsing fails or the content isn't XML, it returns the original content.
///
/// # Arguments
///
/// * `xml_content` - The XML content to format
///
/// # Returns
///
/// A formatted string with proper indentation for display.
pub fn format_xml_for_display(xml_content: &str) -> String {
    // Check if content looks like XML (has tags)
    if !xml_content.contains('<') {
        return xml_content.to_string();
    }

    // Try to parse and pretty-print the XML
    match pretty_print_xml(xml_content) {
        Ok(pretty) if !pretty.is_empty() => pretty,
        _ => xml_content.to_string(),
    }
}

/// Pretty-print XML with proper indentation.
///
/// This is a simple XML pretty-printer that adds indentation
/// based on tag nesting level.
fn pretty_print_xml(xml_content: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut indent: usize = 0;
    let mut in_tag = false;
    let mut in_content = false;
    let mut tag_start = 0;
    let chars: Vec<char> = xml_content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        match c {
            '<' => {
                // Check if this is a closing tag
                if i + 1 < chars.len() && chars[i + 1] == '/' {
                    // Closing tag - decrease indent before outputting
                    if in_content && indent > 0 {
                        result.push('\n');
                    }
                    indent = indent.saturating_sub(1);
                    in_tag = true;
                    in_content = false;
                } else if i + 1 < chars.len() && chars[i + 1] == '?' {
                    // XML declaration - don't indent
                    in_tag = true;
                } else {
                    // Opening tag
                    if in_content {
                        result.push('\n');
                    }
                    in_tag = true;
                    in_content = false;
                }
                tag_start = i;
            }
            '>' => {
                if in_tag {
                    // Check if this is a self-closing tag
                    let is_self_closing = i > 0 && chars[i - 1] == '/';
                    let is_declaration = tag_start > 0
                        && chars[tag_start + 1] == '?'
                        && i > 0
                        && chars[i - 1] == '?';

                    // Extract the tag name
                    let tag_name_start = if chars[tag_start + 1] == '/' {
                        tag_start + 2
                    } else if chars[tag_start + 1] == '?' {
                        tag_start + 2
                    } else {
                        tag_start + 1
                    };

                    let tag_name_end = i;
                    let mut tag_name = String::new();
                    for j in tag_name_start..tag_name_end {
                        if chars[j].is_whitespace() || chars[j] == '/' {
                            break;
                        }
                        tag_name.push(chars[j]);
                    }

                    // Add indentation for opening tags (not self-closing or declaration)
                    if !is_self_closing && !is_declaration && !chars[tag_start + 1].is_whitespace()
                    {
                        if !result.ends_with('\n') && !result.is_empty() {
                            result.push('\n');
                        }
                        for _ in 0..indent {
                            result.push_str("  ");
                        }
                    }

                    // Add the tag content
                    for j in tag_start..=i {
                        result.push(chars[j]);
                    }

                    // Increase indent after opening tag (if not self-closing)
                    if !is_self_closing
                        && !is_declaration
                        && chars[tag_start + 1] != '/'
                        && !tag_name.is_empty()
                    {
                        indent += 1;
                        in_content = true;
                    }

                    in_tag = false;
                } else {
                    result.push(c);
                }
            }
            '\n' | '\r' | '\t' => {
                // Skip whitespace outside of tags
                if !in_tag {
                    // Keep track but don't output
                }
            }
            ' ' => {
                if in_tag {
                    // Keep spaces inside tags
                    result.push(c);
                } else if in_content {
                    // Keep leading spaces for content but not multiple spaces
                    if let Some(last_char) = result.chars().last() {
                        if last_char != ' ' && last_char != '\n' {
                            result.push(c);
                        }
                    } else {
                        result.push(c);
                    }
                }
            }
            _ => {
                // For any other character, push it if we're in a tag or content
                if in_tag || in_content {
                    result.push(c);
                }
            }
        }

        i += 1;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_simple_xml() {
        let xml = r#"<ralph-plan><ralph-summary>Summary</ralph-summary></ralph-plan>"#;
        let formatted = format_xml_for_display(xml);
        assert!(formatted.contains("<ralph-plan>"));
        assert!(formatted.contains("<ralph-summary>"));
        assert!(formatted.contains("Summary"));
    }

    #[test]
    fn test_format_nested_xml() {
        let xml = r#"<ralph-issues><ralph-issue>Issue 1</ralph-issue><ralph-issue>Issue 2</ralph-issue></ralph-issues>"#;
        let formatted = format_xml_for_display(xml);
        assert!(formatted.contains("<ralph-issues>"));
        assert!(formatted.contains("<ralph-issue>"));
    }

    #[test]
    fn test_format_with_attributes() {
        let xml = r#"<ralph-fix-result><ralph-status>all_issues_addressed</ralph-status></ralph-fix-result>"#;
        let formatted = format_xml_for_display(xml);
        assert!(formatted.contains("<ralph-fix-result>"));
        assert!(formatted.contains("<ralph-status>"));
    }

    #[test]
    fn test_format_empty_xml() {
        let xml = "";
        let formatted = format_xml_for_display(xml);
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_format_invalid_xml_returns_original() {
        let xml = "This is not XML at all";
        let formatted = format_xml_for_display(xml);
        assert_eq!(formatted, "This is not XML at all");
    }

    #[test]
    fn test_format_xml_with_declaration() {
        let xml = r#"<?xml version="1.0"?><ralph-plan><ralph-summary>Summary</ralph-summary></ralph-plan>"#;
        let formatted = format_xml_for_display(xml);
        assert!(formatted.contains("<?xml"));
        assert!(formatted.contains("<ralph-plan>"));
    }
}
