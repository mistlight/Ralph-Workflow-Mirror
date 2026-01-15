//! Text Cleaning Functions for LLM Output
//!
//! This module provides functions to clean and filter extracted LLM output,
//! removing AI thought processes, formatted thinking patterns, and other artifacts.

use regex::Regex;

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
pub fn remove_thought_process_patterns(content: &str) -> String {
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

/// Clean plain text output by removing common artifacts.
///
/// This handles:
/// - Markdown code fences
/// - Formatted thinking output (e.g., "\[Claude\] Thinking: ...")
/// - AI thought process patterns (e.g., "Looking at this diff...")
/// - Common prefixes like "Commit message:", "Output:", etc.
/// - Excessive whitespace
/// - JSON escape sequences that weren't properly unescaped
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
mod tests {
    use super::*;

    #[test]
    fn test_remove_thought_process_patterns_basic() {
        let input = "Looking at this diff, I can see changes to parser.\n\nfeat(parser): fix bug";
        let result = remove_thought_process_patterns(input);
        assert_eq!(result, "feat(parser): fix bug");
    }

    #[test]
    fn test_remove_thought_process_patterns_numbered_list() {
        let input = "1. First change\n2. Second change\n\nfix: actual commit";
        let result = remove_thought_process_patterns(input);
        assert_eq!(result, "fix: actual commit");
    }

    #[test]
    fn test_remove_thought_process_patterns_markdown_bold() {
        let input = "1. **Analysis** (file.rs): Description\n\nfeat: add feature";
        let result = remove_thought_process_patterns(input);
        assert_eq!(result, "feat: add feature");
    }

    #[test]
    fn test_looks_like_commit_message_start() {
        assert!(looks_like_commit_message_start("feat: add feature"));
        assert!(looks_like_commit_message_start("fix(parser): resolve bug"));
        assert!(looks_like_commit_message_start(
            "chore(docs): update readme"
        ));
        assert!(!looks_like_commit_message_start("Add feature"));
        assert!(!looks_like_commit_message_start("Update code"));
    }

    #[test]
    fn test_find_conventional_commit_start() {
        let input = "Some analysis text\n\nfeat: add feature";
        let pos = find_conventional_commit_start(input);
        assert_eq!(pos, Some(20)); // Position of "feat" (after "Some analysis text\n\n")
    }

    #[test]
    fn test_looks_like_analysis_text() {
        assert!(looks_like_analysis_text(
            "Looking at this diff, I see changes"
        ));
        assert!(looks_like_analysis_text(
            "1. First change\n2. Second change"
        ));
        assert!(looks_like_analysis_text("Key categories of changes"));
        assert!(!looks_like_analysis_text("feat: add feature"));
    }

    #[test]
    fn test_remove_formatted_thinking_patterns() {
        let input = "[Claude] Thinking: Analyzing...\n\nfeat: actual message";
        let result = remove_formatted_thinking_patterns(input);
        assert!(result.contains("feat: actual message"));
        assert!(!result.contains("[Claude] Thinking"));
    }

    #[test]
    fn test_clean_plain_text() {
        let input = "```text\nfeat: add feature\n```";
        let result = clean_plain_text(input);
        assert_eq!(result, "feat: add feature");
    }

    #[test]
    fn test_unescape_json_strings_newlines() {
        let input = "feat: add feature\\n\\nThis adds new functionality.";
        let result = unescape_json_strings(input);
        assert_eq!(result, "feat: add feature\n\nThis adds new functionality.");
    }

    #[test]
    fn test_unescape_json_strings_tabs() {
        let input = "feat: add feature\\n\\t- bullet point";
        let result = unescape_json_strings(input);
        assert_eq!(result, "feat: add feature\n\t- bullet point");
    }

    #[test]
    fn test_unescape_json_strings_mixed() {
        let input = "feat: add feature\\n\\nDetails:\\r\\n\\t- item 1\\n\\t- item 2";
        let result = unescape_json_strings(input);
        assert_eq!(
            result,
            "feat: add feature\n\nDetails:\r\n\t- item 1\n\t- item 2"
        );
    }

    #[test]
    fn test_unescape_json_strings_no_escape_sequences() {
        let input = "feat: add feature\n\nThis is already correct.";
        let result = unescape_json_strings(input);
        // Should not modify content that doesn't have literal escape sequences
        assert_eq!(result, "feat: add feature\n\nThis is already correct.");
    }

    #[test]
    fn test_clean_plain_text_unescapes() {
        let input = "```text\nfeat: add feature\\n\\nThis adds new functionality.\n```";
        let result = clean_plain_text(input);
        // Note: clean_plain_text removes empty lines, so double newlines become single
        assert_eq!(result, "feat: add feature\nThis adds new functionality.");
    }
}
