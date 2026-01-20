//! Flexible XML extraction module for AI-generated review issues.
//!
//! This module provides robust extraction of XML issues from various
//! AI output formats. AI agents may embed XML in unpredictable ways.
//!
//! Note: Currently unused in production (review phase uses reviewer prompts).
//! Kept for potential future use and test compatibility.

use crate::files::llm_output_extraction::cleaning::unescape_json_strings_aggressive;

/// Extract XML issues from AI output using multiple strategies.
///
/// # Strategies (tried in order)
///
/// 1. **Direct extraction**: Content starts with `<ralph-issues>` tag
/// 2. **Markdown code fence**: XML wrapped in ```xml or ``` fences
/// 3. **JSON string**: XML escaped in a JSON string value
/// 4. **Embedded search**: Look for `<ralph-issues>` anywhere in content
///
/// # Arguments
///
/// * `content` - The raw AI agent output
///
/// # Returns
///
/// * `Some(xml_content)` - The extracted XML content including tags
/// * `None` - No valid XML issues found
#[allow(dead_code)]
pub fn extract_issues_xml(content: &str) -> Option<String> {
    // Strategy 1: Direct XML at start (most efficient)
    if let Some(xml) = try_extract_direct_xml(content) {
        return Some(xml);
    }

    // Strategy 2: XML in markdown code fence
    if let Some(xml) = try_extract_from_markdown_fence(content) {
        return Some(xml);
    }

    // Strategy 3: XML in JSON string (escaped)
    if let Some(xml) = try_extract_from_json_string(content) {
        return Some(xml);
    }

    // Strategy 4: Search for tags anywhere (most permissive)
    try_extract_embedded_xml(content)
}

/// Strategy 1: Extract XML that starts with `<ralph-issues>` tag.
#[allow(dead_code)]
fn try_extract_direct_xml(content: &str) -> Option<String> {
    let trimmed = content.trim();

    if !trimmed.starts_with("<ralph-issues>") {
        return None;
    }

    let start = trimmed.find("<ralph-issues>")?;
    let end = trimmed.find("</ralph-issues>")?;

    if start >= end {
        return None;
    }

    let xml_end = end + "</ralph-issues>".len();
    Some(trimmed[start..xml_end].to_string())
}

/// Strategy 2: Extract XML from markdown code fences.
#[allow(dead_code)]
fn try_extract_from_markdown_fence(content: &str) -> Option<String> {
    // Pattern 1: ```xml fence
    if let Some(start) = content.find("```xml") {
        let after_fence = &content[start + 6..];

        if let Some(end) = after_fence.find("```") {
            let fence_content = after_fence[..end].trim();
            if let Some(xml) = extract_ralph_issues_from_content(fence_content) {
                return Some(xml);
            }
        }
    }

    // Pattern 2: Generic ``` fence (no language specified)
    if let Some(start) = content.find("```") {
        let after_fence = &content[start + 3..];

        if let Some(end) = after_fence.find("```") {
            let fence_content = after_fence[..end].trim();
            if let Some(xml) = extract_ralph_issues_from_content(fence_content) {
                return Some(xml);
            }
        }
    }

    None
}

/// Strategy 3: Extract XML from JSON strings (escaped).
#[allow(dead_code)]
fn try_extract_from_json_string(content: &str) -> Option<String> {
    // Pattern 1: NDJSON stream with result field
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(result) = json.get("result").and_then(|v| v.as_str()) {
                if let Some(xml) = extract_ralph_issues_from_content(result) {
                    return Some(xml);
                }

                let unescaped = unescape_json_strings_aggressive(result);
                if let Some(xml) = extract_ralph_issues_from_content(&unescaped) {
                    return Some(xml);
                }
            }

            for field_name in ["content", "message", "output", "text"] {
                if let Some(field_value) = json.get(field_name).and_then(|v| v.as_str()) {
                    if let Some(xml) = extract_ralph_issues_from_content(field_value) {
                        return Some(xml);
                    }

                    let unescaped = unescape_json_strings_aggressive(field_value);
                    if let Some(xml) = extract_ralph_issues_from_content(&unescaped) {
                        return Some(xml);
                    }
                }
            }
        }
    }

    // Pattern 2: Direct JSON object (not NDJSON)
    let trimmed = content.trim();
    if trimmed.starts_with('{') && trimmed.contains(r#""result""#) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(result) = json.get("result").and_then(|v| v.as_str()) {
                if let Some(xml) = extract_ralph_issues_from_content(result) {
                    return Some(xml);
                }

                let unescaped = unescape_json_strings_aggressive(result);
                if let Some(xml) = extract_ralph_issues_from_content(&unescaped) {
                    return Some(xml);
                }
            }
        }
    }

    None
}

/// Strategy 4: Search for XML tags anywhere in content.
#[allow(dead_code)]
fn try_extract_embedded_xml(content: &str) -> Option<String> {
    extract_ralph_issues_from_content(content)
}

/// Extract `<ralph-issues>...</ralph-issues>` from arbitrary content.
#[allow(dead_code)]
fn extract_ralph_issues_from_content(content: &str) -> Option<String> {
    let start = content.find("<ralph-issues>")?;
    let end = content.find("</ralph-issues>")?;

    if start >= end {
        return None;
    }

    let xml_end = end + "</ralph-issues>".len();
    Some(content[start..xml_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_direct_xml_basic() {
        let content = r"<ralph-issues>
<ralph-issue>First issue</ralph-issue>
</ralph-issues>";
        let result = extract_issues_xml(content);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_extract_from_xml_fence() {
        let content = r"Here's the issues:

```xml
<ralph-issues>
<ralph-issue>First issue</ralph-issue>
</ralph-issues>
```

Done!";
        let result = extract_issues_xml(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("<ralph-issues>"));
    }

    #[test]
    fn test_extract_from_ndjson_result() {
        let content = r#"{"type":"result","result":"<ralph-issues>\n<ralph-issue>First issue</ralph-issue>\n</ralph-issues>"}"#;
        let result = extract_issues_xml(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("<ralph-issues>"));
    }

    #[test]
    fn test_extract_embedded_in_analysis() {
        let content = r"Based on my review:

<ralph-issues>
<ralph-issue>First issue</ralph-issue>
</ralph-issues>

That's all!";
        let result = extract_issues_xml(content);
        assert!(result.is_some());
    }

    #[test]
    fn test_extract_no_xml_returns_none() {
        let content = r"This is just plain text without any XML tags.";
        let result = extract_issues_xml(content);
        assert!(result.is_none());
    }
}
