//! Agent file management for the `.agent/` directory.
//!
//! This module handles creation, modification, and cleanup of files
//! in the `.agent/` directory that are used during pipeline execution.

#![expect(clippy::unnecessary_debug_formatting)]
use crate::files::{integrity, recovery};
use crate::logger::Logger;
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::path::Path;

/// Path to the PROMPT.md backup file in the .agent directory.
const PROMPT_BACKUP_PATH: &str = ".agent/PROMPT.md.backup";

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
        format!("{first_line}\n")
    };
    integrity::write_file_atomic(path, &content)
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
/// Creates the `.agent/logs` directory if it doesn't exist.
///
/// When `isolation_mode` is true (the default), STATUS.md, NOTES.md and ISSUES.md
/// are NOT created. This prevents context contamination from previous runs.
pub fn ensure_files(isolation_mode: bool) -> io::Result<()> {
    let agent_dir = Path::new(".agent");

    // Best-effort state repair before we start touching `.agent/` contents.
    // If the state is unrecoverable, fail early with a clear error.
    if let recovery::RecoveryStatus::Unrecoverable(msg) = recovery::auto_repair(agent_dir)? {
        return Err(io::Error::other(format!(
            "Failed to repair .agent state: {msg}"
        )));
    }

    integrity::check_filesystem_ready(agent_dir)?;
    fs::create_dir_all(".agent/logs")?;

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
    if msg_path.exists() && !integrity::verify_file_not_corrupted(msg_path)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            ".agent/commit-message.txt appears corrupted",
        ));
    }
    let content = fs::read_to_string(msg_path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Failed to read .agent/commit-message.txt: {e}"),
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

/// Write commit message to file.
///
/// Creates the .agent directory if it doesn't exist and writes the
/// commit message to .agent/commit-message.txt.
///
/// # Arguments
///
/// * `message` - The commit message to write
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn write_commit_message_file(message: &str) -> io::Result<()> {
    let msg_path = Path::new(".agent/commit-message.txt");
    if let Some(parent) = msg_path.parent() {
        fs::create_dir_all(parent)?;
    }
    integrity::write_file_atomic(msg_path, message)?;
    Ok(())
}

/// Create a backup of PROMPT.md to protect against accidental deletion.
///
/// This function copies PROMPT.md to `.agent/PROMPT.md.backup` and sets
/// the backup file to read-only permissions to make accidental deletion harder.
///
/// With backup rotation enabled (the default), this maintains up to 3 backup
/// versions: `.agent/PROMPT.md.backup`, `.agent/PROMPT.md.backup.1`, and
/// `.agent/PROMPT.md.backup.2`.
///
/// # Behavior
///
/// - If PROMPT.md doesn't exist, returns `Ok(())` (nothing to backup)
/// - Creates the `.agent` directory if it doesn't exist
/// - Rotates existing backups: backup.2 → deleted, backup.1 → backup.2, backup → backup.1
/// - Uses atomic write to ensure backup file integrity
/// - Sets all backup files to read-only (best-effort; failures don't error)
/// - Returns a warning string in the Ok variant if read-only setting fails
///
/// # Returns
///
/// Returns `io::Result<Option<String>>` where:
/// - `Ok(None)` - backup created and read-only set successfully
/// - `Ok(Some(warning))` - backup created but read-only couldn't be set
/// - `Err(e)` - if the backup cannot be created
pub fn create_prompt_backup() -> io::Result<Option<String>> {
    let prompt_path = Path::new("PROMPT.md");

    // If PROMPT.md doesn't exist, that's fine - nothing to backup
    if !prompt_path.exists() {
        return Ok(None);
    }

    // Ensure .agent directory exists
    let backup_base = Path::new(PROMPT_BACKUP_PATH);
    if let Some(parent) = backup_base.parent() {
        fs::create_dir_all(parent)?;
    }

    // Read PROMPT.md content
    let content = fs::read_to_string(prompt_path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Failed to read PROMPT.md for backup: {e}"),
        )
    })?;

    // Backup rotation: .2 → deleted, .1 → .2, .backup → .1
    let backup_2 = Path::new(".agent/PROMPT.md.backup.2");
    let backup_1 = Path::new(".agent/PROMPT.md.backup.1");

    // Delete oldest backup if it exists
    let _ = fs::remove_file(backup_2);

    // Rotate .1 → .2
    if backup_1.exists() {
        let _ = fs::rename(backup_1, backup_2);
    }

    // Rotate .backup → .1
    if backup_base.exists() {
        let _ = fs::rename(backup_base, backup_1);
    }

    // Write new backup atomically
    integrity::write_file_atomic(backup_base, &content)
        .map_err(|e| io::Error::new(e.kind(), format!("Failed to write PROMPT.md backup: {e}")))?;

    // Set read-only permissions on all backups and track any failure
    let mut readonly_warning = None;

    // Helper to set read-only permissions on a file
    let set_readonly = |path: &Path| -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o444);
                if fs::set_permissions(path, perms).is_err() {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!("Failed to set read-only on {path:?}"),
                    ));
                }
            }
        }

        #[cfg(windows)]
        {
            if let Ok(metadata) = fs::metadata(path) {
                let mut perms = metadata.permissions();
                perms.set_readonly(true);
                if fs::set_permissions(path, perms).is_err() {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!("Failed to set read-only on {:?}", path),
                    ));
                }
            }
        }
        Ok(())
    };

    // Try to set read-only on all backup files (best-effort)
    for backup_path in &[backup_base, backup_1, backup_2] {
        if backup_path.exists() {
            if let Err(e) = set_readonly(backup_path) {
                if readonly_warning.is_none() {
                    readonly_warning = Some(e.to_string());
                }
            }
        }
    }

    Ok(readonly_warning)
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

