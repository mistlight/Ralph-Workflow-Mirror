//! Commit phase execution.
//!
//! This module handles the final commit phase of the Ralph pipeline, which:
//! 1. Stages all changes
//! 2. Reads the generated commit message
//! 3. Creates the commit
//! 4. Cleans up temporary files

use crate::colors::Colors;
use crate::git_helpers::{git_add_all, git_commit};
use crate::utils::{
    clear_checkpoint, delete_commit_message_file, read_commit_message_file, Logger,
};
use std::process::Command;

/// Result of the commit phase.
pub struct CommitResult {
    /// The commit message that was used.
    pub commit_message: String,
    /// Whether a commit was actually created (false if nothing to commit).
    pub commit_created: bool,
}

/// Run the commit phase.
///
/// This phase:
/// 1. Stages all changes with `git add -A`
/// 2. Displays the changes to be committed
/// 3. Reads the commit message from .agent/commit-message.txt
/// 4. Creates the commit
/// 5. Cleans up the commit message file and checkpoint
///
/// # Arguments
///
/// * `logger` - Logger for output
/// * `colors` - Terminal color configuration
///
/// # Returns
///
/// Returns `Ok(CommitResult)` on success, or an error if the commit fails.
pub fn run_commit_phase(logger: &Logger, colors: &Colors) -> anyhow::Result<CommitResult> {
    logger.info("Staging all changes...");
    git_add_all()?;

    // Show what we're committing
    println!();
    println!("{}Changes to commit:{}", colors.bold(), colors.reset());
    let status_output = Command::new("git").args(["status", "--short"]).output()?;
    let lines: Vec<&str> = std::str::from_utf8(&status_output.stdout)
        .unwrap_or("")
        .lines()
        .take(20)
        .collect();
    for line in lines {
        println!("  {}{}{}", colors.dim(), line, colors.reset());
    }
    println!();

    // Read commit message from file (required)
    let commit_message = read_commit_message_file()?;
    logger.info(&format!(
        "Commit message: {}{}{}",
        colors.cyan(),
        commit_message,
        colors.reset()
    ));

    logger.info("Creating commit...");
    let commit_created = git_commit(&commit_message)?;

    if commit_created {
        logger.success("Commit created successfully");
    } else {
        logger.warn("Nothing to commit (working tree clean)");
    }

    // Delete commit-message.txt after committing
    if let Err(err) = delete_commit_message_file() {
        logger.warn(&format!("Failed to delete commit-message.txt: {}", err));
    }

    // Clear checkpoint on successful completion
    if let Err(e) = clear_checkpoint() {
        logger.warn(&format!("Failed to clear checkpoint: {}", e));
    }

    Ok(CommitResult {
        commit_message,
        commit_created,
    })
}
