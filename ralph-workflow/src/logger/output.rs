//! Core Logger implementation.
//!
//! Provides structured, colorized logging output for Ralph's pipeline
//! with support for file logging and various log levels.

use crate::checkpoint::timestamp;
use crate::colors::{
    Colors, ARROW, BOX_BL, BOX_BR, BOX_H, BOX_TL, BOX_TR, BOX_V, CHECK, CROSS, INFO, WARN,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Logger for Ralph output.
///
/// Provides consistent, colorized output with optional file logging.
/// All messages include timestamps and appropriate icons.
pub struct Logger {
    colors: Colors,
    log_file: Option<String>,
}

impl Logger {
    /// Create a new Logger with the given colors configuration.
    pub const fn new(colors: Colors) -> Self {
        Self {
            colors,
            log_file: None,
        }
    }

    /// Configure the logger to also write to a file.
    ///
    /// Log messages written to the file will have ANSI codes stripped.
    pub fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    /// Write a message to the log file (if configured).
    fn log_to_file(&self, msg: &str) {
        if let Some(ref path) = self.log_file {
            // Strip ANSI codes for file logging
            let clean_msg = strip_ansi_codes(msg);
            if let Some(parent) = Path::new(path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{clean_msg}");
            }
        }
    }

    /// Log an informational message.
    pub fn info(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.blue(),
            INFO,
            c.reset(),
            msg
        );
        self.log_to_file(&format!("[{}] [INFO] {}", timestamp(), msg));
    }

    /// Log a success message.
    pub fn success(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.green(),
            CHECK,
            c.reset(),
            c.green(),
            msg,
            c.reset()
        );
        self.log_to_file(&format!("[{}] [OK] {}", timestamp(), msg));
    }

    /// Log a warning message.
    pub fn warn(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.yellow(),
            WARN,
            c.reset(),
            c.yellow(),
            msg,
            c.reset()
        );
        self.log_to_file(&format!("[{}] [WARN] {}", timestamp(), msg));
    }

    /// Log an error message.
    pub fn error(&self, msg: &str) {
        let c = &self.colors;
        eprintln!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.red(),
            CROSS,
            c.reset(),
            c.red(),
            msg,
            c.reset()
        );
        self.log_to_file(&format!("[{}] [ERROR] {}", timestamp(), msg));
    }

    /// Log a step/action message.
    pub fn step(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.magenta(),
            ARROW,
            c.reset(),
            msg
        );
        self.log_to_file(&format!("[{}] [STEP] {}", timestamp(), msg));
    }

    /// Print a section header with box drawing.
    ///
    /// # Arguments
    ///
    /// * `title` - The header title text
    /// * `color_fn` - Function that returns the color to use
    pub fn header(&self, title: &str, color_fn: fn(Colors) -> &'static str) {
        let c = self.colors;
        let color = color_fn(c);
        let width = 60;
        let title_len = title.chars().count();
        let padding = (width - title_len - 2) / 2;

        println!();
        println!(
            "{}{}{}{}{}{}",
            color,
            c.bold(),
            BOX_TL,
            BOX_H.to_string().repeat(width),
            BOX_TR,
            c.reset()
        );
        println!(
            "{}{}{}{}{}{}{}{}{}{}",
            color,
            c.bold(),
            BOX_V,
            " ".repeat(padding),
            c.white(),
            title,
            color,
            " ".repeat(width - padding - title_len),
            BOX_V,
            c.reset()
        );
        println!(
            "{}{}{}{}{}{}",
            color,
            c.bold(),
            BOX_BL,
            BOX_H.to_string().repeat(width),
            BOX_BR,
            c.reset()
        );
    }

    /// Print a sub-header (less prominent than header).
    pub fn subheader(&self, title: &str) {
        let c = &self.colors;
        println!();
        println!("{}{}{} {}{}", c.bold(), c.blue(), ARROW, title, c.reset());
        println!("{}{}──{}", c.dim(), "─".repeat(title.len()), c.reset());
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new(Colors::new())
    }
}

/// Strip ANSI escape sequences from a string.
///
/// Used when writing to log files where ANSI codes are not supported.
pub fn strip_ansi_codes(s: &str) -> String {
    static ANSI_RE: std::sync::LazyLock<Result<regex::Regex, regex::Error>> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;]*m"));
    (*ANSI_RE)
        .as_ref()
        .map_or_else(|_| s.to_string(), |re| re.replace_all(s, "").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[31mred\x1b[0m text";
        assert_eq!(strip_ansi_codes(input), "red text");
    }

    #[test]
    fn test_strip_ansi_codes_no_codes() {
        let input = "plain text";
        assert_eq!(strip_ansi_codes(input), "plain text");
    }

    #[test]
    fn test_strip_ansi_codes_multiple() {
        let input = "\x1b[1m\x1b[32mbold green\x1b[0m \x1b[34mblue\x1b[0m";
        assert_eq!(strip_ansi_codes(input), "bold green blue");
    }
}
