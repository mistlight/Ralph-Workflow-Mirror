//! File I/O hardening utilities for `.agent/` files.
//!
//! This module focuses on preventing partial writes and catching obvious
//! corruption (e.g. zero-length or binary files) in small, text-based agent
//! artifacts like `PLAN.md` and `commit-message.txt`.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use tempfile::NamedTempFile;

use crate::workspace::Workspace;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Maximum reasonable file size for agent text files (10MB).
pub const MAX_AGENT_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Write file content atomically using a temp file + rename pattern.
///
/// This ensures the file is either fully written or not written at all,
/// preventing partial writes or corruption from crashes/interruptions.
///
/// Uses `tempfile::NamedTempFile` which creates secure, unpredictable
/// temporary file names to prevent symlink attacks.
///
/// # Security
///
/// On Unix systems, the temp file is created with mode 0600 (owner read/write
/// only) to prevent other users from reading sensitive content before the
/// atomic rename completes.
pub fn write_file_atomic(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create a NamedTempFile in the same directory as the target file.
    // This ensures atomic rename works (same filesystem).
    let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp_file = NamedTempFile::new_in(parent_dir)?;

    // Set restrictive permissions on temp file (0600 = owner read/write only)
    // This prevents other users from reading the temp file before rename
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(temp_file.path())?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(temp_file.path(), perms)?;
    }

    // Write content to the temp file
    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;
    temp_file.as_file().sync_all()?;

    // Persist the temp file to the target location (atomic rename)
    temp_file.persist(path)?;

    Ok(())
}

/// Write file content using the workspace abstraction.
///
/// This is the workspace-based version of `write_file_atomic`. Instead of using
/// temp file + rename (which provides atomicity on the real filesystem), this
/// uses `Workspace::write()` which:
/// - In production (`WorkspaceFs`): writes directly (parent dirs created automatically)
/// - In tests (`MemoryWorkspace`): writes to in-memory storage
///
/// Note: The atomic semantics (temp file + rename) are NOT preserved with this
/// function since `Workspace::write()` is a simple write. For production code
/// that needs atomic writes, use the original `write_file_atomic()` function.
/// This workspace version is primarily for testability.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `path` - Relative path within the workspace
/// * `content` - Content to write
pub fn write_file_atomic_with_workspace(
    workspace: &dyn Workspace,
    path: &Path,
    content: &str,
) -> io::Result<()> {
    // Ensure parent directories exist
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            workspace.create_dir_all(parent)?;
        }
    }

    workspace.write(path, content)
}

/// Validate that a file is readable UTF-8 text and within size limits.
///
/// Returns `Ok(true)` if the file appears valid, `Ok(false)` if corrupt, or `Err`
/// on access error.
pub fn verify_file_not_corrupted(path: &Path) -> io::Result<bool> {
    let metadata = fs::metadata(path)?;

    if metadata.len() == 0 || metadata.len() > MAX_AGENT_FILE_SIZE {
        return Ok(false);
    }

    let mut file = File::open(path)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    // Null bytes are a simple indicator of binary corruption.
    Ok(!buf.contains('\0'))
}

/// Validate that a file is readable UTF-8 text and within size limits using workspace.
///
/// This is the workspace-based version of `verify_file_not_corrupted`.
///
/// Returns `Ok(true)` if the file appears valid, `Ok(false)` if corrupt, or `Err`
/// on access error.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `path` - Relative path within the workspace
pub fn verify_file_not_corrupted_with_workspace(
    workspace: &dyn Workspace,
    path: &Path,
) -> io::Result<bool> {
    let content = workspace.read_bytes(path)?;

    // Check size limits
    if content.is_empty() || content.len() as u64 > MAX_AGENT_FILE_SIZE {
        return Ok(false);
    }

    // Check if valid UTF-8
    let text = match String::from_utf8(content) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    // Null bytes are a simple indicator of binary corruption.
    Ok(!text.contains('\0'))
}

