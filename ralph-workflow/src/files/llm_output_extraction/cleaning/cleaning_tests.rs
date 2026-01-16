//! Tests for the cleaning module.

#[cfg(test)]
mod tests {
    use crate::files::llm_output_extraction::cleaning::*;

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

    #[test]
    fn test_unescape_json_strings_idempotent() {
        // Calling unescape_json_strings twice should produce the same result
        // as calling it once (idempotent)
        let input = "feat: add feature\\n\\nThis adds new functionality.";
        let once = unescape_json_strings(input);
        let twice = unescape_json_strings(&once);
        assert_eq!(once, twice);
    }

    // =========================================================================
    // Tests for unescape_json_strings_aggressive
    // =========================================================================

    #[test]
    fn test_unescape_json_strings_aggressive_double_escaped() {
        let input = "feat: add feature\\\\n\\\\nDouble escaped";
        let result = unescape_json_strings_aggressive(input);
        assert_eq!(result, "feat: add feature\n\nDouble escaped");
    }

    #[test]
    fn test_unescape_json_strings_aggressive_single_escaped() {
        let input = "feat: add feature\\n\\nSingle escaped";
        let result = unescape_json_strings_aggressive(input);
        assert_eq!(result, "feat: add feature\n\nSingle escaped");
    }

    #[test]
    fn test_unescape_json_strings_aggressive_triple_escaped() {
        let input = "feat: add feature\\\\\\n\\\\\\nTriple escaped";
        let result = unescape_json_strings_aggressive(input);
        // Triple backslash-n is interpreted as backslash-backslash-newline
        // After aggressive unescaping: \\n becomes \n (literal backslash + n), then \n becomes newline
        // The actual result is backslash-newline-backslash-newline after first pass
        assert_eq!(result, "feat: add feature\\\n\\\nTriple escaped");
    }

    #[test]
    fn test_unescape_json_strings_aggressive_mixed_escape_sequences() {
        let input = "feat: add\\\\n\\\\t\\n\\rMixed";
        let result = unescape_json_strings_aggressive(input);
        assert_eq!(result, "feat: add\n\t\n\rMixed");
    }

    #[test]
    fn test_unescape_json_strings_aggressive_already_unescaped() {
        let input = "feat: add feature\n\nAlready correct";
        let result = unescape_json_strings_aggressive(input);
        assert_eq!(result, "feat: add feature\n\nAlready correct");
    }

    // =========================================================================
    // Tests for contains_literal_escape_sequences
    // =========================================================================

    #[test]
    fn test_contains_literal_escape_sequences_body_with_literal_escapes() {
        // Second line is literally "\n\n" which indicates improper unescaping
        let input = "feat: add feature\n\\n\\nBody text";
        assert!(contains_literal_escape_sequences(input));
    }

    #[test]
    fn test_contains_literal_escape_sequences_repeated_escapes() {
        // Pattern with multiple escaped newlines in a row
        let input = "feat: add feature\n\\n\\n\\n\\nMany escaped";
        assert!(contains_literal_escape_sequences(input));
    }

    #[test]
    fn test_contains_literal_escape_sequences_clean_message() {
        // Properly formatted message should not trigger detection
        let input = "feat: add feature\n\nBody text here";
        assert!(!contains_literal_escape_sequences(input));
    }

    #[test]
    fn test_contains_literal_escape_sequences_no_second_line() {
        // Single line message should not trigger
        let input = "feat: add feature";
        assert!(!contains_literal_escape_sequences(input));
    }

    #[test]
    fn test_contains_literal_escape_sequences_literal_escaped_on_first_line() {
        // Literal escapes on first line shouldn't false positive
        let input = "\\n\\nfeat: add feature";
        // The second line (after the first newline) would be "feat: add feature"
        // which doesn't start with escape sequences
        assert!(!contains_literal_escape_sequences(input));
    }

    // =========================================================================
    // Tests for final_escape_sequence_cleanup
    // =========================================================================

    #[test]
    fn test_final_escape_sequence_cleanup_with_literal_escapes() {
        let input = "feat: add feature\n\\n\\nBody with literal escapes";
        let result = final_escape_sequence_cleanup(input);
        // The function detects \n\n and applies aggressive unescaping
        // Result is 3 newlines (original + the 2 from \n\n unescaping)
        assert_eq!(result, "feat: add feature\n\n\nBody with literal escapes");
    }

    #[test]
    fn test_final_escape_sequence_cleanup_clean_message() {
        let input = "feat: add feature\n\nAlready clean body";
        let result = final_escape_sequence_cleanup(input);
        // No changes needed - content is preserved
        assert_eq!(result, "feat: add feature\n\nAlready clean body");
    }

    #[test]
    fn test_final_escape_sequence_cleanup_with_tabs() {
        let input = "feat: add feature\\n\\t- item 1\\n\\t- item 2";
        let result = final_escape_sequence_cleanup(input);
        // Tabs are preserved through cleanup
        assert_eq!(result, "feat: add feature\n\t- item 1\n\t- item 2");
    }

