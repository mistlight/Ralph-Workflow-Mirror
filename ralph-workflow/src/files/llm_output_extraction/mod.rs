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

pub mod cleaning;
mod commit;
mod parsers;
mod types;

// Re-export public types
pub use types::{ExtractionOutput, OutputFormat};

// Re-export public functions from cleaning module
pub use cleaning::preprocess_raw_content;

// Re-export public functions from commit module
pub use commit::{
    detect_agent_errors_in_output, generate_fallback_commit_message, try_extract_structured_commit,
    try_extract_xml_commit, try_salvage_commit_message, validate_commit_message,
    CommitExtractionResult,
};

use cleaning::clean_plain_text;
use parsers::{detect_output_format, extract_by_format};

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
    // Thinking Content Filtering Tests (Regression)
    // =========================================================================

    #[test]
    fn test_claude_filters_thinking_blocks_from_assistant_message() {
        // Test that thinking content blocks are filtered out from assistant messages
        let content = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"Looking at this diff, I can see..."},{"type":"text","text":"feat: add new feature"}]}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add new feature");
        assert!(!result.content.contains("Looking at this diff"));
    }

    #[test]
    fn test_claude_filters_reasoning_blocks_from_assistant_message() {
        // Test that reasoning content blocks are also filtered out
        let content = r#"{"type":"assistant","message":{"content":[{"type":"reasoning","reasoning":"Let me analyze this..."},{"type":"text","text":"fix: resolve parsing issue"}]}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "fix: resolve parsing issue");
        assert!(!result.content.contains("analyze"));
    }

    #[test]
    fn test_claude_filters_multiple_thinking_and_text_blocks() {
        // Test mixed content blocks with multiple thinking blocks
        // Note: The extraction only keeps the LAST text block, not concatenation
        let content = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"First thought..."},{"type":"text","text":"docs: update"},{"type":"thinking","thinking":"Second thought..."},{"type":"text","text":" documentation"}]}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        // Should only extract the last text block (not concatenated)
        assert_eq!(result.content, " documentation");
        assert!(!result.content.contains("First thought"));
        assert!(!result.content.contains("Second thought"));
    }

    #[test]
    fn test_claude_result_field_with_thinking_blocks_present() {
        // Test when thinking blocks are present but result field has the commit message
        let content = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"Analyzing the changes..."}]}}
{"type":"result","result":"chore: improve performance"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        // Should prefer the result field over assistant message
        assert_eq!(result.content, "chore: improve performance");
        assert!(!result.content.contains("Analyzing"));
    }

    #[test]
    fn test_claude_only_thinking_blocks_falls_back_to_empty() {
        // Test when only thinking blocks exist (no text blocks)
        let content = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"This is only thinking content"}]}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        // Should return empty since no text content exists
        assert!(result.content.is_empty() || !result.was_structured);
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
    fn test_gemini_extract_assistant_content() {
        let content = r#"{"type":"init","session_id":"abc","model":"gemini-pro"}
{"type":"message","role":"assistant","content":"feat: add feature"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Gemini));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_gemini_uses_last_message() {
        let content = r#"{"type":"message","role":"assistant","content":"first"}
{"type":"message","role":"assistant","content":"final"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Gemini));
        assert_eq!(result.content, "final");
    }

    #[test]
    fn test_gemini_handles_delta_accumulation() {
        let content = r#"{"type":"message","role":"assistant","content":"feat","delta":true}
{"type":"message","role":"assistant","content":": ","delta":true}
{"type":"message","role":"assistant","content":"add","delta":true}
{"type":"message","role":"assistant","content":" feature","delta":true}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Gemini));
        assert_eq!(result.content, "feat: add feature");
    }

    // =========================================================================
    // OpenCode Extraction Tests
    // =========================================================================

    #[test]
    fn test_opencode_extract_text_parts() {
        let content = r#"{"type":"text","part":{"text":"feat:"}}
{"type":"text","part":{"text":" add feature"}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::OpenCode));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_opencode_handles_mixed_content() {
        let content = r#"{"type":"step_start","step":"thinking"}
{"type":"text","part":{"text":"chore:"}}
{"type":"step_finish","step":"thinking"}
{"type":"text","part":{"text":" update code"}}"#;

        let result = extract_llm_output(content, Some(OutputFormat::OpenCode));
        assert_eq!(result.content, "chore: update code");
    }

    // =========================================================================
    // Fallback Tests
    // =========================================================================

    #[test]
    fn test_fallback_plain_text() {
        let content = "feat: some change";
        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        // Should use plain text fallback
        assert_eq!(result.content, "feat: some change");
    }

    #[test]
    fn test_fallback_removes_markdown_fences() {
        let content = "```\nfeat: add feature\n```";
        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_fallback_removes_prefixes() {
        let content = "Commit message: feat: add feature";
        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_fallback_with_ai_analysis() {
        let content = "Looking at this diff, I can see changes.\n\nfeat: add feature";
        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_empty_input_returns_empty() {
        let result = extract_llm_output("", Some(OutputFormat::Claude));
        assert!(result.content.is_empty());
    }

    #[test]
    fn test_whitespace_only_returns_empty() {
        let result = extract_llm_output("   \n  \n  ", Some(OutputFormat::Claude));
        assert!(result.content.is_empty());
    }

    // =========================================================================
    // Format Auto-Detection Tests
    // =========================================================================

    #[test]
    fn test_auto_detect_claude() {
        let content = r#"{"type":"system","session_id":"abc"}
{"type":"result","result":"test"}"#;
        let result = extract_llm_output(content, None);
        assert_eq!(result.format, OutputFormat::Claude);
    }

    #[test]
    fn test_auto_detect_codex() {
        // Codex format with actual extractable content
        let content = r#"{"type":"thread.started","thread_id":"123"}
{"type":"item.completed","item":{"type":"agent_message","text":"feat: add feature"}}"#;
        let result = extract_llm_output(content, None);
        assert_eq!(result.format, OutputFormat::Codex);
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_auto_detect_gemini() {
        // Gemini format with actual extractable content
        let content = r#"{"type":"init","model":"gemini-pro"}
{"type":"message","role":"assistant","content":"feat: add feature"}"#;
        let result = extract_llm_output(content, None);
        assert_eq!(result.format, OutputFormat::Gemini);
        assert_eq!(result.content, "feat: add feature");
    }

    #[test]
    fn test_auto_detect_generic_plain_text() {
        let content = "Just plain text\nNo JSON here";
        let result = extract_llm_output(content, None);
        assert_eq!(result.format, OutputFormat::Generic);
    }

    // =========================================================================
    // Error Handling Tests
    // =========================================================================

    #[test]
    fn test_malformed_json_falls_back_gracefully() {
        let content = r#"{"type":"result","result":incomplete json}
feat: actual message"#;
        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        // Should fall back to plain text and extract the message
        assert!(result.content.contains("feat: actual message"));
    }

    #[test]
    fn test_unknown_format_tries_all_parsers() {
        let content = r#"{"type":"unknown","result":"feat: add feature"}"#;
        let result = extract_llm_output(content, Some(OutputFormat::Codex));
        // Should try other formats and potentially fall back
        assert!(!result.content.is_empty() || result.warning.is_some());
    }
}
