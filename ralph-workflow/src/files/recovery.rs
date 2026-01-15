//! Minimal recovery mechanisms for `.agent/` state.
//!
//! Ralph uses `.agent/` as a working directory. If it contains corrupted
//! artifacts (e.g. non-UTF8 files from interrupted writes), we attempt a small
//! set of best-effort repairs so the pipeline can proceed.

#![expect(clippy::unnecessary_debug_formatting)]
use std::fs;
use std::io;
use std::path::Path;

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

fn validate_agent_state(agent_dir: &Path) -> io::Result<StateValidation> {
    let mut issues = Vec::new();

    if !agent_dir.exists() {
        return Ok(StateValidation {
            is_valid: false,
            issues: vec![".agent/ directory does not exist".to_string()],
        });
    }

    // Detect unreadable (non-UTF8) files in the top-level `.agent/` directory.
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

    // Zero-length files are a common interruption artifact.
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
/// While Ralph is a developer tool where `agent_dir` is typically constructed
/// internally (not from untrusted input), we still validate the path as a
/// defense-in-depth measure.
pub fn auto_repair(agent_dir: &Path) -> io::Result<RecoveryStatus> {
    // Canonicalize the path to resolve any ".." or symlinks
    let agent_dir = agent_dir
        .canonicalize()
        .unwrap_or_else(|_| agent_dir.to_path_buf());

    // Additional safety check: ensure we're not escaping the current directory
    // This is a defense-in-depth measure; in normal Ralph usage this shouldn't trigger.
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel_path) = agent_dir.strip_prefix(&cwd) {
            // Check if the relative path starts with ".." which would indicate escaping
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

    // Attempt repairs.
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
    use tempfile::TempDir;

    #[test]
    fn auto_repair_creates_missing_directory() {
        let temp = TempDir::new().unwrap();
        let agent_dir = temp.path().join(".agent");

        let status = auto_repair(&agent_dir).unwrap();
        assert_eq!(status, RecoveryStatus::Recovered);
        assert!(agent_dir.join("logs").exists());
    }

    #[test]
    fn auto_repair_removes_zero_length_files() {
        let temp = TempDir::new().unwrap();
        let agent_dir = temp.path().join(".agent");
        fs::create_dir_all(agent_dir.join("logs")).unwrap();
        fs::write(agent_dir.join("PLAN.md"), "").unwrap();

        let status = auto_repair(&agent_dir).unwrap();
        assert_eq!(status, RecoveryStatus::Recovered);
        assert!(!agent_dir.join("PLAN.md").exists());
    }
}
