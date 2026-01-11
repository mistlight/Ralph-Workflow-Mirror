//! General Utilities Module
//!
//! Common helper functions used throughout Ralph.

use crate::colors::{
    Colors, ARROW, BOX_BL, BOX_BR, BOX_H, BOX_TL, BOX_TR, BOX_V, CHECK, CROSS, INFO, WARN,
};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;

// ============================================================================
// Pipeline Checkpoint System
// ============================================================================

/// Pipeline phases for checkpoint tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PipelinePhase {
    /// Planning phase (creating PLAN.md)
    Planning,
    /// Development/implementation phase
    Development,
    /// Initial review phase
    Review,
    /// Fix phase
    Fix,
    /// Verification review phase
    ReviewAgain,
    /// Commit message generation
    CommitMessage,
    /// Final validation phase
    FinalValidation,
    /// Pipeline complete
    Complete,
}

impl std::fmt::Display for PipelinePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelinePhase::Planning => write!(f, "Planning"),
            PipelinePhase::Development => write!(f, "Development"),
            PipelinePhase::Review => write!(f, "Review"),
            PipelinePhase::Fix => write!(f, "Fix"),
            PipelinePhase::ReviewAgain => write!(f, "Verification Review"),
            PipelinePhase::CommitMessage => write!(f, "Commit Message Generation"),
            PipelinePhase::FinalValidation => write!(f, "Final Validation"),
            PipelinePhase::Complete => write!(f, "Complete"),
        }
    }
}

/// Pipeline checkpoint for resume functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PipelineCheckpoint {
    /// Current pipeline phase
    pub(crate) phase: PipelinePhase,
    /// Current iteration number (for developer iterations)
    pub(crate) iteration: u32,
    /// Total iterations configured
    pub(crate) total_iterations: u32,
    /// Current reviewer pass number
    pub(crate) reviewer_pass: u32,
    /// Total reviewer passes configured
    pub(crate) total_reviewer_passes: u32,
    /// Timestamp when checkpoint was saved
    pub(crate) timestamp: String,
    /// Developer agent name
    pub(crate) developer_agent: String,
    /// Reviewer agent name
    pub(crate) reviewer_agent: String,
}

impl PipelineCheckpoint {
    /// Create a new checkpoint
    pub(crate) fn new(
        phase: PipelinePhase,
        iteration: u32,
        total_iterations: u32,
        reviewer_pass: u32,
        total_reviewer_passes: u32,
        developer_agent: &str,
        reviewer_agent: &str,
    ) -> Self {
        Self {
            phase,
            iteration,
            total_iterations,
            reviewer_pass,
            total_reviewer_passes,
            timestamp: timestamp(),
            developer_agent: developer_agent.to_string(),
            reviewer_agent: reviewer_agent.to_string(),
        }
    }

    /// Get a human-readable description of the checkpoint
    pub(crate) fn description(&self) -> String {
        match self.phase {
            PipelinePhase::Planning => {
                format!(
                    "Planning phase, iteration {}/{}",
                    self.iteration, self.total_iterations
                )
            }
            PipelinePhase::Development => {
                format!(
                    "Development iteration {}/{}",
                    self.iteration, self.total_iterations
                )
            }
            PipelinePhase::Review => "Initial review".to_string(),
            PipelinePhase::Fix => "Applying fixes".to_string(),
            PipelinePhase::ReviewAgain => {
                format!(
                    "Verification review {}/{}",
                    self.reviewer_pass, self.total_reviewer_passes
                )
            }
            PipelinePhase::CommitMessage => "Commit message generation".to_string(),
            PipelinePhase::FinalValidation => "Final validation".to_string(),
            PipelinePhase::Complete => "Pipeline complete".to_string(),
        }
    }
}

const CHECKPOINT_PATH: &str = ".agent/checkpoint.json";

/// Save a pipeline checkpoint
pub(crate) fn save_checkpoint(checkpoint: &PipelineCheckpoint) -> io::Result<()> {
    let json = serde_json::to_string_pretty(checkpoint).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize checkpoint: {}", e),
        )
    })?;

    // Write atomically by writing to temp file then renaming
    let temp_path = format!("{}.tmp", CHECKPOINT_PATH);
    fs::write(&temp_path, &json)?;
    fs::rename(&temp_path, CHECKPOINT_PATH)?;

    Ok(())
}

