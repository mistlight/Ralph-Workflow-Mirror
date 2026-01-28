//! Context file management for Ralph's agent files.
//!
//! This module handles operations on context files (STATUS.md, NOTES.md, ISSUES.md)
//! in the `.agent/` directory, including cleanup for isolation mode and fresh eyes
//! for the reviewer phase.
use crate::logger::Logger;
use crate::workspace::Workspace;
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
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`clean_context_for_reviewer_at`] instead.
pub fn clean_context_for_reviewer(logger: &Logger, isolation_mode: bool) -> io::Result<()> {
    clean_context_for_reviewer_at(Path::new("."), logger, isolation_mode)
}

/// Clean context before reviewer phase at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
/// * `logger` - Logger for output
/// * `isolation_mode` - If true, skip cleanup since files don't exist
pub fn clean_context_for_reviewer_at(
    repo_root: &Path,
    logger: &Logger,
    isolation_mode: bool,
) -> io::Result<()> {
    if isolation_mode {
        // In isolation mode, these files don't exist, so nothing to clean
        logger.info("Isolation mode: skipping context cleanup (files don't exist)");
        return Ok(());
    }

    logger.info("Cleaning context for reviewer (fresh eyes)...");

    let agent_dir = repo_root.join(".agent");

    // Remove any archived context; preserving it defeats the "fresh eyes" intent.
    let archive_dir = agent_dir.join("archive");
    if archive_dir.exists() {
        // Best-effort: if this fails, proceed with overwriting the live files.
        let _ = fs::remove_dir_all(&archive_dir);
    }

    // Overwrite live context files with intentionally vague one-liners.
    overwrite_one_liner(&agent_dir.join("STATUS.md"), VAGUE_STATUS_LINE)?;
    overwrite_one_liner(&agent_dir.join("NOTES.md"), VAGUE_NOTES_LINE)?;
    overwrite_one_liner(&agent_dir.join("ISSUES.md"), VAGUE_ISSUES_LINE)?;

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
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`reset_context_for_isolation_at`] instead.
pub fn reset_context_for_isolation(logger: &Logger) -> io::Result<()> {
    reset_context_for_isolation_at(Path::new("."), logger)
}

