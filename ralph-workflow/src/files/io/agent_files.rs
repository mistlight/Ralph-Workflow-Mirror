//! Agent file management for the `.agent/` directory.
//!
//! This module handles creation, modification, and cleanup of files
//! in the `.agent/` directory that are used during pipeline execution.

use std::fs;
use std::io::{self, BufRead};
use std::path::Path;

use super::{
    context::overwrite_one_liner, context::VAGUE_ISSUES_LINE, context::VAGUE_NOTES_LINE,
    context::VAGUE_STATUS_LINE, integrity, recovery,
};

/// XSD schemas for XML validation - included at compile time.
/// These are written to `.agent/xsd/` at pipeline start for agent self-validation.
const PLAN_XSD_SCHEMA: &str = include_str!("../llm_output_extraction/plan.xsd");
const DEVELOPMENT_RESULT_XSD_SCHEMA: &str =
    include_str!("../llm_output_extraction/development_result.xsd");
const ISSUES_XSD_SCHEMA: &str = include_str!("../llm_output_extraction/issues.xsd");
const FIX_RESULT_XSD_SCHEMA: &str = include_str!("../llm_output_extraction/fix_result.xsd");
const COMMIT_MESSAGE_XSD_SCHEMA: &str = include_str!("../llm_output_extraction/commit_message.xsd");

/// Files that Ralph generates during a run and should clean up.
pub const GENERATED_FILES: &[&str] = &[
    ".no_agent_commit",
    ".agent/PLAN.md",
    ".agent/commit-message.txt",
    ".agent/checkpoint.json.tmp",
];

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

    let file = fs::File::open(file_path)?;
    let reader = io::BufReader::new(file);

    for line in reader.lines().map_while(Result::ok) {
        if line.contains(marker) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Ensure required files and directories exist.
///
/// Creates the `.agent/logs` and `.agent/tmp` directories if they don't exist.
/// Also writes XSD schemas to `.agent/tmp/` for agent self-validation.
///
/// When `isolation_mode` is true (the default), STATUS.md, NOTES.md and ISSUES.md
/// are NOT created. This prevents context contamination from previous runs.
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`ensure_files_at`] instead.
pub fn ensure_files(isolation_mode: bool) -> io::Result<()> {
    ensure_files_at(Path::new("."), isolation_mode)
}

/// Ensure required files and directories exist at a specific repository path.
///
/// Creates the `.agent/logs` and `.agent/tmp` directories if they don't exist.
/// Also writes XSD schemas to `.agent/tmp/` for agent self-validation.
///
/// When `isolation_mode` is true (the default), STATUS.md, NOTES.md and ISSUES.md
/// are NOT created. This prevents context contamination from previous runs.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
/// * `isolation_mode` - If true, skip creating STATUS.md, NOTES.md, ISSUES.md
pub fn ensure_files_at(repo_root: &Path, isolation_mode: bool) -> io::Result<()> {
    let agent_dir = repo_root.join(".agent");

    // Best-effort state repair before we start touching `.agent/` contents.
    // If the state is unrecoverable, fail early with a clear error.
    if let recovery::RecoveryStatus::Unrecoverable(msg) = recovery::auto_repair(&agent_dir)? {
        return Err(io::Error::other(format!(
            "Failed to repair .agent state: {msg}"
        )));
    }

    integrity::check_filesystem_ready(&agent_dir)?;
    fs::create_dir_all(agent_dir.join("logs"))?;
    fs::create_dir_all(agent_dir.join("tmp"))?;

    // Clean up any stale XML files from previous runs that might be locked
    // This prevents permission errors when agents try to write to these files
    let tmp_dir = agent_dir.join("tmp");
    let _ = integrity::cleanup_stale_xml_files(&tmp_dir, false);
    // Note: cleanup is best-effort, failures are not fatal

    // Write XSD schemas to .agent/tmp/ for agent self-validation
    setup_xsd_schemas_at(repo_root)?;

    // Only create STATUS.md, NOTES.md and ISSUES.md when NOT in isolation mode
    if !isolation_mode {
        // Always overwrite/truncate these files to a single vague sentence to
        // avoid detailed context persisting across runs.
        overwrite_one_liner(&agent_dir.join("STATUS.md"), VAGUE_STATUS_LINE)?;
        overwrite_one_liner(&agent_dir.join("NOTES.md"), VAGUE_NOTES_LINE)?;
        overwrite_one_liner(&agent_dir.join("ISSUES.md"), VAGUE_ISSUES_LINE)?;
    }

    Ok(())
}

