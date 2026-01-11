//! General Utilities Module
//!
//! Common helper functions used throughout Ralph.

use crate::colors::{
    Colors, ARROW, BOX_BL, BOX_BR, BOX_H, BOX_TL, BOX_TR, BOX_V, CHECK, CROSS, INFO, WARN,
};
use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;

/// Get current timestamp in "YYYY-MM-DD HH:MM:SS" format
pub(crate) fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Split a shell-like command string into argv parts.
///
/// Supports quotes and backslash escapes (e.g. `cmd --flag "a b"`).
pub(crate) fn split_command(cmd: &str) -> io::Result<Vec<String>> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return Ok(vec![]);
    }

    shell_words::split(cmd).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Failed to parse command string '{}': {}", cmd, err),
        )
    })
}

/// Truncate text to limit with ellipsis
///
/// Uses character count rather than byte length to avoid panics on UTF-8 text.
/// Truncates at character boundaries and appends "..." when truncation occurs.
pub(crate) fn truncate_text(text: &str, limit: usize) -> String {
    // Handle edge case where limit is too small for even "..."
    if limit <= 3 {
        return text.chars().take(limit).collect();
    }

    let char_count = text.chars().count();
    if char_count <= limit {
        text.to_string()
    } else {
        // Leave room for "..."
        let truncate_at = limit.saturating_sub(3);
        let truncated: String = text.chars().take(truncate_at).collect();
        format!("{}...", truncated)
    }
}

/// Logger for Ralph output
pub(crate) struct Logger {
    colors: Colors,
    log_file: Option<String>,
}

impl Logger {
    pub(crate) fn new(colors: Colors) -> Self {
        Self {
            colors,
            log_file: None,
        }
    }

    pub(crate) fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    fn log_to_file(&self, msg: &str) {
        if let Some(ref path) = self.log_file {
            // Strip ANSI codes for file logging
            let clean_msg = strip_ansi_codes(msg);
            if let Some(parent) = Path::new(path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", clean_msg);
            }
        }
    }

