//! General Utilities Module
//!
//! Common helper functions used throughout Ralph.

use crate::colors::{Colors, ARROW, CHECK, CROSS, INFO, WARN, BOX_H, BOX_TL, BOX_TR, BOX_BL, BOX_BR, BOX_V};
use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;

/// Get current timestamp in "YYYY-MM-DD HH:MM:SS" format
pub fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Truncate text to limit with ellipsis
pub fn truncate_text(text: &str, limit: usize) -> String {
    if text.len() > limit {
        format!("{}...", &text[..limit])
    } else {
        text.to_string()
    }
}

/// Logger for Ralph output
pub struct Logger {
    colors: Colors,
    log_file: Option<String>,
}

impl Logger {
    pub fn new(colors: Colors) -> Self {
        Self {
            colors,
            log_file: None,
        }
    }

    pub fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    fn log_to_file(&self, msg: &str) {
        if let Some(ref path) = self.log_file {
            // Strip ANSI codes for file logging
            let clean_msg = strip_ansi_codes(msg);
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", clean_msg);
            }
        }
    }

    pub fn info(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}",
            c.dim(), timestamp(), c.reset(),
            c.blue(), INFO, c.reset(),
            msg
        );
        self.log_to_file(&format!("[{}] [INFO] {}", timestamp(), msg));
    }

    pub fn success(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(), timestamp(), c.reset(),
            c.green(), CHECK, c.reset(),
            c.green(), msg, c.reset()
        );
        self.log_to_file(&format!("[{}] [OK] {}", timestamp(), msg));
    }

    pub fn warn(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(), timestamp(), c.reset(),
            c.yellow(), WARN, c.reset(),
            c.yellow(), msg, c.reset()
        );
        self.log_to_file(&format!("[{}] [WARN] {}", timestamp(), msg));
    }

    pub fn error(&self, msg: &str) {
        let c = &self.colors;
        eprintln!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(), timestamp(), c.reset(),
            c.red(), CROSS, c.reset(),
            c.red(), msg, c.reset()
        );
        self.log_to_file(&format!("[{}] [ERROR] {}", timestamp(), msg));
    }

    pub fn step(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}",
            c.dim(), timestamp(), c.reset(),
            c.magenta(), ARROW, c.reset(),
            msg
        );
        self.log_to_file(&format!("[{}] [STEP] {}", timestamp(), msg));
    }

    /// Print a section header with box drawing
    pub fn header(&self, title: &str, color_fn: fn(&Colors) -> &'static str) {
        let c = &self.colors;
        let color = color_fn(c);
        let width = 60;
        let title_len = title.chars().count();
        let padding = (width - title_len - 2) / 2;

        println!();
        println!(
            "{}{}{}{}{}{}",
            color, c.bold(), BOX_TL,
            BOX_H.to_string().repeat(width),
            BOX_TR, c.reset()
        );
        println!(
            "{}{}{}{}{}{}{}{}{}{}",
            color, c.bold(), BOX_V,
            " ".repeat(padding),
            c.white(), title, color,
            " ".repeat(width - padding - title_len),
            BOX_V, c.reset()
        );
        println!(
            "{}{}{}{}{}{}",
            color, c.bold(), BOX_BL,
            BOX_H.to_string().repeat(width),
            BOX_BR, c.reset()
        );
    }

    /// Print a sub-header (less prominent)
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

