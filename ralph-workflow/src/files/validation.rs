//! PROMPT.md validation utilities.
//!
//! Validates the structure and content of PROMPT.md files to ensure
//! they have the required sections for the pipeline to work effectively.

use std::fs;
use std::io::IsTerminal;
use std::path::Path;

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }

    let needle = needle.as_bytes();
    for window in haystack.as_bytes().windows(needle.len()) {
        if window
            .iter()
            .zip(needle.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return true;
        }
    }
    false
}

/// Result of PROMPT.md validation.
///
/// Contains flags indicating what was found and any errors or warnings.
#[derive(Debug, Clone)]
pub struct PromptValidationResult {
    /// Whether PROMPT.md exists
    pub exists: bool,
    /// Whether PROMPT.md has non-empty content
    pub has_content: bool,
    /// Whether a Goal section was found
    pub has_goal: bool,
    /// Whether an Acceptance section was found
    pub has_acceptance: bool,
    /// List of warnings (non-blocking issues)
    pub warnings: Vec<String>,
    /// List of errors (blocking issues)
    pub errors: Vec<String>,
}

impl PromptValidationResult {
    /// Returns true if validation passed (no errors).
    pub const fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns true if validation passed with no warnings.
    pub const fn is_perfect(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }
}

/// Restore PROMPT.md from backup if missing or empty.
///
/// This is a lightweight periodic check called during pipeline execution
/// to detect and recover from accidental PROMPT.md deletion by agents.
/// Unlike `validate_prompt_md()`, this function only checks for file
/// existence and non-empty content - it doesn't validate structure.
///
/// # Auto-Restore
///
/// If PROMPT.md is missing or empty but a backup exists, the backup is
/// automatically copied to PROMPT.md. Tries backups in order:
/// - `.agent/PROMPT.md.backup`
/// - `.agent/PROMPT.md.backup.1`
/// - `.agent/PROMPT.md.backup.2`
///
/// # Returns
///
/// - `Ok(true)` - File exists and has content (no action needed)
/// - `Ok(false)` - File was restored from backup
/// - `Err` - File missing/empty and no valid backup available
pub fn restore_prompt_if_needed() -> anyhow::Result<bool> {
    let prompt_path = Path::new("PROMPT.md");

    // Check if PROMPT.md exists and has content
    let prompt_ok = prompt_path
        .exists()
        .then(|| fs::read_to_string(prompt_path).ok())
        .flatten()
        .is_some_and(|s| !s.trim().is_empty());

    if prompt_ok {
        return Ok(true);
    }

    // PROMPT.md is missing or empty - try to restore from backup chain
    let backup_paths = [
        Path::new(".agent/PROMPT.md.backup"),
        Path::new(".agent/PROMPT.md.backup.1"),
        Path::new(".agent/PROMPT.md.backup.2"),
    ];

    for backup_path in &backup_paths {
        if backup_path.exists() {
            // Verify backup has content
            let backup_content = match fs::read_to_string(backup_path) {
                Ok(c) => c,
                Err(_) => continue, // Try next backup
            };

            if backup_content.trim().is_empty() {
                continue; // Try next backup
            }

            // Restore from backup
            fs::write(prompt_path, backup_content)?;

            // Set read-only permissions on restored file (best-effort)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(prompt_path) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o444);
                    let _ = fs::set_permissions(prompt_path, perms);
                }
            }

            #[cfg(windows)]
            {
                if let Ok(metadata) = fs::metadata(prompt_path) {
                    let mut perms = metadata.permissions();
                    perms.set_readonly(true);
                    let _ = fs::set_permissions(prompt_path, perms);
                }
            }

            return Ok(false);
        }
    }

    // No valid backup available
    anyhow::bail!(
        "PROMPT.md is missing/empty and no valid backup available (tried .agent/PROMPT.md.backup, .agent/PROMPT.md.backup.1, .agent/PROMPT.md.backup.2)"
    );
}

