//! Plumbing commands for low-level git operations.
//!
//! This module handles plumbing commands that operate directly on
//! commit messages and git state without running the full pipeline:
//! - `--show-commit-msg`: Display the stored commit message
//! - `--apply-commit`: Stage and commit using the stored message
//! - `--generate-commit-msg`: Generate a commit message for staged changes

use crate::agents::AgentRegistry;
use crate::config::Config;
use crate::executor::ProcessExecutor;
use crate::files::{
    delete_commit_message_file, read_commit_message_file, write_commit_message_file,
};
use crate::git_helpers::{
    get_repo_root, git_add_all, git_commit, git_diff, git_snapshot, require_git_repo,
};
use crate::logger::Colors;
use crate::logger::Logger;
use crate::phases::generate_commit_message;
use crate::pipeline::PipelineRuntime;
use crate::pipeline::Timer;
use crate::prompts::TemplateContext;
use std::env;
use std::sync::Arc;

/// Configuration for commit message generation in plumbing commands.
///
/// Groups related parameters for `handle_generate_commit_msg` to avoid
/// excessive function arguments.
pub struct CommitGenerationConfig<'a> {
    /// The pipeline configuration.
    pub config: &'a Config,
    /// Template context for prompt expansion.
    pub template_context: &'a TemplateContext,
    /// Agent registry for accessing configured agents.
    pub registry: &'a AgentRegistry,
    /// Logger for info/warning messages.
    pub logger: &'a Logger,
    /// Color configuration for output.
    pub colors: Colors,
    /// Name of the developer agent to use for commit generation.
    pub developer_agent: &'a str,
    /// Name of the reviewer agent (not used, kept for API compatibility).
    pub reviewer_agent: &'a str,
    /// Process executor for external command execution.
    pub executor: Arc<dyn ProcessExecutor>,
}

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
            println!("{msg}");
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("Failed to read commit message: {e}");
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
pub fn handle_apply_commit(logger: &Logger, colors: Colors) -> anyhow::Result<()> {
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
    // for git identity and executor, falling back to git config (via repo.signature())
    // and environment variables for identity fallback.
    if let Some(oid) = git_commit(&commit_msg, None, None, None)? {
        logger.success(&format!("Commit created successfully: {oid}"));
        // Clean up the commit message file
        if let Err(err) = delete_commit_message_file() {
            logger.warn(&format!("Failed to delete commit-message.txt: {err}"));
        }
    } else {
        logger.warn("Nothing to commit (working tree clean)");
    }

    Ok(())
}

/// Handles the `--generate-commit-msg` command.
///
/// Generates a commit message for current changes using the standard pipeline.
/// Uses the same `generate_commit_message()` function as the main workflow,
/// ensuring consistent behavior with proper fallback chain support and logging.
///
/// # Arguments
///
/// * `config` - The pipeline configuration
/// * `registry` - The agent registry
/// * `logger` - Logger for info/warning messages
/// * `colors` - Color configuration for output
/// * `developer_agent` - Name of the developer agent to use (for commit generation)
/// * `_reviewer_agent` - Name of the reviewer agent (not used, kept for API compatibility)
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if generation fails.
pub fn handle_generate_commit_msg(config: CommitGenerationConfig<'_>) -> anyhow::Result<()> {
    config.logger.info("Generating commit message...");

    // Generate the commit message using standard pipeline
    let diff = git_diff()?;
    if diff.trim().is_empty() {
        config.logger.warn("No changes detected to generate a commit message for");
        anyhow::bail!("No changes to commit");
    }

    // Create a timer for the pipeline runtime
    let mut timer = Timer::new();

    // Set up pipeline runtime with the injected executor
    let executor_ref: &dyn ProcessExecutor = &*config.executor;
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: config.logger,
        colors: &config.colors,
        config: config.config,
        executor: executor_ref,
        executor_arc: Arc::clone(&config.executor),
    };

    // Use the standard commit message generation from phases/commit.rs
    // This provides:
    // - Proper fallback chain support
    // - Structured logging to .agent/logs/
    // - Meaningful error diagnostics
    let result = generate_commit_message(
        &diff,
        config.registry,
        &mut runtime,
        config.developer_agent,
        config.template_context,
        &std::collections::HashMap::new(), // Empty prompt history for plumbing command
    )
    .map_err(|e| anyhow::anyhow!("Failed to generate commit message: {e}"))?;

    if !result.success || result.message.trim().is_empty() {
        anyhow::bail!("Commit message generation failed");
    }

    let commit_message = result.message;

    config.logger.success("Commit message generated:");
    println!();
    println!(
        "{}{}{}",
        config.colors.cyan(),
        commit_message,
        config.colors.reset()
    );
    println!();

    // Write the message to file for use with --apply-commit
    write_commit_message_file(&commit_message)?;

    config
        .logger
        .info("Message saved to .agent/commit-message.txt");
    config.logger.info("Run 'ralph --apply-commit' to create the commit");

    Ok(())
}
