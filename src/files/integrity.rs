//! File I/O hardening utilities for `.agent/` files.
//!
//! This module focuses on preventing partial writes and catching obvious
//! corruption (e.g. zero-length or binary files) in small, text-based agent
//! artifacts like `PLAN.md` and `commit-message.txt`.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use tempfile::NamedTempFile;

/// Maximum reasonable file size for agent text files (10MB).
pub const MAX_AGENT_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Write file content atomically using a temp file + rename pattern.
///
/// This ensures the file is either fully written or not written at all,
/// preventing partial writes or corruption from crashes/interruptions.
///
/// Uses `tempfile::NamedTempFile` which creates secure, unpredictable
/// temporary file names to prevent symlink attacks.
pub fn write_file_atomic(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create a NamedTempFile in the same directory as the target file.
    // This ensures atomic rename works (same filesystem).
    let parent_dir = path.parent().unwrap_or(Path::new("."));
    let mut temp_file = NamedTempFile::new_in(parent_dir)?;

    // Write content to the temp file
    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;
    temp_file.as_file().sync_all()?;

    // Persist the temp file to the target location (atomic rename)
    temp_file.persist(path)?;

    Ok(())
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
            if !name.ends_with(".lock") {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        if elapsed > std::time::Duration::from_secs(3600) {
                            return Err(io::Error::other(format!(
                                "Stale lock file found: {}",
                                name
                            )));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
}
