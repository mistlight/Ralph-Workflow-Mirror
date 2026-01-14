//! Output formatting and display utilities.
//!
//! This module contains functions for formatting and displaying output:
//! - JSON output formatting for agent responses
//! - Command-line argument parsing for JSON detection
//! - Banner and summary display helpers

use crate::config::Verbosity;
use crate::utils::truncate_text;

/// Detect if command-line arguments request JSON output.
///
/// Scans the provided argv for common JSON output flags used by various CLIs:
/// - `--json` or `--json=...`
/// - `--output-format` with json value
/// - `--format json`
/// - `-F json`
/// - `-o stream-json` or similar
pub fn argv_requests_json(argv: &[String]) -> bool {
    // Skip argv[0] (the executable); scan flags/args only.
    let mut iter = argv.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        if arg == "--json" || arg.starts_with("--json=") {
            return true;
        }

        if arg == "--output-format" {
            if let Some(next) = iter.peek() {
                let next = next.as_str();
                if next.contains("json") {
                    return true;
                }
            }
        }
        if let Some((flag, value)) = arg.split_once('=') {
            if flag == "--output-format" && value.contains("json") {
                return true;
            }
            if flag == "--format" && value == "json" {
                return true;
            }
        }

        if arg == "--format" {
            if let Some(next) = iter.peek() {
                if next.as_str() == "json" {
                    return true;
                }
            }
        }

        // Some CLIs use short flags like -F json or -o stream-json
        if arg == "-F" {
            if let Some(next) = iter.peek() {
                if next.as_str() == "json" {
                    return true;
                }
            }
        }
        if arg.starts_with("-F") && arg != "-F" && arg.trim_start_matches("-F") == "json" {
            return true;
        }

        if arg == "-o" {
            if let Some(next) = iter.peek() {
                let next = next.as_str();
                if next.contains("json") {
                    return true;
                }
            }
        }
        if arg.starts_with("-o") && arg != "-o" && arg.trim_start_matches("-o").contains("json") {
            return true;
        }
    }
    false
}

/// Format generic JSON output for display.
///
/// Parses the input as JSON and formats it according to verbosity level:
/// - `Full` or `Debug`: Pretty-print with indentation
/// - Other levels: Compact single-line format
///
/// Output is truncated according to verbosity limits.
pub fn format_generic_json_for_display(line: &str, verbosity: Verbosity) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return truncate_text(line, verbosity.truncate_limit("agent_msg"));
    };

    let formatted = match verbosity {
        Verbosity::Full | Verbosity::Debug => {
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| line.to_string())
        }
        _ => serde_json::to_string(&value).unwrap_or_else(|_| line.to_string()),
    };
    truncate_text(&formatted, verbosity.truncate_limit("agent_msg"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::split_command;

    #[test]
    fn argv_requests_json_detects_common_flags() {
        assert!(argv_requests_json(&split_command("tool --json").unwrap()));
        assert!(argv_requests_json(
            &split_command("tool --output-format=stream-json").unwrap()
        ));
        assert!(argv_requests_json(
            &split_command("tool --output-format stream-json").unwrap()
        ));
        assert!(argv_requests_json(
            &split_command("tool --format json").unwrap()
        ));
        assert!(argv_requests_json(&split_command("tool -F json").unwrap()));
        assert!(argv_requests_json(
            &split_command("tool -o stream-json").unwrap()
        ));
    }

    #[test]
    fn format_generic_json_for_display_pretty_prints_when_full() {
        let line = r#"{"type":"message","content":{"text":"hello"}}"#;
        let formatted = format_generic_json_for_display(line, Verbosity::Full);
        assert!(formatted.contains('\n'));
        assert!(formatted.contains("\"type\""));
        assert!(formatted.contains("\"message\""));
    }
}
