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
                                // Apply thought process filtering to result field content
                                let filtered = remove_thought_process_patterns(result);
                                last_result = Some(filtered);
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
                                    let block_type = block.get("type").and_then(|v| v.as_str());
                                    // Skip thinking/reasoning blocks - only extract text content
                                    if block_type == Some("thinking")
                                        || block_type == Some("reasoning")
                                    {
                                        continue;
                                    }
                                    if block_type == Some("text") {
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
                    // Apply thought process filtering to result field content
                    let filtered = remove_thought_process_patterns(result);
                    last_result = Some(filtered);
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
                                    // Apply thought process filtering
                                    let filtered = remove_thought_process_patterns(text);
                                    last_message = Some(filtered);
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

    // Apply thought process filtering to the final accumulated content
    last_assistant_content.map(|content| remove_thought_process_patterns(&content))
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
        // Apply thought process filtering to the accumulated text
        Some(remove_thought_process_patterns(&accumulated_text))
    }
}

/// Remove AI thought process patterns from extracted content.
///
/// This is a helper function that filters out common AI thought process
/// prefixes that may appear in extracted result field content.
///
/// The function handles multiple AI output formats:
/// - Analysis followed by double newline (standard format)
/// - Analysis followed by single newline (aggressive filtering)
/// - Numbered/bullet lists without proper separation
/// - Multi-line analysis that ends with conventional commit format
fn remove_thought_process_patterns(content: &str) -> String {
    let result = content;

    // Remove AI thought process prefixes
    // These are patterns that AI agents commonly use when starting their response
    // We remove everything from the start up to and including the first blank line
    let thought_patterns = [
        "Looking at this diff, I can see",
        "Looking at this diff",
        "I can see",
        "The main changes are",
        "Several distinct categories of changes",
        "Key categories of changes",
        "Based on the diff",
        "Analyzing the changes",
        "This diff shows",
        "Looking at the changes",
        "I've analyzed",
        "After reviewing",
    ];

    for pattern in &thought_patterns {
        if let Some(rest) = result.strip_prefix(pattern) {
            // Find the first blank line after the pattern
            if let Some(blank_line_pos) = rest.find("\n\n") {
                // Don't return immediately - there might be more analysis after the blank line
                // Instead, update result to continue processing
                let remaining = rest[blank_line_pos + 2..].trim();
                if !remaining.is_empty() {
                    // Continue processing with the remaining content
                    // Check if it still starts with analysis patterns (numbered lists, etc.)
                    // If it looks like a clean commit message, return it
                    if looks_like_commit_message_start(remaining) {
                        return remaining.to_string();
                    }
                    // Otherwise, continue to aggressive filtering below
                }
            } else if let Some(single_newline) = rest.find('\n') {
                // If no double newline, try to skip to after the first single newline
                let after_newline = &rest[single_newline + 1..];
                // Check if what follows looks like a commit message (starts with conventional commit type)
                if looks_like_commit_message_start(after_newline.trim()) {
                    return after_newline.to_string();
                }
            }
            break;
        }
    }

    // Remove numbered analysis patterns (e.g., "1. First change\n2. Second change\n\nfix: actual")
    // These are common when AI agents provide numbered analysis before the actual commit message
    let result_lower = result.to_lowercase();
    let numbered_start_patterns = [
        "1. ",
        "1)\n",
        "- first",
        "- the first",
        "* first",
        "* the first",
    ];
    for pattern in &numbered_start_patterns {
        if result_lower.starts_with(pattern) || result.starts_with(pattern) {
            // Try to find the commit message by looking for conventional commit format
            if let Some(commit_start) = find_conventional_commit_start(result) {
                return result[commit_start..].to_string();
            }
            // Fallback: look for a blank line after the analysis
            if let Some(blank_pos) = result.find("\n\n") {
                let after_analysis = &result[blank_pos + 2..];
                // Check if the content after looks like a real commit message
                if after_analysis.trim().starts_with(char::is_alphanumeric) {
                    return after_analysis.to_string();
                }
            }
            break;
        }
    }

    // Remove markdown bold analysis patterns (e.g., "1. **Test assertion style** (file.rs): Description")
    // These patterns use markdown bold formatting for category headers in numbered lists
    if starts_with_markdown_bold_analysis(result) {
        if let Some(commit_start) = find_conventional_commit_start(result) {
            return result[commit_start..].to_string();
        }
        // Fallback: look for double newline after the analysis
        if let Some(blank_pos) = result.find("\n\n") {
            let after_analysis = &result[blank_pos + 2..];
            if after_analysis.trim().starts_with(char::is_alphanumeric) {
                return after_analysis.to_string();
            }
        }
    }

    // Additional aggressive filtering: detect if the content starts with
    // multi-line analysis and ends with a conventional commit format
    if let Some(commit_start) = find_conventional_commit_start(result) {
        // Verify that the content before the commit looks like analysis
        let before_commit = &result[..commit_start];
        if before_commit.contains('\n') && looks_like_analysis_text(before_commit) {
            return result[commit_start..].to_string();
        }
    }

    result.to_string()
}