/// Strip ANSI escape sequences from a string
pub fn strip_ansi_codes(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

/// Print progress bar: [████████░░░░░░░░] 50%
pub fn print_progress(current: u32, total: u32, label: &str) {
    let c = Colors::new();

    if total == 0 {
        println!("{}{}:{} {}[no progress data]{}", c.dim(), label, c.reset(), c.yellow(), c.reset());
        return;
    }

    let pct = current * 100 / total;
    let bar_width = 20;
    let filled = (current * bar_width / total) as usize;
    let empty = bar_width as usize - filled;

    let bar: String = "█".repeat(filled) + &"░".repeat(empty);

    println!(
        "{}{}:{} {}[{}]{} {}{}%{} ({}/{})",
        c.dim(), label, c.reset(),
        c.cyan(), bar, c.reset(),
        c.bold(), pct, c.reset(),
        current, total
    );
}

/// Check if a file contains a specific marker string
pub fn file_contains_marker(file_path: &Path, marker: &str) -> io::Result<bool> {
    if !file_path.exists() {
        return Ok(false);
    }

    let file = File::open(file_path)?;
    let reader = io::BufReader::new(file);

    for line in reader.lines().map_while(Result::ok) {
        if line.contains(marker) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Archive a context file to .agent/archive/ with timestamp
pub fn archive_context_file(file_path: &Path) -> io::Result<()> {
    if !file_path.exists() {
        return Ok(());
    }

    let archive_dir = Path::new(".agent/archive");
    fs::create_dir_all(archive_dir)?;

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let basename = file_path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = file_path.extension().unwrap_or_default().to_string_lossy();

    let archive_path = archive_dir.join(format!("{}_{}.{}", basename, timestamp, ext));
    fs::copy(file_path, archive_path)?;

    Ok(())
}

/// Clear context file by truncating it
pub fn clear_context_file(file_path: &Path) -> io::Result<()> {
    if !file_path.exists() {
        return Ok(());
    }
    File::create(file_path)?;
    Ok(())
}

/// Clean context before reviewer phase
pub fn clean_context_for_reviewer(logger: &Logger) -> io::Result<()> {
    logger.info("Cleaning context for reviewer (fresh eyes)...");

    // Archive current context files
    archive_context_file(Path::new(".agent/STATUS.md"))?;
    archive_context_file(Path::new(".agent/NOTES.md"))?;
    archive_context_file(Path::new(".agent/ISSUES.md"))?;

    // Reset STATUS.md to minimal state
    fs::write(
        ".agent/STATUS.md",
        r#"# STATUS
- Last action: Code changes made
- Blockers: none
- Next action: Evaluate codebase against PROMPT.md goals
"#,
    )?;

    // Clear NOTES.md and ISSUES.md
    clear_context_file(Path::new(".agent/NOTES.md"))?;
    clear_context_file(Path::new(".agent/ISSUES.md"))?;

    logger.success("Context cleaned for reviewer");
    Ok(())
}

/// Reset context between iterations
pub fn reset_iteration_context(iteration: u32, next_action: &str) -> io::Result<()> {
    fs::write(
        ".agent/STATUS.md",
        format!(
            r#"# STATUS
- Last action: Starting iteration {}
- Blockers: none
- Next action: {}
- Updated at: {}
"#,
            iteration, next_action, timestamp()
        ),
    )
}

/// Update the status file
pub fn update_status(last_action: &str, blockers: &str, next_action: &str) -> io::Result<()> {
    fs::write(
        ".agent/STATUS.md",
        format!(
            r#"# STATUS
- Last action: {}
- Blockers: {}
- Next action: {}
- Updated at: {}
"#,
            last_action, blockers, next_action, timestamp()
        ),
    )
}

/// Ensure required files exist
pub fn ensure_files() -> io::Result<()> {
    fs::create_dir_all(".agent/logs")?;

    if !Path::new("PROMPT.md").exists() {
        fs::write(
            "PROMPT.md",
            r#"# PROMPT

## Goal
(Write what you want done)

## Acceptance checks
- (List tests/lint/behaviors that must pass)

## Notes / constraints
- (Optional)
"#,
        )?;
    }

    if !Path::new(".agent/STATUS.md").exists() {
        fs::write(
            ".agent/STATUS.md",
            r#"# STATUS
- Last action: none
- Blockers: none
- Next action: TBD
"#,
        )?;
    }

    if !Path::new(".agent/NOTES.md").exists() {
        File::create(".agent/NOTES.md")?;
    }

    if !Path::new(".agent/ISSUES.md").exists() {
        File::create(".agent/ISSUES.md")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_timestamp_format() {
        let ts = timestamp();
        assert!(ts.contains("-"));
        assert!(ts.contains(":"));
        assert_eq!(ts.len(), 19);
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("hello", 10), "hello");
        assert_eq!(truncate_text("hello world", 5), "hello...");
    }

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[31mred\x1b[0m text";
        assert_eq!(strip_ansi_codes(input), "red text");
    }

    #[test]
    fn test_file_contains_marker() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line1\nMARKER_TEST\nline3").unwrap();

        assert!(file_contains_marker(&file_path, "MARKER_TEST").unwrap());
        assert!(!file_contains_marker(&file_path, "NONEXISTENT").unwrap());
    }

    #[test]
    fn test_file_contains_marker_missing() {
        let result = file_contains_marker(Path::new("/nonexistent/file.txt"), "MARKER");
        assert!(!result.unwrap());
    }

    #[test]
    fn test_archive_context_file() {
        let dir = TempDir::new().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        fs::create_dir_all(".agent").unwrap();
        fs::write(".agent/STATUS.md", "test content").unwrap();

        archive_context_file(Path::new(".agent/STATUS.md")).unwrap();

        assert!(Path::new(".agent/archive").exists());
        let entries: Vec<_> = fs::read_dir(".agent/archive").unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_clear_context_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "some content").unwrap();

        clear_context_file(&file_path).unwrap();

        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "");
    }

    // NOTE: Tests involving set_current_dir can conflict when run in parallel.
    // These tests use a serial test approach or avoid set_current_dir.

    #[test]
    fn test_archive_and_clear_operations() {
        // Test archive and clear without changing directories
        let dir = TempDir::new().unwrap();
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();

        let status_path = agent_dir.join("STATUS.md");
        fs::write(&status_path, "Developer status here").unwrap();

        // Archive the file
        let archive_dir = agent_dir.join("archive");
        fs::create_dir_all(&archive_dir).unwrap();
        let archive_path = archive_dir.join("STATUS_archived.md");
        fs::copy(&status_path, &archive_path).unwrap();

        assert!(archive_path.exists());

        // Clear the file
        clear_context_file(&status_path).unwrap();
        assert!(status_path.exists());
        assert_eq!(fs::read_to_string(&status_path).unwrap(), "");
    }

    // Helper to generate progress bar string for testing
    fn generate_progress_bar(current: u32, total: u32) -> (u32, String) {
        if total == 0 {
            return (0, String::new());
        }
        let pct = current * 100 / total;
        let bar_width = 20;
        let filled = (current * bar_width / total) as usize;
        let empty = bar_width as usize - filled;
        let bar: String = "█".repeat(filled) + &"░".repeat(empty);
        (pct, bar)
    }

    #[test]
    fn test_progress_bar_50_percent() {
        let (pct, bar) = generate_progress_bar(5, 10);
        assert_eq!(pct, 50);
        assert_eq!(bar, "██████████░░░░░░░░░░");
    }

    #[test]
    fn test_progress_bar_100_percent() {
        let (pct, bar) = generate_progress_bar(10, 10);
        assert_eq!(pct, 100);
        assert_eq!(bar, "████████████████████");
    }

    #[test]
    fn test_progress_bar_0_percent() {
        let (pct, bar) = generate_progress_bar(0, 10);
        assert_eq!(pct, 0);
        assert_eq!(bar, "░░░░░░░░░░░░░░░░░░░░");
    }

    #[test]
    fn test_progress_bar_zero_total() {
        let (pct, bar) = generate_progress_bar(0, 0);
        assert_eq!(pct, 0);
        assert_eq!(bar, "");
    }

    #[test]
    fn test_update_status() {
        let dir = TempDir::new().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::create_dir_all(".agent").unwrap();

        update_status("Testing", "none", "Next step").unwrap();

        let content = fs::read_to_string(".agent/STATUS.md").unwrap();
        assert!(content.contains("Testing"));
        assert!(content.contains("none"));
        assert!(content.contains("Next step"));
    }

    #[test]
    fn test_reset_iteration_context() {
        let dir = TempDir::new().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        fs::create_dir_all(".agent").unwrap();

        reset_iteration_context(3, "Continue working").unwrap();

        let content = fs::read_to_string(".agent/STATUS.md").unwrap();
        assert!(content.contains("iteration 3"));
        assert!(content.contains("Continue working"));
    }
}
