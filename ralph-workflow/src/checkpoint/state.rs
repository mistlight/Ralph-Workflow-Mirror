//! Pipeline checkpoint state and persistence.
//!
//! This module contains the checkpoint data structures and file operations
//! for saving and loading pipeline state.

#![allow(dead_code)]

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

/// Default directory for Ralph's internal files.
const AGENT_DIR: &str = ".agent";

/// Default checkpoint file name.
const CHECKPOINT_FILE: &str = "checkpoint.json";

/// Get the checkpoint file path.
///
/// By default, the checkpoint is stored in `.agent/checkpoint.json`
/// relative to the current working directory. This function provides
/// a single point of control for the checkpoint location, making it
/// easier to configure or override in the future if needed.
fn checkpoint_path() -> String {
    format!("{AGENT_DIR}/{CHECKPOINT_FILE}")
}

/// Pipeline phases for checkpoint tracking.
///
/// These phases represent the major stages of the Ralph pipeline.
/// Checkpoints are saved at phase boundaries to enable resume functionality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelinePhase {
    /// Planning phase (creating PLAN.md)
    Planning,
    /// Development/implementation phase
    Development,
    /// Review-fix cycles phase (N iterations of review + fix)
    Review,
    /// Fix phase (deprecated: kept for backward compatibility with old checkpoints)
    Fix,
    /// Verification review phase (deprecated: kept for backward compatibility with old checkpoints)
    ReviewAgain,
    /// Commit message generation
    CommitMessage,
    /// Final validation phase
    FinalValidation,
    /// Pipeline complete
    Complete,
}

impl std::fmt::Display for PipelinePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "Planning"),
            Self::Development => write!(f, "Development"),
            Self::Review => write!(f, "Review"),
            Self::Fix => write!(f, "Fix"),
            Self::ReviewAgain => write!(f, "Verification Review"),
            Self::CommitMessage => write!(f, "Commit Message Generation"),
            Self::FinalValidation => write!(f, "Final Validation"),
            Self::Complete => write!(f, "Complete"),
        }
    }
}

/// Pipeline checkpoint for resume functionality.
///
/// Contains all state needed to resume an interrupted pipeline from
/// where it left off, including iteration counts, agent names, and
/// the timestamp when the checkpoint was saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCheckpoint {
    /// Current pipeline phase
    pub phase: PipelinePhase,
    /// Current iteration number (for developer iterations)
    pub iteration: u32,
    /// Total iterations configured
    pub total_iterations: u32,
    /// Current reviewer pass number
    pub reviewer_pass: u32,
    /// Total reviewer passes configured
    pub total_reviewer_passes: u32,
    /// Timestamp when checkpoint was saved
    pub timestamp: String,
    /// Developer agent name
    pub developer_agent: String,
    /// Reviewer agent name
    pub reviewer_agent: String,
}

impl PipelineCheckpoint {
    /// Create a new checkpoint with the given state.
    ///
    /// # Arguments
    ///
    /// * `phase` - Current pipeline phase
    /// * `iteration` - Current developer iteration number
    /// * `total_iterations` - Total developer iterations configured
    /// * `reviewer_pass` - Current reviewer pass number
    /// * `total_reviewer_passes` - Total reviewer passes configured
    /// * `developer_agent` - Name of the developer agent
    /// * `reviewer_agent` - Name of the reviewer agent
    pub fn new(
        phase: PipelinePhase,
        iteration: u32,
        total_iterations: u32,
        reviewer_pass: u32,
        total_reviewer_passes: u32,
        developer_agent: &str,
        reviewer_agent: &str,
    ) -> Self {
        Self {
            phase,
            iteration,
            total_iterations,
            reviewer_pass,
            total_reviewer_passes,
            timestamp: timestamp(),
            developer_agent: developer_agent.to_string(),
            reviewer_agent: reviewer_agent.to_string(),
        }
    }

    /// Get a human-readable description of the checkpoint.
    ///
    /// Returns a string describing the current phase and progress,
    /// suitable for display to the user when resuming.
    pub fn description(&self) -> String {
        match self.phase {
            PipelinePhase::Planning => {
                format!(
                    "Planning phase, iteration {}/{}",
                    self.iteration, self.total_iterations
                )
            }
            PipelinePhase::Development => {
                format!(
                    "Development iteration {}/{}",
                    self.iteration, self.total_iterations
                )
            }
            PipelinePhase::Review => "Initial review".to_string(),
            PipelinePhase::Fix => "Applying fixes".to_string(),
            PipelinePhase::ReviewAgain => {
                format!(
                    "Verification review {}/{}",
                    self.reviewer_pass, self.total_reviewer_passes
                )
            }
            PipelinePhase::CommitMessage => "Commit message generation".to_string(),
            PipelinePhase::FinalValidation => "Final validation".to_string(),
            PipelinePhase::Complete => "Pipeline complete".to_string(),
        }
    }
}

