//! Backup management for PROMPT.md.
//!
//! This module handles creation and rotation of PROMPT.md backups to protect
//! against accidental deletion or modification.

use std::fs;
use std::io;
use std::path::Path;

use super::integrity;

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
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`create_prompt_backup_at`] instead.
pub fn create_prompt_backup() -> io::Result<Option<String>> {
    create_prompt_backup_at(Path::new("."))
}

/// Create a backup of PROMPT.md at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
///
/// # Returns
///
/// Returns `io::Result<Option<String>>` where:
/// - `Ok(None)` - backup created and read-only set successfully
/// - `Ok(Some(warning))` - backup created but read-only couldn't be set
/// - `Err(e)` - if the backup cannot be created
pub fn create_prompt_backup_at(repo_root: &Path) -> io::Result<Option<String>> {
    let prompt_path = repo_root.join("PROMPT.md");

    // If PROMPT.md doesn't exist, that's fine - nothing to backup
    if !prompt_path.exists() {
        return Ok(None);
    }

    // Ensure .agent directory exists
    let agent_dir = repo_root.join(".agent");
    let backup_base = agent_dir.join("PROMPT.md.backup");
    fs::create_dir_all(&agent_dir)?;

    // Read PROMPT.md content
    let content = fs::read_to_string(&prompt_path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Failed to read PROMPT.md for backup: {e}"),
        )
    })?;

    // Backup rotation: .2 → deleted, .1 → .2, .backup → .1
    let backup_2 = agent_dir.join("PROMPT.md.backup.2");
    let backup_1 = agent_dir.join("PROMPT.md.backup.1");

    // Delete oldest backup if it exists
    let _ = fs::remove_file(&backup_2);

    // Rotate .1 → .2
    if backup_1.exists() {
        let _ = fs::rename(&backup_1, &backup_2);
    }

    // Rotate .backup → .1
    if backup_base.exists() {
        let _ = fs::rename(&backup_base, &backup_1);
    }

    // Write new backup atomically
    integrity::write_file_atomic(&backup_base, &content)
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
                        format!("Failed to set read-only on {}", path.display()),
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
    for backup_path in [&backup_base, &backup_1, &backup_2] {
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
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`make_prompt_read_only_at`] instead.
pub fn make_prompt_read_only() -> Option<String> {
    make_prompt_read_only_at(Path::new("."))
}

/// Make PROMPT.md read-only at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
///
/// # Returns
///
/// Returns `Option<String>` where:
/// - `None` - permissions set successfully or file doesn't exist
/// - `Some(warning)` - couldn't set read-only permissions
pub fn make_prompt_read_only_at(repo_root: &Path) -> Option<String> {
    let prompt_path = repo_root.join("PROMPT.md");

    // If PROMPT.md doesn't exist, that's fine - nothing to protect
    if !prompt_path.exists() {
        return None;
    }

    // Try to set read-only permissions and track any failure
    let mut readonly_warning = None;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match fs::metadata(&prompt_path) {
            Ok(metadata) => {
                let mut perms = metadata.permissions();
                perms.set_mode(0o444); // Read-only for all
                if fs::set_permissions(&prompt_path, perms).is_err() {
                    readonly_warning =
                        Some("Failed to set read-only permissions on PROMPT.md".to_string());
                }
            }
            Err(_) => {
                readonly_warning = Some("Failed to read PROMPT.md metadata".to_string());
            }
        }
    }

    #[cfg(windows)]
    {
        match fs::metadata(&prompt_path) {
            Ok(metadata) => {
                let mut perms = metadata.permissions();
                perms.set_readonly(true);
                if fs::set_permissions(&prompt_path, perms).is_err() {
                    readonly_warning =
                        Some("Failed to set read-only permissions on PROMPT.md".to_string());
                }
            }
            Err(_) => {
                readonly_warning = Some("Failed to read PROMPT.md metadata".to_string());
            }
        }
    }

    readonly_warning
}

/// Make PROMPT.md writable again after pipeline completion.
///
/// This function restores write permissions on PROMPT.md after Ralph exits.
/// It reverses the `make_prompt_read_only` operation so users can edit the file
/// normally when Ralph is not running.
///
/// # Behavior
///
/// - If PROMPT.md doesn't exist, returns `None` (nothing to modify)
/// - Uses platform-specific permission setting
/// - Returns a warning string if setting permissions fails (best-effort)
///
/// # Returns
///
/// Returns `Option<String>` where:
/// - `None` - permissions restored successfully or file doesn't exist
/// - `Some(warning)` - couldn't restore write permissions
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`make_prompt_writable_at`] instead.
pub fn make_prompt_writable() -> Option<String> {
    make_prompt_writable_at(Path::new("."))
}

