// Tests for file path extraction.

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
- [ ] [ralph-workflow/src/prompts/templates/fix_mode_xml.txt:10] Template issue
";
        let files = extract_file_paths_from_issues(content);
        assert_eq!(
            files,
            vec![
                "ralph-workflow/src/files/result_extraction/mod.rs",
                "ralph-workflow/src/prompts/templates/fix_mode_xml.txt"
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
