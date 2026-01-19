//! Text Cleaning Functions for LLM Output
//!
//! This module provides functions to clean and filter extracted LLM output,
//! removing AI thought processes, formatted thinking patterns, and other artifacts.

#[cfg(test)]
use regex::Regex;
#[cfg(test)]
use std::sync::OnceLock;

/// Strip thought pattern and look for commit message with blank line detection.
///
/// Handles pattern stripping with blank line detection. Returns Some(result) if
/// pattern was found and processed, None otherwise.
#[cfg(test)]
fn strip_thought_pattern_with_blank_line(content: &str, pattern: &str) -> Option<String> {
    let rest = content.strip_prefix(pattern)?;

    // Find the first blank line after the pattern
    if let Some(blank_line_pos) = rest.find("\n\n") {
        let remaining = rest[blank_line_pos + 2..].trim();
        if !remaining.is_empty() {
            // Check if it looks like a clean commit message
            if looks_like_commit_message_start(remaining) {
                return Some(remaining.to_string());
            }
            // Otherwise, update result to continue with aggressive filtering below
            return Some(remaining.to_string());
        }
    } else if let Some(single_newline) = rest.find('\n') {
        // If no double newline, try to skip to after the first single newline
        let after_newline = &rest[single_newline + 1..];
        // Check if what follows looks like a commit message
        if looks_like_commit_message_start(after_newline.trim()) {
            return Some(after_newline.to_string());
        }
        // If not, check if the rest starts with numbered analysis
        if after_newline.trim().starts_with("1. ")
            || after_newline.trim().starts_with("1. **")
            || after_newline.trim().starts_with("- ")
        {
            return Some(after_newline.trim().to_string());
        }
    }

    // If the remaining content after the pattern is all analysis (no valid commit), return empty
    let rest_trimmed = rest.trim();
    if looks_like_analysis_text(rest_trimmed)
        && find_conventional_commit_start(rest_trimmed).is_none()
    {
        return Some(String::new());
    }

    None
}

/// Strip numbered analysis patterns and extract commit message.
///
/// Handles numbered list patterns like "1. First change\n2. Second change\n\nfix: actual".
/// Returns Some(result) if pattern was found and processed, None otherwise.
#[cfg(test)]
fn strip_numbered_analysis_patterns(content: &str) -> Option<String> {
    let result_lower = content.to_lowercase();
    let numbered_start_patterns = [
        "1. ",
        "1)\n",
        "- first",
        "- the first",
        "* first",
        "* the first",
    ];

    for pattern in &numbered_start_patterns {
        if result_lower.starts_with(pattern) || content.starts_with(pattern) {
            // Try to find the commit message by looking for conventional commit format
            if let Some(commit_start) = find_conventional_commit_start(content) {
                return Some(content[commit_start..].to_string());
            }
            // Fallback: look for a blank line after the analysis
            if let Some(blank_pos) = content.find("\n\n") {
                let after_analysis = &content[blank_pos + 2..];
                // Check if the content after looks like a real commit message
                if after_analysis.trim().starts_with(char::is_alphanumeric) {
                    return Some(after_analysis.to_string());
                }
            }
            return Some(content.to_string());
        }
    }

    None
}

/// Strip markdown bold analysis patterns and extract commit message.
///
/// Handles markdown bold patterns like "1. **Test assertion style** (file.rs): Description".
/// Returns Some(result) if pattern was found and processed, None otherwise.
#[cfg(test)]
fn strip_markdown_bold_analysis(content: &str) -> Option<String> {
    if starts_with_markdown_bold_analysis(content) {
        if let Some(commit_start) = find_conventional_commit_start(content) {
            return Some(content[commit_start..].to_string());
        }
        // Fallback: look for double newline after the analysis
        if let Some(blank_pos) = content.find("\n\n") {
            let after_analysis = &content[blank_pos + 2..];
            if after_analysis.trim().starts_with(char::is_alphanumeric) {
                return Some(after_analysis.to_string());
            }
        }
        return Some(content.to_string());
    }
    None
}