/// Verify that the filesystem is ready for `.agent/` file operations.
///
/// Checks:
/// - Directory exists and is writable
/// - No obviously stale lock files (best-effort)
pub fn check_filesystem_ready(path: &Path) -> io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }

    // Check writability using a tiny temp file.
    let test_file = path.join(".write_test");
    fs::write(&test_file, b"test")?;
    fs::remove_file(&test_file)?;

    // Best-effort stale lock detection: fail only on clear cases.
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            if !name.to_ascii_lowercase().ends_with(".lock") {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        if elapsed > std::time::Duration::from_secs(3600) {
                            return Err(io::Error::other(format!("Stale lock file found: {name}")));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if a specific XML file is writable and clean up if locked.
///
/// This function performs a surgical check on critical XML files to detect
/// if they are locked by stale processes. It attempts to:
/// 1. Test writability by appending and removing a blank line
/// 2. Detect if file is locked (permission denied during write)
/// 3. Optionally force cleanup of the locked file
///
/// # Arguments
///
/// * `xml_path` - Path to the XML file to check
/// * `force_cleanup` - If true, delete the file if it's locked
///
/// # Returns
///
/// - `Ok(true)` - File is writable
/// - `Ok(false)` - File doesn't exist (not an error)
/// - `Err(...)` - File is locked or not writable
///
/// # Example
///
/// ```ignore
/// // Check if issues.xml is writable before agent runs
/// match check_xml_file_writable(Path::new(".agent/tmp/issues.xml"), false) {
///     Ok(true) => println!("File is writable"),
///     Ok(false) => println!("File doesn't exist yet"),
///     Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
///         // File is locked - attempt cleanup
///         check_xml_file_writable(Path::new(".agent/tmp/issues.xml"), true)?;
///     }
///     Err(e) => return Err(e),
/// }
/// ```
pub fn check_xml_file_writable(xml_path: &Path, force_cleanup: bool) -> io::Result<bool> {
    // If file doesn't exist, it's writable (we can create it)
    if !xml_path.exists() {
        return Ok(false);
    }

    // Try to open the file in append mode to test if it's locked
    match fs::OpenOptions::new().append(true).open(xml_path) {
        Ok(mut file) => {
            // File is writable - verify by writing and removing a blank line
            use std::io::Write;

            // Get current file size
            let original_size = file.metadata()?.len();

            // Try to append a blank line
            if let Err(e) = writeln!(file) {
                if force_cleanup {
                    drop(file); // Close file handle before deleting
                    fs::remove_file(xml_path)?;
                    return Ok(false);
                }
                return Err(e);
            }

            // Flush to ensure write succeeded
            file.flush()?;

            // Truncate back to original size (removes the blank line we added)
            file.set_len(original_size)?;

            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            // File is locked or not writable
            if force_cleanup {
                // Try to forcefully remove the locked file
                // On Windows, this may still fail if another process has it open
                fs::remove_file(xml_path)?;
                Ok(false)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "File {} is locked or not writable. This may indicate a stale process \
                         is holding the file open. Consider restarting or using force_cleanup.",
                        xml_path.display()
                    ),
                ))
            }
        }
        Err(e) => Err(e),
    }
}

/// Check if a specific XML file is writable before agent retry.
///
/// This is a convenience function meant to be called before XSD retries
/// to detect and clean up locked files from previous agent runs.
///
/// # Arguments
///
/// * `xml_path` - Path to the XML file (e.g., ".agent/tmp/issues.xml")
/// * `logger` - Logger for diagnostic messages
///
/// # Returns
///
/// `Ok(())` if file is writable or was successfully cleaned up.
/// `Err(...)` if cleanup failed.
pub fn check_and_cleanup_xml_before_retry(
    xml_path: &Path,
    logger: &crate::logger::Logger,
) -> io::Result<()> {
    // Try to detect if file is locked
    match check_xml_file_writable(xml_path, false) {
        Ok(true) => {
            // File exists and is writable - all good
            Ok(())
        }
        Ok(false) => {
            // File doesn't exist yet - all good
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            // File is locked - attempt cleanup
            logger.warn(&format!(
                "XML file {} may be locked: {}. Attempting cleanup...",
                xml_path.display(),
                e
            ));

            // Force cleanup
            match check_xml_file_writable(xml_path, true) {
                Ok(_) => {
                    logger.info(&format!(
                        "Successfully cleaned up locked file: {}",
                        xml_path.display()
                    ));
                    Ok(())
                }
                Err(cleanup_err) => {
                    logger.error(&format!(
                        "Failed to cleanup locked file {}: {}",
                        xml_path.display(),
                        cleanup_err
                    ));
                    Err(cleanup_err)
                }
            }
        }
        Err(e) => {
            // Other error
            logger.warn(&format!("Error checking {}: {}", xml_path.display(), e));
            Err(e)
        }
    }
}