/// Check if text starts with markdown bold analysis patterns.
///
/// Returns true if the text starts with patterns like:
/// - "1. **Category** (file.rs): Description"
/// - "**Category**:"
/// - Multiple numbered lines with **bold** headers
fn starts_with_markdown_bold_analysis(text: &str) -> bool {
    let trimmed = text.trim();

    // Check for patterns like "1. **Category**" or "**Category**:"
    // These are markdown bold patterns used for analysis headers
    let lines: Vec<&str> = trimmed.lines().collect();

    if lines.is_empty() {
        return false;
    }

    // Check the first line for markdown bold patterns
    let first_line = lines[0].trim();

    // Pattern 1: "1. **Bold Text**" or "1. **Bold Text** (file.rs): description"
    if first_line.starts_with("1. **") || first_line.starts_with("1. **") {
        return true;
    }

    // Pattern 2: Line starts with ** (markdown bold opening)
    if first_line.starts_with("**") {
        // Check if it looks like a header/analysis, not a valid commit message
        // Valid commits don't start with **, but analysis headers do
        return true;
    }

    // Pattern 3: Check if first few lines contain markdown bold patterns
    // like "**Category**:" which indicates analysis breakdown
    if lines.len() >= 2 {
        let mut bold_header_count = 0;
        for line in lines.iter().take(5) {
            let trimmed = line.trim();
            // Check for patterns like "**Category**:" or "**Category** (file):"
            if (trimmed.contains("**") && trimmed.contains("**:"))
                || (trimmed.contains("**") && trimmed.contains("** ("))
            {
                bold_header_count += 1;
            }
        }
        if bold_header_count >= 1 {
            return true;
        }
    }

    false
}

/// Check if text starts with a conventional commit type pattern.
///
/// Returns true if the text starts with patterns like:
/// - "feat:", "fix:", "chore:", "docs:", "test:", "refactor:", "perf:", "style:"
/// - With optional scope in parentheses: "feat(parser):", "fix(api):"
fn looks_like_commit_message_start(text: &str) -> bool {
    let trimmed = text.trim();
    let conventional_types = [
        "feat", "fix", "chore", "docs", "test", "refactor", "perf", "style", "build", "ci",
        "revert",
    ];

    for commit_type in &conventional_types {
        // Check for "type:" or "type(scope):" pattern
        if let Some(rest) = trimmed.strip_prefix(commit_type) {
            if rest.starts_with(':')
                || (rest.starts_with('(') && rest[1..].contains("):"))
                || (rest.starts_with('(') && rest[1..].contains("): "))
            {
                return true;
            }
        }
    }

    false
}

/// Find the position of a conventional commit message in the text.
///
/// Returns Some(position) if found, None otherwise.
fn find_conventional_commit_start(text: &str) -> Option<usize> {
    let conventional_types = [
        "feat", "fix", "chore", "docs", "test", "refactor", "perf", "style", "build", "ci",
        "revert",
    ];

    // Look for each commit type pattern
    for commit_type in &conventional_types {
        let mut search_pos = 0;
        while search_pos < text.len() {
            if let Some(pos) = text[search_pos..].find(commit_type) {
                let actual_pos = search_pos + pos;
                let rest = &text[actual_pos + commit_type.len()..];

                // Check if this is a valid conventional commit pattern
                if rest.starts_with(':') || (rest.starts_with('(') && rest[1..].contains("):")) {
                    // Make sure it's at the start of a line or preceded by newline
                    let prefix = &text[..actual_pos];
                    if prefix.is_empty() || prefix.ends_with('\n') {
                        return Some(actual_pos);
                    }
                }
                search_pos = actual_pos + commit_type.len();
            } else {
                break;
            }
        }
    }

    None
}

/// Check if text looks like AI analysis (not a commit message).
///
/// Returns true if the text contains patterns typical of AI analysis
/// such as numbered lists, bullet points, or analysis phrases.
fn looks_like_analysis_text(text: &str) -> bool {
    let text_lower = text.to_lowercase();

    // Check for analysis indicator phrases
    let analysis_indicators = [
        "looking at",
        "analyzing",
        "the changes",
        "the diff",
        "i can see",
        "main changes",
        "categories",
        "first change",
        "second change",
        "third change",
    ];

    for indicator in &analysis_indicators {
        if text_lower.contains(indicator) {
            return true;
        }
    }

    // Check for numbered/bullet list patterns
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() >= 2 {
        let mut numbered_count = 0;
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with("1. ")
                || trimmed.starts_with("2. ")
                || trimmed.starts_with("3. ")
                || trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
            {
                numbered_count += 1;
            }
        }
        if numbered_count >= 2 {
            return true;
        }
    }

    false
}