/// Make PROMPT.md writable again at a specific repository path.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
///
/// # Returns
///
/// Returns `Option<String>` where:
/// - `None` - permissions restored successfully or file doesn't exist
/// - `Some(warning)` - couldn't restore write permissions
pub fn make_prompt_writable_at(repo_root: &Path) -> Option<String> {
    let prompt_path = repo_root.join("PROMPT.md");

    // If PROMPT.md doesn't exist, that's fine - nothing to modify
    if !prompt_path.exists() {
        return None;
    }

    // Try to restore write permissions and track any failure
    let mut writable_warning = None;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match fs::metadata(&prompt_path) {
            Ok(metadata) => {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644); // Owner read-write, group/others read-only
                if fs::set_permissions(&prompt_path, perms).is_err() {
                    writable_warning =
                        Some("Failed to set write permissions on PROMPT.md".to_string());
                }
            }
            Err(_) => {
                writable_warning = Some("Failed to read PROMPT.md metadata".to_string());
            }
        }
    }

    #[cfg(windows)]
    {
        match fs::metadata(&prompt_path) {
            Ok(metadata) => {
                let mut perms = metadata.permissions();
                perms.set_readonly(false);
                if fs::set_permissions(&prompt_path, perms).is_err() {
                    writable_warning =
                        Some("Failed to set write permissions on PROMPT.md".to_string());
                }
            }
            Err(_) => {
                writable_warning = Some("Failed to read PROMPT.md metadata".to_string());
            }
        }
    }

    writable_warning
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::with_temp_cwd;

    #[test]
    fn test_create_prompt_backup_creates_file() {
        with_temp_cwd(|_dir| {
            // Create a PROMPT.md
            fs::write("PROMPT.md", "# Test Prompt\n\nThis is a test prompt.").unwrap();

            // Create backup
            create_prompt_backup().unwrap();

            // Verify backup exists
            assert!(std::path::Path::new(".agent/PROMPT.md.backup").exists());

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
            assert!(!std::path::Path::new("PROMPT.md").exists());

            // Should succeed without error
            create_prompt_backup().unwrap();

            // Backup should not be created
            assert!(!std::path::Path::new(".agent/PROMPT.md.backup").exists());
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
            assert!(std::path::Path::new(".agent/PROMPT.md.backup").exists());
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
            assert!(std::path::Path::new(".agent/PROMPT.md.backup").exists());
            assert_eq!(
                fs::read_to_string(".agent/PROMPT.md.backup").unwrap(),
                "# Version 1\n"
            );
            assert!(!std::path::Path::new(".agent/PROMPT.md.backup.1").exists());

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
            assert!(!std::path::Path::new(".agent/PROMPT.md.backup.2").exists());

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
            assert!(std::path::Path::new(".agent/PROMPT.md.backup").exists());
            assert!(std::path::Path::new(".agent/PROMPT.md.backup.1").exists());
            assert!(std::path::Path::new(".agent/PROMPT.md.backup.2").exists());

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
            assert!(!std::path::Path::new("PROMPT.md").exists());

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

    #[test]
    fn test_make_prompt_writable_restores_permissions() {
        with_temp_cwd(|_dir| {
            // Create a PROMPT.md
            fs::write("PROMPT.md", "# Test Prompt\n\nThis is a test prompt.").unwrap();

            // Make it read-only
            make_prompt_read_only();

            // Now make it writable again
            make_prompt_writable();

            // On Unix, verify permissions are writable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata("PROMPT.md").unwrap();
                let perms = metadata.permissions();
                let mode = perms.mode();
                // Check that owner write bit is set (0o644 = rw-r--r--)
                assert_eq!(mode & 0o777, 0o644);
            }

            // On Windows, verify readonly flag is not set
            #[cfg(windows)]
            {
                let metadata = fs::metadata("PROMPT.md").unwrap();
                let perms = metadata.permissions();
                assert!(!perms.readonly());
            }

            // Verify we can write to the file
            fs::write("PROMPT.md", "# Updated content").unwrap();
            let content = fs::read_to_string("PROMPT.md").unwrap();
            assert_eq!(content, "# Updated content");
        });
    }

    #[test]
    fn test_make_prompt_writable_handles_missing_prompt() {
        with_temp_cwd(|_dir| {
            // No PROMPT.md exists
            assert!(!std::path::Path::new("PROMPT.md").exists());

            // Should succeed without error
            let result = make_prompt_writable();
            assert!(result.is_none());
        });
    }
}
