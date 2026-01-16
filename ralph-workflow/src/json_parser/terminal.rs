//! Terminal mode detection for streaming output.
//!
//! This module provides terminal capability detection to control whether
//! ANSI escape sequences (cursor positioning, colors) should be emitted.
//!
//! # Terminal Modes
//!
//! - **Full**: Full ANSI support including cursor positioning, colors
//! - **Basic**: Basic TTY with colors but no cursor positioning (e.g., `TERM=dumb`)
//! - **None**: Non-TTY output (pipes, redirects, CI environments)
//!
//! # Environment Variables
//!
//! The detection respects standard environment variables:
//! - `NO_COLOR=1`: Disables all ANSI output
//! - `TERM=dumb`: Enables Basic mode (colors without cursor positioning)
//! - `CLICOLOR=0`: Disables colors on macOS
//! - `CLICOLOR_FORCE=1`: Forces colors even in non-TTY

use std::io::IsTerminal;

/// Terminal capability mode for streaming output.
///
/// Determines what level of ANSI escape sequences are appropriate
/// for the current output destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMode {
    /// Full ANSI support: cursor positioning, colors, all escapes
    ///
    /// Used when stdout is a TTY with capable terminal (xterm, vt100, etc.)
    Full,
    /// Basic TTY: colors without cursor positioning
    ///
    /// Used when:
    /// - `TERM=dumb` (basic terminal with color support)
    /// - Terminal type is unknown but TTY is detected
    Basic,
    /// Non-TTY output: no ANSI sequences
    ///
    /// Used when:
    /// - Output is piped (`ralph | tee log.txt`)
    /// - Output is redirected (`ralph > output.txt`)
    /// - CI environment (no TTY detected)
    /// - `NO_COLOR=1` is set
    None,
}

impl TerminalMode {
    /// Get the terminal width in columns.
    ///
    /// This checks:
    /// 1. `COLUMNS` environment variable (common in shells)
    /// 2. Returns a reasonable default (80) if unable to detect
    ///
    /// # Returns
    ///
    /// Terminal width in columns, or 80 if unable to detect.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ralph::json_parser::TerminalMode;
    ///
    /// let width = TerminalMode::get_width();
    /// println!("Terminal width: {} columns", width);
    /// ```
    pub fn get_width() -> usize {
        // Check COLUMNS environment variable first (set by most shells)
        if let Ok(cols_str) = std::env::var("COLUMNS") {
            if let Ok(cols) = cols_str.parse::<usize>() {
                if cols > 0 {
                    return cols;
                }
            }
        }

        // Fallback to reasonable default
        80
    }

    /// Detect the current terminal mode from environment.
    ///
    /// This checks:
    /// 1. `NO_COLOR` environment variable (respects user preference)
    /// 2. `CLICOLOR_FORCE` (forces colors even in non-TTY)
    /// 3. `CLICOLOR` (macOS color disable)
    /// 4. `TERM` environment variable for capability detection
    /// 5. Whether stdout is a terminal using `IsTerminal` trait
    ///
    /// # Environment Variables
    ///
    /// - `NO_COLOR=1`: Disables all ANSI output
    /// - `NO_COLOR=0` or unset: No effect
    /// - `CLICOLOR_FORCE=1`: Forces colors even in non-TTY
    /// - `CLICOLOR_FORCE=0` or unset: No effect
    /// - `CLICOLOR=0`: Disables colors on macOS
    /// - `CLICOLOR=1` or unset: No effect on macOS
    /// - `TERM=xterm-256color`: Full ANSI support
    /// - `TERM=dumb`: Basic TTY with colors but no cursor positioning
    /// - `TERM=vt100`, `TERM=screen`: Full ANSI support
    ///
    /// # Returns
    ///
    /// - `Full`: stdout is TTY with capable terminal
    /// - `Basic`: stdout is TTY but terminal is basic or TERM is unknown
    /// - `None`: stdout is not a TTY or colors are disabled
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ralph::json_parser::TerminalMode;
    ///
    /// let mode = TerminalMode::detect();
    /// match mode {
    ///     TerminalMode::Full => println!("Full terminal support"),
    ///     TerminalMode::Basic => println!("Basic terminal (colors only)"),
    ///     TerminalMode::None => println!("Non-TTY output"),
    /// }
    /// ```
    pub fn detect() -> Self {
        // Check NO_COLOR first - this is the strongest user preference
        // See https://no-color.org/
        if std::env::var("NO_COLOR").is_ok() {
            return Self::None;
        }

        // Check CLICOLOR_FORCE - forces colors even in non-TTY
        // See https://man.openbsd.org/man1/ls.1#CLICOLOR_FORCE
        if let Ok(val) = std::env::var("CLICOLOR_FORCE") {
            if val != "0" {
                // Force is enabled - check if we're a TTY for cursor support
                return if std::io::stdout().is_terminal() {
                    Self::Full
                } else {
                    // Non-TTY but colors forced - use Basic (colors only, no cursor)
                    Self::Basic
                };
            }
        }

        // Check CLICOLOR (macOS) - 0 means disable colors
        if let Ok(val) = std::env::var("CLICOLOR") {
            if val == "0" {
                return Self::None;
            }
        }

        // Check if stdout is a terminal
        if !std::io::stdout().is_terminal() {
            return Self::None;
        }

        // We have a TTY - check TERM for capability detection
        match std::env::var("TERM") {
            Ok(term) => {
                // Normalize TERM variable for comparison
                let term_lower = term.to_lowercase();

                // Dumb terminal - basic color support but no cursor positioning
                if term_lower == "dumb" {
                    return Self::Basic;
                }

                // Check for known capable terminals
                // These support full ANSI including cursor positioning
                let capable_terminals = [
                    "xterm",
                    "xterm-",
                    "vt100",
                    "vt102",
                    "vt220",
                    "vt320",
                    "screen",
                    "tmux",
                    "ansi",
                    "rxvt",
                    "konsole",
                    "gnome-terminal",
                    "iterm",
                    "alacritty",
                    "kitty",
                    "wezterm",
                    "foot",
                ];

                for capable in &capable_terminals {
                    if term_lower.starts_with(capable) {
                        return Self::Full;
                    }
                }

                // Unknown but we're a TTY - conservatively use Basic mode
                // (colors without cursor positioning)
                Self::Basic
            }
            Err(_) => {
                // No TERM variable set but we're a TTY
                // Conservatively use Basic mode
                Self::Basic
            }
        }
    }
}

