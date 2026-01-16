//! Commit Message Validation and Recovery Functions
//!
//! This module provides utilities for validating commit messages, salvaging
//! valid messages from mixed output, and generating fallback messages from
//! git diff metadata.

use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

use super::cleaning::{
    final_escape_sequence_cleanup, find_conventional_commit_start, looks_like_analysis_text,
    unescape_json_strings, unescape_json_strings_aggressive,
};
use crate::agents::AgentErrorKind;

/// Detect if agent output contains unrecoverable errors that should trigger fallback.
///
/// This handles cases where agents output errors in their result field instead of stderr.
/// Patterns include: "Prompt is too long", "token limit exceeded", etc.
///
/// Enhanced with more patterns to catch additional error variations from different agents.
pub fn detect_agent_errors_in_output(content: &str) -> Option<AgentErrorKind> {
    let content_lower = content.to_lowercase();

    // Check for token/context exhaustion patterns in output
    // These patterns indicate the prompt was too large for the agent's context window
    if content_lower.contains("prompt is too long")
        || content_lower.contains("token limit exceeded")
        || content_lower.contains("context length exceeded")
        || content_lower.contains("maximum context")
        || content_lower.contains("input too large")
        || content_lower.contains("context window")
        || content_lower.contains("max tokens")
        || content_lower.contains("token limit")
        || content_lower.contains("too many tokens")
        || content_lower.contains("exceeds context")
        || content_lower.contains("model's context length")
        || content_lower.contains("input exceeds")
    {
        return Some(AgentErrorKind::TokenExhausted);
    }

    // Check for agent failure patterns
    // These indicate other types of agent errors (API issues, invalid requests, etc.)
    if content_lower.contains("invalid request")
        || content_lower.contains("request failed")
        || content_lower.contains("api error")
        || content_lower.contains("rate limit")
        || content_lower.contains("service unavailable")
    {
        return Some(AgentErrorKind::InvalidResponse);
    }

    None
}

/// Result of commit message extraction with detail about the extraction method.
///
/// This enum allows callers to distinguish between different extraction outcomes
/// and take appropriate action (e.g., re-prompt when receiving a Fallback result).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitExtractionResult {
    /// Successfully extracted from structured agent output (JSON schema or pattern-based)
    Extracted(String),
    /// Recovered via salvage mechanism (found conventional commit within mixed output)
    Salvaged(String),
    /// Using deterministic fallback generated from diff metadata
    Fallback(String),
    /// Agent error detected in output (should trigger fallback)
    AgentError(AgentErrorKind),
}

impl CommitExtractionResult {
    /// Convert into the inner message string with final escape sequence cleanup.
    ///
    /// This applies the final rendering step to ensure no escape sequences leak through
    /// to the actual commit message.
    pub fn into_message(self) -> String {
        match self {
            Self::Extracted(msg) | Self::Salvaged(msg) | Self::Fallback(msg) => {
                render_final_commit_message(&msg)
            }
            Self::AgentError(_) => String::new(), // Errors produce empty message
        }
    }

    /// Check if this was a fallback result (should trigger re-prompt).
    pub const fn is_fallback(&self) -> bool {
        matches!(self, Self::Fallback(_))
    }

    /// Check if this was an agent error (should trigger fallback immediately).
    pub const fn is_agent_error(&self) -> bool {
        matches!(self, Self::AgentError(_))
    }

    /// Get the error kind if this is an agent error.
    pub const fn error_kind(&self) -> Option<AgentErrorKind> {
        match self {
            Self::AgentError(kind) => Some(*kind),
            _ => None,
        }
    }
}

/// Structured commit message schema for JSON parsing.
#[derive(Debug, Deserialize)]
struct StructuredCommitMessage {
    subject: String,
    body: Option<String>,
}

/// Try to extract commit message from XML format with detailed tracing.
///
/// This function looks for the distinctive `<ralph-commit>` tags used by our
/// XML-based commit prompt template. The XML format is preferred because:
/// - No escape sequence issues (actual newlines work fine)
/// - Distinctive tags unlikely to appear in LLM analysis text
/// - Clear boundaries for parsing
///
/// # Expected Format
///
/// ```xml
/// <ralph-commit>
/// <ralph-subject>type(scope): description</ralph-subject>
/// <ralph-body>Optional body text here.
/// Can span multiple lines.</ralph-body>
/// </ralph-commit>
/// ```
///
/// The `<ralph-body>` tag is optional and may be omitted for commits without a body.
///
/// # Returns
///
/// A tuple of `(Option<String>, String)`:
/// - First element: `Some(message)` if valid XML with a valid conventional commit subject was found, `None` otherwise
/// - Second element: Detailed reason string explaining what was found/not found (for debugging)
pub fn try_extract_xml_commit_with_trace(content: &str) -> (Option<String>, String) {
    let content_len = content.len();
    let content_preview = if content_len > 50 {
        format!("{}...", &content[..50].replace('\n', "\\n"))
    } else {
        content.replace('\n', "\\n")
    };

    // Find the <ralph-commit> block
    let Some(commit_start) = content.find("<ralph-commit>") else {
        return (
            None,
            format!(
                "No <ralph-commit> tag found (content length: {content_len}, starts with: '{content_preview}')"
            ),
        );
    };

    let Some(commit_end) = content.find("</ralph-commit>") else {
        return (
            None,
            format!(
                "Found <ralph-commit> at pos {commit_start}, but no closing </ralph-commit> tag"
            ),
        );
    };

    if commit_start >= commit_end {
        return (
            None,
            format!(
                "Malformed XML: </ralph-commit> at {commit_end} appears before <ralph-commit> at {commit_start}"
            ),
        );
    }

    // Extract content between the tags
    let commit_block = &content[commit_start + "<ralph-commit>".len()..commit_end];

    // Extract subject (required)
    let Some(subject) = extract_xml_tag_content(commit_block, "ralph-subject") else {
        return (
            None,
            format!(
                "Found <ralph-commit> at {commit_start}, but <ralph-subject> tag not found within commit block"
            ),
        );
    };

    let subject = subject.trim();
    if subject.is_empty() {
        return (
            None,
            format!("Found <ralph-subject> but it is empty (at pos {commit_start})"),
        );
    }

    // Validate conventional commit format
    if !is_conventional_commit_subject(subject) {
        return (
            None,
            format!(
                "Found subject '{}' but it doesn't match conventional commit format (type: ...)",
                if subject.len() > 50 {
                    format!("{}...", &subject[..50])
                } else {
                    subject.to_string()
                }
            ),
        );
    }

    // Extract body (optional)
    let body = extract_xml_tag_content(commit_block, "ralph-body");

    // Format the commit message
    let has_body = body.as_ref().is_some_and(|b| !b.trim().is_empty());
    let message = match &body {
        Some(body_content) if !body_content.trim().is_empty() => {
            format!("{}\n\n{}", subject, body_content.trim())
        }
        _ => subject.to_string(),
    };
    (
        Some(message.clone()),
        format!(
            "Found <ralph-commit> at pos {commit_start}, <ralph-subject> extracted, body={}, message: '{}'",
            if has_body { "present" } else { "absent" },
            if message.len() > 80 {
                format!("{}...", &message[..80].replace('\n', "\\n"))
            } else {
                message.replace('\n', "\\n")
            }
        ),
    )
}

