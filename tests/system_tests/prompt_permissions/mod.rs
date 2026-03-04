//! System tests for real filesystem PROMPT.md permission toggling.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::workspace::{Workspace, WorkspaceFs, PROMPT_MD};
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;
use test_helpers::init_git_repo;

#[test]
#[serial]
fn test_prompt_md_permission_toggle() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().expect("create temp dir");
        let _repo = init_git_repo(&temp_dir);

        let workspace = WorkspaceFs::new(temp_dir.path().to_path_buf());
        let prompt_path = temp_dir.path().join(PROMPT_MD);

        workspace
            .set_readonly(Path::new(PROMPT_MD))
            .expect("set PROMPT.md read-only");
        assert_prompt_readonly(&prompt_path);

        workspace
            .set_writable(Path::new(PROMPT_MD))
            .expect("set PROMPT.md writable");
        assert_prompt_writable(&prompt_path);
    });
}

fn assert_prompt_readonly(path: &Path) {
    let metadata = std::fs::metadata(path).expect("stat PROMPT.md");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o222, 0, "expected PROMPT.md to be read-only");
    }

    #[cfg(windows)]
    {
        assert!(
            metadata.permissions().readonly(),
            "expected PROMPT.md to be read-only"
        );
    }
}

fn assert_prompt_writable(path: &Path) {
    let metadata = std::fs::metadata(path).expect("stat PROMPT.md");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        assert_ne!(mode & 0o200, 0, "expected PROMPT.md to be writable");
    }

    #[cfg(windows)]
    {
        assert!(
            !metadata.permissions().readonly(),
            "expected PROMPT.md to be writable"
        );
    }
}
