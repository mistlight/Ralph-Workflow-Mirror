//! System tests for file protection: PROMPT.md backup/restore security and
//! CWD-based validation.
//!
//! Tests requiring real filesystem (symlinks, hardlinks, CWD-relative I/O) live here.
//! Workspace-based variants (using `MemoryWorkspace`) remain in src/ unit tests.

use ralph_workflow::files::protection::monitoring::PromptMonitor;
use ralph_workflow::files::protection::validation::{restore_prompt_if_needed, validate_prompt_md};
use serial_test::serial;
use std::fs;
use test_helpers::with_temp_cwd;

// ── Monitoring: backup/restore security (unix only) ──────────────────────────

#[cfg(unix)]
#[test]
#[serial]
fn test_restore_from_backup_does_not_follow_prompt_symlink() {
    use std::os::unix::fs as unix_fs;

    with_temp_cwd(|_dir| {
        fs::create_dir_all(".agent").expect("create .agent dir");
        fs::write(".agent/PROMPT.md.backup", "SAFE\n").expect("write backup");

        fs::write("victim.txt", "SECRET\n").expect("write victim");
        unix_fs::symlink("victim.txt", "PROMPT.md").expect("create PROMPT.md symlink");

        let before = fs::read_to_string("victim.txt").expect("read victim");
        assert!(before.contains("SECRET"));

        let restored = PromptMonitor::restore_from_backup();
        assert!(restored, "expected restore to succeed from backup");

        let after = fs::read_to_string("victim.txt").expect("read victim");
        assert_eq!(after, before, "restore must not overwrite symlink target");

        let meta = fs::symlink_metadata("PROMPT.md").expect("stat PROMPT.md");
        assert!(meta.is_file(), "PROMPT.md should be a regular file");
        let prompt = fs::read_to_string("PROMPT.md").expect("read PROMPT.md");
        assert!(prompt.contains("SAFE"));
    });
}

#[cfg(unix)]
#[test]
#[serial]
fn test_restore_from_backup_rejects_symlink_backup_file() {
    use std::os::unix::fs as unix_fs;

    with_temp_cwd(|_dir| {
        fs::create_dir_all(".agent").expect("create .agent dir");
        fs::write("source.txt", "MALICIOUS\n").expect("write source");
        unix_fs::symlink("source.txt", ".agent/PROMPT.md.backup").expect("create backup symlink");

        fs::write("PROMPT.md", "ORIGINAL\n").expect("write prompt");

        let restored = PromptMonitor::restore_from_backup();
        assert!(!restored, "expected restore to skip symlink backups");
        let prompt = fs::read_to_string("PROMPT.md").expect("read PROMPT.md");
        assert!(prompt.contains("ORIGINAL"));
    });
}

#[cfg(unix)]
#[test]
#[serial]
fn test_restore_from_backup_rejects_hardlinked_backup_file() {
    with_temp_cwd(|_dir| {
        fs::create_dir_all(".agent").expect("create .agent dir");
        fs::write("victim.txt", "SECRET\n").expect("write victim");
        fs::hard_link("victim.txt", ".agent/PROMPT.md.backup").expect("create hardlink backup");

        let restored = PromptMonitor::restore_from_backup();
        assert!(!restored, "expected restore to skip hardlinked backups");
        assert!(
            !std::path::Path::new("PROMPT.md").exists(),
            "PROMPT.md should not be created"
        );
    });
}

// ── Validation: restore_prompt_if_needed ────────────────────────────────────

#[test]
#[serial]
fn test_restore_prompt_if_needed_ok() {
    with_temp_cwd(|_dir| {
        fs::write("PROMPT.md", "# Test\n\nContent").expect("write PROMPT.md");
        let result = restore_prompt_if_needed().expect("restore ok");
        assert!(result, "returns true when PROMPT.md exists and has content");
    });
}

#[test]
#[serial]
fn test_restore_prompt_if_needed_missing() {
    with_temp_cwd(|_dir| {
        // No PROMPT.md, no backup — should error
        let result = restore_prompt_if_needed();
        assert!(result.is_err(), "errors when no file and no backup");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no valid backup available"));
    });
}

