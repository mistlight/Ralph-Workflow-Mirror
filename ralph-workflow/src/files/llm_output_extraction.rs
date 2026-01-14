//! LLM Output Extraction Module
//!
//! This module provides robust extraction of structured content from various LLM CLI output formats.
//! It supports multiple parser types and gracefully degrades when encountering unexpected formats.
//!
//! # Supported Formats
//!
//! - **Claude**: NDJSON with `{"type": "result", "result": "..."}` events
//! - **Codex**: NDJSON with `item.completed` events containing `agent_message` items
//! - **Gemini**: NDJSON with `{"type": "result"}` and `{"type": "message"}` events
//! - **`OpenCode`**: NDJSON with `{"type": "text"}` events
//! - **Generic**: Plain text output (fallback)
//!
//! # Design Principles
//!
//! 1. **Always return something**: Even if parsing fails, return the cleaned raw output
//! 2. **Try multiple strategies**: Each format has multiple extraction patterns
//! 3. **Auto-detection**: Can detect format from content if not specified
//! 4. **Validation**: Optional validation for extracted content

#![expect(clippy::too_many_lines)]
use regex::Regex;
use serde_json::Value as JsonValue;

/// Parser types supported by the extraction system.
/// Matches `crate::agents::parser::JsonParserType` but kept separate to avoid circular deps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Claude CLI stream-json format (also used by CCS, Qwen)
    #[default]
    Claude,
    /// `OpenAI` Codex CLI format
    Codex,
    /// Google Gemini CLI format
    Gemini,
    /// `OpenCode` NDJSON format
    OpenCode,
    /// Generic/plain text (fallback)
    Generic,
}

impl OutputFormat {
    /// Parse format from string name
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude" | "ccs" | "qwen" => Self::Claude,
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            "opencode" => Self::OpenCode,
            _ => Self::Generic,
        }
    }
}

/// Result of LLM output extraction
#[derive(Debug, Clone)]
pub struct ExtractionOutput {
    /// The extracted content (always present if input was non-empty)
    pub content: String,
    /// Whether extraction used a structured format vs fallback
    pub was_structured: bool,
    /// The detected/used format
    pub format: OutputFormat,
    /// Any warning or diagnostic message
    pub warning: Option<String>,
}

impl ExtractionOutput {
    const fn structured(content: String, format: OutputFormat) -> Self {
        Self {
            content,
            was_structured: true,
            format,
            warning: None,
        }
    }

    fn fallback(content: String, warning: &str) -> Self {
        Self {
            content,
            was_structured: false,
            format: OutputFormat::Generic,
            warning: Some(warning.to_string()),
        }
    }

    fn empty() -> Self {
        Self {
            content: String::new(),
            was_structured: false,
            format: OutputFormat::Generic,
            warning: Some("No content found in output".to_string()),
        }
    }
}

/// Extract result content from LLM CLI output.
///
/// This function attempts to extract meaningful content from the output of various
/// LLM CLI tools. It will:
///
/// 1. Try the specified format's extraction strategy
/// 2. Fall back to auto-detection if the specified format fails
/// 3. Fall back to plain text extraction as a last resort
///
/// # Arguments
///
/// * `output` - The raw output from the LLM CLI
/// * `format` - Optional format hint (if None, will auto-detect)
///
/// # Returns
///
/// An `ExtractionOutput` containing the extracted content and metadata.
///
/// # Example
///
/// ```ignore
/// let output = r#"{"type":"result","result":"feat: add feature"}"#;
/// let result = extract_llm_output(output, Some(OutputFormat::Claude));
/// assert_eq!(result.content, "feat: add feature");
/// ```
pub fn extract_llm_output(output: &str, format: Option<OutputFormat>) -> ExtractionOutput {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return ExtractionOutput::empty();
    }

    // Determine format - use provided or auto-detect
    let detected_format = format.unwrap_or_else(|| detect_output_format(trimmed));

    // Try the detected format first
    if let Some(content) = extract_by_format(trimmed, detected_format) {
        return ExtractionOutput::structured(content, detected_format);
    }

    // If specified format failed, try auto-detection with all formats
    if format.is_some() {
        for try_format in [
            OutputFormat::Claude,
            OutputFormat::Codex,
            OutputFormat::Gemini,
            OutputFormat::OpenCode,
        ] {
            if try_format != detected_format {
                if let Some(content) = extract_by_format(trimmed, try_format) {
                    return ExtractionOutput::structured(content, try_format);
                }
            }
        }
    }

    // Fall back to plain text extraction
    let cleaned = clean_plain_text(trimmed);
    if cleaned.is_empty() {
        ExtractionOutput::empty()
    } else {
        ExtractionOutput::fallback(
            cleaned,
            "Used plain text fallback - no structured format detected",
        )
    }
}

