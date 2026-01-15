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

/// File existence state for PROMPT.md validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// File does not exist
    Missing,
    /// File exists but is empty
    Empty,
    /// File exists with content
    Present,
}

/// Result of PROMPT.md validation.
///
/// Contains flags indicating what was found and any errors or warnings.
#[derive(Debug, Clone)]
pub struct PromptValidationResult {
    /// File existence and content state
    pub file_state: FileState,
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
    /// Returns true if PROMPT.md exists.
    pub const fn exists(&self) -> bool {
        matches!(self.file_state, FileState::Present | FileState::Empty)
    }

    /// Returns true if PROMPT.md has non-empty content.
    pub const fn has_content(&self) -> bool {
        matches!(self.file_state, FileState::Present)
    }
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
            let Ok(backup_content) = fs::read_to_string(backup_path) else {
                continue;
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

/// Attempt to restore PROMPT.md from backup files.
///
/// Tries to restore from backup files in order:
/// 1. `.agent/PROMPT.md.backup`
/// 2. `.agent/PROMPT.md.backup.1`
/// 3. `.agent/PROMPT.md.backup.2`
///
/// # Returns
///
/// `Some(String)` with the backup source name if restored, `None` otherwise.
fn try_restore_from_backup(prompt_path: &Path) -> Option<String> {
    let backup_paths = [
        (Path::new(".agent/PROMPT.md.backup"), ".agent/PROMPT.md.backup"),
        (Path::new(".agent/PROMPT.md.backup.1"), ".agent/PROMPT.md.backup.1"),
        (Path::new(".agent/PROMPT.md.backup.2"), ".agent/PROMPT.md.backup.2"),
    ];

    for (backup_path, name) in backup_paths {
        if backup_path.exists() {
            let Ok(backup_content) = fs::read_to_string(backup_path) else {
                continue;
            };

            if backup_content.trim().is_empty() {
                continue;
            }

            if fs::copy(backup_path, prompt_path).is_ok() {
                return Some(name.to_string());
            }
        }
    }

    None
}

/// Check content for Goal section.
fn check_goal_section(content: &str) -> bool {
    content.contains("## Goal") || content.contains("# Goal")
}

/// Check content for Acceptance section.
fn check_acceptance_section(content: &str) -> bool {
    content.contains("## Acceptance")
        || content.contains("# Acceptance")
        || content.contains("Acceptance Criteria")
        || contains_ascii_case_insensitive(content, "acceptance")
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
    let file_exists = prompt_path.exists();
    let mut result = PromptValidationResult {
        file_state: if file_exists {
            FileState::Empty
        } else {
            FileState::Missing
        },
        has_goal: false,
        has_acceptance: false,
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    if !result.exists() {
        // Try to restore from backup
        if let Some(source) = try_restore_from_backup(prompt_path) {
            result.file_state = FileState::Empty;
            result
                .warnings
                .push(format!("PROMPT.md was missing and was automatically restored from {source}"));
        } else {
            // No backup available
            if interactive && std::io::stdout().is_terminal() {
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

    result.file_state = if content.trim().is_empty() {
        FileState::Empty
    } else {
        FileState::Present
    };

    if !result.has_content() {
        result.errors.push("PROMPT.md is empty".to_string());
        return result;
    }

    // Check for Goal section
    result.has_goal = check_goal_section(&content);
    if !result.has_goal {
        let msg = "PROMPT.md missing '## Goal' section".to_string();
        if strict {
            result.errors.push(msg);
        } else {
            result.warnings.push(msg);
        }
    }

    // Check for Acceptance section
    result.has_acceptance = check_acceptance_section(&content);
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
    use test_helpers::with_temp_cwd;

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
            assert!(!result.exists());
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
            assert!(result.exists());
            assert!(!result.has_content());
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
            assert!(result.exists());
            assert!(result.has_content());
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
            assert!(result.exists());
            assert!(result.has_content());
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
            assert!(result.exists());
            assert!(result.has_content());
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
