//! Colors & Formatting Module
//!
//! Provides ANSI escape codes for terminal coloring.
//! Respects NO_COLOR env var (https://no-color.org/).
//! Falls back to no colors if terminal doesn't support them.

use std::env;
use std::io::IsTerminal;

/// Check if colors should be enabled
pub(crate) fn colors_enabled() -> bool {
    env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal()
}

/// ANSI color codes
#[derive(Clone, Copy)]
pub(crate) struct Colors {
    pub(crate) enabled: bool,
}

impl Colors {
    pub(crate) fn new() -> Self {
        Self {
            enabled: colors_enabled(),
        }
    }

    // Style codes
    pub(crate) fn bold(&self) -> &'static str {
        if self.enabled {
            "\x1b[1m"
        } else {
            ""
        }
    }

    pub(crate) fn dim(&self) -> &'static str {
        if self.enabled {
            "\x1b[2m"
        } else {
            ""
        }
    }

    pub(crate) fn reset(&self) -> &'static str {
        if self.enabled {
            "\x1b[0m"
        } else {
            ""
        }
    }

    // Foreground colors
    pub(crate) fn red(&self) -> &'static str {
        if self.enabled {
            "\x1b[31m"
        } else {
            ""
        }
    }

    pub(crate) fn green(&self) -> &'static str {
        if self.enabled {
            "\x1b[32m"
        } else {
            ""
        }
    }

    pub(crate) fn yellow(&self) -> &'static str {
        if self.enabled {
            "\x1b[33m"
        } else {
            ""
        }
    }

    pub(crate) fn blue(&self) -> &'static str {
        if self.enabled {
            "\x1b[34m"
        } else {
            ""
        }
    }

    pub(crate) fn magenta(&self) -> &'static str {
        if self.enabled {
            "\x1b[35m"
        } else {
            ""
        }
    }

    pub(crate) fn cyan(&self) -> &'static str {
        if self.enabled {
            "\x1b[36m"
        } else {
            ""
        }
    }

    pub(crate) fn white(&self) -> &'static str {
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
pub(crate) const BOX_TL: char = '╭';
pub(crate) const BOX_TR: char = '╮';
pub(crate) const BOX_BL: char = '╰';
pub(crate) const BOX_BR: char = '╯';
pub(crate) const BOX_H: char = '─';
pub(crate) const BOX_V: char = '│';

/// Icons for output
pub(crate) const ARROW: char = '→';
pub(crate) const CHECK: char = '✓';
pub(crate) const CROSS: char = '✗';
pub(crate) const WARN: char = '⚠';
pub(crate) const INFO: char = 'ℹ';

#[cfg(test)]
mod tests {
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