/// Get current timestamp in "YYYY-MM-DD HH:MM:SS" format.
pub fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Save a pipeline checkpoint to disk.
///
/// Writes the checkpoint atomically by writing to a temp file first,
/// then renaming to the final path. This prevents corruption if the
/// process is interrupted during the write.
///
/// # Errors
///
/// Returns an error if serialization fails or the file cannot be written.
pub fn save_checkpoint(checkpoint: &PipelineCheckpoint) -> io::Result<()> {
    let json = serde_json::to_string_pretty(checkpoint).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize checkpoint: {e}"),
        )
    })?;

    // Ensure the .agent directory exists before attempting to write
    fs::create_dir_all(AGENT_DIR)?;

    // Write atomically by writing to temp file then renaming
    let checkpoint = checkpoint_path();
    let temp_path = format!("{checkpoint}.tmp");

    // Ensure temp file is cleaned up even if write or rename fails
    let write_result = fs::write(&temp_path, &json);
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return write_result;
    }

    let rename_result = fs::rename(&temp_path, &checkpoint);
    if rename_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return rename_result;
    }

    Ok(())
}

/// Load an existing checkpoint if one exists.
///
/// Returns `Ok(Some(checkpoint))` if a valid checkpoint was loaded,
/// `Ok(None)` if no checkpoint file exists, or an error if the file
/// exists but cannot be parsed.
///
/// # Errors
///
/// Returns an error if the checkpoint file exists but cannot be read
/// or contains invalid JSON.
pub fn load_checkpoint() -> io::Result<Option<PipelineCheckpoint>> {
    let checkpoint = checkpoint_path();
    let path = Path::new(&checkpoint);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let checkpoint: PipelineCheckpoint = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse checkpoint: {e}"),
        )
    })?;

    Ok(Some(checkpoint))
}

/// Delete the checkpoint file.
///
/// Called on successful pipeline completion to clean up the checkpoint.
/// Does nothing if the checkpoint file doesn't exist.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be deleted.
pub fn clear_checkpoint() -> io::Result<()> {
    let checkpoint = checkpoint_path();
    let path = Path::new(&checkpoint);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Check if a checkpoint exists.
///
/// Returns `true` if a checkpoint file exists, `false` otherwise.
pub fn checkpoint_exists() -> bool {
    Path::new(&checkpoint_path()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::with_temp_cwd;

    #[test]
    fn test_timestamp_format() {
        let ts = timestamp();
        assert!(ts.contains('-'));
        assert!(ts.contains(':'));
        assert_eq!(ts.len(), 19);
    }

    #[test]
    fn test_pipeline_phase_display() {
        assert_eq!(format!("{}", PipelinePhase::Planning), "Planning");
        assert_eq!(format!("{}", PipelinePhase::Development), "Development");
        assert_eq!(format!("{}", PipelinePhase::Review), "Review");
        assert_eq!(format!("{}", PipelinePhase::Fix), "Fix");
        assert_eq!(
            format!("{}", PipelinePhase::ReviewAgain),
            "Verification Review"
        );
        assert_eq!(
            format!("{}", PipelinePhase::CommitMessage),
            "Commit Message Generation"
        );
        assert_eq!(
            format!("{}", PipelinePhase::FinalValidation),
            "Final Validation"
        );
        assert_eq!(format!("{}", PipelinePhase::Complete), "Complete");
    }

    #[test]
    fn test_checkpoint_new() {
        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::Development, 2, 5, 0, 2, "claude", "codex");

        assert_eq!(checkpoint.phase, PipelinePhase::Development);
        assert_eq!(checkpoint.iteration, 2);
        assert_eq!(checkpoint.total_iterations, 5);
        assert_eq!(checkpoint.reviewer_pass, 0);
        assert_eq!(checkpoint.total_reviewer_passes, 2);
        assert_eq!(checkpoint.developer_agent, "claude");
        assert_eq!(checkpoint.reviewer_agent, "codex");
        assert!(!checkpoint.timestamp.is_empty());
    }

    #[test]
    fn test_checkpoint_description() {
        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::Development, 3, 5, 0, 2, "claude", "codex");
        assert_eq!(checkpoint.description(), "Development iteration 3/5");

        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::ReviewAgain, 5, 5, 2, 3, "claude", "codex");
        assert_eq!(checkpoint.description(), "Verification review 2/3");
    }

    #[test]
    fn test_checkpoint_save_load() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let checkpoint =
                PipelineCheckpoint::new(PipelinePhase::Review, 5, 5, 1, 2, "claude", "codex");

            save_checkpoint(&checkpoint).unwrap();
            assert!(checkpoint_exists());

            let loaded = load_checkpoint()
                .unwrap()
                .expect("checkpoint should exist after save_checkpoint");
            assert_eq!(loaded.phase, PipelinePhase::Review);
            assert_eq!(loaded.iteration, 5);
            assert_eq!(loaded.developer_agent, "claude");
            assert_eq!(loaded.reviewer_agent, "codex");
        });
    }

    #[test]
    fn test_checkpoint_clear() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let checkpoint =
                PipelineCheckpoint::new(PipelinePhase::Development, 1, 5, 0, 2, "claude", "codex");

            save_checkpoint(&checkpoint).unwrap();
            assert!(checkpoint_exists());

            clear_checkpoint().unwrap();
            assert!(!checkpoint_exists());
        });
    }

    #[test]
    fn test_load_checkpoint_nonexistent() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();

            let result = load_checkpoint().unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_checkpoint_serialization() {
        let checkpoint =
            PipelineCheckpoint::new(PipelinePhase::Fix, 3, 5, 1, 2, "aider", "opencode");

        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains("Fix"));
        assert!(json.contains("aider"));
        assert!(json.contains("opencode"));

        let deserialized: PipelineCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.phase, checkpoint.phase);
        assert_eq!(deserialized.iteration, checkpoint.iteration);
    }
}
