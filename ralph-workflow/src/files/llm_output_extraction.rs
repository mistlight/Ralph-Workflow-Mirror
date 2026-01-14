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
use serde::Deserialize;
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
    let mut result = content;

    // Remove AI thought process prefixes
    // These are patterns that AI agents commonly use when starting their response
    // We remove everything from the start up to and including the first blank line
    let thought_patterns = [
        "Looking at this diff, I can see",
        "Looking at this diff",
        "I can see",
        "The main changes are",
        "The main changes I see are:",
        "The main changes I see are",
        "Several distinct categories of changes",
        "Key categories of changes",
        "Based on the diff",
        "Analyzing the changes",
        "This diff shows",
        "Looking at the changes",
        "I've analyzed",
        "After reviewing",
        // Additional patterns to catch more variations
        "Based on the git diff",
        "Based on the git diff, here are the changes",
        "Based on the git diff, here's what changed",
        "Based on the git diff, the following changes",
        "Here are the changes",
        "Here's what changed",
        "Here is what changed",
        "The following changes",
        "The changes include",
        "Changes include",
        "After reviewing the diff",
        "After reviewing the changes",
        "After analyzing the diff",
        "After analyzing the changes",
        "I've analyzed the changes",
        "I've analyzed the diff",
        "Looking at the changes, I can see",
        "Key changes include",
        "Several changes include",
        "This diff shows the following",
        // Additional patterns for GLM agent output
        "The most substantive change is",
        "The most substantive changes are",
        "The most substantive user-facing change is",
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
                    // Otherwise, update result to continue with aggressive filtering below
                    result = remaining;
                    break; // Continue to numbered/bold analysis pattern checks
                }
            } else if let Some(single_newline) = rest.find('\n') {
                // If no double newline, try to skip to after the first single newline
                let after_newline = &rest[single_newline + 1..];
                // Check if what follows looks like a commit message (starts with conventional commit type)
                if looks_like_commit_message_start(after_newline.trim()) {
                    return after_newline.to_string();
                }
                // If not, check if the rest starts with numbered analysis
                if after_newline.trim().starts_with("1. ")
                    || after_newline.trim().starts_with("1. **")
                    || after_newline.trim().starts_with("- ")
                {
                    // Skip to after numbered analysis - continue processing
                    // but don't return yet, let the numbered pattern handler deal with it
                    result = after_newline.trim();
                    break;
                }
            }
            // If we found and stripped a pattern but couldn't find a clean commit message
            // or numbered analysis to continue from, check if rest looks like pure analysis
            // If the remaining content after the pattern is all analysis (no valid commit),
            // return empty
            let rest_trimmed = rest.trim();
            if looks_like_analysis_text(rest_trimmed)
                && find_conventional_commit_start(rest_trimmed).is_none()
            {
                return String::new();
            }
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
        // Check multiple conditions to identify analysis:
        // 1. Contains multiple lines (analysis is typically multi-line)
        // 2. Either looks like analysis text OR contains common analysis patterns
        let is_analysis = before_commit.contains('\n')
            && (looks_like_analysis_text(before_commit)
                || before_commit.to_lowercase().contains("changes")
                || before_commit.to_lowercase().contains("diff")
                || before_commit.contains("1.")
                || before_commit.contains("- "));

        if is_analysis {
            return result[commit_start..].to_string();
        }
    }

    // Final check: if the entire content looks like analysis without a valid commit,
    // return empty string. This catches cases like "The main changes I see are:\n1. **Analysis**"
    // followed by more analysis paragraphs but no proper commit message.
    if looks_like_analysis_text(result) {
        // Check if there's markdown-bold type mention embedded in analysis text
        // like "This is a **refactor**..." which indicates analysis, not a commit
        let result_lower = result.to_lowercase();
        if result_lower.contains("**feat**")
            || result_lower.contains("**fix**")
            || result_lower.contains("**refactor**")
            || result_lower.contains("**chore**")
            || result_lower.contains("**test**")
            || result_lower.contains("**docs**")
            || result_lower.contains("**perf**")
            || result_lower.contains("**style**")
        {
            // Look for the pattern "**type**:" (with colon) which indicates
            // it might be an actual commit message in markdown format
            if result_lower.contains("**feat**:")
                || result_lower.contains("**fix**:")
                || result_lower.contains("**refactor**:")
                || result_lower.contains("**chore**:")
                || result_lower.contains("**test**:")
                || result_lower.contains("**docs**:")
                || result_lower.contains("**perf**:")
                || result_lower.contains("**style**:")
            {
                // This might be a valid commit message in markdown, keep it
                return result.to_string();
            }
            // Otherwise, it's analysis with embedded type mentions, filter it out
            return String::new();
        }
        // If no conventional commit was found and it looks like analysis, return empty
        if find_conventional_commit_start(result).is_none() {
            return String::new();
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
        "the change",
        "the diff",
        "i can see",
        "main changes",
        "substantive change",
        "substantive user-facing change",
        "categories",
        "first change",
        "second change",
        "third change",
        // Additional patterns to catch more variations
        "here are the changes",
        "based on the git diff",
        "based on the diff",
        "the following changes",
        "changes include",
        "here's what changed",
        "here is what changed",
        "after reviewing the diff",
        "after reviewing the changes",
        "after analyzing",
        "this diff shows",
        "i've analyzed the changes",
        "i've analyzed",
        "looking at the changes",
        "key changes",
        "several changes",
        "distinct changes",
        "key categories of changes",
        "several categories of changes",
        "user-facing change",
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

/// Structured commit message from JSON output.
///
/// This struct matches the schema we request from the LLM:
/// `{"subject": "feat: ...", "body": "..."}`
#[derive(Debug, Deserialize)]
struct StructuredCommitMessage {
    subject: String,
    body: Option<String>,
}

/// Try to extract commit message from JSON schema output.
///
/// This function attempts to parse the LLM output as a structured JSON object
/// following the schema `{"subject": "...", "body": "..."}`.
///
/// # Returns
///
/// * `Some(message)` if valid JSON with a valid conventional commit subject was found
/// * `None` if parsing fails or subject is invalid
pub fn try_extract_structured_commit(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // Try direct parse
    if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(trimmed) {
        return format_structured_commit(&msg);
    }

    // Try to find JSON object within content (in case of minor preamble)
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if start < end {
                let json_str = &trimmed[start..=end];
                if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(json_str) {
                    return format_structured_commit(&msg);
                }
            }
        }
    }

    None
}