impl Default for TerminalMode {
    fn default() -> Self {
        Self::detect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_mode_default() {
        let mode = TerminalMode::default();
        // The default depends on the test environment, so we just verify
        // it returns a valid mode without panicking
        match mode {
            TerminalMode::Full | TerminalMode::Basic | TerminalMode::None => {
                // OK - valid mode
            }
        }
    }

    #[test]
    fn test_terminal_mode_detect_respects_no_color() {
        // Save original NO_COLOR value
        let original = std::env::var("NO_COLOR");

        // Set NO_COLOR=1
        std::env::set_var("NO_COLOR", "1");

        // Should return None regardless of TTY status
        let mode = TerminalMode::detect();
        assert_eq!(mode, TerminalMode::None);

        // Restore original value
        match original {
            Ok(val) => std::env::set_var("NO_COLOR", val),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
    }

    #[test]
    fn test_terminal_mode_partial_eq() {
        assert_eq!(TerminalMode::Full, TerminalMode::Full);
        assert_eq!(TerminalMode::Basic, TerminalMode::Basic);
        assert_eq!(TerminalMode::None, TerminalMode::None);

        assert_ne!(TerminalMode::Full, TerminalMode::Basic);
        assert_ne!(TerminalMode::Full, TerminalMode::None);
        assert_ne!(TerminalMode::Basic, TerminalMode::None);
    }

    #[test]
    fn test_terminal_mode_get_width_from_columns_env() {
        // Save original COLUMNS value
        let original = std::env::var("COLUMNS");

        // Set COLUMNS=120
        std::env::set_var("COLUMNS", "120");
        assert_eq!(TerminalMode::get_width(), 120);

        // Set COLUMNS=40
        std::env::set_var("COLUMNS", "40");
        assert_eq!(TerminalMode::get_width(), 40);

        // Restore original value (or remove if not set)
        match original {
            Ok(val) => std::env::set_var("COLUMNS", val),
            Err(_) => std::env::remove_var("COLUMNS"),
        }
    }

    #[test]
    fn test_terminal_mode_get_width_default_when_not_set() {
        // Save original COLUMNS value
        let original = std::env::var("COLUMNS");

        // Remove COLUMNS to ensure default is used
        std::env::remove_var("COLUMNS");
        assert_eq!(
            TerminalMode::get_width(),
            80,
            "Should default to 80 when COLUMNS not set"
        );

        // Restore original value (or remove if not set)
        if let Ok(val) = original {
            std::env::set_var("COLUMNS", val);
        }
    }

    #[test]
    fn test_terminal_mode_get_width_invalid_value() {
        // Save original COLUMNS value and ensure it's removed for this test
        let original = std::env::var("COLUMNS");
        std::env::remove_var("COLUMNS");

        // Set invalid COLUMNS value
        std::env::set_var("COLUMNS", "invalid");
        assert_eq!(
            TerminalMode::get_width(),
            80,
            "Should default to 80 for invalid value"
        );

        // Set COLUMNS=0 (should use default)
        std::env::set_var("COLUMNS", "0");
        assert_eq!(
            TerminalMode::get_width(),
            80,
            "Should default to 80 for zero value"
        );

        // Restore original value (or remove if not set)
        match original {
            Ok(val) => std::env::set_var("COLUMNS", val),
            Err(_) => std::env::remove_var("COLUMNS"),
        }
    }
}