/// Try to extract commit message from JSON format with detailed tracing.
///
/// Returns both the result and a detailed reason string explaining what was found/not found.
pub fn try_extract_structured_commit_with_trace(content: &str) -> (Option<String>, String) {
    let trimmed = content.trim();
    let content_len = trimmed.len();

    // Check for NDJSON stream
    if looks_like_ndjson(trimmed) {
        for line in trimmed.lines() {
            let line = line.trim();
            if !line.starts_with('{') {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json.get("type").and_then(|v| v.as_str()) == Some("result") {
                    if let Some(result_str) = json.get("result").and_then(|v| v.as_str()) {
                        if let Some(msg) = try_extract_from_text(result_str) {
                            return (
                                Some(msg.clone()),
                                format!(
                                    "Extracted from NDJSON result field, message: '{}'",
                                    if msg.len() > 80 {
                                        format!("{}...", &msg[..80].replace('\n', "\\n"))
                                    } else {
                                        msg.replace('\n', "\\n")
                                    }
                                ),
                            );
                        }
                    }
                }
            }
        }
        return (
            None,
            format!("Content looks like NDJSON ({content_len} chars) but no valid commit found in result field"),
        );
    }

    // Try extraction from text content
    if let Some(msg) = try_extract_from_text(trimmed) {
        return (
            Some(msg.clone()),
            format!(
                "Extracted from JSON/text content, message: '{}'",
                if msg.len() > 80 {
                    format!("{}...", &msg[..80].replace('\n', "\\n"))
                } else {
                    msg.replace('\n', "\\n")
                }
            ),
        );
    }

    // Provide detailed failure reason
    let has_brace = trimmed.contains('{');
    let has_subject_key = trimmed.contains("\"subject\"");

    if has_brace && has_subject_key {
        (
            None,
            format!(
                "Content has JSON-like structure ({content_len} chars, has '{{': {has_brace}, has 'subject' key: {has_subject_key}) but parsing failed"
            ),
        )
    } else if has_brace {
        (
            None,
            format!("Content has '{{' but no 'subject' key found ({content_len} chars)"),
        )
    } else {
        (
            None,
            format!("Content does not appear to be JSON ({content_len} chars, no '{{' found)"),
        )
    }
}

/// Extract content between XML-style tags.
///
/// # Arguments
///
/// * `content` - The content to search
/// * `tag_name` - The tag name (without angle brackets)
///
/// # Returns
///
/// * `Some(content)` if the tag was found with content
/// * `None` if the tag was not found or was empty
fn extract_xml_tag_content(content: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{tag_name}>");
    let close_tag = format!("</{tag_name}>");

    let start = content.find(&open_tag)?;
    let end = content.find(&close_tag)?;

    if start >= end {
        return None;
    }

    let inner = &content[start + open_tag.len()..end];

    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

/// Try to extract a structured commit message from text content.
///
/// This handles:
/// - Direct JSON parsing
/// - JSON inside markdown code fences
/// - JSON embedded within other text
fn try_extract_from_text(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // Try extracting from markdown code fence first
    if let Some(json_content) = extract_json_from_code_fence(trimmed) {
        if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(&json_content) {
            return format_structured_commit(&msg);
        }
    }

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

    // Unescape JSON escape sequences that may be in the string values
    // When serde_json parses JSON like `{"subject": "feat: add", "body": "Line 1\\nLine 2"}`
    // the body field contains the literal characters `\` and `n`, not a newline.
    // We need to unescape these to get proper formatting.
    let subject = unescape_json_strings(subject);

    // Validate conventional commit format
    if !is_conventional_commit_subject(&subject) {
        return None;
    }

    // Format the commit message
    match &msg.body {
        Some(body) if !body.trim().is_empty() => {
            let body = unescape_json_strings(body.trim());
            Some(format!("{subject}\n\n{body}",))
        }
        _ => Some(subject),
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

/// Extract JSON content from markdown code fences.
///
/// Handles both regular code fences and nested fences (e.g., within NDJSON streams).
fn extract_json_from_code_fence(content: &str) -> Option<String> {
    // Look for ```json code fence
    let fence_start = content.find("```json")?;
    let after_fence = &content[fence_start + 7..]; // Skip past ```json

    // Find the end of the code fence
    let fence_end = after_fence.find("\n```")?;
    let json_content = after_fence[..fence_end].trim();

    if json_content.is_empty() {
        None
    } else {
        Some(json_content.to_string())
    }
}

/// Check if content looks like NDJSON stream.
fn looks_like_ndjson(content: &str) -> bool {
    content.lines().count() > 1 && content.contains("{\"type\":")
}

/// File count pattern regex - compiled once using `OnceLock` for efficiency.
/// Matches patterns like "chore: N file(s) changed" for any number N.
fn file_count_pattern_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^chore:\s*\d+\s+(?:file\(s\)|files?)\s+changed$")
            .expect("file count regex should be valid")
    })
}

// =========================================================================
// Commit Message Validation Helper Functions
// =========================================================================

/// Validate basic length requirements for commit message content.
fn validate_basic_length(content: &str) -> Result<(), String> {
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

    Ok(())
}