/// Format a structured commit message into the final string format.
fn format_structured_commit(msg: &StructuredCommitMessage) -> Option<String> {
    let subject = msg.subject.trim();

    // Validate conventional commit format
    if !is_conventional_commit_subject(subject) {
        return None;
    }

    // Format the commit message
    match &msg.body {
        Some(body) if !body.trim().is_empty() => Some(format!("{}\n\n{}", subject, body.trim())),
        _ => Some(subject.to_string()),
    }
}

/// Check if a string is a valid conventional commit subject line.
fn is_conventional_commit_subject(subject: &str) -> bool {
    let valid_types = [
        "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
    ];

    // Find the colon
    let Some(colon_pos) = subject.find(':') else {
        return false;
    };

    let prefix = &subject[..colon_pos];

    // Extract type (before optional scope and !)
    let type_end = prefix
        .find('(')
        .unwrap_or_else(|| prefix.find('!').unwrap_or(prefix.len()));
    let commit_type = &prefix[..type_end];

    valid_types.contains(&commit_type)
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
        // Additional patterns to catch more variations
        "based on the git diff",
        "here are the changes",
        "here's what changed",
        "here is what changed",
        "the following changes",
        "changes include",
        "after reviewing the diff",
        "after reviewing the changes",
        "after analyzing",
        "i've analyzed the changes",
        "i've analyzed the diff",
        "key changes",
        "several changes",
        "distinct changes",
        "key changes include",
        "several changes include",
        "this diff shows the following",
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

// =========================================================================
// Commit Message Recovery Functions
// =========================================================================

/// Attempt to salvage a valid commit message from content that failed validation.
///
/// Uses aggressive pattern matching to find a conventional commit message
/// embedded within mixed AI output (thinking + actual message).
///
/// This is used when `validate_commit_message()` fails, to try extracting a valid
/// commit message from mixed output containing thinking patterns and actual content.
///
/// # Returns
///
/// `Some(message)` if a valid commit message was salvaged, `None` otherwise.
pub fn try_salvage_commit_message(content: &str) -> Option<String> {
    // Look for conventional commit pattern anywhere in the content
    let commit_pos = find_conventional_commit_start(content)?;

    // Extract from that position
    let from_commit = &content[commit_pos..];

    // Find where the commit message ends (next blank line or end of content)
    // But include the body paragraph if it follows immediately
    let lines: Vec<&str> = from_commit.lines().collect();

    if lines.is_empty() {
        return None;
    }

    // First line is the subject
    let subject = lines[0].trim();
    if subject.is_empty() {
        return None;
    }

    // Collect body lines (everything after subject until double newline or analysis)
    let mut body_lines: Vec<&str> = Vec::new();
    let mut found_blank = false;

    for line in lines.iter().skip(1) {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if found_blank {
                // Double blank line - end of message
                break;
            }
            found_blank = true;
            body_lines.push("");
            continue;
        }

        // Check if this line looks like analysis starting up again
        if looks_like_analysis_text(trimmed)
            || trimmed.starts_with("1. ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
        {
            break;
        }

        body_lines.push(trimmed);
        found_blank = false;
    }

    // Build the salvaged message
    let mut salvaged = subject.to_string();
    if !body_lines.is_empty() {
        // Remove trailing empty lines from body
        while body_lines.last().is_some_and(|l| l.is_empty()) {
            body_lines.pop();
        }
        if !body_lines.is_empty() {
            salvaged.push('\n');
            salvaged.push_str(&body_lines.join("\n"));
        }
    }

    // Validate the salvaged message before returning
    match validate_commit_message(&salvaged) {
        Ok(()) => Some(salvaged),
        Err(_) => None,
    }
}

/// Generate a deterministic fallback commit message from diff metadata.
///
/// This is the last resort when:
/// 1. Agent output extraction failed
/// 2. Salvage attempt failed
///
/// # Arguments
///
/// * `diff` - The git diff content
///
/// # Returns
///
/// A valid commit message based on changed files. The message is designed to:
/// - Use "chore" type with an appropriate scope
/// - Describe the change semantically (not just file names)
/// - Pass validation (avoids bad patterns like file lists)
pub fn generate_fallback_commit_message(diff: &str) -> String {
    let files = extract_files_from_diff(diff);

    if files.is_empty() {
        // No files found in diff - minimal fallback
        return "chore: apply automated changes".to_string();
    }

    // Find common directory to use as scope
    let common_dir = find_common_directory(&files);

    // Derive a scope from the common directory
    let scope = common_dir
        .as_ref()
        .and_then(|dir| derive_scope_from_path(dir));

    // Determine if this is a single file or multiple files
    let file_count = files.len();

    // Build the commit message
    match (file_count, scope) {
        (1, Some(scope)) => {
            // Single file with a scope - use scope in message
            format!("chore({scope}): update module")
        }
        (1, None) => {
            // Single file without clear scope
            files
                .first()
                .and_then(|f| derive_scope_from_path(f))
                .map_or_else(
                    || "chore: update module".to_string(),
                    |component| format!("chore({component}): update module"),
                )
        }
        (n, Some(scope)) => {
            // Multiple files with common scope
            format!("chore({scope}): update {n} components")
        }
        (n, None) => {
            // Multiple files without common scope
            // Try to find any meaningful scope from the first file
            files
                .first()
                .and_then(|f| derive_scope_from_path(f))
                .map_or_else(
                    || format!("chore: update {n} components"),
                    |component| format!("chore({component}): update {n} components"),
                )
        }
    }
}

/// Extract changed file paths from a git diff.
///
/// Parses "diff --git a/<path> b/<path>" lines to get file paths.
fn extract_files_from_diff(diff: &str) -> Vec<String> {
    let mut files = Vec::new();

    for line in diff.lines() {
        // Match "diff --git a/<path> b/<path>" pattern
        if let Some(rest) = line.strip_prefix("diff --git a/") {
            // The path is up to " b/" (space before b/)
            if let Some(space_b_pos) = rest.find(" b/") {
                let path = &rest[..space_b_pos];
                if !path.is_empty() {
                    files.push(path.to_string());
                }
            }
        }
    }

    files
}

/// Find the common directory prefix for a set of file paths.
///
/// Returns the longest common directory path shared by all files.
fn find_common_directory(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }

    if paths.len() == 1 {
        // For a single file, return its parent directory
        let path = &paths[0];
        if let Some(last_slash) = path.rfind('/') {
            return Some(path[..last_slash].to_string());
        }
        return None;
    }

    // Split all paths into components
    let split_paths: Vec<Vec<&str>> = paths.iter().map(|p| p.split('/').collect()).collect();

    // Find common prefix
    let mut common_components: Vec<&str> = Vec::new();

    // Use the first path as reference
    let first = &split_paths[0];
    for (i, component) in first.iter().enumerate() {
        // Check if all other paths have this component at this position
        let all_match = split_paths.iter().skip(1).all(|path| {
            // Don't compare the filename itself (last component)
            i < path.len().saturating_sub(1) && path.get(i) == Some(component)
        });

        if all_match && i < first.len().saturating_sub(1) {
            common_components.push(component);
        } else {
            break;
        }
    }

    if common_components.is_empty() {
        None
    } else {
        Some(common_components.join("/"))
    }
}