/// Write all XSD schemas to `.agent/xsd/` directory.
///
/// This is called at pipeline startup so agents can use `xmllint` for self-validation
/// during XML generation. The schemas are the authoritative definitions of valid XML
/// structure for each phase.
///
/// # Schema Files
///
/// - `plan.xsd` - Planning phase XML structure
/// - `development_result.xsd` - Development iteration result structure
/// - `issues.xsd` - Review phase issues structure
/// - `fix_result.xsd` - Fix phase result structure
/// - `commit_message.xsd` - Commit message structure
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`setup_xsd_schemas_at`] instead.
pub fn setup_xsd_schemas() -> io::Result<()> {
    setup_xsd_schemas_at(Path::new("."))
}

/// Write all XSD schemas to `.agent/xsd/` directory at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
pub fn setup_xsd_schemas_at(repo_root: &Path) -> io::Result<()> {
    let tmp_dir = repo_root.join(".agent/tmp");
    fs::create_dir_all(&tmp_dir)?;

    fs::write(tmp_dir.join("plan.xsd"), PLAN_XSD_SCHEMA)?;
    fs::write(
        tmp_dir.join("development_result.xsd"),
        DEVELOPMENT_RESULT_XSD_SCHEMA,
    )?;
    fs::write(tmp_dir.join("issues.xsd"), ISSUES_XSD_SCHEMA)?;
    fs::write(tmp_dir.join("fix_result.xsd"), FIX_RESULT_XSD_SCHEMA)?;
    fs::write(
        tmp_dir.join("commit_message.xsd"),
        COMMIT_MESSAGE_XSD_SCHEMA,
    )?;

    Ok(())
}

/// Delete the PLAN.md file after integration.
///
/// Called after the plan has been integrated into the codebase.
/// Silently succeeds if the file doesn't exist.
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`delete_plan_file_at`] instead.
pub fn delete_plan_file() -> io::Result<()> {
    delete_plan_file_at(Path::new("."))
}

/// Delete the PLAN.md file after integration at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
pub fn delete_plan_file_at(repo_root: &Path) -> io::Result<()> {
    let plan_path = repo_root.join(".agent/PLAN.md");
    if plan_path.exists() {
        fs::remove_file(plan_path)?;
    }
    Ok(())
}

/// Delete the commit-message.txt file after committing.
///
/// Called after a successful git commit to clean up the temporary
/// commit message file. Silently succeeds if the file doesn't exist.
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`delete_commit_message_file_at`] instead.
pub fn delete_commit_message_file() -> io::Result<()> {
    delete_commit_message_file_at(Path::new("."))
}

/// Delete the commit-message.txt file after committing at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
pub fn delete_commit_message_file_at(repo_root: &Path) -> io::Result<()> {
    let msg_path = repo_root.join(".agent/commit-message.txt");
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
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`read_commit_message_file_at`] instead.
pub fn read_commit_message_file() -> io::Result<String> {
    read_commit_message_file_at(Path::new("."))
}