/// Detect the output format from content analysis
fn detect_output_format(content: &str) -> OutputFormat {
    // Check if it looks like JSONL
    let first_line = content.lines().next().unwrap_or("");
    if !first_line.trim().starts_with('{') {
        return OutputFormat::Generic;
    }

    // Try to parse first JSON line and detect format
    if let Ok(json) = serde_json::from_str::<JsonValue>(first_line) {
        if let Some(type_field) = json.get("type").and_then(|v| v.as_str()) {
            return match type_field {
                // Claude format indicators
                "system" | "assistant" | "user" | "result" => {
                    // Check for Claude-specific subtype
                    if json.get("subtype").is_some() || json.get("session_id").is_some() {
                        OutputFormat::Claude
                    } else if json.get("event_type").is_some() {
                        OutputFormat::OpenCode
                    } else {
                        OutputFormat::Claude
                    }
                }
                // Codex format indicators
                "thread.started" | "turn.started" | "turn.completed" | "turn.failed"
                | "item.started" | "item.completed" => OutputFormat::Codex,
                // Gemini format indicators
                "init" | "message" => {
                    if json.get("model").is_some() || json.get("role").is_some() {
                        OutputFormat::Gemini
                    } else {
                        OutputFormat::Claude
                    }
                }
                // OpenCode format indicators
                "step_start" | "step_finish" | "tool_use" | "text" => {
                    if json.get("sessionID").is_some() || json.get("part").is_some() {
                        OutputFormat::OpenCode
                    } else {
                        OutputFormat::Generic
                    }
                }
                _ => OutputFormat::Generic,
            };
        }
    }

    OutputFormat::Generic
}

/// Extract content using the specified format's strategy
fn extract_by_format(content: &str, format: OutputFormat) -> Option<String> {
    match format {
        OutputFormat::Claude => extract_claude_result(content),
        OutputFormat::Codex => extract_codex_result(content),
        OutputFormat::Gemini => extract_gemini_result(content),
        OutputFormat::OpenCode => extract_opencode_result(content),
        OutputFormat::Generic => None, // Generic doesn't use JSON extraction
    }
}