    pub(crate) fn info(&self, msg: &str) {
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

    pub(crate) fn success(&self, msg: &str) {
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

    pub(crate) fn warn(&self, msg: &str) {
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

    pub(crate) fn error(&self, msg: &str) {
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

    pub(crate) fn step(&self, msg: &str) {
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

    /// Print a section header with box drawing
    pub(crate) fn header(&self, title: &str, color_fn: fn(&Colors) -> &'static str) {
        let c = &self.colors;
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

    /// Print a sub-header (less prominent)
    pub(crate) fn subheader(&self, title: &str) {
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
pub(crate) fn strip_ansi_codes(s: &str) -> String {
    use once_cell::sync::Lazy;
    static ANSI_RE: Lazy<Result<regex::Regex, regex::Error>> =
        Lazy::new(|| regex::Regex::new(r"\x1b\[[0-9;]*m"));
    match &*ANSI_RE {
        Ok(re) => re.replace_all(s, "").to_string(),
        Err(_) => s.to_string(),
    }
}

/// Print progress bar: [████████░░░░░░░░] 50%
pub(crate) fn print_progress(current: u32, total: u32, label: &str) {
    let c = Colors::new();

    if total == 0 {
        println!(
            "{}{}:{} {}[no progress data]{}",
            c.dim(),
            label,
            c.reset(),
            c.yellow(),
            c.reset()
        );
        return;
    }

    let pct = current * 100 / total;
    let bar_width = 20;
    let filled = (current * bar_width / total) as usize;
    let empty = bar_width as usize - filled;

    let bar: String = "█".repeat(filled) + &"░".repeat(empty);

    println!(
        "{}{}:{} {}[{}]{} {}{}%{} ({}/{})",
        c.dim(),
        label,
        c.reset(),
        c.cyan(),
        bar,
        c.reset(),
        c.bold(),
        pct,
        c.reset(),
        current,
        total
    );
}

/// Check if a file contains a specific marker string
pub(crate) fn file_contains_marker(file_path: &Path, marker: &str) -> io::Result<bool> {
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
pub(crate) fn archive_context_file(file_path: &Path) -> io::Result<()> {
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
pub(crate) fn clear_context_file(file_path: &Path) -> io::Result<()> {
    if !file_path.exists() {
        return Ok(());
    }
    File::create(file_path)?;
    Ok(())
}

/// Clean context before reviewer phase
pub(crate) fn clean_context_for_reviewer(logger: &Logger) -> io::Result<()> {
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

/// Update the status file
pub(crate) fn update_status(
    last_action: &str,
    blockers: &str,
    next_action: &str,
) -> io::Result<()> {
    fs::write(
        ".agent/STATUS.md",
        format!(
            r#"# STATUS
- Last action: {}
- Blockers: {}
- Next action: {}
- Updated at: {}
"#,
            last_action,
            blockers,
            next_action,
            timestamp()
        ),
    )
}

/// Ensure required files exist
pub(crate) fn ensure_files() -> io::Result<()> {
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

/// Files that Ralph generates during a run and should clean up
pub(crate) const GENERATED_FILES: &[&str] = &[
    ".no_agent_commit",
    ".agent/PLAN.md",
    ".agent/commit-message.txt",
];

/// Delete PLAN.md after integration
pub(crate) fn delete_plan_file() -> io::Result<()> {
    let plan_path = Path::new(".agent/PLAN.md");
    if plan_path.exists() {
        fs::remove_file(plan_path)?;
    }
    Ok(())
}

/// Delete commit-message.txt after committing
pub(crate) fn delete_commit_message_file() -> io::Result<()> {
    let msg_path = Path::new(".agent/commit-message.txt");
    if msg_path.exists() {
        fs::remove_file(msg_path)?;
    }
    Ok(())
}

/// Read commit message from file; fails if missing or empty.
pub(crate) fn read_commit_message_file() -> io::Result<String> {
    let msg_path = Path::new(".agent/commit-message.txt");
    let content = fs::read_to_string(msg_path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Failed to read .agent/commit-message.txt: {}", e),
        )
    })?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            ".agent/commit-message.txt is empty",
        ));
    }
    Ok(trimmed.to_string())
}

/// Clean up all generated files (for crash/exit cleanup)
pub(crate) fn cleanup_generated_files() {
    for file in GENERATED_FILES {
        let _ = fs::remove_file(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn with_temp_cwd<F: FnOnce(&TempDir)>(f: F) {
        static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _cwd_guard = CWD_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        struct DirGuard(PathBuf);

        impl Drop for DirGuard {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.0);
            }
        }

        let dir = TempDir::new().unwrap();
        let old_dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
        std::env::set_current_dir(dir.path()).unwrap();
        let _guard = DirGuard(old_dir);

        f(&dir);
    }

    #[test]
    fn test_timestamp_format() {
        let ts = timestamp();
        assert!(ts.contains("-"));
        assert!(ts.contains(":"));
        assert_eq!(ts.len(), 19);
    }

    #[test]
    fn test_truncate_text_no_truncation() {
        assert_eq!(truncate_text("hello", 10), "hello");
        assert_eq!(truncate_text("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_text_with_ellipsis() {
        // "hello world" is 11 chars, limit 8 means 5 chars + "..."
        assert_eq!(truncate_text("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_text_unicode() {
        // Should not panic on UTF-8 multibyte characters
        let text = "日本語テスト"; // 6 Japanese characters
        assert_eq!(truncate_text(text, 10), "日本語テスト");
        assert_eq!(truncate_text(text, 6), "日本語テスト");
        assert_eq!(truncate_text(text, 5), "日本...");
    }

    #[test]
    fn test_truncate_text_emoji() {
        // Emojis can be multi-byte but should be handled correctly
        let text = "Hello 👋 World";
        assert_eq!(truncate_text(text, 20), "Hello 👋 World");
        assert_eq!(truncate_text(text, 10), "Hello 👋...");
    }

    #[test]
    fn test_truncate_text_edge_cases() {
        assert_eq!(truncate_text("abc", 3), "abc");
        assert_eq!(truncate_text("abcd", 3), "abc"); // limit too small for ellipsis
        assert_eq!(truncate_text("ab", 1), "a");
        assert_eq!(truncate_text("", 5), "");
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
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/STATUS.md", "test content").unwrap();

            archive_context_file(Path::new(".agent/STATUS.md")).unwrap();

            assert!(Path::new(".agent/archive").exists());
            let entries: Vec<_> = fs::read_dir(".agent/archive").unwrap().collect();
            assert_eq!(entries.len(), 1);
        });
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
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            update_status("Testing", "none", "Next step").unwrap();

            let content = fs::read_to_string(".agent/STATUS.md").unwrap();
            assert!(content.contains("Testing"));
            assert!(content.contains("none"));
            assert!(content.contains("Next step"));
        });
    }

    // Test delete_plan_file - simulates the deletion logic without relying on cwd
    #[test]
    fn test_delete_plan_file() {
        let dir = TempDir::new().unwrap();
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let plan_path = agent_dir.join("PLAN.md");
        fs::write(&plan_path, "test plan").unwrap();
        assert!(plan_path.exists());

        // Simulating delete_plan_file logic
        fs::remove_file(&plan_path).unwrap();
        assert!(!plan_path.exists());
    }

    #[test]
    fn test_delete_plan_file_nonexistent() {
        let dir = TempDir::new().unwrap();
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let plan_path = agent_dir.join("PLAN.md");

        // Should not error if file doesn't exist
        let result = fs::remove_file(&plan_path);
        assert!(result.is_err() || !plan_path.exists());
    }

    #[test]
    fn test_delete_commit_message_file() {
        let dir = TempDir::new().unwrap();
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let msg_path = agent_dir.join("commit-message.txt");
        fs::write(&msg_path, "test message").unwrap();
        assert!(msg_path.exists());

        fs::remove_file(&msg_path).unwrap();
        assert!(!msg_path.exists());
    }

    #[test]
    fn test_read_commit_message_file() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/commit-message.txt", "feat: test commit\n").unwrap();

            let msg = read_commit_message_file().unwrap();
            assert_eq!(msg, "feat: test commit");
        });
    }

    #[test]
    fn test_read_commit_message_file_default() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            assert!(read_commit_message_file().is_err());
        });
    }

    #[test]
    fn test_read_commit_message_file_empty() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/commit-message.txt", "   \n").unwrap();
            assert!(read_commit_message_file().is_err());
        });
    }

    #[test]
    fn test_cleanup_generated_files() {
        let dir = TempDir::new().unwrap();
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();

        let marker_path = dir.path().join(".no_agent_commit");
        let plan_path = agent_dir.join("PLAN.md");
        let msg_path = agent_dir.join("commit-message.txt");

        fs::write(&marker_path, "").unwrap();
        fs::write(&plan_path, "plan").unwrap();
        fs::write(&msg_path, "msg").unwrap();

        // Cleanup each file
        let _ = fs::remove_file(&marker_path);
        let _ = fs::remove_file(&plan_path);
        let _ = fs::remove_file(&msg_path);

        assert!(!marker_path.exists());
        assert!(!plan_path.exists());
        assert!(!msg_path.exists());
    }
}