/// Delete STATUS.md, NOTES.md and ISSUES.md for isolation mode at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
/// * `logger` - Logger for output
pub fn reset_context_for_isolation_at(repo_root: &Path, logger: &Logger) -> io::Result<()> {
    logger.info("Isolation mode: removing STATUS.md, NOTES.md and ISSUES.md...");

    let agent_dir = repo_root.join(".agent");
    let status_path = agent_dir.join("STATUS.md");
    let notes_path = agent_dir.join("NOTES.md");
    let issues_path = agent_dir.join("ISSUES.md");

    if status_path.exists() {
        fs::remove_file(&status_path)?;
        logger.info("Deleted .agent/STATUS.md");
    }

    if notes_path.exists() {
        fs::remove_file(&notes_path)?;
        logger.info("Deleted .agent/NOTES.md");
    }

    if issues_path.exists() {
        fs::remove_file(&issues_path)?;
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
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`delete_issues_file_for_isolation_at`] instead.
pub fn delete_issues_file_for_isolation(logger: &Logger) -> io::Result<()> {
    delete_issues_file_for_isolation_at(Path::new("."), logger)
}

/// Delete ISSUES.md after the final fix iteration completes in isolation mode at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
/// * `logger` - Logger for output
pub fn delete_issues_file_for_isolation_at(repo_root: &Path, logger: &Logger) -> io::Result<()> {
    let issues_path = repo_root.join(".agent/ISSUES.md");

    if issues_path.exists() {
        fs::remove_file(&issues_path)?;
        logger.info("Isolation mode: deleted .agent/ISSUES.md after final fix");
    }

    Ok(())
}

/// Delete ISSUES.md after the final fix iteration completes using workspace.
///
/// This version uses the [`Workspace`] trait for file operations,
/// allowing tests to use an in-memory workspace instead of the real filesystem.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `logger` - Logger for output
pub fn delete_issues_file_for_isolation_with_workspace(
    workspace: &dyn Workspace,
    logger: &Logger,
) -> io::Result<()> {
    let issues_path = Path::new(".agent/ISSUES.md");

    if workspace.exists(issues_path) {
        workspace.remove(issues_path)?;
        logger.info("Isolation mode: deleted .agent/ISSUES.md after final fix");
    }

    Ok(())
}

/// Overwrite a file with a single-line content using workspace.
///
/// Enforces "1 sentence, 1 line" semantics by taking only the first line.
/// Uses atomic write to ensure file integrity.
fn overwrite_one_liner_with_workspace(
    workspace: &dyn Workspace,
    path: &Path,
    line: &str,
) -> io::Result<()> {
    let first_line = line.lines().next().unwrap_or_default().trim();
    let content = if first_line.is_empty() {
        "\n".to_string()
    } else {
        format!("{first_line}\n")
    };
    workspace.write_atomic(path, &content)
}

/// Clean context before reviewer phase using workspace.
///
/// This version uses the [`Workspace`] trait for file operations,
/// allowing tests to use an in-memory workspace instead of the real filesystem.
///
/// When `isolation_mode` is true (the default), this function does nothing
/// since STATUS.md, NOTES.md and ISSUES.md should not exist in isolation mode.
///
/// In non-isolation mode, this overwrites the context files with vague
/// one-liners to give the reviewer "fresh eyes" without context from
/// the development phase.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `logger` - Logger for output
/// * `isolation_mode` - If true, skip cleanup since files don't exist
pub fn clean_context_for_reviewer_with_workspace(
    workspace: &dyn Workspace,
    logger: &Logger,
    isolation_mode: bool,
) -> io::Result<()> {
    if isolation_mode {
        // In isolation mode, these files don't exist, so nothing to clean
        logger.info("Isolation mode: skipping context cleanup (files don't exist)");
        return Ok(());
    }

    logger.info("Cleaning context for reviewer (fresh eyes)...");

    // Remove any archived context; preserving it defeats the "fresh eyes" intent.
    let archive_dir = Path::new(".agent/archive");
    // Best-effort: if this fails, proceed with overwriting the live files.
    let _ = workspace.remove_dir_all_if_exists(archive_dir);

    // Overwrite live context files with intentionally vague one-liners.
    overwrite_one_liner_with_workspace(
        workspace,
        Path::new(".agent/STATUS.md"),
        VAGUE_STATUS_LINE,
    )?;
    overwrite_one_liner_with_workspace(workspace, Path::new(".agent/NOTES.md"), VAGUE_NOTES_LINE)?;
    overwrite_one_liner_with_workspace(
        workspace,
        Path::new(".agent/ISSUES.md"),
        VAGUE_ISSUES_LINE,
    )?;

    logger.success("Context cleaned for reviewer");
    Ok(())
}

/// Update the status file with minimal, vague content.
///
/// Status is intentionally kept to 1 sentence to prevent context contamination.
/// The content should encourage discovery rather than tracking detailed progress.
///
/// When `isolation_mode` is true (the default), this function does nothing
/// since STATUS.md should not exist in isolation mode.
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`update_status_at`] instead.
pub fn update_status(_status: &str, isolation_mode: bool) -> io::Result<()> {
    update_status_at(Path::new("."), _status, isolation_mode)
}

/// Update the status file with minimal, vague content at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
/// * `_status` - Status string (unused, always writes vague status)
/// * `isolation_mode` - If true, do nothing since STATUS.md should not exist
pub fn update_status_at(repo_root: &Path, _status: &str, isolation_mode: bool) -> io::Result<()> {
    if isolation_mode {
        // In isolation mode, STATUS.md should not exist
        return Ok(());
    }
    overwrite_one_liner(&repo_root.join(".agent/STATUS.md"), VAGUE_STATUS_LINE)
}

