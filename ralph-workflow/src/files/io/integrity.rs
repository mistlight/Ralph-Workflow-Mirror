//! File I/O hardening utilities for `.agent/` files.
//!
//! This module focuses on preventing partial writes and catching obvious
//! corruption (e.g. zero-length or binary files) in small, text-based agent
//! artifacts like `PLAN.md` and `commit-message.txt`.

use std::io;
use std::path::Path;

use crate::workspace::Workspace;

/// Maximum reasonable file size for agent text files (10MB).
pub const MAX_AGENT_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Write file content atomically using the workspace abstraction.
///
/// This delegates to `Workspace::write_atomic()` which:
/// - In production (`WorkspaceFs`): Uses temp file + rename for true atomicity
/// - In tests (`MemoryWorkspace`): Simple write (in-memory is inherently atomic)
///
/// This ensures the file is either fully written or not written at all,
/// preventing partial writes or corruption from crashes/interruptions.
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
    workspace.write_atomic(path, content)
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
///
/// Verify that the filesystem is ready for `.agent/` file operations using workspace.
///
/// This is the workspace-based version of `check_filesystem_ready`.
///
/// Checks:
/// - Directory exists and is writable (creates if needed)
/// - No obviously stale lock files (best-effort)
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `path` - Relative path within the workspace to check
pub fn check_filesystem_ready_with_workspace(
    workspace: &dyn Workspace,
    path: &Path,
) -> io::Result<()> {
    // Create directory if it doesn't exist
    if !workspace.is_dir(path) {
        workspace.create_dir_all(path)?;
    }

    // Check writability using a tiny temp file
    let test_file = path.join(".write_test");
    workspace.write(&test_file, "test")?;
    workspace.remove(&test_file)?;

    // Best-effort stale lock detection: fail only on clear cases.
    if let Ok(entries) = workspace.read_dir(path) {
        for entry in entries {
            let Some(name) = entry.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.to_ascii_lowercase().ends_with(".lock") {
                continue;
            }

            // Check modification time
            if let Some(modified) = entry.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    if elapsed > std::time::Duration::from_secs(3600) {
                        return Err(io::Error::other(format!("Stale lock file found: {name}")));
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
/// Check if XML file is writable using workspace abstraction.
///
/// Note: In `MemoryWorkspace`, files are always considered writable since there's
/// no concept of file locking. This function is primarily for testability.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `xml_path` - Relative path to the XML file
/// * `force_cleanup` - If true, delete the file
///
/// # Returns
///
/// - `Ok(true)` - File exists and is writable
/// - `Ok(false)` - File doesn't exist (not an error)
/// - `Err(...)` - File access error
pub fn check_xml_file_writable_with_workspace(
    workspace: &dyn Workspace,
    xml_path: &Path,
    force_cleanup: bool,
) -> io::Result<bool> {
    // If file doesn't exist, it's writable (we can create it)
    if !workspace.exists(xml_path) {
        return Ok(false);
    }

    if force_cleanup {
        workspace.remove(xml_path)?;
        return Ok(false);
    }

    // In workspace context, files are always writable
    // (MemoryWorkspace doesn't track file locks)
    Ok(true)
}

/// Check if a specific XML file is writable before agent retry using workspace.
///
/// This function detects and cleans up locked files from previous agent runs.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `xml_path` - Relative path to the XML file
/// * `logger` - Logger for diagnostic messages
///
/// # Returns
///
/// `Ok(())` if file is writable or was successfully cleaned up.
/// `Err(...)` if cleanup failed.
pub fn check_and_cleanup_xml_before_retry_with_workspace(
    workspace: &dyn Workspace,
    xml_path: &Path,
    logger: &crate::logger::Logger,
) -> io::Result<()> {
    match check_xml_file_writable_with_workspace(workspace, xml_path, false) {
        Ok(true) | Ok(false) => Ok(()),
        Err(e) => {
            logger.warn(&format!(
                "XML file {} error: {}. Attempting cleanup...",
                xml_path.display(),
                e
            ));

            match check_xml_file_writable_with_workspace(workspace, xml_path, true) {
                Ok(_) => {
                    logger.info(&format!(
                        "Successfully cleaned up file: {}",
                        xml_path.display()
                    ));
                    Ok(())
                }
                Err(cleanup_err) => {
                    logger.error(&format!(
                        "Failed to cleanup file {}: {}",
                        xml_path.display(),
                        cleanup_err
                    ));
                    Err(cleanup_err)
                }
            }
        }
    }
}

/// Check and clean up all XML files in .agent/tmp/ directory using workspace.
///
/// This is the workspace-based version of `cleanup_stale_xml_files`.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `tmp_dir` - Relative path to .agent/tmp/ directory
/// * `force_cleanup` - If true, delete files
///
/// # Returns
///
/// A summary of what was found and cleaned up.
pub fn cleanup_stale_xml_files_with_workspace(
    workspace: &dyn Workspace,
    tmp_dir: &Path,
    force_cleanup: bool,
) -> io::Result<String> {
    let mut report = Vec::new();
    let mut cleaned = 0;
    let mut writable = 0;

    if !workspace.is_dir(tmp_dir) {
        return Ok("Directory doesn't exist yet - nothing to clean".to_string());
    }

    let entries = workspace.read_dir(tmp_dir)?;
    for entry in entries {
        let path = entry.path();

        // Only check .xml files
        let extension = path.extension().and_then(|s| s.to_str());
        if extension != Some("xml") {
            continue;
        }

        if force_cleanup {
            // Remove the file
            if workspace.exists(path) {
                workspace.remove(path)?;
                cleaned += 1;
                report.push(format!("  🗑 Removed file: {}", path.display()));
            }
        } else {
            // Just count it as writable (in memory workspace, everything is writable)
            writable += 1;
            report.push(format!("  ✓ {} is writable", path.display()));
        }
    }

    let summary = format!(
        "XML file check complete: {} writable, {} locked, {} cleaned",
        writable, 0, cleaned
    );

    if !report.is_empty() {
        Ok(format!("{}\n{}", summary, report.join("\n")))
    } else {
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    // =========================================================================
    // Workspace-based tests (for testability without real filesystem)
    // =========================================================================

    #[cfg(feature = "test-utils")]
    mod workspace_tests {
        use super::super::*;
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

        // =====================================================================
        // Tests for check_filesystem_ready_with_workspace
        // =====================================================================

        #[test]
        fn test_check_filesystem_ready_with_workspace_creates_dir() {
            let workspace = MemoryWorkspace::new_test();

            // Directory doesn't exist yet
            assert!(!workspace.is_dir(Path::new(".agent")));

            // Should create the directory and succeed
            check_filesystem_ready_with_workspace(&workspace, Path::new(".agent")).unwrap();

            assert!(workspace.is_dir(Path::new(".agent")));
        }

        #[test]
        fn test_check_filesystem_ready_with_workspace_existing_dir() {
            let workspace = MemoryWorkspace::new_test().with_dir(".agent");

            // Should succeed on existing directory
            check_filesystem_ready_with_workspace(&workspace, Path::new(".agent")).unwrap();
        }

        #[test]
        fn test_check_filesystem_ready_with_workspace_detects_stale_lock() {
            use std::time::{Duration, SystemTime};

            // Create a workspace with a lock file that has an old modification time
            let old_time = SystemTime::now() - Duration::from_secs(7200); // 2 hours ago
            let workspace = MemoryWorkspace::new_test()
                .with_dir(".agent")
                .with_file_at_time(".agent/pipeline.lock", "locked", old_time);

            let result = check_filesystem_ready_with_workspace(&workspace, Path::new(".agent"));
            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("Stale lock file"));
        }

        #[test]
        fn test_check_filesystem_ready_with_workspace_ignores_fresh_lock() {
            // Create a workspace with a fresh lock file
            let workspace = MemoryWorkspace::new_test()
                .with_dir(".agent")
                .with_file(".agent/pipeline.lock", "locked");

            // Fresh lock files should not cause an error
            check_filesystem_ready_with_workspace(&workspace, Path::new(".agent")).unwrap();
        }

        // =====================================================================
        // Tests for cleanup_stale_xml_files_with_workspace
        // =====================================================================

        #[test]
        fn test_cleanup_stale_xml_files_with_workspace_nonexistent() {
            let workspace = MemoryWorkspace::new_test();

            let report =
                cleanup_stale_xml_files_with_workspace(&workspace, Path::new(".agent/tmp"), false)
                    .unwrap();
            assert!(report.contains("doesn't exist"));
        }

        #[test]
        fn test_cleanup_stale_xml_files_with_workspace_empty_dir() {
            let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");

            let report =
                cleanup_stale_xml_files_with_workspace(&workspace, Path::new(".agent/tmp"), false)
                    .unwrap();
            assert!(report.contains("0 writable"));
        }

        #[test]
        fn test_cleanup_stale_xml_files_with_workspace_finds_xml() {
            let workspace = MemoryWorkspace::new_test()
                .with_file(".agent/tmp/issues.xml", "<issues/>")
                .with_file(".agent/tmp/plan.xml", "<plan/>")
                .with_file(".agent/tmp/plan.xsd", "schema"); // XSD should be ignored

            let report =
                cleanup_stale_xml_files_with_workspace(&workspace, Path::new(".agent/tmp"), false)
                    .unwrap();
            assert!(
                report.contains("2 writable"),
                "Should find 2 XML files, got: {}",
                report
            );
        }

        #[test]
        fn test_cleanup_stale_xml_files_with_workspace_force_cleanup() {
            let workspace = MemoryWorkspace::new_test()
                .with_file(".agent/tmp/issues.xml", "<issues/>")
                .with_file(".agent/tmp/plan.xml", "<plan/>");

            // With force_cleanup=true, files should be removed
            let report =
                cleanup_stale_xml_files_with_workspace(&workspace, Path::new(".agent/tmp"), true)
                    .unwrap();

            // Files should be removed
            assert!(!workspace.exists(Path::new(".agent/tmp/issues.xml")));
            assert!(!workspace.exists(Path::new(".agent/tmp/plan.xml")));
            assert!(report.contains("cleaned") || report.contains("Removed"));
        }
    }
}
