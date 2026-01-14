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
use crate::git_helpers::{
    generate_commit_message_with_llm, get_repo_root, git_add_all, git_commit, git_diff,
    git_snapshot, require_git_repo,
};
use crate::files::{delete_commit_message_file, read_commit_message_file, write_commit_message_file};
use crate::logger::Logger;
use std::env;

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

    // Show what we're committing (using libgit2 via git_snapshot)
    if let Ok(status) = git_snapshot() {
        if !status.is_empty() {
            println!("{}Changes to commit:{}", colors.bold(), colors.reset());
            for line in status.lines().take(20) {
                println!("  {}{}{}", colors.dim(), line, colors.reset());
            }
            println!();
        }
    }

    logger.info(&format!(
        "Commit message: {}{}{}",
        colors.cyan(),
        commit_msg,
        colors.reset()
    ));

    logger.info("Creating commit...");
    // Note: Plumbing commands don't have access to config, so we use None
    // for git identity and fall back to git config (via repo.signature())
    if let Some(oid) = git_commit(&commit_msg, None, None)? {
        logger.success(&format!("Commit created successfully: {}", oid));
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
/// Generates a commit message for current changes using the LLM directly.
/// The diff is passed inline to the LLM, which generates a commit message
/// without any git context or file I/O.
///
/// # Arguments
///
/// * `config` - The pipeline configuration
/// * `registry` - The agent registry
/// * `logger` - Logger for info/warning messages
/// * `colors` - Color configuration for output
/// * `developer_agent` - Name of the developer agent to use
/// * `reviewer_agent` - Name of the reviewer agent (not used, kept for API compatibility)
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
    _reviewer_agent: &str,
) -> anyhow::Result<()> {
    logger.info("Generating commit message...");

    // Get the developer agent command for LLM invocation
    // Use config override if available (e.g., RALPH_DEVELOPER_CMD env var)
    let agent_cmd = if let Some(cmd_override) = &config.developer_cmd {
        cmd_override.clone()
    } else {
        registry
            .developer_cmd(developer_agent)
            .ok_or_else(|| anyhow::anyhow!("Developer agent '{}' not found", developer_agent))?
    };

    // Generate the commit message using the new approach (LLM with diff inline)
    // Note: This generates the message but doesn't create the commit yet
    // The user must run --apply-commit to create the actual commit
    let diff = git_diff()?;
    if diff.trim().is_empty() {
        logger.warn("No changes detected to generate a commit message for");
        anyhow::bail!("No changes to commit");
    }

    // Use the internal commit message generation function
    // This calls the LLM with the diff inline and returns the message
    let commit_message = generate_commit_message_with_llm(&diff, &agent_cmd)
        .map_err(|e| anyhow::anyhow!("Failed to generate commit message: {}", e))?;

    logger.success("Commit message generated:");
    println!();
    println!("{}{}{}", colors.cyan(), commit_message, colors.reset());
    println!();

    // Write the message to file for use with --apply-commit
    write_commit_message_file(&commit_message)?;

    logger.info("Message saved to .agent/commit-message.txt");
    logger.info("Run 'ralph --apply-commit' to create the commit");

    Ok(())
}
