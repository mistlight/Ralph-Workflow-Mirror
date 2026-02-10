//! System tests for PROMPT.md permission lifecycle using real filesystem.
//!
//! These tests verify that lock/restore helpers mutate permissions as expected
//! when using WorkspaceFs on a real filesystem.

use ralph_workflow::files::{
    make_prompt_read_only_with_workspace, make_prompt_writable_with_workspace,
};
use ralph_workflow::workspace::WorkspaceFs;
use std::fs;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn test_prompt_permissions_lock_and_restore_real_filesystem() {
    crate::test_timeout::with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let prompt_path = temp_dir.path().join("PROMPT.md");
        fs::write(&prompt_path, "# Prompt\n\nTest task").unwrap();

        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());

        let _warning = make_prompt_read_only_with_workspace(&workspace);

        let locked_metadata = fs::metadata(&prompt_path).unwrap();
        let locked_perms = locked_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = locked_perms.mode();
            assert_eq!(
                mode & 0o777,
                0o444,
                "Expected PROMPT.md to be read-only (0o444), got 0o{:o}",
                mode & 0o777
            );
            assert_eq!(mode & 0o222, 0, "All write bits should be cleared");
        }

        #[cfg(windows)]
        {
            assert!(locked_perms.readonly(), "PROMPT.md should be read-only");
        }

        let _warning = make_prompt_writable_with_workspace(&workspace);

        let restored_metadata = fs::metadata(&prompt_path).unwrap();
        let restored_perms = restored_metadata.permissions();

        #[cfg(unix)]
        {
            let mode = restored_perms.mode();
            assert_eq!(
                mode & 0o777,
                0o644,
                "Expected PROMPT.md to be writable (0o644), got 0o{:o}",
                mode & 0o777
            );
            assert_ne!(mode & 0o200, 0, "Owner write bit should be set");
        }

        #[cfg(windows)]
        {
            assert!(!restored_perms.readonly(), "PROMPT.md should be writable");
        }

        fs::write(&prompt_path, "# Updated Prompt\n\nUpdated task")
            .expect("Should be able to write after restore");
    });
}