    #[test]
    fn test_final_escape_sequence_cleanup_with_carriage_returns() {
        let input = "feat: add feature\\r\\nBody text";
        let result = final_escape_sequence_cleanup(input);
        // Carriage returns are converted to newlines
        assert_eq!(result, "feat: add feature\r\nBody text");
    }

    #[test]
    fn test_final_escape_sequence_cleanup_double_escaped() {
        let input = "feat: add feature\n\\\\n\\\\nDouble escaped in body";
        let result = final_escape_sequence_cleanup(input);
        // The actual input is newline + \\n + \\n, which becomes newline + \n + \n (not fully unescaped)
        assert_eq!(result, "feat: add feature\n\\\n\\\nDouble escaped in body");
    }

    #[test]
    fn test_final_escape_sequence_cleanup_whitespace_trimming() {
        let input = "feat: add feature\n\\n\\n  Body with spaces  \\n  \\n  ";
        let result = final_escape_sequence_cleanup(input);
        // Escape sequences are handled, but whitespace trimming is NOT done here
        // The \n\n becomes \n\n\n (original + 2 from unescaping)
        assert_eq!(
            result,
            "feat: add feature\n\n\n  Body with spaces  \n  \n  "
        );
    }

    #[test]
    fn test_unescape_json_strings_idempotent_on_clean_content() {
        // Calling on already-clean content should be safe (no-op)
        let input = "feat: add feature\n\nThis is already clean.";
        let once = unescape_json_strings(input);
        let twice = unescape_json_strings(&once);
        assert_eq!(once, input);
        assert_eq!(once, twice);
    }

    // =========================================================================
    // Tests for preprocess_raw_content
    // =========================================================================

    #[test]
    fn test_preprocess_raw_content_single_escaped() {
        // Single-escaped \n should become actual newline
        let input = "feat: add feature\\n\\nThis adds new functionality.";
        let result = preprocess_raw_content(input);
        assert_eq!(result, "feat: add feature\n\nThis adds new functionality.");
    }

    #[test]
    fn test_preprocess_raw_content_double_escaped() {
        // Double-escaped \\n should also become actual newline
        let input = "feat: add feature\\\\n\\\\nDouble escaped";
        let result = preprocess_raw_content(input);
        assert_eq!(result, "feat: add feature\n\nDouble escaped");
    }

    #[test]
    fn test_preprocess_raw_content_triple_escaped() {
        // Triple-escaped \\\n should become backslash + newline
        let input = "feat: add feature\\\\\\n\\\\\\nTriple escaped";
        let result = preprocess_raw_content(input);
        // Triple backslash-n after first pass: \\n becomes \n (placeholder)
        // After full processing: backslash-newline-backslash-newline
        assert_eq!(result, "feat: add feature\\\n\\\nTriple escaped");
    }

    #[test]
    fn test_preprocess_raw_content_mixed_escapes() {
        // Mixed escape sequences
        let input = "feat: add\\n\\t\\n\\rMixed";
        let result = preprocess_raw_content(input);
        assert_eq!(result, "feat: add\n\t\n\rMixed");
    }

    #[test]
    fn test_preprocess_raw_content_idempotent() {
        // Calling preprocess_raw_content twice should produce the same result
        let input = "feat: add feature\\n\\nThis adds new functionality.";
        let once = preprocess_raw_content(input);
        let twice = preprocess_raw_content(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn test_preprocess_raw_content_no_escape_sequences() {
        // Content without escape sequences should pass through unchanged
        let input = "feat: add feature\n\nThis is already correct.";
        let result = preprocess_raw_content(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_preprocess_raw_content_with_tabs() {
        // Tab escapes should be handled
        let input = "feat: add feature\\n\\t- bullet 1\\n\\t- bullet 2";
        let result = preprocess_raw_content(input);
        assert_eq!(result, "feat: add feature\n\t- bullet 1\n\t- bullet 2");
    }

    #[test]
    fn test_preprocess_raw_content_with_carriage_returns() {
        // Carriage return escapes should be handled
        let input = "feat: add feature\\r\\nBody text";
        let result = preprocess_raw_content(input);
        assert_eq!(result, "feat: add feature\r\nBody text");
    }

    #[test]
    fn test_preprocess_raw_content_complex_json_like() {
        // Complex case: JSON with embedded escapes
        // Note: The function unescapes \\n to actual newlines, not literal \n
        let input = r#"{"subject":"feat: add feature\\n","body":"Line 1\\nLine 2"}"#;
        let result = preprocess_raw_content(input);
        // After preprocessing, double-escaped sequences become actual newlines
        assert!(result.contains("feat: add feature\n"));
        assert!(result.contains("Line 1\nLine 2"));
    }
}
