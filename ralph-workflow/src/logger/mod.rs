//! Logging and progress display utilities.
//!
//! This module provides structured logging for Ralph's pipeline:
//! - `Logger` struct for consistent, colorized output
//! - Progress bar display
//! - Section headers and formatting
//! - Colors & Formatting for terminal output
//! - Test utilities for capturing log output in tests
//!
//! # Example
//!
//! ```ignore
//! use ralph::logger::Logger;
//! use ralph::logger::Colors;
//!
//! let colors = Colors::new();
//! let logger = Logger::new(colors)
//!     .with_log_file(".agent/logs/pipeline.log");
//!
//! logger.info("Starting pipeline...");
//! logger.success("Task completed");
//! logger.warn("Potential issue detected");
//! logger.error("Critical failure");
//! ```
//!
//! # Testing with `TestLogger`
//!
//! For testing purposes, the `output` module provides `TestLogger` which implements
//! the same traits as `Logger` (`Printable` and `std::io::Write`) for output capture.
//!
//! ```ignore
//! use ralph::logger::output::TestLogger;
//! use std::io::Write;
//!
//! let logger = TestLogger::new();
//! writeln!(logger, "Test message").unwrap();
//!
//! assert!(logger.has_log("Test message"));
//! let logs = logger.get_logs();
//! assert_eq!(logs.len(), 1);
//! ```
//!
//! # Trait Implementation
//!
//! Both `Logger` and `TestLogger` implement:
//! - `Loggable` trait - provides unified interface for log output (info, success, warn, error)
//! - `Printable` trait from `json_parser::printer` - enables terminal detection
//! - `std::io::Write` trait - enables writing to the logger
//!
//! The `Loggable` trait mirrors the `Printable` trait pattern used for printers,
//! providing a consistent API for both production (`Logger`) and test (`TestLogger`) scenarios.

// The output module is pub so that integration tests can access TestLogger
// when the test-utils feature is enabled.
#[cfg(any(test, feature = "test-utils"))]
pub mod output;
#[cfg(not(any(test, feature = "test-utils")))]
mod output;

mod progress;

pub use output::{argv_requests_json, format_generic_json_for_display, Loggable, Logger};

// Note: TestLogger is available in tests through the output module.
// It's not re-exported here to avoid unused import warnings in the binary build.
// Integration tests should use `ralph_workflow::logger::output::TestLogger` directly.
pub use progress::print_progress;

// ===== Colors & Formatting =====

use std::io::IsTerminal;

/// Check if colors should be enabled
///
/// This respects standard environment variables for color control:
/// - `NO_COLOR=1`: Disables all ANSI output (<https://no-color.org/>)
/// - `CLICOLOR_FORCE=1`: Forces colors even in non-TTY
/// - `CLICOLOR=0`: Disables colors on macOS
/// - `TERM=dumb`: Disables colors for basic terminals
///
/// # Returns
///
/// `true` if colors should be used, `false` otherwise.
pub fn colors_enabled() -> bool {
    // Check NO_COLOR first - this is the strongest user preference
    // See <https://no-color.org/>
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    // Check CLICOLOR_FORCE - forces colors even in non-TTY
    // See <https://man.openbsd.org/man1/ls.1#CLICOLOR_FORCE>
    // Per the BSD specification, any non-empty value except "0" forces colors.
    // The empty string check handles the case where the variable is unset or
    // explicitly set to empty (both cases should be ignored).
    if let Ok(val) = std::env::var("CLICOLOR_FORCE") {
        if !val.is_empty() && val != "0" {
            return true;
        }
    }

    // Check CLICOLOR (macOS) - 0 means disable colors
    if let Ok(val) = std::env::var("CLICOLOR") {
        if val == "0" {
            return false;
        }
    }

    // Check TERM for dumb terminal
    if let Ok(term) = std::env::var("TERM") {
        if term.to_lowercase() == "dumb" {
            return false;
        }
    }

    // Default: color if TTY
    std::io::stdout().is_terminal()
}

/// ANSI color codes
#[derive(Clone, Copy)]
pub struct Colors {
    pub(crate) enabled: bool,
}

impl Colors {
    pub fn new() -> Self {
        Self {
            enabled: colors_enabled(),
        }
    }

    // Style codes
    pub const fn bold(self) -> &'static str {
        if self.enabled {
            "\x1b[1m"
        } else {
            ""
        }
    }

    pub const fn dim(self) -> &'static str {
        if self.enabled {
            "\x1b[2m"
        } else {
            ""
        }
    }

    pub const fn reset(self) -> &'static str {
        if self.enabled {
            "\x1b[0m"
        } else {
            ""
        }
    }

    // Foreground colors
    pub const fn red(self) -> &'static str {
        if self.enabled {
            "\x1b[31m"
        } else {
            ""
        }
    }

    pub const fn green(self) -> &'static str {
        if self.enabled {
            "\x1b[32m"
        } else {
            ""
        }
    }

    pub const fn yellow(self) -> &'static str {
        if self.enabled {
            "\x1b[33m"
        } else {
            ""
        }
    }

    pub const fn blue(self) -> &'static str {
        if self.enabled {
            "\x1b[34m"
        } else {
            ""
        }
    }

    pub const fn magenta(self) -> &'static str {
        if self.enabled {
            "\x1b[35m"
        } else {
            ""
        }
    }

    pub const fn cyan(self) -> &'static str {
        if self.enabled {
            "\x1b[36m"
        } else {
            ""
        }
    }

    pub const fn white(self) -> &'static str {
        if self.enabled {
            "\x1b[37m"
        } else {
            ""
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self::new()
    }
}

