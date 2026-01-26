//! Minimal recovery mechanisms for `.agent/` state.
//!
//! Ralph uses `.agent/` as a working directory. If it contains corrupted
//! artifacts (e.g. non-UTF8 files from interrupted writes), we attempt a small
//! set of best-effort repairs so the pipeline can proceed.

use std::fs;
use std::io;
use std::path::Path;

use crate::workspace::Workspace;

/// Status of a recovery operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// No recovery needed - state is valid.
    Valid,
    /// Recovery was performed successfully.
    Recovered,
    /// Recovery failed - state is unrecoverable.
    Unrecoverable(String),
}

#[derive(Debug, Clone)]
pub struct StateValidation {
    pub is_valid: bool,
    pub issues: Vec<String>,
}

// =============================================================================
// Workspace-based implementation (primary, for pipeline layer)
// =============================================================================

fn validate_agent_state_with_workspace(
    workspace: &dyn Workspace,
    agent_dir: &Path,
) -> io::Result<StateValidation> {
    let mut issues = Vec::new();

    if !workspace.exists(agent_dir) {
        return Ok(StateValidation {
            is_valid: false,
            issues: vec![".agent/ directory does not exist".to_string()],
        });
    }

    // Detect unreadable files in the `.agent/` directory.
    if let Ok(entries) = workspace.read_dir(agent_dir) {
        for entry in entries {
            let path = entry.path();
            if !entry.is_file() {
                continue;
            }
            if workspace.read(path).is_err() {
                issues.push(format!("Corrupted file: {}", path.display()));
            }
        }
    }

    // Zero-length files are a common interruption artifact.
    for filename in [
        "PLAN.md",
        "ISSUES.md",
        "STATUS.md",
        "NOTES.md",
        "commit-message.txt",
    ] {
        let file_path = agent_dir.join(filename);
        if !workspace.exists(&file_path) {
            continue;
        }
        if let Ok(content) = workspace.read(&file_path) {
            if content.is_empty() {
                issues.push(format!("Zero-length file: {filename}"));
            }
        }
    }

    Ok(StateValidation {
        is_valid: issues.is_empty(),
        issues,
    })
}

fn remove_zero_length_files_with_workspace(
    workspace: &dyn Workspace,
    agent_dir: &Path,
) -> io::Result<usize> {
    let mut removed = 0;

    for filename in [
        "PLAN.md",
        "ISSUES.md",
        "STATUS.md",
        "NOTES.md",
        "commit-message.txt",
    ] {
        let file_path = agent_dir.join(filename);
        if !workspace.exists(&file_path) {
            continue;
        }
        if let Ok(content) = workspace.read(&file_path) {
            if content.is_empty() {
                workspace.remove(&file_path)?;
                removed += 1;
            }
        }
    }

    Ok(removed)
}

/// Best-effort repair of common `.agent/` state issues using workspace.
///
/// This is the workspace-based version for pipeline layer usage.
pub fn auto_repair_with_workspace(
    workspace: &dyn Workspace,
    agent_dir: &Path,
) -> io::Result<RecoveryStatus> {
    if !workspace.exists(agent_dir) {
        workspace.create_dir_all(&agent_dir.join("logs"))?;
        return Ok(RecoveryStatus::Recovered);
    }

    let validation = validate_agent_state_with_workspace(workspace, agent_dir)?;
    if validation.is_valid {
        workspace.create_dir_all(&agent_dir.join("logs"))?;
        return Ok(RecoveryStatus::Valid);
    }

    // Attempt repairs.
    remove_zero_length_files_with_workspace(workspace, agent_dir)?;
    workspace.create_dir_all(&agent_dir.join("logs"))?;

    let post_validation = validate_agent_state_with_workspace(workspace, agent_dir)?;
    if post_validation.is_valid {
        Ok(RecoveryStatus::Recovered)
    } else {
        Ok(RecoveryStatus::Unrecoverable(format!(
            "Unresolved .agent issues: {}",
            post_validation.issues.join(", ")
        )))
    }
}

// =============================================================================
// std::fs wrapper (for CLI/AppEffect layer only)
// =============================================================================

fn validate_agent_state(agent_dir: &Path) -> io::Result<StateValidation> {
    let mut issues = Vec::new();

    if !agent_dir.exists() {
        return Ok(StateValidation {
            is_valid: false,
            issues: vec![".agent/ directory does not exist".to_string()],
        });
    }

    if let Ok(entries) = fs::read_dir(agent_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if fs::read_to_string(&path).is_err() {
                issues.push(format!("Corrupted file: {}", path.display()));
            }
        }
    }

    for filename in [
        "PLAN.md",
        "ISSUES.md",
        "STATUS.md",
        "NOTES.md",
        "commit-message.txt",
    ] {
        let file_path = agent_dir.join(filename);
        if !file_path.exists() {
            continue;
        }
        let metadata = fs::metadata(&file_path)?;
        if metadata.len() == 0 {
            issues.push(format!("Zero-length file: {filename}"));
        }
    }

    Ok(StateValidation {
        is_valid: issues.is_empty(),
        issues,
    })
}

