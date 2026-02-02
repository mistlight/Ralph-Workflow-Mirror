//! Text Cleaning Functions for LLM Output
//!
//! This module provides functions to clean and filter extracted LLM output,
//! removing AI thought processes, formatted thinking patterns, and other artifacts.

/// Unescape common JSON escape sequences in text.
///
/// This handles cases where LLMs output content with JSON escapes (like `\n` for newline)
/// that weren't properly decoded before being used as commit messages.
///
/// This is needed because some agents output JSON string values with escape sequences
/// that leak through when the JSON is parsed but not fully unescaped.
///
/// # Examples
///
/// ```
/// # use ralph_workflow::files::llm_output_extraction::cleaning::unescape_json_strings;
/// let input = "feat: add feature\\n\\nThis adds new functionality.";
/// let result = unescape_json_strings(input);
/// assert_eq!(result, "feat: add feature\n\nThis adds new functionality.");
/// ```
pub fn unescape_json_strings(content: &str) -> String {
    let mut result = content.to_string();

    // Common JSON escape sequences that might leak
    // Note: We only replace literal backslash-n patterns, not actual newlines
    // We need to be careful to only replace literal \n, \t, \r sequences
    // that appear in the text (which indicates improper JSON unescaping)

    // We use a more careful approach: replace literal backslash followed by n/t/r
    // But we must be careful not to double-escape already correct content

    // Check if we have literal escape sequences (backslash followed by n/t/r)
    // This is indicated by the presence of "\n" (two characters: backslash, n)
    // NOT a newline character
    if result.contains("\\n") || result.contains("\\t") || result.contains("\\r") {
        result = result.replace("\\n", "\n"); // newline
        result = result.replace("\\t", "\t"); // tab
        result = result.replace("\\r", "\r"); // carriage return
    }

    result
}

/// Aggressively unescape all JSON escape sequences, including multiple passes.
///
/// This function is more aggressive than `unescape_json_strings()` and performs
/// multiple passes to catch escape sequences that may be embedded in different ways.
///
/// This is used as a final cleanup step to ensure no escape sequences leak through.
///
/// # Examples
///
/// ```
/// # use ralph_workflow::files::llm_output_extraction::cleaning::unescape_json_strings_aggressive;
/// let input = "feat: add feature\\\\n\\\\nDouble escaped";
/// let result = unescape_json_strings_aggressive(input);
/// assert_eq!(result, "feat: add feature\n\nDouble escaped");
/// ```
pub fn unescape_json_strings_aggressive(content: &str) -> String {
    let mut result = content.to_string();
    let mut previous_len: usize;

    // Multiple passes: handle double-escaped sequences like \\n -> \n -> actual newline
    loop {
        previous_len = result.len();

        // Replace all escape sequences
        result = result.replace("\\\\n", "\n"); // double-escaped newline
        result = result.replace("\\\\t", "\t"); // double-escaped tab
        result = result.replace("\\\\r", "\r"); // double-escaped carriage return
        result = result.replace("\\n", "\n"); // single-escaped newline
        result = result.replace("\\t", "\t"); // single-escaped tab
        result = result.replace("\\r", "\r"); // single-escaped carriage return

        // If no changes were made, we're done
        if result.len() == previous_len {
            break;
        }
    }

    result
}

/// Check if content contains literal escape sequences that indicate improper unescaping.
///
/// Returns true if the content contains patterns like `\n`, `\t`, `\r` that suggest
/// JSON escape sequences were not properly converted to actual characters.
///
/// This is used to detect cases where unescaping failed and we need to apply it again.
pub fn contains_literal_escape_sequences(content: &str) -> bool {
    // We look for literal escape sequences that are likely from improper JSON unescaping
    // To avoid false positives on legitimate content (like code examples), we check
    // for patterns that are characteristic of unescaping failures

    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Check for body starting with literal escape sequences (after subject line)
        // Pattern: "feat: add\n\\n\\nBody text" - the second line is literally "\n\n"
        if i == 1 && (trimmed == "\\n" || trimmed == "\\n\\n" || trimmed.starts_with("\\n\\n")) {
            return true;
        }

        // Check for repeated escape sequences that suggest bulk unescaping failure
        // Pattern: "\\n\\n\\n" or "\\n\\n\\n\\n" - multiple escaped newlines
        if trimmed.contains("\\n\\n\\n") || trimmed.contains("\\n\\n\\n\\n") {
            return true;
        }
    }

    false
}

/// Apply final post-processing to ensure no escape sequences remain in commit message.
///
/// This is called as the last step before returning a commit message to ensure
/// any escape sequences that leaked through the pipeline are caught and fixed.
///
/// Returns the cleaned commit message.
pub fn final_escape_sequence_cleanup(message: &str) -> String {
    let mut result = message.to_string();

    // If we detect literal escape sequences, apply aggressive unescaping
    if contains_literal_escape_sequences(&result) {
        result = unescape_json_strings_aggressive(&result);
    } else {
        // Even without detection, apply standard unescaping to be safe
        result = unescape_json_strings(&result);
    }

    result
}

/// Pre-process raw log content by applying aggressive escape sequence unescaping.
///
/// This is the FIRST transformation applied to raw log content to handle cases where
/// agents output JSON with improperly escaped strings. This handles:
/// - Single-escaped: \n -> newline
/// - Double-escaped: \\n -> newline
/// - Triple-escaped: \\\n -> backslash + newline
///
/// The function is idempotent - calling it multiple times produces the same result.
pub fn preprocess_raw_content(content: &str) -> String {
    let mut result = content.to_string();
    let mut previous_len: usize;

    // Multiple passes to handle nested escaping
    loop {
        previous_len = result.len();

        // Handle all escape sequence variations using placeholder tokens
        // This allows us to distinguish between single and double escaped sequences
        result = result.replace("\\\\n", "\x00NEWLINE\x00"); // Mark double-escaped
        result = result.replace("\\n", "\n"); // Single to actual
        result = result.replace("\x00NEWLINE\x00", "\n"); // Double to actual

        // Same for tabs and carriage returns
        result = result.replace("\\\\t", "\x00TAB\x00");
        result = result.replace("\\t", "\t");
        result = result.replace("\x00TAB\x00", "\t");

        result = result.replace("\\\\r", "\x00CR\x00");
        result = result.replace("\\r", "\r");
        result = result.replace("\x00CR\x00", "\r");

        // If no changes, we're done
        if result.len() == previous_len {
            break;
        }
    }

    result
}

#[cfg(test)]
include!("cleaning/test_helpers_thought_stripping.rs");

#[cfg(test)]
include!("cleaning/test_helpers_formatting.rs");

#[cfg(test)]
mod cleaning_tests;
