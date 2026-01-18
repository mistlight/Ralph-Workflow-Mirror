//! File path extraction from ISSUES content.
//!
//! This module provides utilities to extract file paths from ISSUES markdown content.
//! The fix agent uses this to identify which files it may modify without needing
//! to explore the repository.

use std::collections::BTreeSet;

/// Extract file paths from ISSUES markdown content.
///
/// This function parses common issue citation formats to find file references:
/// - Bracketed with line numbers: `[src/main.rs:42]`
/// - Bracketed without line numbers: `[src/lib.rs]`
/// - Parenthesized: `(src/utils.rs)`
/// - Bare colon format: `src/helpers.rs:123`
/// - Backtick format: `` `src/file.rs:42` ``
///
/// File paths are deduplicated and sorted alphabetically for consistency.
///
/// # Arguments
///
/// * `content` - The ISSUES markdown content to parse
///
/// # Returns
///
/// A sorted vector of unique file paths found in the content.
///
/// # Examples
///
/// ```
/// use ralph_workflow::files::result_extraction::extract_file_paths_from_issues;
///
/// let issues = r#"
/// # Issues
///
/// Critical:
/// - [ ] [src/main.rs:42] Bug in main function
/// - [ ] High: [src/lib.rs:10] Style issue
///
/// Medium:
/// - [ ] (src/utils.rs) Missing documentation
/// "#;
///
/// let files = extract_file_paths_from_issues(issues);
/// assert_eq!(files, vec!["src/lib.rs", "src/main.rs", "src/utils.rs"]);
/// ```
pub fn extract_file_paths_from_issues(content: &str) -> Vec<String> {
    let mut files = BTreeSet::new();

    // Pattern 1: Bracketed format with optional line numbers
    // Matches: [src/main.rs:42], [src/lib.rs], [path/to/file.rs:100]
    // Use non-greedy match to avoid partial matches
    let bracket_pattern = regex::Regex::new(r"\[([^\]]+?\.[a-z]+(?::\d+)?)\]").unwrap();

    // Pattern 2: Parenthesized format
    // Matches: (src/main.rs), (path/to/file.rs)
    // Use non-greedy match to avoid partial matches
    let paren_pattern = regex::Regex::new(r"\(([^\)]+?\.[a-z]+)\)").unwrap();

    // Pattern 3: Backtick format (used by some agents like Codex)
    // Matches: `src/main.rs:42`, `path/to/file.rs`
    // Use non-greedy match to avoid partial matches
    let backtick_pattern = regex::Regex::new(r"`([^`]+?\.[a-z]+(?::\d+)?)`").unwrap();

    // Pattern 4: Bare colon format (file.rs:line)
    // Matches: src/main.rs:42, lib.rs:123 (but not URLs or similar)
    // Use word boundaries and non-greedy matching
    let bare_pattern = regex::Regex::new(r"\b([\w/-]+?\.[a-z]+:\d+)\b").unwrap();

    // Extract from bracketed format
    for caps in bracket_pattern.captures_iter(content) {
        if let Some(file_ref) = caps.get(1) {
            let path = file_ref.as_str().trim();
            // Remove line number if present
            let file_path = path.split(':').next().unwrap_or(path);
            if looks_like_file_path(file_path) {
                files.insert(file_path.to_string());
            }
        }
    }

    // Extract from parenthesized format
    for caps in paren_pattern.captures_iter(content) {
        if let Some(file_ref) = caps.get(1) {
            let path = file_ref.as_str().trim();
            if looks_like_file_path(path) {
                files.insert(path.to_string());
            }
        }
    }

    // Extract from backtick format
    for caps in backtick_pattern.captures_iter(content) {
        if let Some(file_ref) = caps.get(1) {
            let path = file_ref.as_str().trim();
            // Remove line number if present
            let file_path = path.split(':').next().unwrap_or(path);
            if looks_like_file_path(file_path) {
                files.insert(file_path.to_string());
            }
        }
    }

    // Extract from bare colon format
    for caps in bare_pattern.captures_iter(content) {
        if let Some(file_ref) = caps.get(1) {
            let path = file_ref.as_str().trim();
            // Remove line number
            let file_path = path.split(':').next().unwrap_or(path);
            // Avoid duplicates of already-found bracketed/parenthesized files
            if looks_like_file_path(file_path) && !files.contains(file_path) {
                files.insert(file_path.to_string());
            }
        }
    }

    files.into_iter().collect()
}