fn remove_corrupted_files(agent_dir: &Path) -> io::Result<usize> {
    let mut removed = 0;

    let Ok(entries) = fs::read_dir(agent_dir) else {
        return Ok(0);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if fs::read_to_string(&path).is_err() {
            fs::remove_file(&path)?;
            removed += 1;
        }
    }

    Ok(removed)
}

fn remove_zero_length_files(agent_dir: &Path) -> io::Result<usize> {
    let mut removed = 0;

    for filename in [
        "PLAN.md",
        "ISSUES.md",
        "STATUS.md",
        "NOTES.md",
        "commit-message.txt",
    ] {
        let file_path = agent_dir.join(filename);
        if !file_path.exists() {
            continue;
        }
        let metadata = fs::metadata(&file_path)?;
        if metadata.len() == 0 {
            fs::remove_file(&file_path)?;
            removed += 1;
        }
    }

    Ok(removed)
}

/// Best-effort repair of common `.agent/` state issues.
///
/// # Security
///
/// This function canonicalizes the input path to prevent path traversal attacks.
pub fn auto_repair(agent_dir: &Path) -> io::Result<RecoveryStatus> {
    let agent_dir = agent_dir
        .canonicalize()
        .unwrap_or_else(|_| agent_dir.to_path_buf());

    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel_path) = agent_dir.strip_prefix(&cwd) {
            let rel_str = rel_path.to_string_lossy();
            if rel_str.starts_with("..") || rel_str.contains("/..") || rel_str.contains("\\..") {
                return Ok(RecoveryStatus::Unrecoverable(
                    "Invalid agent directory: path escapes current directory".to_string(),
                ));
            }
        }
    }

    if !agent_dir.exists() {
        fs::create_dir_all(agent_dir.join("logs"))?;
        return Ok(RecoveryStatus::Recovered);
    }

    let validation = validate_agent_state(&agent_dir)?;
    if validation.is_valid {
        fs::create_dir_all(agent_dir.join("logs"))?;
        return Ok(RecoveryStatus::Valid);
    }

    remove_corrupted_files(&agent_dir)?;
    remove_zero_length_files(&agent_dir)?;
    fs::create_dir_all(agent_dir.join("logs"))?;

    let post_validation = validate_agent_state(&agent_dir)?;
    if post_validation.is_valid {
        Ok(RecoveryStatus::Recovered)
    } else {
        Ok(RecoveryStatus::Unrecoverable(format!(
            "Unresolved .agent issues: {}",
            post_validation.issues.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;
    use std::path::Path;

    #[test]
    fn auto_repair_with_workspace_creates_missing_directory() {
        let workspace = MemoryWorkspace::new_test();
        let agent_dir = Path::new(".agent");

        let status = auto_repair_with_workspace(&workspace, agent_dir).unwrap();

        assert_eq!(status, RecoveryStatus::Recovered);
        assert!(workspace.exists(&agent_dir.join("logs")));
    }

    #[test]
    fn auto_repair_with_workspace_removes_zero_length_files() {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/logs/.keep", "")
            .with_file(".agent/PLAN.md", ""); // Empty file

        let agent_dir = Path::new(".agent");
        let status = auto_repair_with_workspace(&workspace, agent_dir).unwrap();

        assert_eq!(status, RecoveryStatus::Recovered);
        assert!(!workspace.exists(&agent_dir.join("PLAN.md")));
    }

    #[test]
    fn auto_repair_with_workspace_valid_state() {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/logs/.keep", "")
            .with_file(".agent/PLAN.md", "# Plan\nSome content");

        let agent_dir = Path::new(".agent");
        let status = auto_repair_with_workspace(&workspace, agent_dir).unwrap();

        assert_eq!(status, RecoveryStatus::Valid);
        assert!(workspace.exists(&agent_dir.join("PLAN.md")));
    }

    #[test]
    fn auto_repair_with_workspace_multiple_zero_length_files() {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/logs/.keep", "")
            .with_file(".agent/PLAN.md", "")
            .with_file(".agent/ISSUES.md", "")
            .with_file(".agent/STATUS.md", "valid content");

        let agent_dir = Path::new(".agent");
        let status = auto_repair_with_workspace(&workspace, agent_dir).unwrap();

        assert_eq!(status, RecoveryStatus::Recovered);
        assert!(!workspace.exists(&agent_dir.join("PLAN.md")));
        assert!(!workspace.exists(&agent_dir.join("ISSUES.md")));
        assert!(workspace.exists(&agent_dir.join("STATUS.md")));
    }
}
