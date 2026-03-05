//! System tests for CWD-dependent rebase checkpoint I/O.

use ralph_workflow::git_helpers::rebase_checkpoint::{
    clear_rebase_checkpoint, load_rebase_checkpoint, rebase_checkpoint_exists,
    save_rebase_checkpoint, RebaseCheckpoint, RebasePhase,
};
use serial_test::serial;
use test_helpers::with_temp_cwd;

#[test]
#[serial]
fn test_save_load_rebase_checkpoint() {
    with_temp_cwd(|_dir| {
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_phase(RebasePhase::ConflictDetected)
            .with_conflicted_file("file1.rs".to_string())
            .with_conflicted_file("file2.rs".to_string());

        save_rebase_checkpoint(&checkpoint).unwrap();
        assert!(rebase_checkpoint_exists());

        let loaded = load_rebase_checkpoint()
            .unwrap()
            .expect("checkpoint should exist after save");
        assert_eq!(loaded.phase, RebasePhase::ConflictDetected);
        assert_eq!(loaded.upstream_branch, "main");
        assert_eq!(
            loaded.conflicted_files.len(),
            2,
            "Should have two conflicted files"
        );
        assert!(
            loaded.conflicted_files.contains(&"file1.rs".to_string()),
            "Should contain file1.rs"
        );
        assert!(
            loaded.conflicted_files.contains(&"file2.rs".to_string()),
            "Should contain file2.rs"
        );
    });
}

#[test]
#[serial]
fn test_clear_rebase_checkpoint() {
    with_temp_cwd(|_dir| {
        let checkpoint = RebaseCheckpoint::new("main".to_string());
        save_rebase_checkpoint(&checkpoint).unwrap();
        assert!(rebase_checkpoint_exists());

        clear_rebase_checkpoint().unwrap();
        assert!(!rebase_checkpoint_exists());
    });
}

#[test]
#[serial]
fn test_load_nonexistent_rebase_checkpoint() {
    with_temp_cwd(|_dir| {
        let result = load_rebase_checkpoint().unwrap();
        assert!(result.is_none());
        assert!(!rebase_checkpoint_exists());
    });
}

#[test]
#[serial]
fn test_atomic_checkpoint_write() {
    with_temp_cwd(|_dir| {
        // Write checkpoint atomically and verify it survives a second overwrite.
        let checkpoint1 =
            RebaseCheckpoint::new("main".to_string()).with_phase(RebasePhase::RebaseInProgress);
        save_rebase_checkpoint(&checkpoint1).unwrap();

        let checkpoint2 = RebaseCheckpoint::new("main".to_string())
            .with_phase(RebasePhase::ConflictDetected)
            .with_conflicted_file("modified.rs".to_string());
        save_rebase_checkpoint(&checkpoint2).unwrap();

        let loaded = load_rebase_checkpoint().unwrap().expect("should exist");
        assert_eq!(loaded.phase, RebasePhase::ConflictDetected);
        assert!(loaded.conflicted_files.contains(&"modified.rs".to_string()));
    });
}
