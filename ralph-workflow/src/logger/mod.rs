//! Logging and progress display utilities.
//!
//! This module provides structured logging for Ralph's pipeline:
//! - `Logger` struct for consistent, colorized output
//! - Progress bar display
//! - Section headers and formatting
//! - Colors & Formatting for terminal output
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

mod output;
mod progress;

pub use output::{argv_requests_json, format_generic_json_for_display, Logger};
pub use progress::print_progress;

// Re-export Colors at the module level

// ===== Colors & Formatting =====

use std::io::IsTerminal;

/// Check if colors should be enabled
pub fn colors_enabled() -> bool {
    std::env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal()
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
}