/// Update the status file with minimal, vague content using workspace.
///
/// This version uses the [`Workspace`] trait for file operations,
/// allowing tests to use an in-memory workspace instead of the real filesystem.
///
/// Status is intentionally kept to 1 sentence to prevent context contamination.
/// When `isolation_mode` is true (the default), this function does nothing
/// since STATUS.md should not exist in isolation mode.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `_status` - Status string (unused, always writes vague status)
/// * `isolation_mode` - If true, do nothing since STATUS.md should not exist
pub fn update_status_with_workspace(
    workspace: &dyn Workspace,
    _status: &str,
    isolation_mode: bool,
) -> io::Result<()> {
    if isolation_mode {
        // In isolation mode, STATUS.md should not exist
        return Ok(());
    }
    overwrite_one_liner_with_workspace(workspace, Path::new(".agent/STATUS.md"), VAGUE_STATUS_LINE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::Colors;
    use crate::workspace::{MemoryWorkspace, Workspace};

    // =========================================================================
    // Workspace-based tests (for testability without real filesystem)
    // =========================================================================
    //
    // Note: Old tests using with_temp_cwd have been removed since production
    // code now uses workspace-based functions. The old std::fs functions
    // are kept for backward compatibility but are not tested here.

    #[test]
    fn test_delete_issues_file_for_isolation_with_workspace() {
        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent")
            .with_file(".agent/ISSUES.md", "some issues");

        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);

        delete_issues_file_for_isolation_with_workspace(&workspace, &logger).unwrap();

        assert!(
            !workspace.exists(Path::new(".agent/ISSUES.md")),
            "ISSUES.md should be deleted via workspace"
        );
    }

    #[test]
    fn test_delete_issues_file_for_isolation_with_workspace_nonexistent() {
        // File doesn't exist - should succeed silently
        let workspace = MemoryWorkspace::new_test().with_dir(".agent");

        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);

        let result = delete_issues_file_for_isolation_with_workspace(&workspace, &logger);
        assert!(result.is_ok(), "Should succeed when file doesn't exist");
    }

    #[test]
    fn test_clean_context_for_reviewer_with_workspace_non_isolation() {
        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent")
            .with_file(".agent/STATUS.md", "old status")
            .with_file(".agent/NOTES.md", "old notes")
            .with_file(".agent/ISSUES.md", "old issues")
            .with_dir(".agent/archive")
            .with_file(".agent/archive/old.txt", "archived");

        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);

        // Non-isolation mode should overwrite context files with vague content
        clean_context_for_reviewer_with_workspace(&workspace, &logger, false).unwrap();

        // Files should be overwritten with vague one-liners
        assert_eq!(
            workspace.read(Path::new(".agent/STATUS.md")).unwrap(),
            "In progress.\n"
        );
        assert_eq!(
            workspace.read(Path::new(".agent/NOTES.md")).unwrap(),
            "Notes.\n"
        );
        assert_eq!(
            workspace.read(Path::new(".agent/ISSUES.md")).unwrap(),
            "No issues recorded.\n"
        );
        // Archive directory should be removed
        assert!(
            !workspace.exists(Path::new(".agent/archive")),
            "Archive should be removed"
        );
    }

    #[test]
    fn test_clean_context_for_reviewer_with_workspace_isolation_mode() {
        let workspace = MemoryWorkspace::new_test().with_dir(".agent");

        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);

        // Isolation mode should do nothing
        clean_context_for_reviewer_with_workspace(&workspace, &logger, true).unwrap();

        // No files should be created
        assert!(
            !workspace.exists(Path::new(".agent/STATUS.md")),
            "STATUS.md should not be created in isolation mode"
        );
    }

    #[test]
    fn test_update_status_with_workspace_non_isolation() {
        let workspace = MemoryWorkspace::new_test().with_dir(".agent");

        // Non-isolation mode should write vague status
        update_status_with_workspace(&workspace, "In progress.", false).unwrap();

        let content = workspace.read(Path::new(".agent/STATUS.md")).unwrap();
        assert_eq!(content, "In progress.\n");
    }

    #[test]
    fn test_update_status_with_workspace_isolation_mode() {
        let workspace = MemoryWorkspace::new_test().with_dir(".agent");

        // Isolation mode should do nothing
        update_status_with_workspace(&workspace, "In progress.", true).unwrap();

        // STATUS.md should NOT be created
        assert!(
            !workspace.exists(Path::new(".agent/STATUS.md")),
            "STATUS.md should not be created in isolation mode"
        );
    }
}