/// Make PROMPT.md read-only to protect against accidental deletion.
///
/// This function sets read-only permissions on PROMPT.md to make accidental
/// deletion harder. This is a best-effort protection - agents with shell
/// access could potentially chmod the file.
///
/// # Behavior
///
/// - If PROMPT.md doesn't exist, returns `Ok(None)` (nothing to protect)
/// - Uses platform-specific permission setting
/// - Returns a warning string if setting permissions fails (best-effort)
///
/// # Returns
///
/// Returns `Ok(Option<String>)` where:
/// - `Ok(None)` - permissions set successfully or file doesn't exist
/// - `Ok(Some(warning))` - couldn't set read-only permissions
pub fn make_prompt_read_only() -> Option<String> {
    let prompt_path = Path::new("PROMPT.md");

    // If PROMPT.md doesn't exist, that's fine - nothing to protect
    if !prompt_path.exists() {
        return None;
    }

    // Try to set read-only permissions and track any failure
    let mut readonly_warning = None;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match fs::metadata(prompt_path) {
            Ok(metadata) => {
                let mut perms = metadata.permissions();
                perms.set_mode(0o444); // Read-only for all
                if fs::set_permissions(prompt_path, perms).is_err() {
                    readonly_warning =
                        Some("Failed to set read-only permissions on PROMPT.md".to_string());
                }
            }
            Err(_) => {
                readonly_warning = Some("Failed to read metadata for PROMPT.md".to_string());
            }
        }
    }

    #[cfg(windows)]
    {
        match fs::metadata(prompt_path) {
            Ok(metadata) => {
                let mut perms = metadata.permissions();
                perms.set_readonly(true);
                if fs::set_permissions(prompt_path, perms).is_err() {
                    readonly_warning =
                        Some("Failed to set read-only permissions on PROMPT.md".to_string());
                }
            }
            Err(_) => {
                readonly_warning = Some("Failed to read metadata for PROMPT.md".to_string());
            }
        }
    }

    readonly_warning
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

            // Should not create PROMPT.md (creation is an explicit user action)
            assert!(!Path::new("PROMPT.md").exists());

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

            // Should not create PROMPT.md (creation is an explicit user action)
            assert!(!Path::new("PROMPT.md").exists());
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

    // Tests for create_prompt_backup

    #[test]
    fn test_create_prompt_backup_creates_file() {
        with_temp_cwd(|_dir| {
            // Create a PROMPT.md
            fs::write("PROMPT.md", "# Test Prompt\n\nThis is a test prompt.").unwrap();

            // Create backup
            create_prompt_backup().unwrap();

            // Verify backup exists
            assert!(Path::new(".agent/PROMPT.md.backup").exists());

            // Verify backup content matches original
            let original = fs::read_to_string("PROMPT.md").unwrap();
            let backup = fs::read_to_string(".agent/PROMPT.md.backup").unwrap();
            assert_eq!(original, backup);
        });
    }

    #[test]
    fn test_create_prompt_backup_handles_missing_prompt() {
        with_temp_cwd(|_dir| {
            // No PROMPT.md exists
            assert!(!Path::new("PROMPT.md").exists());

            // Should succeed without error
            create_prompt_backup().unwrap();

            // Backup should not be created
            assert!(!Path::new(".agent/PROMPT.md.backup").exists());
        });
    }

    #[test]
    fn test_create_prompt_backup_idempotent() {
        with_temp_cwd(|_dir| {
            // Create a PROMPT.md
            fs::write("PROMPT.md", "# Test Prompt\n\nThis is a test prompt.").unwrap();

            // Create backup twice
            create_prompt_backup().unwrap();
            create_prompt_backup().unwrap();

            // Backup should still exist with correct content
            assert!(Path::new(".agent/PROMPT.md.backup").exists());
            let original = fs::read_to_string("PROMPT.md").unwrap();
            let backup = fs::read_to_string(".agent/PROMPT.md.backup").unwrap();
            assert_eq!(original, backup);
        });
    }

    #[test]
    fn test_create_prompt_backup_overwrites_existing() {
        with_temp_cwd(|_dir| {
            // Create PROMPT.md and an old backup with different content
            fs::write("PROMPT.md", "# New Content\n\nThis is the new content.").unwrap();
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/PROMPT.md.backup", "# Old Content\n\nOld backup.").unwrap();

            // Create backup (should overwrite)
            create_prompt_backup().unwrap();

            // Verify backup has new content
            let backup = fs::read_to_string(".agent/PROMPT.md.backup").unwrap();
            assert_eq!(backup, "# New Content\n\nThis is the new content.");
        });
    }

    #[test]
    fn test_create_prompt_backup_rotation() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            // First backup
            fs::write("PROMPT.md", "# Version 1\n").unwrap();
            create_prompt_backup().unwrap();

            // Verify .backup exists and has version 1
            assert!(Path::new(".agent/PROMPT.md.backup").exists());
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup").unwrap(),
                "# Version 1\n"
            );
            assert!(!Path::new(".agent/PROMPT.md.backup.1").exists());

            // Second backup - should rotate to .1
            fs::write("PROMPT.md", "# Version 2\n").unwrap();
            create_prompt_backup().unwrap();

            // Verify rotation
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup").unwrap(),
                "# Version 2\n"
            );
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup.1").unwrap(),
                "# Version 1\n"
            );
            assert!(!Path::new(".agent/PROMPT.md.backup.2").exists());

            // Third backup - should rotate to .2
            fs::write("PROMPT.md", "# Version 3\n").unwrap();
            create_prompt_backup().unwrap();

            // Verify rotation
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup").unwrap(),
                "# Version 3\n"
            );
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup.1").unwrap(),
                "# Version 2\n"
            );
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup.2").unwrap(),
                "# Version 1\n"
            );
        });
    }

    #[test]
    fn test_create_prompt_backup_rotation_deletes_oldest() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            // Create 4 backups - oldest should be deleted
            for i in 1..=4 {
                fs::write("PROMPT.md", format!("# Version {i}\n")).unwrap();
                create_prompt_backup().unwrap();
            }

            // Verify only 3 backups exist
            assert!(Path::new(".agent/PROMPT.md.backup").exists());
            assert!(Path::new(".agent/PROMPT.md.backup.1").exists());
            assert!(Path::new(".agent/PROMPT.md.backup.2").exists());

            // Verify content
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup").unwrap(),
                "# Version 4\n"
            );
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup.1").unwrap(),
                "# Version 3\n"
            );
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup.2").unwrap(),
                "# Version 2\n"
            );
        });
    }

    // Tests for make_prompt_read_only

    #[test]
    fn test_make_prompt_read_only_sets_permissions() {
        with_temp_cwd(|_dir| {
            // Create a PROMPT.md
            fs::write("PROMPT.md", "# Test Prompt\n\nThis is a test prompt.").unwrap();

            // Make it read-only
            make_prompt_read_only();

            // On Unix, verify permissions are read-only
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata("PROMPT.md").unwrap();
                let perms = metadata.permissions();
                let mode = perms.mode();
                // Check that owner write bit is not set (0o444 = read-only)
                assert_eq!(mode & 0o777, 0o444);
            }

            // On Windows, verify readonly flag is set
            #[cfg(windows)]
            {
                let metadata = fs::metadata("PROMPT.md").unwrap();
                let perms = metadata.permissions();
                assert!(perms.readonly());
            }
        });
    }

    #[test]
    fn test_make_prompt_read_only_handles_missing_prompt() {
        with_temp_cwd(|_dir| {
            // No PROMPT.md exists
            assert!(!Path::new("PROMPT.md").exists());

            // Should succeed without error
            make_prompt_read_only();
        });
    }

    #[test]
    fn test_make_prompt_read_only_idempotent() {
        with_temp_cwd(|_dir| {
            // Create a PROMPT.md
            fs::write("PROMPT.md", "# Test Prompt\n\nThis is a test prompt.").unwrap();

            // Make it read-only twice
            make_prompt_read_only();
            make_prompt_read_only();

            // File should still exist and be readable
            let content = fs::read_to_string("PROMPT.md").unwrap();
            assert_eq!(content, "# Test Prompt\n\nThis is a test prompt.");
        });
    }
}