/// Validate PROMPT.md structure and content.
///
/// Checks for:
/// - File existence and non-empty content (auto-restores from backup if missing)
/// - Goal section (## Goal or # Goal)
/// - Acceptance section (## Acceptance, Acceptance Criteria, or acceptance)
///
/// # Auto-Restore
///
/// If PROMPT.md is missing but `.agent/PROMPT.md.backup` exists, the backup is
/// automatically copied to PROMPT.md. This prevents accidental deletion by agents.
///
/// # Arguments
///
/// * `strict` - In strict mode, missing sections are errors; otherwise they're warnings.
/// * `interactive` - If true and PROMPT.md doesn't exist, prompt to create from template.
///   Also requires stdout to be a terminal for interactive prompts.
///
/// # Returns
///
/// A `PromptValidationResult` containing validation findings.
pub fn validate_prompt_md(strict: bool, interactive: bool) -> PromptValidationResult {
    let prompt_path = Path::new("PROMPT.md");
    let mut result = PromptValidationResult {
        exists: prompt_path.exists(),
        has_content: false,
        has_goal: false,
        has_acceptance: false,
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    if !result.exists {
        // Auto-restore from backup with fallback chain
        // Try backup, backup.1, backup.2 in order
        let backup_paths = [
            Path::new(".agent/PROMPT.md.backup"),
            Path::new(".agent/PROMPT.md.backup.1"),
            Path::new(".agent/PROMPT.md.backup.2"),
        ];

        let mut restored = false;
        let mut backup_used = None;

        for (idx, backup_path) in backup_paths.iter().enumerate() {
            if backup_path.exists() {
                // Check if backup has content before restoring
                let backup_content = match fs::read_to_string(backup_path) {
                    Ok(c) => c,
                    Err(_) => continue, // Try next backup
                };

                if backup_content.trim().is_empty() {
                    continue; // Try next backup
                }

                match fs::copy(backup_path, prompt_path) {
                    Ok(_) => {
                        result.exists = true;
                        restored = true;
                        backup_used = Some(match idx {
                            0 => ".agent/PROMPT.md.backup",
                            1 => ".agent/PROMPT.md.backup.1",
                            2 => ".agent/PROMPT.md.backup.2",
                            _ => "unknown",
                        });
                        break;
                    }
                    Err(_) => {
                        // Try next backup
                        continue;
                    }
                }
            }
        }

        if restored {
            if let Some(source) = backup_used {
                result.warnings.push(format!(
                    "PROMPT.md was missing and was automatically restored from {source}"
                ));
            }
        } else {
            if interactive && std::io::stdout().is_terminal() {
                // Interactive mode: the caller should have already prompted
                // We just return an error result so they can handle it
                result.errors.push(
                    "PROMPT.md not found. Use 'ralph --init-prompt <template>' to create one."
                        .to_string(),
                );
            } else {
                result.errors.push(
                    "PROMPT.md not found. Run 'ralph --list-templates' to see available templates, \
                     then 'ralph --init-prompt <template>' to create one."
                        .to_string(),
                );
            }
            return result;
        }
    }

    let content = match fs::read_to_string(prompt_path) {
        Ok(c) => c,
        Err(e) => {
            result.errors.push(format!("Failed to read PROMPT.md: {e}"));
            return result;
        }
    };

    result.has_content = !content.trim().is_empty();
    if !result.has_content {
        result.errors.push("PROMPT.md is empty".to_string());
        return result;
    }

    // Check for Goal section
    result.has_goal = content.contains("## Goal") || content.contains("# Goal");
    if !result.has_goal {
        let msg = "PROMPT.md missing '## Goal' section".to_string();
        if strict {
            result.errors.push(msg);
        } else {
            result.warnings.push(msg);
        }
    }

    // Check for Acceptance section
    result.has_acceptance = content.contains("## Acceptance")
        || content.contains("# Acceptance")
        || content.contains("Acceptance Criteria")
        || contains_ascii_case_insensitive(&content, "acceptance");
    if !result.has_acceptance {
        let msg = "PROMPT.md missing acceptance checks section".to_string();
        if strict {
            result.errors.push(msg);
        } else {
            result.warnings.push(msg);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Use the shared CWD lock from test-helpers to ensure CWD-modifying tests
    // from different modules don't interfere with each other.
    use test_helpers::CWD_LOCK;

    /// RAII guard to restore the working directory on drop.
    struct DirGuard(PathBuf);

    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    /// Run a test function in a temporary directory.
    fn with_temp_cwd<F: FnOnce(&TempDir)>(f: F) {
        let lock = CWD_LOCK.get_or_init(|| Mutex::new(()));

        // Clear poison if a previous test panicked
        let _cwd_guard = match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let dir = TempDir::new().expect("Failed to create temp directory");
        let old_dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
        std::env::set_current_dir(dir.path()).expect("Failed to change to temp directory");
        let _guard = DirGuard(old_dir);

        f(&dir);
    }

    #[test]
    fn test_restore_prompt_if_needed_ok() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "# Test\n\nContent").unwrap();
            assert!(restore_prompt_if_needed().unwrap());
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_missing() {
        with_temp_cwd(|_dir| {
            // No PROMPT.md, no backup
            let result = restore_prompt_if_needed();
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("no valid backup available"));
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_restores_from_backup() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/PROMPT.md.backup", "# Restored\n\nContent").unwrap();

            // File is missing, should restore from backup
            let was_restored = restore_prompt_if_needed().unwrap();
            assert!(!was_restored);

            // Verify PROMPT.md exists with backup content
            let content = fs::read_to_string("PROMPT.md").unwrap();
            assert_eq!(content, "# Restored\n\nContent");
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_empty_file() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write("PROMPT.md", "").unwrap();
            fs::write(".agent/PROMPT.md.backup", "# Restored\n\nContent").unwrap();

            // File is empty, should restore from backup
            let was_restored = restore_prompt_if_needed().unwrap();
            assert!(!was_restored);

            // Verify PROMPT.md has backup content
            let content = fs::read_to_string("PROMPT.md").unwrap();
            assert_eq!(content, "# Restored\n\nContent");
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_empty_backup() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/PROMPT.md.backup", "").unwrap();

            // Backup is empty, should fail
            let result = restore_prompt_if_needed();
            assert!(result.is_err());
            // Error should mention no valid backup (since empty backup is skipped)
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("no valid backup available"));
        });
    }

    #[test]
    fn test_validate_prompt_md_not_exists() {
        with_temp_cwd(|_dir| {
            let result = validate_prompt_md(false, false);
            assert!(!result.exists);
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("not found")));
            // Verify template suggestion is included
            assert!(result
                .errors
                .iter()
                .any(|e| e.contains("--list-templates") || e.contains("--init-prompt")));
        });
    }

    #[test]
    fn test_validate_prompt_md_empty() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "   \n\n  ").unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.exists);
            assert!(!result.has_content);
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("empty")));
        });
    }

    #[test]
    fn test_validate_prompt_md_complete() {
        with_temp_cwd(|_dir| {
            fs::write(
                "PROMPT.md",
                "# PROMPT

## Goal
Build a feature

## Acceptance
- Tests pass
",
            )
            .unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(result.has_goal);
            assert!(result.has_acceptance);
            assert!(result.is_valid());
            assert!(result.is_perfect());
        });
    }

    #[test]
    fn test_validate_prompt_md_missing_sections_lenient() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "Just some random content").unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(!result.has_goal);
            assert!(!result.has_acceptance);
            // In lenient mode, missing sections are warnings, not errors
            assert!(result.is_valid());
            assert!(!result.is_perfect());
            assert_eq!(result.warnings.len(), 2);
        });
    }

    #[test]
    fn test_validate_prompt_md_missing_sections_strict() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "Just some random content").unwrap();
            let result = validate_prompt_md(true, false);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(!result.has_goal);
            assert!(!result.has_acceptance);
            // In strict mode, missing sections are errors
            assert!(!result.is_valid());
            assert_eq!(result.errors.len(), 2);
        });
    }

    #[test]
    fn test_validate_prompt_md_acceptance_variations() {
        with_temp_cwd(|_dir| {
            // Test "Acceptance Criteria" variant
            fs::write(
                "PROMPT.md",
                "## Goal
Test

## Acceptance Criteria
- Pass
",
            )
            .unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.has_acceptance);

            // Test lowercase "acceptance" variant
            fs::write(
                "PROMPT.md",
                "## Goal
Test

The acceptance tests should pass.
",
            )
            .unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.has_acceptance);
        });
    }
}