/// Remove formatted thinking output patterns from CLI display output.
///
/// This handles formatted thinking content that appears in log files from display
/// formatting, such as `[Claude] Thinking: ...` or `[Agent] Thinking: ...`.
/// These patterns may include ANSI color codes.
///
/// The function removes lines that contain formatted thinking markers and any
/// subsequent content until a blank line or conventional commit pattern is found.
fn remove_formatted_thinking_patterns(content: &str) -> String {
    let mut result = String::new();
    let mut skip_until_blank = false;

    // Check for formatted thinking patterns
    // Patterns like: "[Claude] Thinking:", "[Agent] Thinking:", "Thinking:" with ANSI codes
    let thinking_patterns = [
        "] Thinking:",
        "] thinking:",
        "[Claude] Thinking:",
        "[claude] Thinking:",
        "[Claude] thinking:",
        "[claude] thinking:",
        "[Agent] Thinking:",
        "[agent] Thinking:",
        "[Agent] thinking:",
        "[agent] thinking:",
        "[Assistant] Thinking:",
        "[assistant] Thinking:",
        "[Assistant] thinking:",
        "[assistant] thinking:",
    ];

    // Strip ANSI escape codes for pattern matching
    let strip_ansi = |text: &str| -> String {
        // ANSI escape codes match pattern: \x1b[...m or \x1b[...K
        let re = Regex::new(r"\x1b\[[0-9;]*[mK]").expect("ANSI regex should be valid");
        re.replace_all(text, "").to_string()
    };

    for line in content.lines() {
        let stripped_line = strip_ansi(line);

        let is_thinking_marker = thinking_patterns
            .iter()
            .any(|pattern| stripped_line.contains(pattern));

        if is_thinking_marker {
            skip_until_blank = true;
            continue;
        }

        // Skip lines while we're in a thinking block
        if skip_until_blank {
            // Check if this is a blank line
            if line.trim().is_empty() {
                skip_until_blank = false;
            }
            // Also check if we've hit a conventional commit pattern
            else if looks_like_commit_message_start(line.trim()) {
                skip_until_blank = false;
                // Don't skip this line - it's the actual content
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(line);
            }
            // Otherwise, continue skipping
            continue;
        }

        // Not in a thinking block, keep this line
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(line);
    }

    // If we ended up with empty content, return a cleaned version of the original
    // This handles edge cases where the thinking detection was too aggressive
    if result.trim().is_empty() && !content.trim().is_empty() {
        // Return the original content minus obvious thinking-only lines
        content
            .lines()
            .filter(|line| {
                let stripped = strip_ansi(line);
                !thinking_patterns.iter().any(|p| stripped.contains(p))
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        result
    }
}

/// Clean plain text output by removing common artifacts.
///
/// This handles:
/// - Markdown code fences
/// - Formatted thinking output (e.g., "[Claude] Thinking: ...")
/// - AI thought process patterns (e.g., "Looking at this diff...")
/// - Common prefixes like "Commit message:", "Output:", etc.
/// - Excessive whitespace
fn clean_plain_text(content: &str) -> String {
    let mut result = content.to_string();

    // Remove formatted thinking patterns from CLI display output
    result = remove_formatted_thinking_patterns(&result);

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

    // Remove AI thought process prefixes using the helper function
    result = remove_thought_process_patterns(&result);

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
    for prefix in &prefixes {
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

    // Check for AI thought process leakage at the start of the message
    // This validation catches cases where the filtering in remove_thought_process_patterns
    // failed to remove the AI analysis before the actual commit message
    let thought_process_prefixes = [
        "looking at this diff",
        "i can see",
        "the main changes are",
        "several distinct categories",
        "key categories",
        "based on the diff",
        "analyzing the changes",
        "this diff shows",
        "looking at the changes",
        "i've analyzed",
        "after reviewing",
    ];
    for prefix in &thought_process_prefixes {
        if content_lower.starts_with(prefix) {
            return Err(format!(
                "Commit message starts with AI thought process ({prefix}). This indicates a bug in the thought process filtering."
            ));
        }
    }

    // Check for numbered analysis at the start (1., 2., 3., etc.)
    if content.trim_start().starts_with("1. ")
        || content.trim_start().starts_with("1)\n")
        || content_lower.starts_with("- first")
        || content_lower.starts_with("* first")
    {
        return Err(
            "Commit message starts with numbered analysis. This indicates AI thought process leakage.".to_string()
        );
    }

    // Check for formatted thinking output patterns (e.g., "[Claude] Thinking:")
    // This catches formatted thinking output from CLI display that leaked into the log
    let formatted_thinking_patterns = [
        "[claude] thinking:",
        "[claude] Thinking:",
        "[agent] thinking:",
        "[agent] Thinking:",
        "[assistant] thinking:",
        "[assistant] Thinking:",
        "] thinking:",
        "] Thinking:",
    ];
    for pattern in &formatted_thinking_patterns {
        if content_lower.starts_with(pattern) || content.contains(pattern) {
            return Err(format!(
                "Commit message contains formatted thinking pattern ({pattern}). This indicates AI thinking output leaked into the commit message."
            ));
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

    // =========================================================================
    // Thought Process Filtering Tests (Bug Fix Regression)
    // =========================================================================

    #[test]
    fn test_clean_plain_text_removes_looking_at_diff_pattern() {
        let content =
            "Looking at this diff, I can see the main changes are:\n\nfix: actual commit message";
        let result = clean_plain_text(content);
        assert_eq!(result, "fix: actual commit message");
        assert!(!result.contains("Looking at this diff"));
    }

    #[test]
    fn test_clean_plain_text_removes_looking_at_this_diff_short() {
        let content = "Looking at this diff\n\nfeat: add feature";
        let result = clean_plain_text(content);
        assert_eq!(result, "feat: add feature");
        assert!(!result.contains("Looking at this diff"));
    }

    #[test]
    fn test_clean_plain_text_removes_several_categories_pattern() {
        let content =
            "Several distinct categories of changes in this diff:\n\nchore: update dependencies";
        let result = clean_plain_text(content);
        assert_eq!(result, "chore: update dependencies");
        assert!(!result.contains("Several distinct categories"));
    }

    #[test]
    fn test_clean_plain_text_removes_key_categories_pattern() {
        let content = "Key categories of changes include:\n\nrefactor: improve error handling";
        let result = clean_plain_text(content);
        assert_eq!(result, "refactor: improve error handling");
        assert!(!result.contains("Key categories"));
    }

    #[test]
    fn test_clean_plain_text_removes_numbered_analysis() {
        let content = "1. Added new function\n2. Fixed bug\n\nfix: resolve parsing issue";
        let result = clean_plain_text(content);
        assert_eq!(result, "fix: resolve parsing issue");
        assert!(!result.contains("1. Added"));
    }

    #[test]
    fn test_clean_plain_text_preserves_valid_commit_message() {
        let content = "feat: add new authentication system";
        let result = clean_plain_text(content);
        assert_eq!(result, "feat: add new authentication system");
    }

    #[test]
    fn test_clean_plain_text_preserves_commit_message_with_body() {
        let content = "feat: add new authentication system\n\nThis adds OAuth2 support.";
        let result = clean_plain_text(content);
        assert!(result.contains("feat: add new authentication system"));
        assert!(result.contains("OAuth2"));
    }

    #[test]
    fn test_claude_result_field_with_looking_at_diff() {
        // Test the bug: result field contains "Looking at this diff..." prefix
        let content = r#"{"type":"result","result":"Looking at this diff, I can see the main changes are:\n\nfix: resolve parsing issue"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "fix: resolve parsing issue");
        assert!(!result.content.contains("Looking at this diff"));
    }

    #[test]
    fn test_claude_result_field_with_numbered_list() {
        let content = r#"{"type":"result","result":"1. First change\n2. Second change\n3. Third change\n\nfeat: add new feature"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add new feature");
        assert!(!result.content.contains("1. First"));
    }

    #[test]
    fn test_claude_result_field_with_analysis_then_commit() {
        let content = r#"{"type":"result","result":"The main changes are:\n- Updated parser\n- Fixed tests\n\nchore: update tests"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "chore: update tests");
        assert!(!result.content.contains("The main changes"));
    }

    #[test]
    fn test_claude_result_field_valid_commit_passes_through() {
        // Valid commit message should pass through unchanged
        let content = r#"{"type":"result","result":"docs: update README with new examples"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "docs: update README with new examples");
    }

    #[test]
    fn test_plain_text_with_thought_process_fallback() {
        // Test plain text fallback with thought process
        let content = "Looking at this diff\n\nfix: bug fix";

        let result = extract_llm_output(content, Some(OutputFormat::Generic));
        assert!(!result.was_structured);
        assert_eq!(result.content, "fix: bug fix");
    }

    #[test]
    fn test_thought_pattern_with_single_newline_fallback() {
        // Edge case: only single newline after pattern (no double newline)
        let content = "Looking at this diff, I can see:\nfix: actual message";

        let result = extract_llm_output(content, Some(OutputFormat::Generic));
        assert!(!result.was_structured);
        // Should still extract after the first newline
        assert!(result.content.contains("fix: actual message") || result.content.contains("fix:"));
    }

    #[test]
    fn test_simple_result_json_with_thought_process() {
        // Legacy simple format with thought process
        let content = r#"{"result": "Looking at this diff\n\nfeat: add feature"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add feature");
        assert!(!result.content.contains("Looking at this diff"));
    }

    // =========================================================================
    // Additional Regression Tests for Thought Process Leakage (Bug Fix)
    // =========================================================================

    #[test]
    fn test_regression_multiline_analysis_single_newline_before_commit() {
        // Test multi-line analysis followed by commit message with single newline separator
        let content = r#"{"result":"Looking at this diff, I can see several changes:\n- Updated parser logic\n- Fixed test cases\nfix: resolve parsing issue"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "fix: resolve parsing issue");
        assert!(!result.content.contains("Looking at this diff"));
        assert!(!result.content.contains("Updated parser"));
    }

    #[test]
    fn test_regression_numbered_list_no_double_newline() {
        // Test numbered list analysis (1., 2., 3.) without double newline before commit message
        let content = r#"{"result":"1. Added new function\n2. Fixed bug in parser\n3. Updated tests\nfeat: add new feature"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat: add new feature");
        assert!(!result.content.contains("1. Added"));
        assert!(!result.content.contains("2. Fixed"));
    }

    #[test]
    fn test_regression_bullet_points_before_commit() {
        // Test analysis with bullet points followed by commit message
        let content = r#"{"result":"- First change to the parser\n- Second change to tests\n- Third change to docs\nchore: update dependencies"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "chore: update dependencies");
        assert!(!result.content.contains("- First"));
    }

    #[test]
    fn test_regression_exact_real_world_bug_format() {
        // Test the exact real-world commit message format from the bug report
        // Multi-line analysis followed by commit message without proper separation
        let content = r#"{"result":"Looking at this diff, I can see several distinct types of changes:\n1. Changes to the parser module\n2. Updates to test cases\n3. Documentation updates\nfeat(parser): add support for new syntax"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "feat(parser): add support for new syntax");
        assert!(!result.content.contains("Looking at this diff"));
        assert!(!result.content.contains("1. Changes"));
    }

    #[test]
    fn test_regression_commit_immediately_after_analysis_no_separator() {
        // Test edge case where commit message follows immediately after analysis with no separator
        let content =
            r#"{"result":"The main changes are to the parser\nfix: resolve edge case in parsing"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        // Should extract the commit message by finding the conventional commit format
        assert!(result.was_structured);
        assert!(result.content.contains("fix: resolve edge case"));
        assert!(!result.content.contains("The main changes"));
    }

    #[test]
    fn test_regression_analysis_with_multiple_newlines_before_commit() {
        // Test analysis followed by multiple newlines before commit
        let content = r#"{"result":"Several categories of changes:\n\n\nfix: bug fix"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "fix: bug fix");
        assert!(!result.content.contains("Several categories"));
    }

    #[test]
    fn test_regression_analysis_with_scope_in_commit() {
        // Test that scope in conventional commits is preserved
        let content = r#"{"result":"Looking at the changes I can see:\nfeat(core): add new module\ndocs(core): update API documentation"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(result.content.contains("feat(core):"));
        assert!(!result.content.contains("Looking at the changes"));
    }

    #[test]
    fn test_regression_long_multiline_analysis() {
        // Test longer multi-line analysis with various phrases
        let content = r#"{"result":"After reviewing this diff, I can see the following:\nThe codebase has been updated with new features\nKey areas modified include:\n- Parser improvements\n- Test coverage enhancements\n- Documentation updates\nrefactor(api): improve error handling"}"#;

        let result = extract_llm_output(content, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert_eq!(result.content, "refactor(api): improve error handling");
        assert!(!result.content.contains("After reviewing"));
        assert!(!result.content.contains("Key areas"));
    }

    // =========================================================================
    // Validation Tests for Thought Process Leakage
    // =========================================================================

    #[test]
    fn test_validate_rejects_thought_process_looking_at_diff() {
        let err = validate_commit_message("Looking at this diff, I can see...").unwrap_err();
        assert!(err.contains("thought process"));
        assert!(err.contains("looking at this diff"));
    }

    #[test]
    fn test_validate_rejects_thought_process_i_can_see() {
        let err = validate_commit_message("I can see several changes...").unwrap_err();
        assert!(err.contains("thought process"));
    }

    #[test]
    fn test_validate_rejects_thought_process_numbered_analysis() {
        let err = validate_commit_message("1. First change\n2. Second change").unwrap_err();
        assert!(err.contains("numbered analysis"));
    }

    #[test]
    fn test_validate_rejects_thought_process_dash_first() {
        let err = validate_commit_message("- first change to parser").unwrap_err();
        assert!(err.contains("numbered analysis"));
    }

    #[test]
    fn test_validate_accepts_commit_after_analysis_removal() {
        // After filtering, the clean commit message should be accepted
        assert!(validate_commit_message("feat: add new feature").is_ok());
        assert!(validate_commit_message("fix(parser): resolve edge case").is_ok());
    }

    // =========================================================================
    // Integration Tests: End-to-End Commit Message Generation
    // =========================================================================

    #[test]
    fn test_integration_claude_result_field_with_thought_process_leakage() {
        // Integration test: Real-world scenario where AI agent outputs thought process
        // followed by the actual commit message in the result field
        let agent_output = r#"{"type":"system","subtype":"init"}
{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Looking at this diff, I can see several changes"}}}
{"type":"result","result":"Looking at this diff, I can see several distinct types of changes:\n1. Updates to parser module\n2. New test cases\n3. Documentation improvements\nfeat(parser): add support for new syntax"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        // Should extract structured content
        assert!(result.was_structured);

        // The extracted content should NOT contain the thought process
        assert!(!result.content.contains("Looking at this diff"));
        assert!(!result.content.contains("1. Updates"));

        // The extracted content should be the clean commit message
        assert_eq!(result.content, "feat(parser): add support for new syntax");

        // Should pass validation
        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_integration_numbered_list_without_proper_separation() {
        // Test the bug scenario: numbered list analysis without double newline
        let agent_output = r#"{"result":"1. Added new function to parser\n2. Fixed bug in token handling\n3. Updated tests for edge cases\nfix: resolve tokenization edge cases"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert_eq!(result.content, "fix: resolve tokenization edge cases");
        assert!(!result.content.contains("1. Added"));
        assert!(!result.content.contains("2. Fixed"));

        // Validate the clean message passes
        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_integration_bullet_point_analysis_before_commit() {
        // Test bullet point analysis pattern
        let agent_output = r#"{"result":"- Updated error handling\n- Improved performance\n- Fixed memory leak\nperf(core): optimize memory usage"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert!(result.content.contains("perf(core):"));
        assert!(!result.content.contains("- Updated"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_integration_validation_catches_thought_process_leakage() {
        // If filtering fails, validation should catch the problem
        let leaked_message = "Looking at this diff, I can see several changes...";

        let validation_result = validate_commit_message(leaked_message);
        assert!(validation_result.is_err());
        let err = validation_result.unwrap_err();
        assert!(err.contains("thought process"));
    }

    #[test]
    fn test_integration_validation_catches_numbered_analysis_leakage() {
        // Validation should also catch numbered list leakage
        let leaked_message = "1. First change\n2. Second change\n3. Third change";

        let validation_result = validate_commit_message(leaked_message);
        assert!(validation_result.is_err());
        let err = validation_result.unwrap_err();
        assert!(err.contains("numbered analysis"));
    }

    #[test]
    fn test_integration_clean_commit_message_passes_validation() {
        // Verify that clean commit messages pass validation
        let clean_commits = [
            "feat: add new authentication system",
            "fix(parser): resolve edge case in parsing",
            "docs: update README with new examples",
            "chore: update dependencies",
            "refactor(api): extract validation logic",
            "perf(core): optimize memory usage",
        ];

        for commit in clean_commits {
            assert!(
                validate_commit_message(commit).is_ok(),
                "Clean commit should pass validation: {commit}"
            );
        }
    }

    #[test]
    fn test_integration_conventional_commit_with_scope_preserved() {
        // Test that scopes in conventional commits are preserved through extraction
        let agent_output = r#"{"result":"Looking at this diff, I can see changes to multiple modules:\nfeat(core): add new module\nfeat(api): add new endpoint\ndocs(api): update API documentation"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        // Should preserve the first conventional commit
        assert!(result.content.contains("feat(core):"));
        assert!(!result.content.contains("Looking at this diff"));
    }

    #[test]
    fn test_integration_multiline_commit_body_preserved() {
        // Test that commit message body is preserved after filtering
        let agent_output = r#"{"result":"Looking at this diff I can see:\n\nfeat: add new authentication system\n\nThis adds OAuth2 support with the following features:\n- Authorization code flow\n- Refresh token handling\n- Secure token storage"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert!(result
            .content
            .contains("feat: add new authentication system"));
        assert!(result.content.contains("OAuth2"));
        assert!(!result.content.contains("Looking at this diff"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_integration_empty_result_field_falls_back_gracefully() {
        // Test that empty result field is handled
        let agent_output = r#"{"type":"system"}
{"result":""}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        // Should return empty content, not crash
        assert!(result.content.is_empty() || !result.was_structured);
    }

    #[test]
    fn test_integration_codex_format_with_thought_process() {
        // Test Codex format also applies thought process filtering
        let agent_output = r#"{"type":"thread.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"Looking at this diff:\n1. First change\n2. Second change\nfix: actual commit message"}}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Codex));

        assert!(result.was_structured);
        assert!(result.content.contains("fix: actual commit message"));
        assert!(!result.content.contains("Looking at this diff"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_integration_gemini_format_with_thought_process() {
        // Test Gemini format also applies thought process filtering
        let agent_output = r#"{"type":"init"}
{"type":"message","role":"assistant","content":"After reviewing the changes, I can see:\n- Updated parser\n- Fixed tests\nrefactor: improve error handling"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Gemini));

        assert!(result.was_structured);
        assert!(result.content.contains("refactor: improve error handling"));
        assert!(!result.content.contains("After reviewing"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_integration_auto_detection_with_thought_process() {
        // Test auto-detection (no format specified) with thought process
        let agent_output = r#"{"type":"result","result":"I can see the changes are:\n1. Bug fixes\n2. New features\nfeat: add feature"}"#;

        let result = extract_llm_output(agent_output, None);

        assert!(result.was_structured);
        assert!(result.content.contains("feat: add feature"));
        assert!(!result.content.contains("I can see"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    // =========================================================================
    // Exact Bug Scenario Reproduction Test (Task-Specific)
    // =========================================================================

    #[test]
    fn test_exact_bug_scenario_from_task() {
        // Reproduction test for the exact bug scenario from the task description.
        // This test demonstrates that the AI thought process leakage bug is fixed.
        //
        // The bug manifested when AI agents included their internal analysis
        // before the actual commit message, and the entire output (including the
        // analysis) was used as the commit message.
        //
        // Example broken output:
        // Looking at this diff, I can see several distinct types of changes...
        // 1. **Test assertion style**...
        // 2. **Refactoring**...
        // The main cohesive theme is **code quality improvements**...
        // refactor: extract PipelineContext and improve error handling

        // Note: Using single-line JSON with \n escape sequences to match how
        // real JSONL output would format this (the extract_claude_result function
        // processes content line-by-line)
        let broken_output = r#"{"result":"Looking at this diff, I can see several distinct types of changes across multiple files:\n\n1. **Test assertion style** (agents/config.rs, pipeline/tests.rs): Replacing...\n2. **Refactoring** (app/mod.rs): Extracting...\n3. **String literal consistency**...\n\nThe main cohesive theme is **code quality improvements**...\n\nrefactor: extract PipelineContext and improve error handling"}"#;

        let result = extract_llm_output(broken_output, Some(OutputFormat::Claude));

        // Should extract structured content
        assert!(result.was_structured);

        // The extracted content should be just the commit message, not the analysis
        assert!(result.content.contains("refactor: extract PipelineContext"));
        assert!(!result.content.contains("Looking at this diff"));
        assert!(!result.content.contains("1. **Test assertion style**"));
        assert!(!result.content.contains("2. **Refactoring**"));
        assert!(!result.content.contains("3. **String literal consistency**"));
        assert!(!result.content.contains("The main cohesive theme"));
        assert!(!result.content.contains("code quality improvements"));

        // The result should pass validation
        assert!(validate_commit_message(&result.content).is_ok());
    }

    // =========================================================================
    // Additional Regression Tests for Markdown Bold Patterns
    // =========================================================================

    #[test]
    fn test_regression_markdown_bold_pattern_with_numbered_list() {
        // Test the exact bug scenario: markdown bold headers in numbered list
        let agent_output = r#"{"result":"1. **Test assertion style** (agents/config.rs, pipeline/tests.rs): Replacing assertion macros with idiomatically Rust equivalents\n2. **Refactoring** (app/mod.rs): Extracting...\n\nrefactor: extract PipelineContext and improve error handling"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert!(result.content.contains("refactor: extract PipelineContext"));
        assert!(!result.content.contains("1. **Test assertion style**"));
        assert!(!result.content.contains("2. **Refactoring**"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_regression_markdown_bold_single_line() {
        // Test single line starting with markdown bold
        let agent_output = r#"{"result":"**Analysis**: This diff shows changes to the parser module\n\nfix: resolve parsing edge case"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert!(result.content.contains("fix: resolve parsing edge case"));
        assert!(!result.content.contains("**Analysis**"));
        assert!(!result.content.contains("**"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_regression_markdown_bold_multiple_lines() {
        // Test multiple lines with markdown bold patterns
        let agent_output = r#"{"result":"1. **Parser changes**: Updated token handling\n2. **Test updates**: Added edge case coverage\n3. **Documentation**: Updated API docs\n\nfeat(parser): add support for new syntax"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert!(result.content.contains("feat(parser): add support"));
        assert!(!result.content.contains("**Parser changes**"));
        assert!(!result.content.contains("**Test updates**"));
        assert!(!result.content.contains("**Documentation**"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_regression_markdown_bold_with_colon_pattern() {
        // Test pattern like "**Category**:" which is common in analysis
        let agent_output = r#"{"result":"**Summary**:\nThe main change is improving error handling\n\n**Details**:\n- Updated parser\n- Fixed tests\n\nrefactor: improve error handling"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert!(result.content.contains("refactor: improve error handling"));
        assert!(!result.content.contains("**Summary**"));
        assert!(!result.content.contains("**Details**"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    #[test]
    fn test_regression_markdown_bold_at_start_only() {
        // Test content that starts with ** but is actually analysis
        let agent_output = r#"{"result":"**Main Theme**: Code quality improvements across multiple files\n\nChanges include updated tests and refactored parsing logic\n\nstyle: improve code quality and test consistency"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));

        assert!(result.was_structured);
        assert!(result.content.contains("style: improve code quality"));
        assert!(!result.content.contains("**Main Theme**"));

        assert!(validate_commit_message(&result.content).is_ok());
    }

    // =========================================================================
    // Regression Test: Validation Rejection
    // =========================================================================

    #[test]
    fn test_validation_rejection_of_thought_process_leakage() {
        // Regression test for the bug where invalid commit messages (with AI thought
        // process leakage) were still being used despite validation failures.
        //
        // This test verifies that validation correctly rejects various types of
        // invalid commit messages. When extract_commit_message_from_logs encounters
        // these validation failures, it now returns Ok(None) instead of Ok(Some(invalid_message)).

        // Case 1: Thought process leakage at the start (no actual commit message)
        // This represents a case where filtering completely failed
        let thought_process_only =
            "Looking at this diff, I can see the changes are about refactoring error handling.";
        let validation_result = validate_commit_message(thought_process_only);
        assert!(
            validation_result.is_err(),
            "Validation should fail for thought process leakage: {thought_process_only}"
        );
        assert!(validation_result
            .unwrap_err()
            .contains("AI thought process"));

        // Case 2: Thought process with analysis keywords
        let analysis_style =
            "The main changes are:\n- Updated error handling\n- Fixed parsing bugs\n- Added tests";
        let validation_result = validate_commit_message(analysis_style);
        assert!(
            validation_result.is_err(),
            "Validation should fail for analysis style: {analysis_style}"
        );

        // Case 3: Numbered analysis at the start (no commit message found)
        let numbered_only = "1. First, let me analyze the diff\n2. Then create a commit message\n3. Finally validate";
        let validation_result = validate_commit_message(numbered_only);
        assert!(
            validation_result.is_err(),
            "Validation should fail for numbered analysis: {numbered_only}"
        );
        assert!(validation_result.unwrap_err().contains("numbered analysis"));

        // Case 4: JSON artifacts (extraction failure)
        let json_artifact_output =
            "fix: some fix\n\nNote: This also contains {\"type\":\"result\" which should fail";
        let validation_result = validate_commit_message(json_artifact_output);
        assert!(
            validation_result.is_err(),
            "Validation should fail for JSON artifacts"
        );
        assert!(validation_result.unwrap_err().contains("JSON artifacts"));

        // Case 5: Empty message
        let validation_result = validate_commit_message("");
        assert!(
            validation_result.is_err(),
            "Validation should fail for empty message"
        );
        assert!(validation_result.unwrap_err().contains("empty"));

        // Case 6: Too short
        let validation_result = validate_commit_message("fix");
        assert!(
            validation_result.is_err(),
            "Validation should fail for too short message"
        );
        assert!(validation_result.unwrap_err().contains("too short"));

        // Verify that valid commit messages still pass validation
        let valid_message = "fix: handle edge case in parsing

The previous implementation did not account for empty strings in the input
parser, causing a panic when processing certain edge cases.

Fixes #123";
        let validation_result = validate_commit_message(valid_message);
        assert!(
            validation_result.is_ok(),
            "Valid commit message should pass validation: {valid_message}"
        );

        // Verify that extraction + validation works for the full flow
        // Using proper JSONL format (single line JSON)
        let valid_output_jsonl = r#"{"result":"fix: handle edge case in parsing. The previous implementation did not account for empty strings, causing a panic. Fixes #123"}"#;
        let result = extract_llm_output(valid_output_jsonl, Some(OutputFormat::Claude));
        assert!(
            validate_commit_message(&result.content).is_ok(),
            "Valid extracted commit message should pass validation: {}",
            result.content
        );
    }

    // =========================================================================
    // Regression Test: Formatted Thinking Output in Logs
    // =========================================================================

    #[test]
    fn test_regression_formatted_thinking_output_in_logs() {
        // Regression test for the bug where formatted thinking output like
        // "[Claude] Thinking: ..." appears in log files from CLI display formatting
        // and gets extracted as the commit message.

        // Case 1: Simple formatted thinking followed by actual commit message
        let log_with_thinking = r"[Claude] Thinking: Looking at this diff, I can see the changes...

fix: handle edge case in parsing

The previous implementation did not account for empty strings.";

        let result = extract_llm_output(log_with_thinking, Some(OutputFormat::Generic));
        assert!(
            !result.content.contains("[Claude] Thinking:"),
            "Formatted thinking marker should be removed from: {}",
            result.content
        );
        assert!(
            !result.content.contains("Looking at this diff"),
            "Thinking content should be filtered out"
        );
        assert!(
            result.content.contains("fix: handle edge case"),
            "Actual commit message should be preserved"
        );

        // Case 2: Formatted thinking with ANSI color codes
        let log_with_ansi_thinking = "\x1b[1m[claude] Thinking:\x1b[0m Analyzing the changes...\n\nfeat(parser): add support for new syntax";

        let result = extract_llm_output(log_with_ansi_thinking, Some(OutputFormat::Generic));
        assert!(
            !result.content.contains("Thinking:"),
            "Formatted thinking marker should be removed (with ANSI)"
        );
        assert!(
            !result.content.contains("Analyzing"),
            "Thinking content should be filtered out"
        );
        assert!(
            result.content.contains("feat(parser):"),
            "Actual commit message should be preserved"
        );

        // Case 3: Multiple thinking blocks
        let log_with_multiple_thinking = r"[Agent] Thinking: Let me analyze this diff first...
[Agent] Thinking: The changes involve error handling...

fix(error): improve error messages

Error messages are now more descriptive.";

        let result = extract_llm_output(log_with_multiple_thinking, Some(OutputFormat::Generic));
        assert!(
            !result.content.contains("[Agent] Thinking:"),
            "All formatted thinking markers should be removed"
        );
        assert!(
            !result.content.contains("analyze") && !result.content.contains("Let me"),
            "All thinking content should be filtered out"
        );
        assert!(
            result.content.contains("fix(error):"),
            "Actual commit message should be preserved"
        );

        // Case 4: Thinking content without blank line separator
        let log_without_separator =
            "[Assistant] thinking: Reviewing the code changes\nfix: update imports";

        let result = extract_llm_output(log_without_separator, Some(OutputFormat::Generic));
        assert!(
            !result.content.contains("[Assistant] thinking:"),
            "Formatted thinking marker should be removed"
        );
        assert!(
            result.content.contains("fix:"),
            "Commit message after thinking content should be found"
        );

        // Case 5: Only thinking content, no actual commit message
        let log_only_thinking =
            "[claude] Thinking: I need to analyze this more carefully to understand the changes";

        let result = extract_llm_output(log_only_thinking, Some(OutputFormat::Generic));
        // Should return empty or very minimal content since everything was filtered
        assert!(
            result.content.trim().len() < 50,
            "Content should be mostly empty when only thinking is present: '{}'",
            result.content
        );

        // Case 6: Verify validation rejects formatted thinking patterns at start
        let formatted_thinking_at_start =
            "[Agent] Thinking: Looking at the diff...\n\nfix: actual message";

        let result = extract_llm_output(formatted_thinking_at_start, Some(OutputFormat::Generic));
        assert!(
            !result.content.contains("[Agent] Thinking:"),
            "Formatted thinking should be removed during extraction"
        );
        // The result should now be clean (just the actual message)
        assert!(
            result.content.contains("fix: actual message") || result.content.contains("fix:"),
            "Clean commit message should remain after filtering"
        );
    }
}