/// Check and clean up all XML files in .agent/tmp/ directory.
///
/// This is useful to run before starting an agent to ensure no stale
/// XML files from previous runs are blocking operations.
///
/// # Arguments
///
/// * `tmp_dir` - Path to .agent/tmp/ directory
/// * `force_cleanup` - If true, delete locked files
///
/// # Returns
///
/// A summary of what was found and cleaned up.
pub fn cleanup_stale_xml_files(tmp_dir: &Path, force_cleanup: bool) -> io::Result<String> {
    let mut report = Vec::new();
    let mut cleaned = 0;
    let mut locked = 0;
    let mut writable = 0;

    if !tmp_dir.exists() {
        return Ok("Directory doesn't exist yet - nothing to clean".to_string());
    }

    for entry in fs::read_dir(tmp_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only check .xml files
        if path.extension().and_then(|s| s.to_str()) != Some("xml") {
            continue;
        }

        match check_xml_file_writable(&path, force_cleanup) {
            Ok(true) => {
                writable += 1;
                report.push(format!("  ✓ {} is writable", path.display()));
            }
            Ok(false) => {
                if force_cleanup {
                    cleaned += 1;
                    report.push(format!("  🗑 Removed locked file: {}", path.display()));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                locked += 1;
                report.push(format!("  ⚠ LOCKED: {} - {}", path.display(), e));
            }
            Err(e) => {
                report.push(format!("  ✗ Error checking {}: {}", path.display(), e));
            }
        }
    }

    let summary = format!(
        "XML file check complete: {} writable, {} locked, {} cleaned",
        writable, locked, cleaned
    );

    if !report.is_empty() {
        Ok(format!("{}\n{}", summary, report.join("\n")))
    } else {
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // Workspace-based tests (for testability without real filesystem)
    // =========================================================================

    #[cfg(feature = "test-utils")]
    mod workspace_tests {
        use super::*;
        use crate::workspace::{MemoryWorkspace, Workspace};
        use std::path::Path;

        #[test]
        fn test_write_file_atomic_with_workspace() {
            let workspace = MemoryWorkspace::new_test();

            write_file_atomic_with_workspace(&workspace, Path::new("test.txt"), "content").unwrap();

            assert_eq!(workspace.read(Path::new("test.txt")).unwrap(), "content");
        }

        #[test]
        fn test_write_file_atomic_with_workspace_creates_parent_dirs() {
            let workspace = MemoryWorkspace::new_test();

            write_file_atomic_with_workspace(
                &workspace,
                Path::new(".agent/tmp/output.txt"),
                "nested content",
            )
            .unwrap();

            assert!(workspace.exists(Path::new(".agent/tmp/output.txt")));
            assert_eq!(
                workspace.read(Path::new(".agent/tmp/output.txt")).unwrap(),
                "nested content"
            );
        }

        #[test]
        fn test_verify_file_not_corrupted_with_workspace_valid() {
            let workspace =
                MemoryWorkspace::new_test().with_file("valid.txt", "valid content\nwith lines");

            let result =
                verify_file_not_corrupted_with_workspace(&workspace, Path::new("valid.txt"));
            assert!(result.unwrap());
        }

        #[test]
        fn test_verify_file_not_corrupted_with_workspace_empty() {
            let workspace = MemoryWorkspace::new_test().with_file("empty.txt", "");

            let result =
                verify_file_not_corrupted_with_workspace(&workspace, Path::new("empty.txt"));
            assert!(!result.unwrap());
        }

        #[test]
        fn test_verify_file_not_corrupted_with_workspace_null_bytes() {
            let workspace =
                MemoryWorkspace::new_test().with_file_bytes("binary.txt", b"hello\x00world");

            let result =
                verify_file_not_corrupted_with_workspace(&workspace, Path::new("binary.txt"));
            assert!(!result.unwrap());
        }

        #[test]
        fn test_verify_file_not_corrupted_with_workspace_not_found() {
            let workspace = MemoryWorkspace::new_test();

            let result =
                verify_file_not_corrupted_with_workspace(&workspace, Path::new("nonexistent.txt"));
            assert!(result.is_err());
        }
    }

    // =========================================================================
    // Original tests using real filesystem (kept for backward compatibility)
    // =========================================================================

    #[test]
    fn test_write_file_atomic() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("atomic.txt");

        write_file_atomic(&file_path, "atomic content").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "atomic content");
    }

    #[test]
    fn test_verify_file_not_corrupted_valid() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("valid.txt");

        fs::write(&file_path, "valid content\nwith multiple lines").unwrap();
        assert!(verify_file_not_corrupted(&file_path).unwrap());
    }

    #[test]
    fn test_verify_file_not_corrupted_zero_size() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("empty.txt");

        fs::write(&file_path, "").unwrap();
        assert!(!verify_file_not_corrupted(&file_path).unwrap());
    }

    #[test]
    fn test_check_filesystem_ready_creates_directory() {
        let temp = TempDir::new().unwrap();
        let new_dir = temp.path().join("new_dir");

        assert!(!new_dir.exists());
        check_filesystem_ready(&new_dir).unwrap();
        assert!(new_dir.exists());
    }

    #[test]
    fn test_check_xml_file_writable_nonexistent() {
        let temp = TempDir::new().unwrap();
        let xml_path = temp.path().join("nonexistent.xml");

        // Non-existent file should return Ok(false)
        let result = check_xml_file_writable(&xml_path, false).unwrap();
        assert!(!result, "Non-existent file should return false");
    }

    #[test]
    fn test_check_xml_file_writable_valid_file() {
        let temp = TempDir::new().unwrap();
        let xml_path = temp.path().join("test.xml");
        fs::write(&xml_path, "<test>content</test>").unwrap();

        // Writable file should return Ok(true)
        let result = check_xml_file_writable(&xml_path, false).unwrap();
        assert!(result, "Writable file should return true");

        // Content should be unchanged
        let content = fs::read_to_string(&xml_path).unwrap();
        assert_eq!(content, "<test>content</test>");
    }

    #[test]
    fn test_check_xml_file_writable_preserves_content() {
        let temp = TempDir::new().unwrap();
        let xml_path = temp.path().join("preserve.xml");
        let original = "<ralph-plan><ralph-summary>Test</ralph-summary></ralph-plan>";
        fs::write(&xml_path, original).unwrap();

        // Check writability
        check_xml_file_writable(&xml_path, false).unwrap();

        // Content should be exactly the same (no extra newlines)
        let after = fs::read_to_string(&xml_path).unwrap();
        assert_eq!(after, original);
    }

    #[test]
    fn test_cleanup_stale_xml_files_empty_dir() {
        let temp = TempDir::new().unwrap();
        let tmp_dir = temp.path().join("tmp");
        fs::create_dir(&tmp_dir).unwrap();

        let report = cleanup_stale_xml_files(&tmp_dir, false).unwrap();
        assert!(report.contains("0 writable"));
    }

    #[test]
    fn test_cleanup_stale_xml_files_with_files() {
        let temp = TempDir::new().unwrap();
        let tmp_dir = temp.path().join("tmp");
        fs::create_dir(&tmp_dir).unwrap();

        // Create some XML files
        fs::write(tmp_dir.join("test1.xml"), "<test/>").unwrap();
        fs::write(tmp_dir.join("test2.xml"), "<test/>").unwrap();
        fs::write(tmp_dir.join("test.xsd"), "schema").unwrap(); // Should be ignored

        let report = cleanup_stale_xml_files(&tmp_dir, false).unwrap();
        assert!(
            report.contains("2 writable"),
            "Should find 2 writable XML files"
        );
    }

    #[test]
    fn test_cleanup_stale_xml_files_nonexistent_dir() {
        let temp = TempDir::new().unwrap();
        let tmp_dir = temp.path().join("nonexistent");

        let report = cleanup_stale_xml_files(&tmp_dir, false).unwrap();
        assert!(report.contains("doesn't exist"));
    }
}
