//! Agent file management for the `.agent/` directory.
//!
//! This module handles creation, modification, and cleanup of files
//! in the `.agent/` directory that are used during pipeline execution.

use crate::logger::Logger;
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::path::Path;

// Vague status line constants (for isolation mode)
const VAGUE_STATUS_LINE: &str = "In progress.";
const VAGUE_NOTES_LINE: &str = "Notes.";
const VAGUE_ISSUES_LINE: &str = "No issues recorded.";

/// Files that Ralph generates during a run and should clean up.
pub const GENERATED_FILES: &[&str] = &[
    ".no_agent_commit",
    ".agent/PLAN.md",
    ".agent/commit-message.txt",
    ".agent/checkpoint.json.tmp",
];

/// Overwrite a file with a single-line content.
///
/// Enforces "1 sentence, 1 line" semantics by taking only the first line.
fn overwrite_one_liner(path: &Path, line: &str) -> io::Result<()> {
    let first_line = line.lines().next().unwrap_or_default().trim();
    let content = if first_line.is_empty() {
        "\n".to_string()
    } else {
        format!("{}\n", first_line)
    };
    fs::write(path, content)
}

/// Check if a file contains a specific marker string.
///
/// Useful for detecting specific content patterns in files without
/// loading the entire file into memory.
///
/// # Arguments
///
/// * `file_path` - Path to the file to check
/// * `marker` - String to search for
///
/// # Returns
///
/// `Ok(true)` if the marker is found, `Ok(false)` if not found or file doesn't exist.
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

/// Ensure required files and directories exist.
///
/// Creates the `.agent/logs` directory and PROMPT.md template if they don't exist.
///
/// When `isolation_mode` is true (the default), STATUS.md, NOTES.md and ISSUES.md
/// are NOT created. This prevents context contamination from previous runs.
pub fn ensure_files(isolation_mode: bool) -> io::Result<()> {
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

/// Delete the PLAN.md file after integration.
///
/// Called after the plan has been integrated into the codebase.
/// Silently succeeds if the file doesn't exist.
pub fn delete_plan_file() -> io::Result<()> {
    let plan_path = Path::new(".agent/PLAN.md");
    if plan_path.exists() {
        fs::remove_file(plan_path)?;
    }
    Ok(())
}

/// Delete the commit-message.txt file after committing.
///
/// Called after a successful git commit to clean up the temporary
/// commit message file. Silently succeeds if the file doesn't exist.
pub fn delete_commit_message_file() -> io::Result<()> {
    let msg_path = Path::new(".agent/commit-message.txt");
    if msg_path.exists() {
        fs::remove_file(msg_path)?;
    }
    Ok(())
}

/// Read commit message from file; fails if missing or empty.
///
/// # Errors
///
/// Returns an error if the file doesn't exist, cannot be read, or is empty.
pub fn read_commit_message_file() -> io::Result<String> {
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

/// Clean up all generated files.
///
/// Removes temporary files that may have been left behind by an interrupted
/// pipeline run. This includes PLAN.md, commit-message.txt, and other
/// artifacts listed in [`GENERATED_FILES`].
///
/// This function is best-effort: individual file deletion failures are
/// silently ignored since we're in a cleanup context.
pub fn cleanup_generated_files() {
    for file in GENERATED_FILES {
        let _ = fs::remove_file(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::Colors;
    use crate::test_utils::testing::with_temp_cwd;
    use tempfile::TempDir;

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
    fn test_read_commit_message_file() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/commit-message.txt", "feat: test commit\n").unwrap();

            let msg = read_commit_message_file().unwrap();
            assert_eq!(msg, "feat: test commit");
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
    fn test_ensure_files_isolation_mode() {
        with_temp_cwd(|_dir| {
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
    fn test_ensure_files_non_isolation_mode() {
        with_temp_cwd(|_dir| {
            ensure_files(false).unwrap();

            // Should create all files
            assert!(Path::new("PROMPT.md").exists());
            assert!(Path::new(".agent/STATUS.md").exists());
            assert!(Path::new(".agent/NOTES.md").exists());
            assert!(Path::new(".agent/ISSUES.md").exists());
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