/// Extract result from Claude CLI NDJSON output.
///
/// Claude outputs JSONL with various event types. The result is in:
/// - `{"type": "result", "result": "..."}` - primary result event
/// - `{"type": "assistant", "message": {"content": [{"type": "text", "text": "..."}]}}` - assistant messages
fn extract_claude_result(content: &str) -> Option<String> {
    let mut last_result: Option<String> = None;
    let mut last_assistant_text: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<JsonValue>(line) {
            if let Some(type_field) = json.get("type").and_then(|v| v.as_str()) {
                match type_field {
                    "result" => {
                        // Primary result event - highest priority
                        if let Some(result) = json.get("result").and_then(|v| v.as_str()) {
                            if !result.trim().is_empty() {
                                last_result = Some(result.to_string());
                            }
                        }
                    }
                    "assistant" => {
                        // Extract text from assistant message content blocks
                        if let Some(message) = json.get("message") {
                            if let Some(content_arr) =
                                message.get("content").and_then(|v| v.as_array())
                            {
                                for block in content_arr {
                                    if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                                        if let Some(text) =
                                            block.get("text").and_then(|v| v.as_str())
                                        {
                                            if !text.trim().is_empty() {
                                                last_assistant_text = Some(text.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Also check for simple {"result": "..."} format (legacy/other agents)
            else if let Some(result) = json.get("result").and_then(|v| v.as_str()) {
                if !result.trim().is_empty() {
                    last_result = Some(result.to_string());
                }
            }
        }
    }

    // Prefer explicit result event, fall back to last assistant text
    last_result.or(last_assistant_text)
}

/// Extract result from Codex CLI NDJSON output.
///
/// Codex outputs JSONL with item events. The result comes from:
/// - `{"type": "item.completed", "item": {"type": "agent_message", "text": "..."}}`
fn extract_codex_result(content: &str) -> Option<String> {
    let mut last_message: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<JsonValue>(line) {
            if let Some(type_field) = json.get("type").and_then(|v| v.as_str()) {
                if type_field == "item.completed" {
                    if let Some(item) = json.get("item") {
                        if item.get("type").and_then(|v| v.as_str()) == Some("agent_message") {
                            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                if !text.trim().is_empty() {
                                    last_message = Some(text.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    last_message
}

/// Extract result from Gemini CLI NDJSON output.
///
/// Gemini outputs JSONL with message events. The result comes from:
/// - `{"type": "message", "role": "assistant", "content": "..."}`
/// - `{"type": "result", ...}` may contain final stats but not the actual output
fn extract_gemini_result(content: &str) -> Option<String> {
    let mut last_assistant_content: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<JsonValue>(line) {
            let is_assistant_message = json.get("type").and_then(|v| v.as_str()) == Some("message")
                && json.get("role").and_then(|v| v.as_str()) == Some("assistant");

            if is_assistant_message {
                if let Some(msg_content) = json.get("content").and_then(|v| v.as_str()) {
                    if !msg_content.trim().is_empty() {
                        // For streaming, accumulate or replace based on delta flag
                        if json.get("delta").and_then(serde_json::Value::as_bool) == Some(true) {
                            // Delta message - accumulate
                            if let Some(ref mut existing) = last_assistant_content {
                                existing.push_str(msg_content);
                            } else {
                                last_assistant_content = Some(msg_content.to_string());
                            }
                        } else {
                            // Full message - replace
                            last_assistant_content = Some(msg_content.to_string());
                        }
                    }
                }
            }
        }
    }

    last_assistant_content
}

/// Extract result from `OpenCode` CLI NDJSON output.
///
/// `OpenCode` outputs JSONL with nested part structures. The result comes from:
/// - `{"type": "text", "part": {"text": "..."}}`
fn extract_opencode_result(content: &str) -> Option<String> {
    let mut accumulated_text = String::new();

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<JsonValue>(line) {
            if let Some(type_field) = json.get("type").and_then(|v| v.as_str()) {
                if type_field == "text" {
                    if let Some(part) = json.get("part") {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            if !text.trim().is_empty() {
                                if !accumulated_text.is_empty() {
                                    accumulated_text.push(' ');
                                }
                                accumulated_text.push_str(text.trim());
                            }
                        }
                    }
                }
            }
        }
    }

    if accumulated_text.is_empty() {
        None
    } else {
        Some(accumulated_text)
    }
}

/// Clean plain text output by removing common artifacts.
///
/// This handles:
/// - Markdown code fences
/// - Common prefixes like "Commit message:", "Output:", etc.
/// - Excessive whitespace
fn clean_plain_text(content: &str) -> String {
    let mut result = content.to_string();

    // Remove markdown code fences
    if result.starts_with("```") {
        if let Some(end) = result.rfind("```") {
            if end > 3 {
                // Find the end of the first line (language specifier)
                let start = result.find('\n').map_or(3, |i| i + 1);
                result = result[start..end].to_string();
            }
        }
    }

    // Remove common prefixes (case-insensitive)
    let prefixes = [
        "commit message:",
        "message:",
        "output:",
        "result:",
        "response:",
        "here is the commit message:",
        "here's the commit message:",
        "git commit -m",
    ];

    let result_lower = result.to_lowercase();
    for prefix in prefixes {
        if result_lower.starts_with(prefix) {
            result = result[prefix.len()..].to_string();
            break;
        }
    }

    // Remove quotes if the entire result is quoted
    let trimmed = result.trim();
    if ((trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
        && trimmed.len() > 2
    {
        result = trimmed[1..trimmed.len() - 1].to_string();
    }

    // Clean up whitespace
    result
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Validate extracted content for use as a commit message.
///
/// # Returns
///
/// `Ok(())` if valid, `Err(reason)` if invalid
pub fn validate_commit_message(content: &str) -> Result<(), String> {
    let content = content.trim();

    // Check for empty
    if content.is_empty() {
        return Err("Commit message is empty".to_string());
    }

    // Check minimum length
    if content.len() < 5 {
        return Err(format!(
            "Commit message too short ({} chars, minimum 5)",
            content.len()
        ));
    }

    // Check maximum length (Git convention: first line <72, total <1000)
    if content.len() > 2000 {
        return Err(format!(
            "Commit message too long ({} chars, maximum 2000)",
            content.len()
        ));
    }

    // Check for JSON artifacts that indicate extraction failure
    let json_indicators = [
        r#"{"type":"#,
        r#"{"result":"#,
        r#"{"content":"#,
        r#""session_id":"#,
        r#""timestamp":"#,
        "stream_event",
        "content_block",
    ];
    for indicator in json_indicators {
        if content.contains(indicator) {
            return Err(format!(
                "Commit message contains JSON artifacts: {}...",
                &indicator[..indicator.len().min(20)]
            ));
        }
    }

    // Check for error markers
    let error_markers = [
        "error:",
        "failed to",
        "unable to",
        "i cannot",
        "i'm unable",
        "as an ai",
        "i don't have access",
        "cannot generate",
    ];
    let content_lower = content.to_lowercase();
    for marker in error_markers {
        if content_lower.starts_with(marker) {
            return Err(format!("Commit message starts with error marker: {marker}"));
        }
    }

    // Check for placeholder content
    let placeholders = [
        "[commit message]",
        "<commit message>",
        "placeholder",
        "your commit message here",
        "[insert",
        "<insert",
    ];
    for placeholder in placeholders {
        if content_lower.contains(placeholder) {
            return Err(format!(
                "Commit message contains placeholder: {placeholder}"
            ));
        }
    }

    // Check for bad commit message patterns (vague, meaningless messages)
    // Use regex to catch ALL variants, not just hardcoded numbers

    // Pattern 1: "chore: N file(s) changed" for ANY number N
    // Handles: "file(s) changed", "files changed", "file changed" variations
    let file_count_pattern = Regex::new(r"^chore:\s*\d+\s+(?:file\(s\)|files?)\s+changed$")
        .expect("file count regex should be valid");
    if file_count_pattern.is_match(&content_lower) {
        return Err(format!(
            "Commit message matches bad pattern (file count pattern): '{content}'. Use semantic description instead."
        ));
    }

    // Pattern 2: Generic vague patterns
    let vague_patterns = [
        ("chore: apply changes", "vague 'apply changes' pattern"),
        ("chore: update code", "vague 'update code' pattern"),
    ];
    for (pattern, description) in vague_patterns {
        if content_lower == pattern {
            return Err(format!(
                "Commit message matches bad pattern ({description}): {pattern}"
            ));
        }
    }

    // Check for filename list patterns like "chore: update src/file.rs" or "chore: src/file.rs, src/other.rs"
    // These are bad because they just list filenames without semantic meaning
    let first_line = content.lines().next().unwrap_or(content);
    let first_line_lower = first_line.to_lowercase();

    // Check both "chore: update <path>" and "chore: <path>" patterns
    if first_line_lower.starts_with("chore: update ") || first_line_lower.starts_with("chore:") {
        let subject = first_line_lower
            .replacen("chore: update ", "", 1)
            .replacen("chore:", "", 1)
            .trim()
            .to_string();

        // Check if subject looks like a file path or list of file paths
        // File paths contain '/' or end with common extensions
        // We need to check for multiple patterns:
        // 1. Single file path: "src/file.rs"
        // 2. Multiple files with commas: "src/a.rs, src/b.rs"
        // 3. Multiple files with "and": "src/a.rs and src/b.rs"

        // Common code file extensions
        let code_extensions = [
            ".rs", ".js", ".ts", ".py", ".go", ".java", ".c", ".cpp", ".h", ".cs", ".php", ".rb",
            ".swift", ".kt",
        ];

        // Check if subject looks like a file path or list of file paths
        let looks_like_file_list = subject.contains('/') ||
            subject.contains('\\') ||  // Windows paths
            code_extensions.iter().any(|ext| subject.ends_with(ext));

        // Additional check: if there are commas and file extensions, it's definitely a file list
        let has_comma_separated_files =
            subject.contains(", ") && code_extensions.iter().any(|ext| subject.contains(ext));

        // Check for "and" separated files
        let has_and_separated_files =
            subject.contains(" and ") && code_extensions.iter().any(|ext| subject.contains(ext));

        if looks_like_file_list || has_comma_separated_files || has_and_separated_files {
            return Err(format!(
                "Commit message appears to be a file list: '{}'. Use semantic description instead.",
                first_line.trim()
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Format Detection Tests
    // =========================================================================

    #[test]
    fn test_detect_claude_format() {
        let content = r#"{"type":"system","subtype":"init","session_id":"abc123"}
{"type":"result","result":"test message"}"#;
        assert_eq!(detect_output_format(content), OutputFormat::Claude);
    }

    #[test]
    fn test_detect_codex_format() {
        let content = r#"{"type":"thread.started","thread_id":"thread_123"}"#;
        assert_eq!(detect_output_format(content), OutputFormat::Codex);
    }

    #[test]
    fn test_detect_gemini_format() {
        let content = r#"{"type":"init","session_id":"abc","model":"gemini-pro"}"#;
        assert_eq!(detect_output_format(content), OutputFormat::Gemini);
    }

    #[test]
    fn test_detect_opencode_format() {
        let content = r#"{"type":"step_start","sessionID":"ses_123","part":{}}"#;
        assert_eq!(detect_output_format(content), OutputFormat::OpenCode);
    }

    #[test]
    fn test_detect_generic_format() {
        let content = "Just some plain text output";
        assert_eq!(detect_output_format(content), OutputFormat::Generic);
    }

    // =========================================================================
    // Claude Extraction Tests
    // =========================================================================

    #[test]
    fn test_claude_extract_result_event() {
        let content = r#"{"type":"system","subtype":"init"}
{"type":"stream_event","event":{"type":"text_delta"}}
{"type":"result","subtype":"success","result":"feat: add new feature"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add new feature");
    }

    #[test]
    fn test_claude_extract_from_assistant_message() {
        let content = r#"{"type":"system","subtype":"init"}
{"type":"assistant","message":{"content":[{"type":"text","text":"fix: resolve bug in parser"}]}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "fix: resolve bug in parser");
    }

    #[test]
    fn test_claude_prefers_result_over_assistant() {
        let content = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"assistant text"}]}}
{"type":"result","result":"result text"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert_eq!(result.content, "result text");
    }

    #[test]
    fn test_claude_real_world_streaming_output() {
        // This is a simplified version of real Claude CLI output
        let content = r#"{"type":"system","subtype":"init","cwd":"/test","session_id":"858002c2"}
{"type":"stream_event","event":{"type":"message_start","message":{"model":"claude-opus-4-5-20251101"}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"docs"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"(cli)"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":": add feature"}}}
{"type":"assistant","message":{"model":"claude-opus-4-5-20251101","content":[{"type":"text","text":"docs(cli): add feature"}]}}
{"type":"stream_event","event":{"type":"message_stop"}}
{"type":"result","subtype":"success","result":"docs(cli): add feature","duration_ms":4688,"total_cost_usd":0.47}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.format, OutputFormat::Claude);
        assert_eq!(result.content, "docs(cli): add feature");
    }

    #[test]
    fn test_claude_handles_empty_result() {
        let content = r#"{"type":"system","subtype":"init"}
{"type":"result","result":""}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        // Empty result should fall back
        assert!(!result.was_structured || result.content.is_empty());
    }

    // =========================================================================
    // Codex Extraction Tests
    // =========================================================================

    #[test]
    fn test_codex_extract_agent_message() {
        let content = r#"{"type":"thread.started","thread_id":"thread_abc"}
{"type":"turn.started"}
{"type":"item.started","item":{"type":"agent_message"}}
{"type":"item.completed","item":{"type":"agent_message","text":"chore: update dependencies"}}
{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Codex));
        assert!(result.was_structured);
        assert_eq!(result.content, "chore: update dependencies");
    }

    #[test]
    fn test_codex_uses_last_message() {
        let content = r#"{"type":"item.completed","item":{"type":"agent_message","text":"first message"}}
{"type":"item.completed","item":{"type":"agent_message","text":"final message"}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Codex));
        assert_eq!(result.content, "final message");
    }

    #[test]
    fn test_codex_ignores_non_agent_messages() {
        let content = r#"{"type":"item.completed","item":{"type":"reasoning","text":"thinking..."}}
{"type":"item.completed","item":{"type":"agent_message","text":"actual output"}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Codex));
        assert_eq!(result.content, "actual output");
    }

    // =========================================================================
    // Gemini Extraction Tests
    // =========================================================================

    #[test]
    fn test_gemini_extract_assistant_message() {
        let content = r#"{"type":"init","session_id":"abc","model":"gemini-pro"}
{"type":"message","role":"user","content":"generate commit message"}
{"type":"message","role":"assistant","content":"refactor: improve error handling"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Gemini));
        assert!(result.was_structured);
        assert_eq!(result.content, "refactor: improve error handling");
    }

    #[test]
    fn test_gemini_accumulates_delta_messages() {
        let content = r#"{"type":"message","role":"assistant","content":"feat","delta":true}
{"type":"message","role":"assistant","content":": add","delta":true}
{"type":"message","role":"assistant","content":" feature","delta":true}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Gemini));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_gemini_ignores_user_messages() {
        let content = r#"{"type":"message","role":"user","content":"user input"}
{"type":"message","role":"assistant","content":"assistant output"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Gemini));
        assert_eq!(result.content, "assistant output");
    }

    // =========================================================================
    // OpenCode Extraction Tests
    // =========================================================================

    #[test]
    fn test_opencode_extract_text_events() {
        let content = r#"{"type":"step_start","sessionID":"ses_123","part":{}}
{"type":"text","sessionID":"ses_123","part":{"text":"fix: resolve parsing issue"}}
{"type":"step_finish","sessionID":"ses_123","part":{"reason":"end_turn"}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::OpenCode));
        assert!(result.was_structured);
        assert_eq!(result.content, "fix: resolve parsing issue");
    }

    #[test]
    fn test_opencode_accumulates_text() {
        let content = r#"{"type":"text","part":{"text":"I'll analyze"}}
{"type":"text","part":{"text":"the code"}}
{"type":"text","part":{"text":"and generate"}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::OpenCode));
        assert!(result.was_structured);
        assert!(result.content.contains("I'll analyze"));
        assert!(result.content.contains("and generate"));
    }

    // =========================================================================
    // Generic/Fallback Tests
    // =========================================================================

    #[test]
    fn test_generic_plain_text() {
        let content = "feat: add new authentication system";

        let result = extract_llm_output(content, Some(OutputFormat::Generic));
        assert!(!result.was_structured);
        assert_eq!(result.content, "feat: add new authentication system");
    }

    #[test]
    fn test_fallback_removes_markdown_fences() {
        let content = "```\nfix: bug fix\n```";

        let result = extract_llm_output(content, None);
        assert_eq!(result.content, "fix: bug fix");
    }

    #[test]
    fn test_fallback_removes_prefixes() {
        let content = "Commit message: feat: add feature";

        let result = extract_llm_output(content, None);
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_fallback_removes_quotes() {
        let content = r#""fix: quoted message""#;

        let result = extract_llm_output(content, None);
        assert_eq!(result.content, "fix: quoted message");
    }

    // =========================================================================
    // Auto-Detection Tests
    // =========================================================================

    #[test]
    fn test_auto_detect_claude() {
        let content = r#"{"type":"result","result":"auto-detected claude"}"#;

        let result = extract_llm_output(content, None);
        assert!(result.was_structured);
        assert_eq!(result.format, OutputFormat::Claude);
        assert_eq!(result.content, "auto-detected claude");
    }

    #[test]
    fn test_auto_detect_with_wrong_hint() {
        // Give wrong hint, should still extract correctly via fallback
        let content = r#"{"type":"result","result":"found it"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Codex));
        // Should fall back to Claude extraction
        assert!(result.was_structured);
        assert_eq!(result.content, "found it");
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn test_empty_input() {
        let result = extract_llm_output("", None);
        assert!(result.content.is_empty());
        assert!(result.warning.is_some());
    }

    #[test]
    fn test_whitespace_only_input() {
        let result = extract_llm_output("   \n\n   ", None);
        assert!(result.content.is_empty());
    }

    #[test]
    fn test_malformed_json_falls_back() {
        let content = "{not valid json}\nplain text output";

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        // Should fall back to plain text
        assert!(!result.was_structured);
        assert!(result.content.contains("plain text output"));
    }

    #[test]
    fn test_mixed_json_and_text() {
        let content = r#"Starting process...
{"type":"result","result":"the actual result"}
Done."#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "the actual result");
    }

    // =========================================================================
    // Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_good_commit_message() {
        assert!(validate_commit_message("feat: add new feature").is_ok());
        assert!(validate_commit_message("fix(parser): resolve edge case").is_ok());
        assert!(
            validate_commit_message("chore: update dependencies\n\nThis updates all deps.").is_ok()
        );
    }

    #[test]
    fn test_validate_rejects_empty() {
        assert!(validate_commit_message("").is_err());
        assert!(validate_commit_message("   ").is_err());
    }

    #[test]
    fn test_validate_rejects_too_short() {
        assert!(validate_commit_message("fix").is_err());
    }

    #[test]
    fn test_validate_rejects_json_artifacts() {
        let json_message = r#"{"type":"result","result":"feat: add feature"}"#;
        let err = validate_commit_message(json_message).unwrap_err();
        assert!(err.contains("JSON artifacts"));
    }

    #[test]
    fn test_validate_rejects_stream_events() {
        let stream_message = r"stream_event content_block more stuff";
        let err = validate_commit_message(stream_message).unwrap_err();
        assert!(err.contains("JSON artifacts"));
    }

    #[test]
    fn test_validate_rejects_error_prefix() {
        let err = validate_commit_message("Error: could not generate message").unwrap_err();
        assert!(err.contains("error marker"));
    }

    #[test]
    fn test_validate_rejects_placeholder() {
        let err = validate_commit_message("[commit message] goes here").unwrap_err();
        assert!(err.contains("placeholder"));
    }

    #[test]
    fn test_validate_rejects_apply_changes_pattern() {
        let err = validate_commit_message("chore: apply changes").unwrap_err();
        assert!(err.contains("bad pattern"));
        assert!(err.contains("apply changes"));
    }

    #[test]
    fn test_validate_rejects_update_code_pattern() {
        let err = validate_commit_message("chore: update code").unwrap_err();
        assert!(err.contains("bad pattern"));
        assert!(err.contains("update code"));
    }

    #[test]
    fn test_validate_rejects_file_count_pattern() {
        let err = validate_commit_message("chore: 6 file(s) changed").unwrap_err();
        assert!(err.contains("bad pattern"));
        assert!(err.contains("file count"));
    }

    #[test]
    fn test_validate_rejects_file_count_pattern_7_files() {
        // Regression test for commits with 7+ files that were bypassing validation
        let err = validate_commit_message("chore: 7 file(s) changed").unwrap_err();
        assert!(err.contains("file count pattern"));
    }

    #[test]
    fn test_validate_rejects_file_count_pattern_8_files() {
        // Regression test for commits with 8+ files that were bypassing validation
        let err = validate_commit_message("chore: 8 file(s) changed").unwrap_err();
        assert!(err.contains("file count pattern"));
    }

    #[test]
    fn test_validate_rejects_file_count_pattern_many_files() {
        // Regression test for commits with many files (e.g., 15, 20, 100)
        let err = validate_commit_message("chore: 15 file(s) changed").unwrap_err();
        assert!(err.contains("file count pattern"));
        assert!(validate_commit_message("chore: 100 files changed").is_err());
    }

    #[test]
    fn test_validate_rejects_file_count_pattern_with_spaces() {
        // Test with different spacing variations
        assert!(validate_commit_message("chore:  7  file(s)  changed").is_err());
        assert!(validate_commit_message("chore:  8 file(s) changed").is_err());
    }

    #[test]
    fn test_validate_rejects_single_file_path_pattern() {
        let err =
            validate_commit_message("chore: update src/files/result_extraction.rs").unwrap_err();
        assert!(err.contains("file list"));
    }

    #[test]
    fn test_validate_rejects_multiple_file_paths_pattern() {
        let err =
            validate_commit_message("chore: update src/a.rs, src/b.rs, src/c.rs").unwrap_err();
        assert!(err.contains("file list"));
    }

    #[test]
    fn test_validate_rejects_multiple_file_paths_js() {
        // Test with JavaScript files
        let err = validate_commit_message("chore: update a.js, b.js, c.js").unwrap_err();
        assert!(err.contains("file list"));
    }

    #[test]
    fn test_validate_rejects_multiple_file_paths_py() {
        // Test with Python files
        let err = validate_commit_message("chore: update module.py, test.py").unwrap_err();
        assert!(err.contains("file list"));
    }

    #[test]
    fn test_validate_rejects_multiple_file_paths_mixed() {
        // Test with mixed file types
        let err = validate_commit_message("chore: update src/lib.rs, tests/test.rs, README.md")
            .unwrap_err();
        assert!(err.contains("file list"));
    }

    #[test]
    fn test_validate_rejects_file_paths_with_and() {
        // Test with "and" separator
        let err = validate_commit_message("chore: update src/a.rs and src/b.rs").unwrap_err();
        assert!(err.contains("file list"));
    }

    #[test]
    fn test_validate_accepts_good_chore_message() {
        // A good chore message should be accepted
        assert!(validate_commit_message("chore: update dependencies").is_ok());
        assert!(validate_commit_message("chore: fix formatting").is_ok());
    }

    #[test]
    fn test_validate_accepts_semantic_messages() {
        // Semantic messages should be accepted
        assert!(validate_commit_message("feat: add new feature").is_ok());
        assert!(validate_commit_message("fix: prevent null pointer").is_ok());
        assert!(validate_commit_message("refactor(api): extract validation").is_ok());
    }

    // =========================================================================
    // Real-World Regression Tests
    // =========================================================================

    #[test]
    fn test_real_broken_commit_from_issue() {
        // This is the actual broken output that was being used as commit message
        let broken_output = r#"{"type":"system","subtype":"init","cwd":"/test","session_id":"858002c2-5428-46a2-ac55-447bd9712dee","tools":["Task"]}
{"type":"stream_event","event":{"type":"message_start","message":{"model":"claude-opus-4-5-20251101"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"docs(cli)"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":": add -f short flag"}}}
{"type":"assistant","message":{"content":[{"type":"text","text":"docs(cli): add -f short flag for --full verbosity option\n\nUpdate documentation and CLI help."}]}}
{"type":"result","subtype":"success","result":"docs(cli): add -f short flag for --full verbosity option\n\nUpdate documentation and CLI help.","duration_ms":4688,"total_cost_usd":0.47}"#;

        let result = extract_llm_output(broken_output, Some(OutputFormat::Claude));

        // Should extract the clean message, not the raw JSON
        assert!(result.was_structured);
        assert!(result.content.starts_with("docs(cli):"));
        assert!(!result.content.contains(r#"{"type":"#));

        // Should pass validation
        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_simple_result_json() {
        // Legacy simple format
        let content = r#"{"result": "feat: simple feature"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: simple feature");
    }
}