/// Box-drawing characters for visual structure
pub const BOX_TL: char = '╭';
pub const BOX_TR: char = '╮';
pub const BOX_BL: char = '╰';
pub const BOX_BR: char = '╯';
pub const BOX_H: char = '─';
pub const BOX_V: char = '│';

/// Icons for output
pub const ARROW: char = '→';
pub const CHECK: char = '✓';
pub const CROSS: char = '✗';
pub const WARN: char = '⚠';
pub const INFO: char = 'ℹ';

#[cfg(test)]
mod colors_tests {
    use super::*;

    #[test]
    fn test_colors_disabled() {
        let c = Colors { enabled: false };
        assert_eq!(c.bold(), "");
        assert_eq!(c.red(), "");
        assert_eq!(c.reset(), "");
    }

    #[test]
    fn test_colors_enabled() {
        let c = Colors { enabled: true };
        assert_eq!(c.bold(), "\x1b[1m");
        assert_eq!(c.red(), "\x1b[31m");
        assert_eq!(c.reset(), "\x1b[0m");
    }

    #[test]
    fn test_box_chars() {
        assert_eq!(BOX_TL, '╭');
        assert_eq!(BOX_TR, '╮');
        assert_eq!(BOX_H, '─');
    }

    #[test]
    fn test_colors_enabled_respects_no_color() {
        // Save original NO_COLOR value
        let original = std::env::var("NO_COLOR");

        // Set NO_COLOR=1
        std::env::set_var("NO_COLOR", "1");

        // Should return false regardless of TTY status
        assert!(!colors_enabled(), "NO_COLOR=1 should disable colors");

        // Restore original value
        match original {
            Ok(val) => std::env::set_var("NO_COLOR", val),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
    }

    #[test]
    fn test_colors_enabled_respects_clicolor_force() {
        // Save original values
        let original_no_color = std::env::var("NO_COLOR");
        let original_clicolor_force = std::env::var("CLICOLOR_FORCE");

        // Ensure NO_COLOR is not set
        std::env::remove_var("NO_COLOR");

        // Set CLICOLOR_FORCE=1
        std::env::set_var("CLICOLOR_FORCE", "1");

        // Should return true even if not a TTY
        assert!(colors_enabled(), "CLICOLOR_FORCE=1 should enable colors");

        // Restore original values
        match original_no_color {
            Ok(val) => std::env::set_var("NO_COLOR", val),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
        match original_clicolor_force {
            Ok(val) => std::env::set_var("CLICOLOR_FORCE", val),
            Err(_) => std::env::remove_var("CLICOLOR_FORCE"),
        }
    }

    #[test]
    fn test_colors_enabled_respects_clicolor_zero() {
        // Save original values
        let original_no_color = std::env::var("NO_COLOR");
        let original_clicolor = std::env::var("CLICOLOR");

        // Ensure NO_COLOR is not set
        std::env::remove_var("NO_COLOR");

        // Set CLICOLOR=0
        std::env::set_var("CLICOLOR", "0");

        // Should return false
        assert!(!colors_enabled(), "CLICOLOR=0 should disable colors");

        // Restore original values
        match original_no_color {
            Ok(val) => std::env::set_var("NO_COLOR", val),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
        match original_clicolor {
            Ok(val) => std::env::set_var("CLICOLOR", val),
            Err(_) => std::env::remove_var("CLICOLOR"),
        }
    }

    #[test]
    fn test_colors_enabled_respects_term_dumb() {
        // Save original values
        let original_no_color = std::env::var("NO_COLOR");
        let original_term = std::env::var("TERM");

        // Ensure NO_COLOR is not set
        std::env::remove_var("NO_COLOR");

        // Set TERM=dumb
        std::env::set_var("TERM", "dumb");

        // Should return false
        assert!(!colors_enabled(), "TERM=dumb should disable colors");

        // Restore original values
        match original_no_color {
            Ok(val) => std::env::set_var("NO_COLOR", val),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
        match original_term {
            Ok(val) => std::env::set_var("TERM", val),
            Err(_) => std::env::remove_var("TERM"),
        }
    }

    #[test]
    fn test_colors_enabled_no_color_takes_precedence() {
        // Save original values
        let original_no_color = std::env::var("NO_COLOR");
        let original_clicolor_force = std::env::var("CLICOLOR_FORCE");

        // Set both NO_COLOR=1 and CLICOLOR_FORCE=1
        std::env::set_var("NO_COLOR", "1");
        std::env::set_var("CLICOLOR_FORCE", "1");

        // NO_COLOR should take precedence
        assert!(
            !colors_enabled(),
            "NO_COLOR should take precedence over CLICOLOR_FORCE"
        );

        // Restore original values
        match original_no_color {
            Ok(val) => std::env::set_var("NO_COLOR", val),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
        match original_clicolor_force {
            Ok(val) => std::env::set_var("CLICOLOR_FORCE", val),
            Err(_) => std::env::remove_var("CLICOLOR_FORCE"),
        }
    }

    #[test]
    fn test_colors_enabled_term_dumb_case_insensitive() {
        // Save original values
        let original_no_color = std::env::var("NO_COLOR");
        let original_term = std::env::var("TERM");

        // Ensure NO_COLOR is not set
        std::env::remove_var("NO_COLOR");

        // Test various case combinations
        for term_value in ["dumb", "DUMB", "Dumb", "DuMb"] {
            std::env::set_var("TERM", term_value);
            assert!(
                !colors_enabled(),
                "TERM={term_value} should disable colors (case-insensitive)"
            );
        }

        // Restore original values
        match original_no_color {
            Ok(val) => std::env::set_var("NO_COLOR", val),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
        match original_term {
            Ok(val) => std::env::set_var("TERM", val),
            Err(_) => std::env::remove_var("TERM"),
        }
    }
}