/// Validate that content does not contain JSON parsing artifacts.
fn validate_no_json_artifacts(content: &str) -> Result<(), String> {
    let json_indicators = [
        r#"{"type":"#,
        r#"{"result":"#,
        r#"{"content":"#,
        r#"{"subject":"#, // Structured commit message JSON that wasn't parsed
        r#"{"body":"#,    // Partial structured commit JSON
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
    Ok(())
}

/// Validate that content does not contain literal escape sequences indicating JSON unescaping failure.
fn validate_no_literal_escape_sequences(content: &str) -> Result<(), String> {
    // Pattern 1: Body starts with literal \n\n (most common JSON escaping issue)
    // After a subject line like "feat: add", the body should start with actual newlines,
    // not literal "\n\n" characters. This indicates the JSON wasn't properly unescaped.
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() >= 2 {
        let second_line = lines[1].trim();
        // Check if body starts with literal escape sequences
        if second_line == "\\n" || second_line == "\\n\\n" || second_line.starts_with("\\n\\n") {
            return Err(
                "Commit message body appears to contain literal escape sequences (\\n\\n). \
                 This indicates JSON was not properly unescaped. \
                 Expected actual newlines after subject line."
                    .to_string(),
            );
        }
    }

    // Pattern 2: Check for literal escape sequences COMBINED WITH JSON artifacts
    // This is a safety check for cases where unescaping failed but only when
    // combined with other JSON indicators that indicate actual parsing failure.
    // Individual literal \n, \t, \r without JSON artifacts may be legitimate
    // content in commit messages (e.g., "fix: handle \\n in filenames")
    let json_and_escape_patterns = [
        (r#"{"type":"#, "\\n"),
        (r#"{"result":"#, "\\n"),
        (r#"{"content":"#, "\\n"),
        (r#""session_id":"#, "\\n"),
    ];
    for (json_pattern, escape_pattern) in json_and_escape_patterns {
        if content.contains(json_pattern) && content.contains(escape_pattern) {
            return Err(format!(
                "Commit message contains both JSON artifacts ({json_pattern}) and literal escape sequences ({escape_pattern}). This indicates JSON parsing failure."
            ));
        }
    }

    // Pattern 3: Check for repeated literal escape sequences that suggest bulk unescaping failure
    // This catches cases where \\n\\n\\n appears (multiple escaped newlines that weren't processed)
    if content.contains("\\n\\n\\n") || content.contains("\\n\\n\\n\\n") {
        return Err(
            "Commit message contains repeated literal escape sequences (\\n\\n\\n). \
             This indicates JSON string values were not properly unescaped."
                .to_string(),
        );
    }

    Ok(())
}

/// Validate that content does not start with error markers.
fn validate_no_error_markers(content: &str) -> Result<(), String> {
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
    Ok(())
}

/// Validate that content does not contain agent error messages.
fn validate_no_agent_errors(content: &str) -> Result<(), String> {
    // Check for agent error messages that leaked into output
    // This handles cases where agents output errors in their result field
    // that bypassed the normal stderr error detection
    let agent_error_patterns = [
        "prompt is too long",
        "token limit exceeded",
        "context length exceeded",
        "maximum context",
        "input too large",
        "invalid request",
        "request failed",
    ];
    let content_lower = content.to_lowercase();
    for pattern in &agent_error_patterns {
        if content_lower.contains(pattern) {
            return Err(format!(
                "Output contains agent error message ({pattern}). Cannot use as commit message."
            ));
        }
    }
    Ok(())
}

/// Validate that content does not contain AI thought process leakage.
fn validate_no_thought_process_leakage(content: &str) -> Result<(), String> {
    let content_lower = content.to_lowercase();

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

    Ok(())
}

/// Validate that content does not contain placeholder text.
///
/// This function uses word boundary checks to prevent false positives
/// from log metadata or other technical content that may contain words
/// like "placeholder" in valid contexts (e.g., "placeholder element",
/// "placeholder attribute").
///
/// Template variables like `{{PROMPT}}`, `{{PLAN}}`, `{{DIFF}}` are
/// explicitly excluded from being flagged as placeholders since they
/// are valid template syntax that gets substituted before use.
fn validate_no_placeholders(content: &str) -> Result<(), String> {
    // Valid template variables that should NOT be flagged as placeholders
    let valid_template_vars = ["{{prompt}}", "{{plan}}", "{{diff}}"];
    let content_lower = content.to_lowercase();

    // First, check if the content only contains valid template variables
    // If so, skip the placeholder check entirely
    for valid_var in &valid_template_vars {
        if content_lower == *valid_var {
            return Ok(());
        }
    }

    // Placeholder patterns with word boundary checks
    // These patterns must appear as complete phrases, not as substrings
    let placeholder_patterns = [
        r"(?i)\[commit message\]",           // [commit message]
        r"(?i)<commit message>",             // <commit message>
        r"(?i)\byour commit message here\b", // your commit message here (as standalone phrase)
        r"(?i)\[commit\s*\]",                // [commit] or [commit  ]
        r"(?i)<commit\s*>",                  // <commit> or <commit  >
        r"(?i)\[insert\b",                   // [insert (but not [inserted])
        r"(?i)<insert\b",                    // <insert (but not <inserted>)
    ];

    // Build a regex that matches any of the placeholder patterns
    let combined_pattern = placeholder_patterns.join("|");
    if let Ok(re) = regex::Regex::new(&combined_pattern) {
        if re.is_match(content) {
            return Err(
                "Commit message contains placeholder text (e.g., '[commit message]', '<commit message>', or similar)".to_string()
            );
        }
    }

    // For "placeholder" specifically, only flag it if it appears as a standalone
    // word that isn't part of a valid technical term like "placeholder attribute"
    // We check for phrases like "is a placeholder", "placeholder for", etc.
    let placeholder_context_patterns = [
        r"(?i)\bplaceholder\s+for\b",     // "placeholder for"
        r"(?i)\bplaceholder\s+text\b",    // "placeholder text"
        r"(?i)\bplaceholder\s+value\b",   // "placeholder value"
        r"(?i)\bplaceholder\s+here\b",    // "placeholder here"
        r"(?i)\bplaceholder\s*\)",        // "placeholder)"
        r"(?i)is\s+a\s+placeholder\b",    // "is a placeholder"
        r"(?i)this\s+is\s+placeholder\b", // "this is placeholder" (grammatically incorrect, likely placeholder)
    ];

    let combined_placeholder_context = placeholder_context_patterns.join("|");
    if let Ok(re) = regex::Regex::new(&combined_placeholder_context) {
        if re.is_match(content) {
            return Err(
                "Commit message contains placeholder text (e.g., 'placeholder for', 'placeholder text', or similar)".to_string()
            );
        }
    }

    // Also check for bare "placeholder" word with word boundaries, but exclude
    // common valid technical uses like "placeholder attribute", "placeholder element"
    let bare_placeholder = regex::Regex::new(r"(?i)\bplaceholder\b").unwrap();
    if bare_placeholder.is_match(content) {
        // Check if it's in a valid technical context
        let valid_contexts = [
            "placeholder attribute",
            "placeholder element",
            "placeholder div",
            "placeholder span",
            "placeholder class",
        ];

        let content_lower_for_ctx = content.to_lowercase();
        let mut in_valid_context = false;
        for valid_ctx in &valid_contexts {
            if content_lower_for_ctx.contains(valid_ctx) {
                in_valid_context = true;
                break;
            }
        }

        if !in_valid_context {
            return Err(
                "Commit message contains 'placeholder'. If this refers to HTML/UI attributes, use more specific language like 'placeholder attribute'".to_string()
            );
        }
    }

    Ok(())
}

/// Validate that content does not match bad commit message patterns.
fn validate_no_bad_patterns(content: &str) -> Result<(), String> {
    let content_lower = content.to_lowercase();

    // Check for bad commit message patterns (vague, meaningless messages)
    // Use regex to catch ALL variants, not just hardcoded numbers

    // Pattern 1: "chore: N file(s) changed" for ANY number N
    // Handles: "file(s) changed", "files changed", "file changed" variations
    if file_count_pattern_regex().is_match(&content_lower) {
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
        let looks_like_file_list = subject.contains('/')
            || subject.contains('\\') || // Windows paths
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

/// Validate extracted content for use as a commit message.
///
/// # Returns
///
/// `Ok(())` if valid, `Err(reason)` if invalid
pub fn validate_commit_message(content: &str) -> Result<(), String> {
    let content = content.trim();

    // Run all validation checks in order
    validate_basic_length(content)?;
    validate_no_json_artifacts(content)?;
    validate_no_literal_escape_sequences(content)?;
    validate_no_error_markers(content)?;
    validate_no_agent_errors(content)?;
    validate_no_thought_process_leakage(content)?;
    validate_no_placeholders(content)?;
    validate_no_bad_patterns(content)?;

    Ok(())
}

/// Result of a single validation check.
#[derive(Debug, Clone)]
pub struct ValidationCheckResult {
    /// Name of the validation check.
    pub name: &'static str,
    /// Whether the check passed.
    pub passed: bool,
    /// Error message if the check failed.
    pub error: Option<String>,
}

impl ValidationCheckResult {
    /// Create a passing check result.
    const fn pass(name: &'static str) -> Self {
        Self {
            name,
            passed: true,
            error: None,
        }
    }

    /// Create a failing check result.
    const fn fail(name: &'static str, error: String) -> Self {
        Self {
            name,
            passed: false,
            error: Some(error),
        }
    }
}

/// Complete validation report showing all checks run.
///
/// Unlike `validate_commit_message` which short-circuits on first failure,
/// this runs ALL checks and reports results for each one.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Results of all validation checks.
    pub checks: Vec<ValidationCheckResult>,
}

impl ValidationReport {
    /// Check if all validations passed.
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    /// Format all failed checks as a descriptive string.
    pub fn format_failures(&self) -> Option<String> {
        let failures: Vec<_> = self
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| format!("{}: {}", c.name, c.error.as_deref().unwrap_or("failed")))
            .collect();

        if failures.is_empty() {
            None
        } else {
            Some(failures.join("; "))
        }
    }
}

/// Validate commit message and return detailed report of all checks.
///
/// Unlike `validate_commit_message`, this does NOT short-circuit on first failure.
/// It runs all checks and returns a complete report.
pub fn validate_commit_message_with_report(content: &str) -> ValidationReport {
    let content = content.trim();

    // Run all validation checks without short-circuiting
    let checks = vec![
        match validate_basic_length(content) {
            Ok(()) => ValidationCheckResult::pass("basic_length"),
            Err(e) => ValidationCheckResult::fail("basic_length", e),
        },
        match validate_no_json_artifacts(content) {
            Ok(()) => ValidationCheckResult::pass("no_json_artifacts"),
            Err(e) => ValidationCheckResult::fail("no_json_artifacts", e),
        },
        match validate_no_literal_escape_sequences(content) {
            Ok(()) => ValidationCheckResult::pass("no_literal_escape_sequences"),
            Err(e) => ValidationCheckResult::fail("no_literal_escape_sequences", e),
        },
        match validate_no_error_markers(content) {
            Ok(()) => ValidationCheckResult::pass("no_error_markers"),
            Err(e) => ValidationCheckResult::fail("no_error_markers", e),
        },
        match validate_no_agent_errors(content) {
            Ok(()) => ValidationCheckResult::pass("no_agent_errors"),
            Err(e) => ValidationCheckResult::fail("no_agent_errors", e),
        },
        match validate_no_thought_process_leakage(content) {
            Ok(()) => ValidationCheckResult::pass("no_thought_process_leakage"),
            Err(e) => ValidationCheckResult::fail("no_thought_process_leakage", e),
        },
        match validate_no_placeholders(content) {
            Ok(()) => ValidationCheckResult::pass("no_placeholders"),
            Err(e) => ValidationCheckResult::fail("no_placeholders", e),
        },
        match validate_no_bad_patterns(content) {
            Ok(()) => ValidationCheckResult::pass("no_bad_patterns"),
            Err(e) => ValidationCheckResult::fail("no_bad_patterns", e),
        },
    ];

    ValidationReport { checks }
}

// =========================================================================
// Final Commit Message Rendering
// =========================================================================

/// Render the final commit message with all cleanup applied.
///
/// This is the final step before returning a commit message for use in git commit.
/// It applies:
/// 1. Escape sequence cleanup (aggressive unescaping)
/// 2. Validation to ensure no escape sequences leaked through
/// 3. Final formatting checks
///
/// If rendering fails (e.g., validation still fails after cleanup), this returns
/// the best effort cleaned message. The caller should handle validation failures.
///
/// # Arguments
///
/// * `message` - The commit message to render
///
/// # Returns
///
/// The fully rendered commit message with all escape sequences properly handled.
pub fn render_final_commit_message(message: &str) -> String {
    let mut result = message.to_string();

    // Step 1: Apply final escape sequence cleanup
    // This handles any escape sequences that leaked through the pipeline
    result = final_escape_sequence_cleanup(&result);

    // Step 2: Validate the result
    // If validation fails due to escape sequences, try aggressive cleanup
    if let Err(e) = validate_commit_message(&result) {
        // Check if the error is about escape sequences
        let error_lower = e.to_lowercase();
        if error_lower.contains("escape sequence") || error_lower.contains("\\n") {
            // Apply aggressive unescaping
            result = unescape_json_strings_aggressive(&result);
        }
        // Note: We don't re-validate here because the caller should handle validation
        // We just do our best to clean up the message
    }

    // Step 3: Final whitespace cleanup
    result = result
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    result
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
        let trimmed: &str = line.trim();

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
/// Parses diff headers like `diff --git a/path/to/file b/path/to/file` to get file paths.
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

    #[test]
    fn test_validate_empty_message() {
        let result = validate_commit_message("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_too_short() {
        let result = validate_commit_message("fix");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    #[test]
    fn test_validate_valid_message() {
        let result = validate_commit_message("feat: add new feature");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_json_artifacts() {
        let result = validate_commit_message("feat: add feature {\"type\":\"result\"}");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON artifacts"));
    }

    #[test]
    fn test_validate_error_markers() {
        let result = validate_commit_message("error: unable to generate");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("error marker"));
    }

    #[test]
    fn test_validate_thought_process_leakage() {
        let result = validate_commit_message("Looking at this diff, I can see changes");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AI thought process"));
    }

    #[test]
    fn test_validate_numbered_analysis() {
        let result = validate_commit_message("1. First change\n2. Second change");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("numbered analysis"));
    }

    #[test]
    fn test_validate_bad_file_count_pattern() {
        let result = validate_commit_message("chore: 5 files changed");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file count pattern"));
    }

    #[test]
    fn test_validate_file_list_pattern() {
        let result = validate_commit_message("chore: update src/file.rs");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file list"));
    }

    #[test]
    fn test_try_salvage_commit_message() {
        let content = "Looking at this diff...\n\nfeat: add feature";
        let salvaged = try_salvage_commit_message(content);
        assert!(salvaged.is_some());
        assert_eq!(salvaged.unwrap(), "feat: add feature");
    }

    #[test]
    fn test_try_salvage_with_body() {
        let content = "Analysis text\n\nfix(parser): resolve bug\n\nAdd proper error handling.";
        let salvaged = try_salvage_commit_message(content);
        assert!(salvaged.is_some());
        let msg = salvaged.unwrap();
        assert!(msg.starts_with("fix(parser):"));
        assert!(msg.contains("Add proper error handling"));
    }

    #[test]
    fn test_generate_fallback_empty_diff() {
        let fallback = generate_fallback_commit_message("");
        assert_eq!(fallback, "chore: apply automated changes");
    }

    #[test]
    fn test_generate_fallback_single_file() {
        let diff = r"diff --git a/src/files/extraction.rs b/src/files/extraction.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        assert!(fallback.contains("files") || fallback.contains("update"));
    }

    #[test]
    fn test_generate_fallback_multiple_files_same_dir() {
        let diff = r"diff --git a/src/files/a.rs b/src/files/a.rs
diff --git a/src/files/b.rs b/src/files/b.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        assert!(fallback.contains("files") || fallback.contains("components"));
    }

    #[test]
    fn test_generate_fallback_multiple_dirs() {
        let diff = r"diff --git a/src/a.rs b/src/a.rs
diff --git a/lib/b.rs b/lib/b.rs
diff --git a/tests/c.rs b/tests/c.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
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
    // Tests for agent error detection in output
    // =========================================================================

    #[test]
    fn test_detect_agent_errors_in_output_prompt_too_long() {
        // "Prompt is too long" should be detected as TokenExhausted
        let content = r#"{"type":"result","result":"Prompt is too long"}"#;
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_in_output_token_limit() {
        // "token limit exceeded" should be detected as TokenExhausted
        let content = r#"{"type":"result","result":"token limit exceeded"}"#;
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_in_output_context_length() {
        // "context length exceeded" should be detected as TokenExhausted
        let content = "error: context length exceeded for this model";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_in_output_maximum_context() {
        // "maximum context" should be detected as TokenExhausted
        let content = "maximum context size reached";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_in_output_input_too_large() {
        // "input too large" should be detected as TokenExhausted
        let content = "input too large for this model";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_in_output_invalid_request() {
        // "invalid request" should be detected as InvalidResponse
        let content = "invalid request to the API";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::InvalidResponse)
        );
    }

    #[test]
    fn test_detect_agent_errors_in_output_request_failed() {
        // "request failed" should be detected as InvalidResponse
        let content = "request failed due to server error";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::InvalidResponse)
        );
    }

    #[test]
    fn test_detect_agent_errors_in_output_valid_commit_message() {
        // Normal commit message should return None
        let content = r#"{"type":"result","result":"feat: add feature"}"#;
        assert_eq!(detect_agent_errors_in_output(content), None);
    }

    #[test]
    fn test_detect_agent_errors_in_output_case_insensitive() {
        // Detection should be case-insensitive
        let content = "PROMPT IS TOO LONG";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    // =========================================================================
    // Tests for enhanced error detection (Step 4 improvements)
    // =========================================================================

    #[test]
    fn test_detect_agent_errors_context_window() {
        // "context window" should be detected as TokenExhausted
        let content = "error: context window exceeded";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_max_tokens() {
        // "max tokens" should be detected as TokenExhausted
        let content = "max tokens exceeded for this request";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_token_limit() {
        // "token limit" should be detected as TokenExhausted
        let content = "token limit reached";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_too_many_tokens() {
        // "too many tokens" should be detected as TokenExhausted
        let content = "error: too many tokens in input";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_exceeds_context() {
        // "exceeds context" should be detected as TokenExhausted
        let content = "input exceeds context length";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_model_context_length() {
        // "model's context length" should be detected as TokenExhausted
        let content = "input exceeds the model's context length";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_input_exceeds() {
        // "input exceeds" should be detected as TokenExhausted
        let content = "input exceeds maximum length";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::TokenExhausted)
        );
    }

    #[test]
    fn test_detect_agent_errors_api_error() {
        // "api error" should be detected as InvalidResponse
        let content = "api error occurred";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::InvalidResponse)
        );
    }

    #[test]
    fn test_detect_agent_errors_rate_limit() {
        // "rate limit" should be detected as InvalidResponse
        let content = "rate limit exceeded";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::InvalidResponse)
        );
    }

    #[test]
    fn test_detect_agent_errors_service_unavailable() {
        // "service unavailable" should be detected as InvalidResponse
        let content = "service unavailable, try again later";
        assert_eq!(
            detect_agent_errors_in_output(content),
            Some(AgentErrorKind::InvalidResponse)
        );
    }

    #[test]
    fn test_validate_rejects_prompt_too_long() {
        // Validation should reject "Prompt is too long" messages
        let result = validate_commit_message("Prompt is too long");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("agent error"));
    }

    #[test]
    fn test_validate_rejects_token_limit_exceeded() {
        // Validation should reject "token limit exceeded" messages
        let result = validate_commit_message("token limit exceeded");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("agent error"));
    }

    #[test]
    fn test_validate_rejects_context_length() {
        // Validation should reject "context length exceeded" messages
        // Note: "error: context length exceeded" starts with "error:" which is caught by error_markers first
        // So we use a message that doesn't start with "error:" to test agent_error_patterns
        let result = validate_commit_message("The context length exceeded for this model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("agent error"));
    }

    #[test]
    fn test_validate_accepts_valid_message_with_error_words() {
        // Valid commit message containing words like "error" in a different context should pass
        // For example, "fix: resolve parsing error" is valid
        let result = validate_commit_message("fix(parser): resolve parsing error");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_json_artifacts_with_escape_sequences() {
        // Validation should reject JSON artifacts (happens before combined check)
        let result = validate_commit_message(r#"feat: add feature{"type":"result"}\\nBody text"#);
        assert!(result.is_err());
        // The JSON artifacts check runs first, so it reports JSON artifacts
        assert!(result.unwrap_err().contains("JSON artifacts"));
    }

    #[test]
    fn test_validate_rejects_json_artifacts_without_escape_sequences() {
        // Even without escape sequences, JSON artifacts should be rejected
        let result = validate_commit_message(r#"feat: add feature{"type":"result"}Body text"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON artifacts"));
    }

    #[test]
    fn test_validate_accepts_literal_escape_without_json_artifacts() {
        // Validation should accept literal \n when no JSON artifacts are present
        // This is legitimate content (e.g., "fix: handle \n in filenames")
        let result = validate_commit_message("feat: add feature\\nBody text");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_accepts_literal_tab_without_json_artifacts() {
        // Validation should accept literal \t when no JSON artifacts are present
        let result = validate_commit_message("feat: add feature\\t- bullet");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_accepts_actual_newlines() {
        // Validation should accept actual newlines (not literal escape sequences)
        let result = validate_commit_message("feat: add feature\n\nBody text here");
        assert!(result.is_ok());
    }

    // =========================================================================
    // Tests for enhanced escape sequence validation (Step 1 improvements)
    // =========================================================================

    #[test]
    fn test_validate_rejects_body_starts_with_literal_newline_sequences() {
        // Validation should reject when body starts with literal \n\n after subject
        // This happens when JSON like {"subject": "feat", "body": "\\n\\ntext"}
        // is parsed but not unescaped - the body value contains literal \n\n
        // The test input has an actual newline after the subject, then literal \\n\\n
        let result = validate_commit_message("feat: add feature\n\\n\\nBody text here");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("literal escape sequences"));
    }

    #[test]
    fn test_validate_rejects_body_second_line_is_literal_escape() {
        // Validation should reject when second line is literally "\\n"
        let result = validate_commit_message("feat: add feature\n\\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("literal escape sequences"));
    }

    #[test]
    fn test_validate_rejects_body_second_line_is_double_literal_escape() {
        // Validation should reject when second line is literally "\\n\\n"
        let result = validate_commit_message("feat: add feature\n\\n\\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("literal escape sequences"));
    }

    #[test]
    fn test_validate_rejects_repeated_literal_escape_sequences() {
        // Validation should reject repeated literal \\n\\n\\n patterns
        let result = validate_commit_message("feat: add feature\\n\\n\\nBody text");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("repeated literal escape sequences"));
    }

    #[test]
    fn test_validate_rejects_quadruple_literal_escape_sequences() {
        // Validation should reject \\n\\n\\n\\n patterns
        let result = validate_commit_message("feat: add feature\\n\\n\\n\\nBody text");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("repeated literal escape sequences"));
    }

    #[test]
    fn test_validate_accepts_legitimate_single_escape_in_middle() {
        // Validation should accept single \\n in middle of text (legitimate content)
        let result = validate_commit_message("feat: handle backslash-n in parser");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_accepts_body_with_actual_newlines() {
        // Validation should accept actual newlines in body
        let result =
            validate_commit_message("feat: add feature\n\nThis is the body\nwith multiple lines");
        assert!(result.is_ok());
    }

    // =========================================================================
    // Tests for CommitExtractionResult::AgentError variant
    // =========================================================================

    #[test]
    fn test_commit_extraction_result_agent_error() {
        // Test AgentError variant methods
        let result = CommitExtractionResult::AgentError(AgentErrorKind::TokenExhausted);

        assert!(result.is_agent_error());
        assert!(!result.is_fallback());
        assert_eq!(result.error_kind(), Some(AgentErrorKind::TokenExhausted));
        assert_eq!(result.into_message(), String::new());
    }

    #[test]
    fn test_commit_extraction_result_extracted_not_agent_error() {
        // Test that Extracted variant is not an agent error
        let result = CommitExtractionResult::Extracted("feat: add feature".to_string());

        assert!(!result.is_agent_error());
        assert!(!result.is_fallback());
        assert_eq!(result.error_kind(), None);
        assert_eq!(result.into_message(), "feat: add feature");
    }

    // =========================================================================
    // Tests for format_structured_commit with escaped sequences
    // =========================================================================

    #[test]
    fn test_format_structured_commit_unescapes_body_newlines() {
        // Test that format_structured_commit properly unescapes \n in body
        let msg = StructuredCommitMessage {
            subject: "feat: add feature".to_string(),
            body: Some("Line 1\\nLine 2\\nLine 3".to_string()),
        };
        let result = format_structured_commit(&msg);
        assert!(result.is_some());
        let formatted = result.unwrap();
        assert!(formatted.contains("Line 1\nLine 2\nLine 3"));
        assert!(!formatted.contains("\\n"));
    }

    #[test]
    fn test_format_structured_commit_unescapes_subject_newlines() {
        // Test that format_structured_commit properly unescapes \n in subject
        // Note: After unescaping, "feat: add\nfeature" contains an actual newline.
        // The is_conventional_commit_subject check only validates the prefix (feat:),
        // so this passes validation. The resulting commit message would have an embedded
        // newline in the subject, which is unusual but technically passes the checks.
        let msg = StructuredCommitMessage {
            subject: "feat: add\\nfeature".to_string(),
            body: None,
        };
        let result = format_structured_commit(&msg);
        // The result is Some because "feat:" is a valid type prefix
        assert!(result.is_some());
        // The subject has been unescaped, so it contains an actual newline
        assert!(result.unwrap().contains('\n'));
    }

    #[test]
    fn test_format_structured_commit_with_empty_body() {
        // Test that format_structured_commit works with empty body
        let msg = StructuredCommitMessage {
            subject: "fix: resolve bug".to_string(),
            body: None,
        };
        let result = format_structured_commit(&msg);
        assert_eq!(result, Some("fix: resolve bug".to_string()));
    }

    #[test]
    fn test_format_structured_commit_with_body_containing_tabs() {
        // Test that format_structured_commit properly unescapes \t in body
        let msg = StructuredCommitMessage {
            subject: "feat: add feature".to_string(),
            body: Some("- item 1\\t- item 2".to_string()),
        };
        let result = format_structured_commit(&msg);
        assert!(result.is_some());
        let formatted = result.unwrap();
        assert!(formatted.contains("- item 1\t- item 2"));
        assert!(!formatted.contains("\\t"));
    }

    // =========================================================================
    // Tests for render_final_commit_message
    // =========================================================================

    #[test]
    fn test_render_final_commit_message_with_literal_escapes() {
        // Test that render_final_commit_message cleans up escape sequences
        // Note: whitespace cleanup removes blank lines
        let input = "feat: add feature\n\\n\\nBody with literal escapes";
        let result = render_final_commit_message(input);
        assert_eq!(result, "feat: add feature\nBody with literal escapes");
    }

    #[test]
    fn test_render_final_commit_message_already_clean() {
        // Test that already-clean messages pass through (whitespace cleanup applied)
        let input = "feat: add feature\n\nBody text here";
        let result = render_final_commit_message(input);
        assert_eq!(result, "feat: add feature\nBody text here");
    }

    #[test]
    fn test_render_final_commit_message_with_tabs() {
        // Test that tab escapes are properly handled
        let input = "feat: add feature\\n\\t- item 1\\n\\t- item 2";
        let result = render_final_commit_message(input);
        // Tabs are stripped by whitespace cleanup (trim() removes leading whitespace)
        assert_eq!(result, "feat: add feature\n- item 1\n- item 2");
    }

    #[test]
    fn test_render_final_commit_message_with_carriage_returns() {
        // Test that carriage return escapes are properly handled
        let input = "feat: add feature\\r\\nBody text";
        let result = render_final_commit_message(input);
        // Carriage returns are converted, but whitespace cleanup removes extra blank lines
        assert_eq!(result, "feat: add feature\nBody text");
    }

    #[test]
    fn test_render_final_commit_message_double_escaped() {
        // Test that double-escaped sequences are handled
        let input = "feat: add feature\n\\\\n\\\\nDouble escaped";
        let result = render_final_commit_message(input);
        // Double backslash-n becomes backslash-n (literal backslash + n) after cleanup
        // The whitespace cleanup then removes the blank lines
        assert_eq!(result, "feat: add feature\n\\\n\\\nDouble escaped");
    }

    #[test]
    fn test_render_final_commit_message_whitespace_cleanup() {
        // Test that trailing empty lines are removed
        let input = "feat: add feature\n\nBody text\n\n\n  \n  ";
        let result = render_final_commit_message(input);
        assert_eq!(result, "feat: add feature\nBody text");
    }

    #[test]
    fn test_render_final_commit_message_mixed_escape_sequences() {
        // Test handling of mixed escape sequences
        let input = "feat: add feature\\n\\nDetails:\\r\\n\\t- item 1\\n\\t- item 2";
        let result = render_final_commit_message(input);
        // Carriage returns normalized to newlines, tabs stripped by trim, blank lines removed
        assert_eq!(result, "feat: add feature\nDetails:\n- item 1\n- item 2");
    }

    #[test]
    fn test_render_final_commit_message_trailing_whitespace_lines() {
        // Test that empty lines with only whitespace are cleaned up
        let input = "feat: add feature\n\\n\\n  Body with spaces  \\n  \\n  ";
        let result = render_final_commit_message(input);
        // Whitespace cleanup removes blank lines and trims each line
        assert_eq!(result, "feat: add feature\nBody with spaces");
    }

    // =========================================================================
    // Tests for try_extract_structured_commit
    // =========================================================================

    #[test]
    fn test_try_extract_structured_commit_direct_json() {
        // Test that direct JSON with subject and body is extracted correctly
        let json = r#"{"subject":"fix(commit): try simpler prompts after agent errors","body":"When all agents fail for a prompt variant, keep iterating through progressively simpler prompt strategies instead of aborting the retry loop."}"#;
        let result = try_extract_structured_commit_with_trace(json).0;
        assert!(result.is_some(), "Should extract commit from direct JSON");
        let msg = result.unwrap();
        assert!(msg.starts_with("fix(commit):"), "Should start with type");
        assert!(msg.contains("try simpler prompts after agent errors"));
        assert!(msg.contains("When all agents fail"));
    }

    #[test]
    fn test_try_extract_structured_commit_json_no_body() {
        // Test JSON with subject only
        let json = r#"{"subject":"feat: add new feature"}"#;
        let result = try_extract_structured_commit_with_trace(json).0;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "feat: add new feature");
    }

    #[test]
    fn test_try_extract_structured_commit_code_fence() {
        // Test JSON inside markdown code fence
        let content = r#"Here is the commit message:
```json
{"subject":"fix: resolve bug","body":"Details about the fix."}
```
"#;
        let result = try_extract_structured_commit_with_trace(content).0;
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.starts_with("fix: resolve bug"));
        assert!(msg.contains("Details about the fix"));
    }

    #[test]
    fn test_try_extract_structured_commit_with_preamble() {
        // Test JSON with some preamble text
        let content = r#"Based on the diff, here is my commit:
{"subject":"refactor: simplify logic","body":"Removed unnecessary complexity."}"#;
        let result = try_extract_structured_commit_with_trace(content).0;
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.starts_with("refactor:"));
    }

    #[test]
    fn test_try_extract_structured_commit_invalid_type() {
        // Test JSON with invalid conventional commit type
        let json = r#"{"subject":"invalid: not a real type","body":"Body"}"#;
        let result = try_extract_structured_commit_with_trace(json).0;
        assert!(result.is_none(), "Should reject invalid commit type");
    }

    #[test]
    fn test_try_extract_structured_commit_from_ndjson() {
        // Test extraction from NDJSON stream with result type
        let ndjson = r#"{"type":"stream_event","data":"..."}
{"type":"result","result":"{\"subject\":\"docs: update readme\",\"body\":\"Add usage examples.\"}"}
"#;
        let result = try_extract_structured_commit_with_trace(ndjson).0;
        assert!(result.is_some(), "Should extract from NDJSON result field");
        let msg = result.unwrap();
        assert!(msg.starts_with("docs: update readme"));
    }

    #[test]
    fn test_try_extract_structured_commit_from_ndjson_with_markdown_fence() {
        // Test extraction from NDJSON stream where result contains markdown with JSON code fence
        // This is the format used by PROMPT-LOG5.log (GLM-4.7 with markdown JSON)
        let ndjson = r#"{"type":"stream_event","data":"..."}
{"type":"result","result":"The changes look clean. Now I'll generate the commit message:\n\n```json\n{\n  \"subject\": \"refactor(review): pass diff directly to all review prompts\",\n  \"body\": \"Previously, review prompts would tell agents to run git commands to\\nfetch the diff. This change:\\n\\n1. Fetches the diff once at the start of build_review_prompt\\n2. Passes it directly to all review prompt functions\"\n}\n```"}
"#;
        let result = try_extract_structured_commit_with_trace(ndjson).0;
        assert!(
            result.is_some(),
            "Should extract from NDJSON result field with markdown code fence"
        );
        let msg = result.unwrap();
        assert!(msg.starts_with("refactor(review):"));
        assert!(msg.contains("pass diff directly"));
    }

    // =========================================================================
    // Tests for validate_commit_message - JSON artifact detection
    // =========================================================================

    #[test]
    fn test_validate_commit_message_raw_json_structure() {
        // Test that raw JSON commit structure is rejected
        let raw_json = r#"{"subject":"fix: something","body":"Details"}"#;
        let result = validate_commit_message(raw_json);
        assert!(result.is_err(), "Raw JSON should be rejected");
        assert!(
            result.unwrap_err().contains("JSON"),
            "Error should mention JSON"
        );
    }

    #[test]
    fn test_validate_commit_message_json_with_subject_key() {
        // Regression test: {"subject":...} pattern should be detected as JSON artifact
        let bad_msg = r#"{"subject":"feat: add feature","body":"Some body"}"#;
        let result = validate_commit_message(bad_msg);
        assert!(
            result.is_err(),
            "Commit message containing {{\"subject\":}} should be rejected"
        );
    }

    // =========================================================================
    // Tests for XML extraction (try_extract_xml_commit)
    // =========================================================================

    #[test]
    fn test_xml_extract_basic_subject_only() {
        // Test basic XML extraction with subject only
        let content = r"<ralph-commit>
<ralph-subject>feat: add new feature</ralph-subject>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should extract from basic XML");
        assert_eq!(result.unwrap(), "feat: add new feature");
    }

    #[test]
    fn test_xml_extract_with_body() {
        // Test XML extraction with subject and body
        let content = r"<ralph-commit>
<ralph-subject>feat(auth): add OAuth2 login flow</ralph-subject>
<ralph-body>Implement Google and GitHub OAuth providers.
Add session management for OAuth tokens.</ralph-body>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should extract from XML with body");
        let msg = result.unwrap();
        assert!(msg.starts_with("feat(auth): add OAuth2 login flow"));
        assert!(msg.contains("Implement Google and GitHub OAuth providers"));
        assert!(msg.contains("Add session management"));
    }

    #[test]
    fn test_xml_extract_with_empty_body() {
        // Test XML extraction with empty body tags
        let content = r"<ralph-commit>
<ralph-subject>fix: resolve bug</ralph-subject>
<ralph-body></ralph-body>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should extract even with empty body");
        // Empty body should be treated as no body
        assert_eq!(result.unwrap(), "fix: resolve bug");
    }

    #[test]
    fn test_xml_extract_ignores_preamble() {
        // Test that content before <ralph-commit> is ignored
        let content = r"Here is the commit message based on my analysis:

Looking at the diff, I can see...

<ralph-commit>
<ralph-subject>refactor: simplify logic</ralph-subject>
</ralph-commit>

That's all!";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should ignore preamble and extract XML");
        assert_eq!(result.unwrap(), "refactor: simplify logic");
    }

    #[test]
    fn test_xml_extract_fails_missing_tags() {
        // Test that extraction fails when tags are missing
        let content = "Just some text without XML tags";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_none(), "Should fail when XML tags are missing");
    }

    #[test]
    fn test_xml_extract_fails_invalid_commit_type() {
        // Test that extraction fails for invalid conventional commit types
        let content = r"<ralph-commit>
<ralph-subject>invalid: not a real type</ralph-subject>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_none(), "Should reject invalid commit type");
    }

    #[test]
    fn test_xml_extract_fails_missing_subject() {
        // Test that extraction fails when subject is missing
        let content = r"<ralph-commit>
<ralph-body>Just a body, no subject</ralph-body>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_none(), "Should fail when subject is missing");
    }

    #[test]
    fn test_xml_extract_fails_empty_subject() {
        // Test that extraction fails when subject is empty
        let content = r"<ralph-commit>
<ralph-subject></ralph-subject>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_none(), "Should fail when subject is empty");
    }

    #[test]
    fn test_xml_extract_handles_whitespace_in_subject() {
        // Test that whitespace around subject is trimmed
        let content = r"<ralph-commit>
<ralph-subject>   docs: update readme   </ralph-subject>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should handle whitespace in subject");
        assert_eq!(result.unwrap(), "docs: update readme");
    }

    #[test]
    fn test_xml_extract_with_breaking_change() {
        // Test XML extraction with breaking change indicator
        let content = r"<ralph-commit>
<ralph-subject>feat!: drop Python 3.7 support</ralph-subject>
<ralph-body>BREAKING CHANGE: Minimum Python version is now 3.8.</ralph-body>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should handle breaking change indicator");
        let msg = result.unwrap();
        assert!(msg.starts_with("feat!:"));
        assert!(msg.contains("BREAKING CHANGE"));
    }

    #[test]
    fn test_xml_extract_with_scope() {
        // Test XML extraction with scope
        let content = r"<ralph-commit>
<ralph-subject>test(parser): add coverage for edge cases</ralph-subject>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should handle scope in subject");
        assert_eq!(result.unwrap(), "test(parser): add coverage for edge cases");
    }

    #[test]
    fn test_xml_extract_body_preserves_newlines() {
        // Test that newlines in body are preserved
        let content = r"<ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
<ralph-body>Line 1
Line 2
Line 3</ralph-body>
</ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_some(), "Should preserve newlines in body");
        let msg = result.unwrap();
        assert!(msg.contains("Line 1\nLine 2\nLine 3"));
    }

    #[test]
    fn test_xml_extract_fails_malformed_tags() {
        // Test that extraction fails for malformed tags (end before start)
        let content = r"</ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
<ralph-commit>";
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(result.is_none(), "Should fail for malformed tags");
    }

    #[test]
    fn test_xml_extract_handles_markdown_code_fence() {
        // Test that XML inside markdown code fence is NOT extracted
        // (the XML tags should be directly in the output, not wrapped)
        let content = r"```xml
<ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
</ralph-commit>
```";
        // The XML extractor looks for tags directly, so this should still work
        // since the tags are present in the content
        let result = try_extract_xml_commit_with_trace(content).0;
        assert!(
            result.is_some(),
            "Should extract from XML even inside code fence"
        );
    }
}