/// Check if a string looks like a source file path.
///
/// This is a conservative check to avoid false positives from URLs,
/// issue numbers, or other colon-separated values.
///
/// # Arguments
///
/// * `s` - The string to check
///
/// # Returns
///
/// `true` if the string appears to be a file path, `false` otherwise.
fn looks_like_file_path(s: &str) -> bool {
    // Must have a file extension
    if !s.contains('.') {
        return false;
    }

    // Avoid common non-file patterns
    // Check for things that look like URLs or domains
    if s.contains("://") || s.contains("www.") || s.starts_with("http") {
        return false;
    }

    // Avoid short patterns that are likely not file paths
    if s.len() < 4 {
        return false;
    }

    // Must have a recognized file extension (common source file types)
    let extensions = [
        "rs", "toml", "md", "txt", "json", "yaml", "yml", "xml", "html", "css", "js", "ts", "py",
        "go", "java", "c", "cpp", "h", "hpp", "cs", "rb", "php", "sh", "bash", "zsh",
    ];
    let has_known_extension = s.split('.').any(|ext| {
        // Remove any line number suffix from the extension check
        let ext = ext.split(':').next().unwrap_or(ext);
        extensions.contains(&ext)
    });

    has_known_extension
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bracketed_format_with_line_numbers() {
        let content = r"
# Issues
- [ ] [src/main.rs:42] Bug in main function
- [ ] [src/lib.rs:10] Style issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_extract_bracketed_format_without_line_numbers() {
        let content = r"
# Issues
- [ ] [src/main.rs] Bug in main function
- [ ] [src/lib.rs] Style issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_extract_parenthesized_format() {
        let content = r"
# Issues
- [ ] (src/utils.rs) Missing documentation
- [ ] (src/helpers.rs) Unused import
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/helpers.rs", "src/utils.rs"]);
    }

    #[test]
    fn test_extract_bare_colon_format() {
        let content = r"
# Issues
- [ ] src/main.rs:42 Bug in main function
- [ ] Fix lib.rs:10 style issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_extract_mixed_formats() {
        let content = r"
# Issues

Critical:
- [ ] [src/main.rs:42] Critical bug
- [ ] (src/lib.rs) Style issue
- [ ] src/utils.rs:100 Missing docs

Medium:
- [ ] [src/helpers.rs:5] Unused import
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec![
                "src/helpers.rs",
                "src/lib.rs",
                "src/main.rs",
                "src/utils.rs"
            ]
        );
    }

    #[test]
    fn test_deduplicates_and_sorts() {
        let content = r"
# Issues
- [ ] [src/main.rs:42] First issue
- [ ] [src/main.rs:100] Second issue (same file)
- [ ] (src/lib.rs) Third issue
- [ ] src/lib.rs:50 Fourth issue (same file)
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_empty_content() {
        let content = "";
        let files = extract_file_paths_from_issues(content);
        assert!(files.is_empty());
    }

    #[test]
    fn test_no_file_references() {
        let content = r"
# Issues
- [ ] Fix the build system
- [ ] Update documentation
";
        let files = extract_file_paths_from_issues(content);
        assert!(files.is_empty());
    }

    #[test]
    fn test_avoids_urls_and_domains() {
        let content = r"
# Issues
- [ ] [src/main.rs:42] Bug in main
- [ ] See https://example.com/docs for reference
- [ ] Check www.example.org for details
- [ ] [src/lib.rs:10] Style issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_handles_nested_paths() {
        let content = r"
# Issues
- [ ] [ralph-workflow/src/files/result_extraction/mod.rs:42] Missing export
- [ ] [ralph-workflow/src/prompts/templates/fix_mode.txt:10] Template issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec![
                "ralph-workflow/src/files/result_extraction/mod.rs",
                "ralph-workflow/src/prompts/templates/fix_mode.txt"
            ]
        );
    }

    #[test]
    fn test_various_file_extensions() {
        let content = r"
# Issues
- [ ] [src/main.rs:42] Rust issue
- [ ] [config.toml:5] Config issue
- [ ] [README.md:10] Doc issue
- [ ] [data.json:1] Data issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec!["README.md", "config.toml", "data.json", "src/main.rs"]
        );
    }

    #[test]
    fn test_looks_like_file_path_valid() {
        assert!(looks_like_file_path("src/main.rs"));
        assert!(looks_like_file_path("lib.rs"));
        assert!(looks_like_file_path("Cargo.toml"));
        assert!(looks_like_file_path("README.md"));
        assert!(looks_like_file_path("src/utils/helper.rs"));
    }

    #[test]
    fn test_looks_like_file_path_invalid() {
        assert!(!looks_like_file_path("https://example.com"));
        assert!(!looks_like_file_path("www.example.com"));
        assert!(!looks_like_file_path("http://test.org"));
        assert!(!looks_like_file_path("abc"));
        assert!(!looks_like_file_path("noextension"));
    }

    #[test]
    fn test_handles_files_with_numbers() {
        let content = r"
# Issues
- [ ] [src/main2.rs:42] Numbered file
- [ ] [test/v1.2.3/test.rs:10] Versioned path
- [ ] [file123.go:5] Numbered extension
";
        let files = extract_file_paths_from_issues(content);
        assert!(files.contains(&"src/main2.rs".to_string()));
        assert!(files.contains(&"test/v1.2.3/test.rs".to_string()));
        assert!(files.contains(&"file123.go".to_string()));
    }

    #[test]
    fn test_handles_paths_with_dots() {
        let content = r"
# Issues
- [ ] [src/lib.v1.rs:42] Dotted file
- [ ] [build/script.min.js:10] Minified file
";
        let files = extract_file_paths_from_issues(content);
        assert!(files.contains(&"src/lib.v1.rs".to_string()));
        assert!(files.contains(&"build/script.min.js".to_string()));
    }

    #[test]
    fn test_handles_special_characters_in_paths() {
        let content = r"
# Issues
- [ ] [src/utils_helper.rs:42] Underscored file
- [ ] [src/my-file.rs:10] Dashed file
- [ ] [src/my.file.rs:5] Multiple dots
";
        let files = extract_file_paths_from_issues(content);
        assert!(files.contains(&"src/utils_helper.rs".to_string()));
        assert!(files.contains(&"src/my-file.rs".to_string()));
        assert!(files.contains(&"src/my.file.rs".to_string()));
    }

    #[test]
    fn test_ignores_markdown_links_with_colons() {
        let content = r"
# Issues
- [ ] [src/main.rs:42] Bug in main
- [ ] See [this link](https://example.com:8080/path) for details
- [ ] Another [link](http://localhost:3000) reference
- [ ] [src/lib.rs:10] Style issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_handles_windows_style_paths() {
        let content = r"
# Issues
- [ ] [src\\main.rs:42] Windows style path
- [ ] [src/lib.rs:10] Unix style path
";
        let files = extract_file_paths_from_issues(content);
        // Should handle both styles (though we normalize to forward slashes)
        assert!(files.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_extract_backtick_format_with_line_numbers() {
        let content = r"
# Issues
- [ ] Critical: `src/main.rs:42` Bug in main function
- [ ] High: `src/lib.rs:10` Style issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_extract_backtick_format_without_line_numbers() {
        let content = r"
# Issues
- [ ] `src/main.rs` Bug in main function
- [ ] `src/lib.rs` Style issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_extract_backtick_format_nested_paths() {
        let content = r"
# Issues
- [ ] Critical: `ralph-workflow/src/app/config_init.rs:11` Duplicate entries
- [ ] High: `ralph-workflow/src/cli/args.rs:90` Duplicate field name
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec![
                "ralph-workflow/src/app/config_init.rs",
                "ralph-workflow/src/cli/args.rs"
            ]
        );
    }

    #[test]
    fn test_extract_backtick_format_mixed_with_other_formats() {
        let content = r"
# Issues

Critical:
- [ ] [src/main.rs:42] Critical bug (bracket format)
- [ ] (src/lib.rs) Style issue (paren format)
- [ ] `src/utils.rs:100` Missing docs (backtick format)

Medium:
- [ ] [src/helpers.rs:5] Unused import
- [ ] `src/another.rs:10` Another issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec![
                "src/another.rs",
                "src/helpers.rs",
                "src/lib.rs",
                "src/main.rs",
                "src/utils.rs"
            ]
        );
    }

    #[test]
    fn test_extract_backtick_format_deduplicates() {
        let content = r"
# Issues
- [ ] `src/main.rs:42` First issue
- [ ] `src/main.rs:100` Second issue (same file)
- [ ] [src/lib.rs] Third issue (different format)
- [ ] `src/lib.rs:50` Fourth issue (same file)
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_extract_backtick_format_with_various_extensions() {
        let content = r"
# Issues
- [ ] `src/main.rs:42` Rust issue
- [ ] `config.toml:5` Config issue
- [ ] `README.md:10` Doc issue
- [ ] `data.json:1` Data issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec!["README.md", "config.toml", "data.json", "src/main.rs"]
        );
    }

    #[test]
    fn test_extract_codex_style_output() {
        // This matches the actual format Codex CLI uses
        let content = r"
[OpenAI Codex CLI] - [ ] Critical: `ralph-workflow/src/app/config_init.rs:11` Duplicate `use crate::cli::{...}` entries (e.g., `apply_args_to_config`, `handle_init_global`, etc. appear twice) will fail to compile (`E0252` duplicate imports).
- [ ] Critical: `ralph-workflow/src/cli/args.rs:90` `UnifiedInitFlags` defines `pub init: Option<String>` and later also defines `pub init: bool` (duplicate field name) → will not compile.
- [ ] Critical: `ralph-workflow/src/cli/args.rs:101` Multiple `#[arg(...)]` attributes contain duplicate keys (`conflicts_with_all`, `help`, `long`) in the same attribute list (e.g., `help = ...` twice) → clap derive macro will fail to compile.
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec![
                "ralph-workflow/src/app/config_init.rs",
                "ralph-workflow/src/cli/args.rs"
            ]
        );
    }

    #[test]
    fn test_backtick_format_ignores_non_file_paths() {
        let content = r"
# Issues
- [ ] `src/main.rs:42` Real file
- [ ] `not_a_file` Not a file (no extension)
- [ ] `https://example.com` URL with backticks
- [ ] `src/lib.rs:10` Another real file
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }
}
