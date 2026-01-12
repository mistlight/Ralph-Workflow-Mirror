//! Plumbing commands for low-level git operations.
//!
//! This module handles plumbing commands that operate directly on
//! commit messages and git state without running the full pipeline:
//! - `--show-commit-msg`: Display the stored commit message
//! - `--apply-commit`: Stage and commit using the stored message
//! - `--generate-commit-msg`: Generate a commit message for staged changes

use crate::agents::AgentRegistry;
use crate::colors::Colors;
use crate::config::Config;
use crate::git_helpers::{get_repo_root, git_add_all, git_commit, require_git_repo};
use crate::phases::{generate_commit_message, PhaseContext};
use crate::pipeline::Stats;
use crate::timer::Timer;
use crate::utils::{delete_commit_message_file, read_commit_message_file, Logger};
use std::env;
use std::process::Command;

/// Handles the `--show-commit-msg` command.
///
/// Reads and displays the commit message from `.agent/commit-message.txt`.
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if the file cannot be read.
pub fn handle_show_commit_msg() -> anyhow::Result<()> {
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;

    match read_commit_message_file() {
        Ok(msg) => {
            println!("{}", msg);
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("Failed to read commit message: {}", e);
        }
    }
}

/// Handles the `--apply-commit` command.
///
/// Stages all changes and creates a commit using the stored commit message.
/// After successful commit, deletes the commit message file.
///
/// # Arguments
///
/// * `logger` - Logger for info/warning messages
/// * `colors` - Color configuration for output
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if commit fails.
pub fn handle_apply_commit(logger: &Logger, colors: &Colors) -> anyhow::Result<()> {
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;

    let commit_msg = read_commit_message_file()?;

    logger.info("Staging all changes...");
    git_add_all()?;

    // Show what we're committing
    let status_output = Command::new("git").args(["status", "--short"]).output()?;
    let status_str = std::str::from_utf8(&status_output.stdout).unwrap_or("");
    if !status_str.is_empty() {
        println!("{}Changes to commit:{}", colors.bold(), colors.reset());
        for line in status_str.lines().take(20) {
            println!("  {}{}{}", colors.dim(), line, colors.reset());
        }
        println!();
    }

    logger.info(&format!(
        "Commit message: {}{}{}",
        colors.cyan(),
        commit_msg,
        colors.reset()
    ));

    logger.info("Creating commit...");
    if git_commit(&commit_msg)? {
        logger.success("Commit created successfully");
        // Clean up the commit message file
        if let Err(err) = delete_commit_message_file() {
            logger.warn(&format!("Failed to delete commit-message.txt: {}", err));
        }
    } else {
        logger.warn("Nothing to commit (working tree clean)");
    }

    Ok(())
}

/// Handles the `--generate-commit-msg` command.
///
/// Runs only the commit message generation phase using the developer agent.
/// Generates a commit message for currently staged changes.
///
/// # Arguments
///
/// * `config` - The pipeline configuration
/// * `registry` - The agent registry
/// * `logger` - Logger for info/warning messages
/// * `colors` - Color configuration for output
/// * `developer_agent` - Name of the developer agent to use
/// * `reviewer_agent` - Name of the reviewer agent (for context)
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if generation fails.
pub fn handle_generate_commit_msg(
    config: &Config,
    registry: &AgentRegistry,
    logger: &Logger,
    colors: &Colors,
    developer_agent: &str,
    reviewer_agent: &str,
) -> anyhow::Result<()> {
    let mut timer = Timer::new();
    let mut stats = Stats::new();

    logger.info("Generating commit message...");

    let mut ctx = PhaseContext {
        config,
        registry,
        logger,
        colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent,
        reviewer_agent,
        review_guidelines: None,
    };
    generate_commit_message(&mut ctx)?;

    // Verify and display the generated message
    match read_commit_message_file() {
        Ok(msg) => {
            logger.success("Commit message generated:");
            println!();
            println!("{}{}{}", colors.cyan(), msg, colors.reset());
            println!();
            logger.info("Message saved to .agent/commit-message.txt");
            logger.info("Run 'ralph --apply-commit' to create the commit");
            Ok(())
        }
        Err(e) => {
            logger.error(&format!("Failed to generate commit message: {}", e));
            anyhow::bail!("Commit message generation failed");
        }
    }
}