#[test]
#[serial]
fn test_restore_prompt_if_needed_restores_from_backup() {
    with_temp_cwd(|_dir| {
        fs::create_dir_all(".agent").expect("create .agent");
        fs::write(".agent/PROMPT.md.backup", "## Goal\nRestored").expect("write backup");

        let result = restore_prompt_if_needed().expect("restore ok");
        assert!(
            !result,
            "returns false when PROMPT.md was restored from backup"
        );

        let content = fs::read_to_string("PROMPT.md").expect("read PROMPT.md");
        assert!(content.contains("Restored"));
    });
}

#[test]
#[serial]
fn test_restore_prompt_if_needed_empty_file() {
    with_temp_cwd(|_dir| {
        fs::write("PROMPT.md", "").expect("write empty PROMPT.md");
        fs::create_dir_all(".agent").expect("create .agent");
        fs::write(".agent/PROMPT.md.backup", "backup content").expect("write backup");

        let result = restore_prompt_if_needed().expect("restore ok");
        assert!(
            !result,
            "returns false when PROMPT.md is empty and restored from backup"
        );
    });
}

#[test]
#[serial]
fn test_restore_prompt_if_needed_empty_backup() {
    with_temp_cwd(|_dir| {
        fs::create_dir_all(".agent").expect("create .agent");
        fs::write(".agent/PROMPT.md.backup", "").expect("write empty backup");

        let result = restore_prompt_if_needed();
        assert!(
            result.is_err(),
            "errors when only backup available is empty"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no valid backup available"));
    });
}

// ── Validation: validate_prompt_md ───────────────────────────────────────────

#[test]
#[serial]
fn test_validate_prompt_md_not_exists() {
    with_temp_cwd(|_dir| {
        let result = validate_prompt_md(false, false);
        assert!(!result.exists());
        assert!(!result.is_valid());
    });
}

#[test]
#[serial]
fn test_validate_prompt_md_empty() {
    with_temp_cwd(|_dir| {
        fs::write("PROMPT.md", "   \n\n  ").expect("write PROMPT.md");
        let result = validate_prompt_md(false, false);
        assert!(result.exists());
        assert!(!result.is_valid());
    });
}

#[test]
#[serial]
fn test_validate_prompt_md_complete() {
    with_temp_cwd(|_dir| {
        fs::write(
            "PROMPT.md",
            "# PROMPT\n\n## Goal\nBuild a feature\n\n## Acceptance\n- Tests pass\n",
        )
        .expect("write PROMPT.md");
        let result = validate_prompt_md(false, false);
        assert!(result.exists());
        assert!(result.has_content());
        assert!(result.has_goal);
        assert!(result.has_acceptance);
        assert!(result.is_valid());
    });
}

#[test]
#[serial]
fn test_validate_prompt_md_missing_sections_lenient() {
    with_temp_cwd(|_dir| {
        fs::write("PROMPT.md", "Just some random content").expect("write PROMPT.md");
        let result = validate_prompt_md(false, false);
        assert!(result.exists());
        assert!(result.has_content());
        assert!(!result.has_goal);
        assert!(!result.has_acceptance);
        // In lenient mode, missing sections are warnings, not errors
        assert!(result.is_valid());
    });
}

#[test]
#[serial]
fn test_validate_prompt_md_missing_sections_strict() {
    with_temp_cwd(|_dir| {
        fs::write("PROMPT.md", "Just some random content").expect("write PROMPT.md");
        let result = validate_prompt_md(true, false);
        assert!(result.exists());
        assert!(!result.is_valid());
    });
}

#[test]
#[serial]
fn test_validate_prompt_md_acceptance_variations() {
    with_temp_cwd(|_dir| {
        // "Acceptance Criteria" variant
        fs::write(
            "PROMPT.md",
            "## Goal\nTest\n\n## Acceptance Criteria\n- Pass\n",
        )
        .expect("write PROMPT.md");
        let result = validate_prompt_md(false, false);
        assert!(
            result.has_acceptance,
            "should recognise 'Acceptance Criteria' header"
        );
    });
}