/// Extract commit from analysis by detecting multi-line analysis with conventional commit.
///
/// Handles aggressive filtering with commit detection. Returns Some(result) if pattern
/// was found and processed, None otherwise.
#[cfg(test)]
fn extract_commit_from_analysis(content: &str) -> Option<String> {
    if let Some(commit_start) = find_conventional_commit_start(content) {
        // Verify that the content before the commit looks like analysis
        let before_commit = &content[..commit_start];
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
            return Some(content[commit_start..].to_string());
        }
    }
    None
}

/// Filter analysis text with embedded conventional commit type mentions.
///
/// Handles analysis text filtering with embedded type mentions like "**feat**".
/// Returns Some(result) if content should be filtered, None otherwise.
#[cfg(test)]
fn filter_analysis_with_embedded_types(content: &str) -> Option<String> {
    if !looks_like_analysis_text(content) {
        return None;
    }

    // Check if there's markdown-bold type mention embedded in analysis text
    // like "This is a **refactor**..." which indicates analysis, not a commit
    let result_lower = content.to_lowercase();
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
            return Some(content.to_string());
        }
        // Otherwise, it's analysis with embedded type mentions, filter it out
        return Some(String::new());
    }

    // If no conventional commit was found and it looks like analysis, return empty
    if find_conventional_commit_start(content).is_none() {
        return Some(String::new());
    }

    None
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
#[cfg(test)]
pub fn remove_thought_process_patterns(content: &str) -> String {
    let mut result = content.to_string();

    // Remove AI thought process prefixes
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
        "The most substantive change is",
        "The most substantive changes are",
        "The most substantive user-facing change is",
    ];

    for pattern in &thought_patterns {
        if let Some(new_result) = strip_thought_pattern_with_blank_line(&result, pattern) {
            result = new_result;
            break;
        }
    }

    // Remove numbered analysis patterns
    if let Some(new_result) = strip_numbered_analysis_patterns(&result) {
        return new_result;
    }

    // Remove markdown bold analysis patterns
    if let Some(new_result) = strip_markdown_bold_analysis(&result) {
        return new_result;
    }

    // Additional aggressive filtering: detect analysis ending with conventional commit
    if let Some(new_result) = extract_commit_from_analysis(&result) {
        return new_result;
    }

    // Final check: filter analysis with embedded type mentions
    if let Some(new_result) = filter_analysis_with_embedded_types(&result) {
        return new_result;
    }

    result
}

/// Check if text starts with markdown bold analysis patterns.
///
/// Returns true if the text starts with patterns like:
/// - "1. **Category** (file.rs): Description"
/// - "**Category**:"
/// - Multiple numbered lines with **bold** headers
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub fn find_conventional_commit_start(text: &str) -> Option<usize> {
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
#[cfg(test)]
pub fn looks_like_analysis_text(text: &str) -> bool {
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

/// ANSI escape code regex - compiled once using `OnceLock` for efficiency.
/// Matches patterns like \x1b[...m or \x1b[...K used for terminal formatting.
#[cfg(test)]
fn ansi_escape_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[mK]").expect("ANSI regex should be valid"))
}

/// Remove formatted thinking output patterns from CLI display output.
///
/// This handles formatted thinking content that appears in log files from display
/// formatting, such as `[Claude] Thinking: ...` or `[Agent] Thinking: ...`.
/// These patterns may include ANSI color codes.
///
/// The function removes lines that contain formatted thinking markers and any
/// subsequent content until a blank line or conventional commit pattern is found.
#[cfg(test)]
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
        ansi_escape_regex().replace_all(text, "").to_string()
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

/// Clean plain text output by removing common artifacts.
///
/// This handles:
/// - Markdown code fences
/// - Formatted thinking output (e.g., "\[Claude\] Thinking: ...")
/// - AI thought process patterns (e.g., "Looking at this diff...")
/// - Common prefixes like "Commit message:", "Output:", etc.
/// - Excessive whitespace
/// - JSON escape sequences that weren't properly unescaped
#[cfg(test)]
pub fn clean_plain_text(content: &str) -> String {
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

    // Unescape JSON escape sequences (final cleanup step)
    // This handles cases where LLMs output literal \n instead of actual newlines
    result = unescape_json_strings(&result);

    // Clean up whitespace
    result
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod cleaning_tests;
