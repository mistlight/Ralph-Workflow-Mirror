#![allow(dead_code)]
//! Context file management for Ralph's agent files.
//!
//! This module handles operations on context files (STATUS.md, NOTES.md, ISSUES.md)
//! in the `.agent/` directory, including cleanup for isolation mode and fresh eyes
//! for the reviewer phase.
use crate::logger::Logger;
use std::fs;
use std::io;
use std::path::Path;

use super::integrity;

// Vague status line constants (for isolation mode)
pub const VAGUE_STATUS_LINE: &str = "In progress.";
pub const VAGUE_NOTES_LINE: &str = "Notes.";
pub const VAGUE_ISSUES_LINE: &str = "No issues recorded.";

/// Overwrite a file with a single-line content.
///
/// Enforces "1 sentence, 1 line" semantics by taking only the first line.
pub fn overwrite_one_liner(path: &Path, line: &str) -> io::Result<()> {
    let first_line = line.lines().next().unwrap_or_default().trim();
    let content = if first_line.is_empty() {
        "\n".to_string()
    } else {
        format!("{first_line}\n")
    };
    integrity::write_file_atomic(path, &content)
}

/// Clean context before reviewer phase.
///
/// When `isolation_mode` is true (the default), this function does nothing
/// since STATUS.md, NOTES.md and ISSUES.md should not exist in isolation mode.
///
/// In non-isolation mode, this overwrites the context files with vague
/// one-liners to give the reviewer "fresh eyes" without context from
/// the development phase.
pub fn clean_context_for_reviewer(logger: &Logger, isolation_mode: bool) -> io::Result<()> {
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
pub fn reset_context_for_isolation(logger: &Logger) -> io::Result<()> {
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

/// Delete ISSUES.md after the final fix iteration completes in isolation mode.
///
/// This function is called at the end of the review-fix cycle when isolation mode
/// is enabled. Between Review and Fix phases, ISSUES.md must persist so the Fix
/// agent knows what to fix. But after all cycles complete, ISSUES.md should be
/// deleted to prevent context contamination for subsequent runs.
pub fn delete_issues_file_for_isolation(logger: &Logger) -> io::Result<()> {
    let issues_path = Path::new(".agent/ISSUES.md");

    if issues_path.exists() {
        fs::remove_file(issues_path)?;
        logger.info("Isolation mode: deleted .agent/ISSUES.md after final fix");
    }

    Ok(())
}

/// Update the status file with minimal, vague content.
///
/// Status is intentionally kept to 1 sentence to prevent context contamination.
/// The content should encourage discovery rather than tracking detailed progress.
///
/// When `isolation_mode` is true (the default), this function does nothing
/// since STATUS.md should not exist in isolation mode.
pub fn update_status(_status: &str, isolation_mode: bool) -> io::Result<()> {
    if isolation_mode {
        // In isolation mode, STATUS.md should not exist
        return Ok(());
    }
    overwrite_one_liner(Path::new(".agent/STATUS.md"), VAGUE_STATUS_LINE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::Colors;
    use test_helpers::with_temp_cwd;

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

    #[test]
    fn test_reset_context_for_isolation() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/STATUS.md", "some status").unwrap();
            fs::write(".agent/NOTES.md", "some notes").unwrap();
            fs::write(".agent/ISSUES.md", "some issues").unwrap();

            let colors = Colors { enabled: false };
            let logger = Logger::new(colors);
            reset_context_for_isolation(&logger).unwrap();

            assert!(!Path::new(".agent/STATUS.md").exists());
            assert!(!Path::new(".agent/NOTES.md").exists());
            assert!(!Path::new(".agent/ISSUES.md").exists());
        });
    }

    #[test]
    fn test_delete_issues_file_for_isolation() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/ISSUES.md", "some issues").unwrap();

            let colors = Colors { enabled: false };
            let logger = Logger::new(colors);
            delete_issues_file_for_isolation(&logger).unwrap();

            assert!(!Path::new(".agent/ISSUES.md").exists());
        });
    }
}