/// Derive a semantic scope name from a file path.
///
/// Extracts a meaningful component name from the path, preferring:
/// - Last directory name for nested paths (e.g., "files" from "src/files/extraction.rs")
/// - First directory for shallow paths (e.g., "src" from "src/lib.rs")
fn derive_scope_from_path(path: &str) -> Option<String> {
    let components: Vec<&str> = path.split('/').collect();

    if components.is_empty() {
        return None;
    }

    // Filter out common non-semantic directories
    let skip_dirs = ["src", "lib", "bin", "tests", "test", "benches", "examples"];

    // Try to find a meaningful component (prefer second-to-last directory)
    for component in components.iter().rev().skip(1) {
        let comp_lower = component.to_lowercase();
        if !skip_dirs.contains(&comp_lower.as_str()) && !component.is_empty() {
            return Some(component.to_string());
        }
    }

    // If all directories are skipped, try the first non-skip directory
    for component in &components {
        if !skip_dirs.contains(&component.to_lowercase().as_str())
            && !component.is_empty()
            && !component.contains('.')
        {
            return Some(component.to_string());
        }
    }

    None
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

    // =========================================================================
    // Regression Tests: Additional Thought Process Patterns
    // =========================================================================

    #[test]
    fn test_regression_based_on_git_diff_pattern() {
        // Test filtering of "Based on the git diff" analysis pattern
        let agent_output = r#"{"result":"Based on the git diff, here are the changes:\n- Updated parser\n- Fixed tests\n\nfeat: add new feature"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("Based on the git diff"));
        assert!(!result.content.contains("- Updated"));
        assert_eq!(result.content, "feat: add new feature");
    }

    #[test]
    fn test_regression_here_are_the_changes_pattern() {
        // Test filtering of "Here are the changes" analysis pattern
        let agent_output = r#"{"result":"Here are the changes I made:\n1. Fixed bug\n2. Added tests\n\nfix: resolve bug"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("Here are the changes"));
        assert!(!result.content.contains("1. Fixed"));
        assert_eq!(result.content, "fix: resolve bug");
    }

    #[test]
    fn test_regression_heres_what_changed_pattern() {
        // Test filtering of "Here's what changed" analysis pattern
        let agent_output = r#"{"result":"Here's what changed:\n- First change\n- Second change\n\nchore: update dependencies"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("Here's what changed"));
        assert!(!result.content.contains("- First"));
        assert_eq!(result.content, "chore: update dependencies");
    }

    #[test]
    fn test_regression_the_following_changes_pattern() {
        // Test filtering of "The following changes" analysis pattern
        let agent_output = r#"{"result":"The following changes were made:\n1. Updated code\n2. Fixed tests\n\nrefactor: improve code structure"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("The following changes"));
        assert!(!result.content.contains("1. Updated"));
        assert_eq!(result.content, "refactor: improve code structure");
    }

    #[test]
    fn test_regression_changes_include_pattern() {
        // Test filtering of "Changes include" analysis pattern
        let agent_output = r#"{"result":"Changes include:\n- Bug fixes\n- New features\n\nfeat: implement feature"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("Changes include"));
        assert!(!result.content.contains("- Bug"));
        assert_eq!(result.content, "feat: implement feature");
    }

    #[test]
    fn test_regression_after_reviewing_pattern() {
        // Test filtering of "After reviewing" analysis pattern
        let agent_output = r#"{"result":"After reviewing the diff, I can see:\n- Multiple fixes\n\nfix: apply bug fixes"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("After reviewing"));
        assert!(!result.content.contains("- Multiple"));
        assert_eq!(result.content, "fix: apply bug fixes");
    }

    #[test]
    fn test_regression_this_diff_shows_pattern() {
        // Test filtering of "This diff shows" analysis pattern
        let agent_output = r#"{"result":"This diff shows the following:\n1. Performance improvements\n\nperf: optimize performance"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("This diff shows"));
        assert!(!result.content.contains("1. Performance"));
        assert_eq!(result.content, "perf: optimize performance");
    }

    #[test]
    fn test_regression_ive_analyzed_pattern() {
        // Test filtering of "I've analyzed" analysis pattern
        let agent_output = r#"{"result":"I've analyzed the changes:\n- Key improvements\n\nfeat: add improvements"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("I've analyzed"));
        assert!(!result.content.contains("- Key"));
        assert_eq!(result.content, "feat: add improvements");
    }

    #[test]
    fn test_regression_key_changes_include_pattern() {
        // Test filtering of "Key changes include" analysis pattern
        let agent_output = r#"{"result":"Key changes include:\n1. Security fixes\n2. Performance boost\n\nfix: security vulnerability"}"#;

        let result = extract_llm_output(agent_output, Some(OutputFormat::Claude));
        assert!(result.was_structured);
        assert!(!result.content.contains("Key changes include"));
        assert!(!result.content.contains("1. Security"));
        assert_eq!(result.content, "fix: security vulnerability");
    }

    // =========================================================================
    // Regression Tests for Specific Bug Patterns
    // =========================================================================

    #[test]
    fn test_regression_glm_bug_pattern_606b907() {
        // Regression test for commit 606b907 - GLM agent output with "The main changes I see are:"
        // followed by numbered markdown-bold analysis without proper separation
        // Test the filter function directly with the raw text
        let raw_output = "The main changes I see are:
1. **Rust code modernization** - Converting from older patterns to newer Rust idioms
2. **Thought process filtering** - Adding comprehensive logic to detect and filter AI thought process patterns from commit messages
3. **Test reorganization** - Splitting a large test file into 5 focused modules
4. **String handling improvements** - Using `map_or_else`, `is_some_and`, `cloned()` where appropriate
5. **Regex updates** - `once_cell::sync::Lazy` → `std::sync::LazyLock`
6. **Const fn changes** - Methods taking `&self` now taking `self` by value
7. **Various code quality improvements** - Error handling, visibility adjustments, etc.

The most substantive user-facing change is the **thought process filtering** in `llm_output_extraction.rs` - this adds significant new functionality to prevent AI analysis text from leaking into generated commit messages. The rest is primarily refactoring/modernization.

This is a **refactor** with significant improvements and bug fixes";

        let result = remove_thought_process_patterns(raw_output);

        // The filtered content should NOT contain the thought process analysis
        assert!(!result.contains("The main changes I see are"));

        // Should not contain numbered analysis items
        assert!(!result.contains("1. **Rust code modernization**"));
        assert!(!result.contains("2. **Thought process filtering**"));

        // Should not contain the paragraph with "most substantive"
        assert!(!result.contains("The most substantive user-facing change"));

        // The result should be empty or very minimal since there's no proper commit message
        // After filtering, we expect empty content or the last paragraph only
        assert!(!result.contains("**Rust code modernization**"));
        assert!(!result.contains("**Thought process filtering**"));
    }

    #[test]
    fn test_regression_glm_bug_pattern_d6ca2a5() {
        // Regression test for commit d6ca2a5 - "Looking at the diff" pattern
        // with multi-paragraph analysis followed by "fix:" commit
        let raw_output = "Looking at the diff, I can see these are related changes across the three JSON parser files (claude.rs, codex.rs, gemini.rs) that all deal with the same issue: tracking and resetting streaming state to prevent duplicate content display.

The key change in claude.rs is:
- Adding a `has_streamed_content` field to track if content has been streamed for the current message
- Using this flag to skip displaying text in `format_message` if it was already streamed
- Resetting this flag on `MessageStart`

Similar state-resetting logic is added to codex.rs and gemini.rs.

This is a fix for duplicate content display during streaming.

fix(json_parser): prevent duplicate content display in streaming output

Track streaming state per-message to avoid displaying text content twice:
once during streaming and again in the final message summary.";

        let result = remove_thought_process_patterns(raw_output);

        // The filtered content should NOT contain the thought process analysis
        assert!(!result.contains("Looking at the diff, I can see"));

        // Should not contain the analysis paragraphs
        assert!(!result.contains("The key change in claude.rs is"));
        assert!(!result.contains("Similar state-resetting logic"));

        // Should extract only the clean commit message
        assert_eq!(result, "fix(json_parser): prevent duplicate content display in streaming output\n\nTrack streaming state per-message to avoid displaying text content twice:\nonce during streaming and again in the final message summary.");
    }

    // =========================================================================
    // Salvage and Fallback Recovery Tests
    // =========================================================================

    #[test]
    fn test_salvage_commit_from_thinking_mixed_output() {
        // The bug scenario: thinking mixed with actual commit message
        let mixed_output = r"] Thinking: Looking at this diff, I can see several changes...

feat: add commit message recovery

This adds recovery logic when validation fails.";

        let salvaged = try_salvage_commit_message(mixed_output);
        assert!(salvaged.is_some());
        let msg = salvaged.unwrap();
        assert!(msg.starts_with("feat:"));
        assert!(validate_commit_message(&msg).is_ok());
    }

    #[test]
    fn test_salvage_commit_from_analysis_with_commit() {
        let mixed = "1. First change\n2. Second change\n\nfix: resolve parsing bug";
        let salvaged = try_salvage_commit_message(mixed);
        assert!(salvaged.is_some());
        assert_eq!(salvaged.unwrap(), "fix: resolve parsing bug");
    }

    #[test]
    fn test_salvage_returns_none_for_pure_analysis() {
        let analysis_only = "Looking at this diff, I can see several changes:\n1. First\n2. Second";
        let salvaged = try_salvage_commit_message(analysis_only);
        assert!(salvaged.is_none());
    }

    #[test]
    fn test_salvage_extracts_subject_and_body() {
        let mixed = r"Here's my analysis of the changes...

chore(files): update extraction logic

This commit improves the extraction by adding
better error handling and recovery paths.";

        let salvaged = try_salvage_commit_message(mixed);
        assert!(salvaged.is_some());
        let msg = salvaged.unwrap();
        assert!(msg.starts_with("chore(files):"));
        assert!(msg.contains("better error handling"));
    }

    #[test]
    fn test_salvage_stops_at_analysis_after_body() {
        let mixed = r"feat: add feature

Body paragraph here.

1. Additional analysis
2. More analysis";

        let salvaged = try_salvage_commit_message(mixed);
        assert!(salvaged.is_some());
        let msg = salvaged.unwrap();
        assert!(!msg.contains("Additional analysis"));
        assert!(!msg.contains("More analysis"));
    }

    #[test]
    fn test_fallback_generates_valid_message_single_file() {
        let diff = "diff --git a/src/files/extraction.rs b/src/files/extraction.rs\n--- a/src/files/extraction.rs\n+++ b/src/files/extraction.rs\n@@ -1,1 +1,1 @@";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        assert!(fallback.contains("files") || fallback.contains("chore"));
    }

    #[test]
    fn test_fallback_generates_valid_message_multiple_files_same_dir() {
        let diff = r"diff --git a/src/files/a.rs b/src/files/a.rs
--- a/src/files/a.rs
+++ b/src/files/a.rs
diff --git a/src/files/b.rs b/src/files/b.rs
--- a/src/files/b.rs
+++ b/src/files/b.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        // Should NOT produce "chore: 2 file(s) changed" (bad pattern)
        assert!(!fallback.contains("file(s) changed"));
        // Should have a scope
        assert!(fallback.contains("(files)") || fallback.contains("chore"));
    }

    #[test]
    fn test_fallback_avoids_file_list_pattern() {
        let diff = "diff --git a/src/foo.rs b/src/foo.rs\ndiff --git a/src/bar.rs b/src/bar.rs";
        let fallback = generate_fallback_commit_message(diff);
        // Should not produce "chore: update src/foo.rs, src/bar.rs"
        assert!(!fallback.contains("foo.rs"));
        assert!(!fallback.contains("bar.rs"));
        assert!(validate_commit_message(&fallback).is_ok());
    }

    #[test]
    fn test_fallback_empty_diff() {
        let diff = "";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        assert_eq!(fallback, "chore: apply automated changes");
    }

    #[test]
    fn test_fallback_generates_count_for_multiple_dirs() {
        let diff = r"diff --git a/src/a.rs b/src/a.rs
diff --git a/lib/b.rs b/lib/b.rs
diff --git a/tests/c.rs b/tests/c.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        // Should mention count but not file names
        assert!(fallback.contains("3 components") || fallback.contains("chore"));
    }

    #[test]
    fn test_regression_thinking_leakage_recovery() {
        // The exact scenario from the bug report
        let log_content = r"[Claude] Thinking: Looking at this diff, I need to analyze...

feat(pipeline): add recovery mechanism

When commit validation fails, attempt to salvage valid message.";

        // Verify salvage recovers it
        let salvaged = try_salvage_commit_message(log_content);
        assert!(salvaged.is_some());
        let msg = salvaged.unwrap();
        assert!(validate_commit_message(&msg).is_ok());
        assert!(msg.starts_with("feat(pipeline):"));
    }

    #[test]
    fn test_extract_files_from_diff() {
        let diff = r"diff --git a/src/files/extraction.rs b/src/files/extraction.rs
--- a/src/files/extraction.rs
+++ b/src/files/extraction.rs
diff --git a/src/phases/commit.rs b/src/phases/commit.rs
--- a/src/phases/commit.rs
+++ b/src/phases/commit.rs";

        let files = extract_files_from_diff(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "src/files/extraction.rs");
        assert_eq!(files[1], "src/phases/commit.rs");
    }

    #[test]
    fn test_find_common_directory_same_dir() {
        let paths = vec![
            "src/files/a.rs".to_string(),
            "src/files/b.rs".to_string(),
            "src/files/c.rs".to_string(),
        ];
        let common = find_common_directory(&paths);
        assert_eq!(common, Some("src/files".to_string()));
    }

    #[test]
    fn test_find_common_directory_partial_overlap() {
        let paths = vec![
            "src/files/extraction.rs".to_string(),
            "src/phases/commit.rs".to_string(),
        ];
        let common = find_common_directory(&paths);
        assert_eq!(common, Some("src".to_string()));
    }

    #[test]
    fn test_find_common_directory_no_overlap() {
        let paths = vec!["src/a.rs".to_string(), "lib/b.rs".to_string()];
        let common = find_common_directory(&paths);
        assert!(common.is_none());
    }

    #[test]
    fn test_derive_scope_from_path() {
        // Should extract "files" from nested path
        assert_eq!(
            derive_scope_from_path("src/files/extraction.rs"),
            Some("files".to_string())
        );

        // Should extract "phases" from path
        assert_eq!(
            derive_scope_from_path("src/phases/commit.rs"),
            Some("phases".to_string())
        );

        // Should skip "src" as non-semantic
        assert_ne!(
            derive_scope_from_path("src/files/foo.rs"),
            Some("src".to_string())
        );
    }

    #[test]
    fn test_derive_scope_from_shallow_path() {
        // For shallow paths like "foo.rs" or "src/lib.rs", should return None or meaningful component
        let scope = derive_scope_from_path("lib.rs");
        // lib.rs has no meaningful directory scope
        assert!(scope.is_none());
    }

    // =========================================================================
    // Structured Commit Message Extraction Tests
    // =========================================================================

    #[test]
    fn test_extract_valid_json_commit() {
        let content = r#"{"subject": "feat: add feature", "body": null}"#;
        let result = try_extract_structured_commit(content);
        assert_eq!(result, Some("feat: add feature".to_string()));
    }

    #[test]
    fn test_extract_json_with_body() {
        let content = r#"{"subject": "feat: add OAuth", "body": "Implement Google provider."}"#;
        let result = try_extract_structured_commit(content);
        assert_eq!(
            result,
            Some("feat: add OAuth\n\nImplement Google provider.".to_string())
        );
    }

    #[test]
    fn test_extract_json_with_multiline_body() {
        let content =
            r#"{"subject": "feat: add auth", "body": "Line 1.\nLine 2.\n\nParagraph 2."}"#;
        let result = try_extract_structured_commit(content);
        assert_eq!(
            result,
            Some("feat: add auth\n\nLine 1.\nLine 2.\n\nParagraph 2.".to_string())
        );
    }

    #[test]
    fn test_extract_json_with_whitespace() {
        let content = "  \n{\"subject\": \"fix: bug\", \"body\": null}\n  ";
        let result = try_extract_structured_commit(content);
        assert_eq!(result, Some("fix: bug".to_string()));
    }

    #[test]
    fn test_extract_json_with_preamble() {
        // Should still work - we extract JSON from within content
        let content = "Here is the commit:\n{\"subject\": \"fix: bug\", \"body\": null}";
        let result = try_extract_structured_commit(content);
        assert_eq!(result, Some("fix: bug".to_string()));
    }

    #[test]
    fn test_reject_invalid_commit_type() {
        let content = r#"{"subject": "invalid: not a type", "body": null}"#;
        let result = try_extract_structured_commit(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_reject_missing_colon() {
        let content = r#"{"subject": "feat add feature", "body": null}"#;
        let result = try_extract_structured_commit(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_analysis_text_not_json() {
        let content = "Looking at this diff, I see changes.\n\nfeat: add feature";
        let result = try_extract_structured_commit(content);
        assert!(result.is_none()); // Not JSON, should fail
    }

    #[test]
    fn test_commit_with_scope() {
        let content = r#"{"subject": "feat(auth): add OAuth2 login", "body": null}"#;
        let result = try_extract_structured_commit(content);
        assert_eq!(result, Some("feat(auth): add OAuth2 login".to_string()));
    }

    #[test]
    fn test_commit_with_breaking_change_marker() {
        let content = r#"{"subject": "feat!: drop Python 3.7 support", "body": null}"#;
        let result = try_extract_structured_commit(content);
        assert_eq!(result, Some("feat!: drop Python 3.7 support".to_string()));
    }

    #[test]
    fn test_commit_with_scope_and_breaking() {
        let content = r#"{"subject": "feat(api)!: redesign endpoint", "body": null}"#;
        let result = try_extract_structured_commit(content);
        assert_eq!(result, Some("feat(api)!: redesign endpoint".to_string()));
    }

    #[test]
    fn test_all_valid_commit_types() {
        let types = [
            "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
        ];
        for commit_type in types {
            let content = format!(r#"{{"subject": "{commit_type}: description", "body": null}}"#);
            let result = try_extract_structured_commit(&content);
            assert!(result.is_some(), "Type '{commit_type}' should be valid");
        }
    }

    #[test]
    fn test_empty_body_treated_as_none() {
        let content = r#"{"subject": "fix: bug", "body": ""}"#;
        let result = try_extract_structured_commit(content);
        // Empty body should result in just the subject
        assert_eq!(result, Some("fix: bug".to_string()));
    }

    #[test]
    fn test_whitespace_body_treated_as_none() {
        let content = r#"{"subject": "fix: bug", "body": "   "}"#;
        let result = try_extract_structured_commit(content);
        // Whitespace-only body should result in just the subject
        assert_eq!(result, Some("fix: bug".to_string()));
    }

    // =========================================================================
    // Regression Tests: Exact Bug Report Output (wt-commit-bug)
    // =========================================================================

    #[test]
    fn test_regression_exact_bug_report_output() {
        // Regression test for the exact output from the bug report that triggered this fix.
        // The bug was that AI agents included their explanatory analysis ("thought process")
        // in the commit message output instead of only the intended commit subject/body.
        let content = "Looking at this diff, I can see several distinct types of changes across multiple files:\n\
\n\
1. **Test assertion style** (agents/config.rs, pipeline/tests.rs): Replacing panic!() with assert!(false, ...) for more idiomatic test failure messages\n\
2. **Refactoring** (app/mod.rs): Extracting many parameters into a PipelineContext struct to improve maintainability\n\
3. **String literal consistency** (files/result_extraction.rs): Using raw strings where needed\n\
4. **Error handling** (git_helpers/repo.rs): Replacing unreachable!() with proper error return\n\
5. **Type conversion safety** (logger/progress.rs): Using try_from() with explicit error handling instead of casting\n\
\n\
The main cohesive theme is **code quality improvements** - fixing warnings, improving error handling, and refactoring for maintainability. The most significant change is the PipelineContext refactoring in app/mod.rs.\n\
\n\
refactor: extract PipelineContext and improve error handling\n\
\n\
Extract PipelineContext struct to reduce parameter count in run_pipeline,\n\
replacing unreachable!() with proper error returns, and standardize test\n\
assertion style to use assert!(false, ...) instead of panic!(). Also\n\
simplify raw string literals and add explicit type conversion handling.";

        let result = extract_llm_output(content, None);

        // Should extract only the commit message, not the thought process
        assert!(
            result.content.contains("refactor: extract PipelineContext"),
            "Should contain the commit subject: {}",
            result.content
        );
        assert!(
            !result.content.contains("Looking at this diff"),
            "Should NOT contain thought process prefix: {}",
            result.content
        );
        assert!(
            !result.content.contains("1. **Test assertion style**"),
            "Should NOT contain numbered markdown-bold analysis: {}",
            result.content
        );
        assert!(
            !result.content.contains("The main cohesive theme"),
            "Should NOT contain summary paragraph: {}",
            result.content
        );

        // Should pass validation
        assert!(
            validate_commit_message(&result.content).is_ok(),
            "Extracted content should pass validation: {}",
            result.content
        );
    }

    #[test]
    fn test_regression_analysis_only_rejected() {
        // Test case where output is analysis-only with no valid commit message
        let content =
            "Looking at this diff, I can see changes to the parser and tests. The main theme is code quality improvements.";

        let result = extract_llm_output(content, None);

        // Should NOT pass validation - no valid commit message exists
        assert!(
            validate_commit_message(&result.content).is_err(),
            "Analysis-only content should fail validation: {}",
            result.content
        );
    }

    #[test]
    fn test_regression_glm_substantive_change_pattern() {
        // GLM agent specific pattern with "most substantive change" phrasing
        let content =
            "The most substantive change is to the parser module\n\nfix(parser): resolve edge case";

        let result = extract_llm_output(content, None);

        assert_eq!(result.content, "fix(parser): resolve edge case");
        assert!(
            !result.content.contains("substantive change"),
            "Should NOT contain GLM analysis pattern"
        );
    }

    #[test]
    fn test_regression_json_with_leading_analysis() {
        // JSON schema output with leading analysis text that should be ignored
        let content = r#"Here's the commit message:
{"subject": "feat: add feature", "body": null}"#;

        let result = try_extract_structured_commit(content);

        // Should extract the valid JSON, ignoring the preamble
        assert!(result.is_some(), "Should extract JSON despite preamble");
        let extracted = result.unwrap();
        assert_eq!(extracted, "feat: add feature");
    }

    #[test]
    fn test_regression_two_commit_messages_deterministic() {
        // When two potential commit messages exist, extraction should be deterministic
        let content = "fix(parser): resolve edge case in parsing\n\nfeat: add new feature to the parser";

        let result = extract_llm_output(content, None);

        // Should pick one deterministically (first one found)
        assert!(
            result.content.starts_with("fix(parser): resolve edge case")
                || result.content.starts_with("feat: add new feature"),
            "Should extract a valid commit message: {}",
            result.content
        );
        // Run twice to verify determinism
        let result2 = extract_llm_output(content, None);
        assert_eq!(
            result.content, result2.content,
            "Extraction should be deterministic"
        );
    }
}
