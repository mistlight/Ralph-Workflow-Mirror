//! System tests for WorkspaceFs permission methods.
//!
//! Tests real filesystem permission changes using WorkspaceFs trait implementation.
//! These tests verify platform-specific behavior (Unix mode bits / Windows readonly attribute).

use ralph_workflow::workspace::Workspace;
use ralph_workflow::workspace::WorkspaceFs;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Test that set_readonly actually removes write permissions on real filesystem.
#[test]
fn test_set_readonly_removes_write_permissions() {
    super::super::test_timeout::with_default_timeout(|| {
        // Given: Temp directory with a writable file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        fs::write(&file_path, "test content").unwrap();

        // Verify file starts writable
        let initial_metadata = fs::metadata(&file_path).unwrap();
        let initial_perms = initial_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = initial_perms.mode();
            assert_ne!(
                mode & 0o200,
                0,
                "File should start with owner write bit set"
            );
        }

        #[cfg(windows)]
        {
            assert!(!initial_perms.readonly(), "File should start writable");
        }

        // When: Call set_readonly via WorkspaceFs
        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());
        workspace
            .set_readonly(Path::new("test_file.txt"))
            .expect("set_readonly should succeed");

        // Then: File should be read-only
        let readonly_metadata = fs::metadata(&file_path).unwrap();
        let readonly_perms = readonly_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = readonly_perms.mode();
            // Unix: should set mode to 0o444 (read-only for all)
            assert_eq!(
                mode & 0o777,
                0o444,
                "Expected mode 0o444 (read-only), got 0o{:o}",
                mode & 0o777
            );
            // Verify all write bits are cleared
            assert_eq!(mode & 0o222, 0, "All write bits should be cleared");
        }

        #[cfg(windows)]
        {
            assert!(
                readonly_perms.readonly(),
                "File should have readonly attribute set"
            );
        }

        // Additional verification: attempt to write should fail
        let write_result = fs::OpenOptions::new().write(true).open(&file_path);
        assert!(
            write_result.is_err(),
            "Writing to read-only file should fail"
        );
    });
}

/// Test that set_writable restores write permissions on real filesystem.
#[test]
fn test_set_writable_restores_write_permissions() {
    super::super::test_timeout::with_default_timeout(|| {
        // Given: Temp directory with a read-only file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        fs::write(&file_path, "test content").unwrap();

        // Make it read-only first
        let metadata = fs::metadata(&file_path).unwrap();
        let mut perms = metadata.permissions();

        #[cfg(unix)]
        {
            perms.set_mode(0o444);
        }

        #[cfg(windows)]
        {
            perms.set_readonly(true);
        }

        fs::set_permissions(&file_path, perms).unwrap();

        // Verify file is read-only
        let readonly_metadata = fs::metadata(&file_path).unwrap();
        let readonly_perms = readonly_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = readonly_perms.mode();
            assert_eq!(mode & 0o222, 0, "File should be read-only");
        }

        #[cfg(windows)]
        {
            assert!(readonly_perms.readonly(), "File should be read-only");
        }

        // When: Call set_writable via WorkspaceFs
        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());
        workspace
            .set_writable(Path::new("test_file.txt"))
            .expect("set_writable should succeed");

        // Then: File should be writable
        let writable_metadata = fs::metadata(&file_path).unwrap();
        let writable_perms = writable_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = writable_perms.mode();
            // Unix: should set mode to 0o644 (rw-r--r--)
            assert_eq!(
                mode & 0o777,
                0o644,
                "Expected mode 0o644 (rw-r--r--), got 0o{:o}",
                mode & 0o777
            );
            // Verify owner write bit is set
            assert_ne!(mode & 0o200, 0, "Owner write bit should be set");
        }

        #[cfg(windows)]
        {
            assert!(
                !writable_perms.readonly(),
                "File should not have readonly attribute"
            );
        }

        // Additional verification: attempt to write should succeed
        fs::write(&file_path, "modified content").expect("Writing should succeed");
    });
}

/// Test that set_readonly on missing file is a no-op (does not error).
#[test]
fn test_set_readonly_missing_file_noop() {
    super::super::test_timeout::with_default_timeout(|| {
        // Given: Temp directory without the target file
        let temp_dir = TempDir::new().unwrap();
        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());

        // When: Call set_readonly on non-existent file
        let result = workspace.set_readonly(Path::new("missing_file.txt"));

        // Then: Should succeed (no-op)
        assert!(
            result.is_ok(),
            "set_readonly on missing file should be no-op"
        );
    });
}

/// Test that set_writable on missing file is a no-op (does not error).
#[test]
fn test_set_writable_missing_file_noop() {
    super::super::test_timeout::with_default_timeout(|| {
        // Given: Temp directory without the target file
        let temp_dir = TempDir::new().unwrap();
        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());

        // When: Call set_writable on non-existent file
        let result = workspace.set_writable(Path::new("missing_file.txt"));

        // Then: Should succeed (no-op)
        assert!(
            result.is_ok(),
            "set_writable on missing file should be no-op"
        );
    });
}

/// Test complete lock/restore cycle matches expected behavior.
#[test]
fn test_permission_cycle_lock_and_restore() {
    super::super::test_timeout::with_default_timeout(|| {
        // Given: Temp directory with PROMPT.md file (simulating real use case)
        let temp_dir = TempDir::new().unwrap();
        let prompt_path = temp_dir.path().join("PROMPT.md");
        fs::write(&prompt_path, "# User Prompt\n\nTest task").unwrap();

        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());

        // When: Lock permissions (simulating pipeline startup)
        workspace
            .set_readonly(Path::new("PROMPT.md"))
            .expect("Lock should succeed");

        // Then: File should be read-only
        let locked_metadata = fs::metadata(&prompt_path).unwrap();
        let locked_perms = locked_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = locked_perms.mode();
            assert_eq!(
                mode & 0o222,
                0,
                "PROMPT.md should be read-only (no write bits)"
            );
        }

        #[cfg(windows)]
        {
            assert!(
                locked_perms.readonly(),
                "PROMPT.md should have readonly attribute"
            );
        }

        // When: Restore permissions (simulating pipeline termination)
        workspace
            .set_writable(Path::new("PROMPT.md"))
            .expect("Restore should succeed");

        // Then: File should be writable again
        let restored_metadata = fs::metadata(&prompt_path).unwrap();
        let restored_perms = restored_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = restored_perms.mode();
            assert_ne!(
                mode & 0o200,
                0,
                "PROMPT.md should be writable (owner write bit set)"
            );
        }

        #[cfg(windows)]
        {
            assert!(
                !restored_perms.readonly(),
                "PROMPT.md should not have readonly attribute"
            );
        }

        // Final verification: can modify the file
        fs::write(&prompt_path, "# Modified Prompt\n\nUpdated task")
            .expect("Should be able to write to restored file");
    });
}