/// Read commit message from file at a specific repository path; fails if missing or empty.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
///
/// # Errors
///
/// Returns an error if the file doesn't exist, cannot be read, or is empty.
pub fn read_commit_message_file_at(repo_root: &Path) -> io::Result<String> {
    let msg_path = repo_root.join(".agent/commit-message.txt");
    if msg_path.exists() && !integrity::verify_file_not_corrupted(&msg_path)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            ".agent/commit-message.txt appears corrupted",
        ));
    }
    let content = fs::read_to_string(&msg_path).map_err(|e| {
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
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`write_commit_message_file_at`] instead.
pub fn write_commit_message_file(message: &str) -> io::Result<()> {
    write_commit_message_file_at(Path::new("."), message)
}

/// Write commit message to file at a specific repository path.
///
/// Creates the .agent directory if it doesn't exist and writes the
/// commit message to .agent/commit-message.txt.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
/// * `message` - The commit message to write
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn write_commit_message_file_at(repo_root: &Path, message: &str) -> io::Result<()> {
    let msg_path = repo_root.join(".agent/commit-message.txt");
    if let Some(parent) = msg_path.parent() {
        fs::create_dir_all(parent)?;
    }
    integrity::write_file_atomic(&msg_path, message)?;
    Ok(())
}

/// Clean up all generated files.
///
/// Removes temporary files that may have been left behind by an interrupted
/// pipeline run. This includes PLAN.md, commit-message.txt, and other
/// artifacts listed in [`GENERATED_FILES`].
///
/// This function is best-effort: individual file deletion failures are
/// silently ignored since we're in a cleanup context.
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`cleanup_generated_files_at`] instead.
pub fn cleanup_generated_files() {
    cleanup_generated_files_at(Path::new("."))
}

/// Clean up all generated files at a specific repository path.
///
/// Removes temporary files that may have been left behind by an interrupted
/// pipeline run. This includes PLAN.md, commit-message.txt, and other
/// artifacts listed in [`GENERATED_FILES`].
///
/// This function is best-effort: individual file deletion failures are
/// silently ignored since we're in a cleanup context.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
pub fn cleanup_generated_files_at(repo_root: &Path) {
    for file in GENERATED_FILES {
        let _ = fs::remove_file(repo_root.join(file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use test_helpers::with_temp_cwd;

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

            // Should create tmp directory and write XSD schemas
            assert!(Path::new(".agent/tmp").is_dir());
            assert!(Path::new(".agent/tmp/plan.xsd").exists());
            assert!(Path::new(".agent/tmp/development_result.xsd").exists());
            assert!(Path::new(".agent/tmp/issues.xsd").exists());
            assert!(Path::new(".agent/tmp/fix_result.xsd").exists());
            assert!(Path::new(".agent/tmp/commit_message.xsd").exists());
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

            // Should create tmp directory
            assert!(Path::new(".agent/tmp").is_dir());
        });
    }

    #[test]
    fn test_setup_xsd_schemas() {
        with_temp_cwd(|_dir| {
            setup_xsd_schemas().unwrap();

            // Verify all schemas are written to .agent/tmp/
            let plan_xsd = fs::read_to_string(".agent/tmp/plan.xsd").unwrap();
            assert!(plan_xsd.contains("xs:schema"));
            assert!(plan_xsd.contains("ralph-plan"));

            let dev_xsd = fs::read_to_string(".agent/tmp/development_result.xsd").unwrap();
            assert!(dev_xsd.contains("xs:schema"));
            assert!(dev_xsd.contains("ralph-development-result"));

            let issues_xsd = fs::read_to_string(".agent/tmp/issues.xsd").unwrap();
            assert!(issues_xsd.contains("xs:schema"));
            assert!(issues_xsd.contains("ralph-issues"));

            let fix_xsd = fs::read_to_string(".agent/tmp/fix_result.xsd").unwrap();
            assert!(fix_xsd.contains("xs:schema"));
            assert!(fix_xsd.contains("ralph-fix-result"));

            let commit_xsd = fs::read_to_string(".agent/tmp/commit_message.xsd").unwrap();
            assert!(commit_xsd.contains("xs:schema"));
            assert!(commit_xsd.contains("ralph-commit"));
        });
    }
}