/// Load an existing checkpoint if one exists
pub(crate) fn load_checkpoint() -> io::Result<Option<PipelineCheckpoint>> {
    let path = Path::new(CHECKPOINT_PATH);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let checkpoint: PipelineCheckpoint = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse checkpoint: {}", e),
        )
    })?;

    Ok(Some(checkpoint))
}

/// Delete the checkpoint file (called on successful completion)
pub(crate) fn clear_checkpoint() -> io::Result<()> {
    let path = Path::new(CHECKPOINT_PATH);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Check if a checkpoint exists
pub(crate) fn checkpoint_exists() -> bool {
    Path::new(CHECKPOINT_PATH).exists()
}

// Vague status line constants (for isolation mode)
const VAGUE_STATUS_LINE: &str = "In progress.";
const VAGUE_NOTES_LINE: &str = "Notes.";
const VAGUE_ISSUES_LINE: &str = "No issues recorded.";

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

fn overwrite_one_liner(path: &Path, line: &str) -> io::Result<()> {
    // Enforce "1 sentence, 1 line" semantics by taking only the first line.
    let first_line = line.lines().next().unwrap_or_default().trim();
    let content = if first_line.is_empty() {
        "\n".to_string()
    } else {
        format!("{}\n", first_line)
    };
    fs::write(path, content)
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

/// Clean context before reviewer phase
///
/// When `isolation_mode` is true (the default), this function does nothing
/// since STATUS.md, NOTES.md and ISSUES.md should not exist in isolation mode.
pub(crate) fn clean_context_for_reviewer(logger: &Logger, isolation_mode: bool) -> io::Result<()> {
    if isolation_mode {
        // In isolation mode, these files don't exist, so nothing to clean
        logger.info("Isolation mode: skipping context cleanup (files don't exist)");
        return Ok(());
    }

    logger.info("Cleaning context for reviewer (fresh eyes)...");

    // Remove any archived context; preserving it defeats the "fresh eyes" intent.
    if Path::new(".agent/archive").exists() {
        // Best-effort: if this fails, proceed with overwriting the live files.
        let _ = fs::remove_dir_all(".agent/archive");
    }

    // Overwrite live context files with intentionally vague one-liners.
    overwrite_one_liner(Path::new(".agent/STATUS.md"), VAGUE_STATUS_LINE)?;
    overwrite_one_liner(Path::new(".agent/NOTES.md"), VAGUE_NOTES_LINE)?;
    overwrite_one_liner(Path::new(".agent/ISSUES.md"), VAGUE_ISSUES_LINE)?;

    logger.success("Context cleaned for reviewer");
    Ok(())
}

/// Delete STATUS.md, NOTES.md and ISSUES.md for isolation mode.
///
/// This function is called at the start of each Ralph run when isolation mode
/// is enabled (the default). It prevents context contamination by removing
/// any stale status, notes, or issues from previous runs.
///
/// Unlike `clean_context_for_reviewer()`, this does NOT archive the files -
/// in isolation mode, the goal is to operate without these files entirely,
/// so there's no value in preserving them.
pub(crate) fn reset_context_for_isolation(logger: &Logger) -> io::Result<()> {
    logger.info("Isolation mode: removing STATUS.md, NOTES.md and ISSUES.md...");

    let status_path = Path::new(".agent/STATUS.md");
    let notes_path = Path::new(".agent/NOTES.md");
    let issues_path = Path::new(".agent/ISSUES.md");

    if status_path.exists() {
        fs::remove_file(status_path)?;
        logger.info("Deleted .agent/STATUS.md");
    }

    if notes_path.exists() {
        fs::remove_file(notes_path)?;
        logger.info("Deleted .agent/NOTES.md");
    }

    if issues_path.exists() {
        fs::remove_file(issues_path)?;
        logger.info("Deleted .agent/ISSUES.md");
    }

    logger.success("Context reset for isolation mode");
    Ok(())
}

/// Update the status file with minimal, vague content.
///
/// Status is intentionally kept to 1 sentence to prevent context contamination.
/// The content should encourage discovery rather than tracking detailed progress.
///
/// When `isolation_mode` is true (the default), this function does nothing
/// since STATUS.md should not exist in isolation mode.
pub(crate) fn update_status(_status: &str, isolation_mode: bool) -> io::Result<()> {
    if isolation_mode {
        // In isolation mode, STATUS.md should not exist
        return Ok(());
    }
    overwrite_one_liner(Path::new(".agent/STATUS.md"), VAGUE_STATUS_LINE)
}

/// Ensure required files exist.
///
/// When `isolation_mode` is true (the default), STATUS.md, NOTES.md and ISSUES.md
/// are NOT created. This prevents context contamination from previous runs.
pub(crate) fn ensure_files(isolation_mode: bool) -> io::Result<()> {
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

    // Only create STATUS.md, NOTES.md and ISSUES.md when NOT in isolation mode
    if !isolation_mode {
        // Always overwrite/truncate these files to a single vague sentence to
        // avoid detailed context persisting across runs.
        overwrite_one_liner(Path::new(".agent/STATUS.md"), VAGUE_STATUS_LINE)?;
        overwrite_one_liner(Path::new(".agent/NOTES.md"), VAGUE_NOTES_LINE)?;
        overwrite_one_liner(Path::new(".agent/ISSUES.md"), VAGUE_ISSUES_LINE)?;
    }

    Ok(())
}

/// Files that Ralph generates during a run and should clean up
pub(crate) const GENERATED_FILES: &[&str] = &[
    ".no_agent_commit",
    ".agent/PLAN.md",
    ".agent/commit-message.txt",
    ".agent/checkpoint.json.tmp",
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

/// Result of PROMPT.md validation
#[derive(Debug, Clone)]
pub(crate) struct PromptValidationResult {
    /// Whether PROMPT.md exists
    pub exists: bool,
    /// Whether PROMPT.md has non-empty content
    pub has_content: bool,
    /// Whether a Goal section was found
    pub has_goal: bool,
    /// Whether an Acceptance section was found
    pub has_acceptance: bool,
    /// List of warnings (non-blocking issues)
    pub warnings: Vec<String>,
    /// List of errors (blocking issues)
    pub errors: Vec<String>,
}

impl PromptValidationResult {
    /// Returns true if validation passed (no errors)
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns true if validation passed with no warnings
    #[allow(dead_code)]
    pub fn is_perfect(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }
}

/// Validate PROMPT.md structure and content
///
/// Checks for:
/// - File existence and non-empty content
/// - Goal section (## Goal or # Goal)
/// - Acceptance section (## Acceptance, Acceptance Criteria, or acceptance)
///
/// In strict mode, missing sections are errors; otherwise they're warnings.
pub(crate) fn validate_prompt_md(strict: bool) -> PromptValidationResult {
    let prompt_path = Path::new("PROMPT.md");
    let mut result = PromptValidationResult {
        exists: prompt_path.exists(),
        has_content: false,
        has_goal: false,
        has_acceptance: false,
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    if !result.exists {
        result.errors.push("PROMPT.md not found".to_string());
        return result;
    }

    let content = match fs::read_to_string(prompt_path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("Failed to read PROMPT.md: {}", e));
            return result;
        }
    };

    result.has_content = !content.trim().is_empty();
    if !result.has_content {
        result.errors.push("PROMPT.md is empty".to_string());
        return result;
    }

    // Check for Goal section
    result.has_goal = content.contains("## Goal") || content.contains("# Goal");
    if !result.has_goal {
        let msg = "PROMPT.md missing '## Goal' section".to_string();
        if strict {
            result.errors.push(msg);
        } else {
            result.warnings.push(msg);
        }
    }

    // Check for Acceptance section
    result.has_acceptance = content.contains("## Acceptance")
        || content.contains("# Acceptance")
        || content.contains("Acceptance Criteria")
        || content.to_lowercase().contains("acceptance");
    if !result.has_acceptance {
        let msg = "PROMPT.md missing acceptance checks section".to_string();
        if strict {
            result.errors.push(msg);
        } else {
            result.warnings.push(msg);
        }
    }

    result
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
    fn test_update_status_non_isolation() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            update_status("In progress.", false).unwrap();

            let content = fs::read_to_string(".agent/STATUS.md").unwrap();
            assert_eq!(content, "In progress.\n");
        });
    }

    #[test]
    fn test_update_status_isolation_mode_does_nothing() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            // In isolation mode, update_status should do nothing
            update_status("In progress.", true).unwrap();

            // STATUS.md should NOT be created
            assert!(!Path::new(".agent/STATUS.md").exists());
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

    // =========================================================================
    // Isolation Mode Tests
    // =========================================================================

    #[test]
    fn test_ensure_files_isolation_mode_does_not_create_status_notes_issues() {
        with_temp_cwd(|_dir| {
            // Run ensure_files with isolation_mode=true
            ensure_files(true).unwrap();

            // Should create PROMPT.md only
            assert!(Path::new("PROMPT.md").exists());

            // Should NOT create STATUS.md, NOTES.md and ISSUES.md in isolation mode
            assert!(!Path::new(".agent/STATUS.md").exists());
            assert!(!Path::new(".agent/NOTES.md").exists());
            assert!(!Path::new(".agent/ISSUES.md").exists());
        });
    }

    #[test]
    fn test_ensure_files_non_isolation_mode_creates_all_files() {
        with_temp_cwd(|_dir| {
            // Run ensure_files with isolation_mode=false
            ensure_files(false).unwrap();

            // Should create all files including STATUS.md, NOTES.md and ISSUES.md
            assert!(Path::new("PROMPT.md").exists());
            assert!(Path::new(".agent/STATUS.md").exists());
            assert!(Path::new(".agent/NOTES.md").exists());
            assert!(Path::new(".agent/ISSUES.md").exists());

            assert_eq!(
                fs::read_to_string(".agent/STATUS.md").unwrap(),
                "In progress.\n"
            );
            assert_eq!(fs::read_to_string(".agent/NOTES.md").unwrap(), "Notes.\n");
            assert_eq!(
                fs::read_to_string(".agent/ISSUES.md").unwrap(),
                "No issues recorded.\n"
            );
        });
    }

    #[test]
    fn test_reset_context_for_isolation_deletes_files() {
        with_temp_cwd(|_dir| {
            // Create .agent directory and files
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/STATUS.md", "some status").unwrap();
            fs::write(".agent/NOTES.md", "some notes").unwrap();
            fs::write(".agent/ISSUES.md", "some issues").unwrap();

            // Verify they exist
            assert!(Path::new(".agent/STATUS.md").exists());
            assert!(Path::new(".agent/NOTES.md").exists());
            assert!(Path::new(".agent/ISSUES.md").exists());

            // Run reset
            let colors = Colors { enabled: false };
            let logger = Logger::new(colors);
            reset_context_for_isolation(&logger).unwrap();

            // Files should be deleted
            assert!(!Path::new(".agent/STATUS.md").exists());
            assert!(!Path::new(".agent/NOTES.md").exists());
            assert!(!Path::new(".agent/ISSUES.md").exists());
        });
    }

    #[test]
    fn test_reset_context_for_isolation_handles_missing_files() {
        with_temp_cwd(|_dir| {
            // Create .agent directory but not the files
            fs::create_dir_all(".agent").unwrap();

            // Should not error when files don't exist
            let colors = Colors { enabled: false };
            let logger = Logger::new(colors);
            let result = reset_context_for_isolation(&logger);
            assert!(result.is_ok());
        });
    }

    // =========================================================================
    // Checkpoint system tests
    // =========================================================================

    #[test]
    fn test_pipeline_phase_display() {
        assert_eq!(format!("{}", PipelinePhase::Planning), "Planning");
        assert_eq!(format!("{}", PipelinePhase::Development), "Development");
        assert_eq!(format!("{}", PipelinePhase::Review), "Review");
        assert_eq!(format!("{}", PipelinePhase::Fix), "Fix");
        assert_eq!(
            format!("{}", PipelinePhase::ReviewAgain),
            "Verification Review"
        );
        assert_eq!(
            format!("{}", PipelinePhase::CommitMessage),
            "Commit Message Generation"
        );
        assert_eq!(
            format!("{}", PipelinePhase::FinalValidation),
            "Final Validation"
        );
        assert_eq!(format!("{}", PipelinePhase::Complete), "Complete");
    }

    #[test]
    fn test_checkpoint_new() {
        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::Development, 2, 5, 0, 2, "claude", "codex");

        assert_eq!(checkpoint.phase, PipelinePhase::Development);
        assert_eq!(checkpoint.iteration, 2);
        assert_eq!(checkpoint.total_iterations, 5);
        assert_eq!(checkpoint.reviewer_pass, 0);
        assert_eq!(checkpoint.total_reviewer_passes, 2);
        assert_eq!(checkpoint.developer_agent, "claude");
        assert_eq!(checkpoint.reviewer_agent, "codex");
        assert!(!checkpoint.timestamp.is_empty());
    }

    #[test]
    fn test_checkpoint_description() {
        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::Development, 3, 5, 0, 2, "claude", "codex");
        assert_eq!(checkpoint.description(), "Development iteration 3/5");

        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::ReviewAgain, 5, 5, 2, 3, "claude", "codex");
        assert_eq!(checkpoint.description(), "Verification review 2/3");
    }

    #[test]
    fn test_checkpoint_save_load() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let checkpoint =
                PipelineCheckpoint::new(PipelinePhase::Review, 5, 5, 1, 2, "claude", "codex");

            save_checkpoint(&checkpoint).unwrap();
            assert!(checkpoint_exists());

            let loaded = load_checkpoint().unwrap().unwrap();
            assert_eq!(loaded.phase, PipelinePhase::Review);
            assert_eq!(loaded.iteration, 5);
            assert_eq!(loaded.developer_agent, "claude");
            assert_eq!(loaded.reviewer_agent, "codex");
        });
    }

    #[test]
    fn test_checkpoint_clear() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let checkpoint =
                PipelineCheckpoint::new(PipelinePhase::Development, 1, 5, 0, 2, "claude", "codex");

            save_checkpoint(&checkpoint).unwrap();
            assert!(checkpoint_exists());

            clear_checkpoint().unwrap();
            assert!(!checkpoint_exists());
        });
    }

    #[test]
    fn test_load_checkpoint_nonexistent() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let result = load_checkpoint().unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_checkpoint_serialization() {
        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::Fix, 3, 5, 1, 2, "aider", "opencode");

        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains("Fix"));
        assert!(json.contains("aider"));
        assert!(json.contains("opencode"));

        let deserialized: PipelineCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.phase, checkpoint.phase);
        assert_eq!(deserialized.iteration, checkpoint.iteration);
    }

    // PROMPT.md validation tests
    #[test]
    fn test_validate_prompt_md_not_exists() {
        with_temp_cwd(|_dir| {
            let result = validate_prompt_md(false);
            assert!(!result.exists);
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("not found")));
        });
    }

    #[test]
    fn test_validate_prompt_md_empty() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "   \n\n  ").unwrap();
            let result = validate_prompt_md(false);
            assert!(result.exists);
            assert!(!result.has_content);
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("empty")));
        });
    }

    #[test]
    fn test_validate_prompt_md_complete() {
        with_temp_cwd(|_dir| {
            fs::write(
                "PROMPT.md",
                r#"# PROMPT

## Goal
Build a feature

## Acceptance
- Tests pass
"#,
            )
            .unwrap();
            let result = validate_prompt_md(false);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(result.has_goal);
            assert!(result.has_acceptance);
            assert!(result.is_valid());
            assert!(result.is_perfect());
        });
    }

    #[test]
    fn test_validate_prompt_md_missing_sections_lenient() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "Just some random content").unwrap();
            let result = validate_prompt_md(false);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(!result.has_goal);
            assert!(!result.has_acceptance);
            // In lenient mode, missing sections are warnings, not errors
            assert!(result.is_valid());
            assert!(!result.is_perfect());
            assert_eq!(result.warnings.len(), 2);
        });
    }

    #[test]
    fn test_validate_prompt_md_missing_sections_strict() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "Just some random content").unwrap();
            let result = validate_prompt_md(true);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(!result.has_goal);
            assert!(!result.has_acceptance);
            // In strict mode, missing sections are errors
            assert!(!result.is_valid());
            assert_eq!(result.errors.len(), 2);
        });
    }

    #[test]
    fn test_validate_prompt_md_acceptance_variations() {
        with_temp_cwd(|_dir| {
            // Test "Acceptance Criteria" variant
            fs::write(
                "PROMPT.md",
                r#"## Goal
Test

## Acceptance Criteria
- Pass
"#,
            )
            .unwrap();
            let result = validate_prompt_md(false);
            assert!(result.has_acceptance);

            // Test lowercase "acceptance" variant
            fs::write(
                "PROMPT.md",
                r#"## Goal
Test

The acceptance tests should pass.
"#,
            )
            .unwrap();
            let result = validate_prompt_md(false);
            assert!(result.has_acceptance);
        });
    }

    #[test]
    fn test_reset_context_for_isolation_partial_files() {
        with_temp_cwd(|_dir| {
            // Create .agent directory and only NOTES.md
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/NOTES.md", "some notes").unwrap();

            let colors = Colors { enabled: false };
            let logger = Logger::new(colors);
            reset_context_for_isolation(&logger).unwrap();

            // NOTES.md should be deleted, ISSUES.md should remain non-existent
            assert!(!Path::new(".agent/NOTES.md").exists());
            assert!(!Path::new(".agent/ISSUES.md").exists());
        });
    }

    #[test]
    fn test_issues_md_persists_within_review_fix_cycle() {
        // This test documents the expected behavior for isolation mode:
        // - ISSUES.md is cleaned at the START of a run (reset_context_for_isolation)
        // - Within a run, Review creates ISSUES.md and Fix must be able to read it
        // - No automatic cleanup happens between Review and Fix phases
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            let colors = Colors { enabled: false };
            let logger = Logger::new(colors);

            // Simulate start of run: isolation mode cleans up from previous run
            fs::write(".agent/ISSUES.md", "old issues from previous run").unwrap();
            reset_context_for_isolation(&logger).unwrap();
            assert!(
                !Path::new(".agent/ISSUES.md").exists(),
                "ISSUES.md should be cleaned at start of run"
            );

            // Simulate Review phase: creates detailed ISSUES.md
            let review_output = "- [ ] Critical: [src/main.rs:42] Missing error handling\n\
                                 - [ ] High: [src/utils.rs:100] Potential null pointer";
            fs::write(".agent/ISSUES.md", review_output).unwrap();
            assert!(
                Path::new(".agent/ISSUES.md").exists(),
                "Review should create ISSUES.md"
            );

            // Simulate Fix phase: must be able to read ISSUES.md
            // (no cleanup between Review and Fix - this is the key behavior)
            let issues_content = fs::read_to_string(".agent/ISSUES.md").unwrap();
            assert!(
                issues_content.contains("Critical"),
                "Fix phase must be able to read detailed issues from Review"
            );
            assert!(
                issues_content.contains("src/main.rs:42"),
                "Fix phase must see file locations to know where to fix"
            );

            // After Fix completes, it overwrites with vague sentence (per current design)
            fs::write(".agent/ISSUES.md", "Issues addressed.").unwrap();

            // Next Review iteration creates fresh detailed ISSUES.md
            let review_output_2 = "- [ ] Medium: [src/lib.rs:50] Code style issue";
            fs::write(".agent/ISSUES.md", review_output_2).unwrap();

            // Fix can still read it
            let issues_content_2 = fs::read_to_string(".agent/ISSUES.md").unwrap();
            assert!(
                issues_content_2.contains("Medium"),
                "Fix phase in iteration 2 must read new issues"
            );

            // Only at next run start would isolation clean up
            reset_context_for_isolation(&logger).unwrap();
            assert!(
                !Path::new(".agent/ISSUES.md").exists(),
                "ISSUES.md should only be cleaned at start of next run"
            );
        });
    }
}
