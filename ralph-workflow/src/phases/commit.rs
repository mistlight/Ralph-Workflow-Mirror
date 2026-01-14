//! Commit message generation phase.
//!
//! This module handles automated commit message generation using the standard
//! agent pipeline with fallback support. It replaces the custom implementation
//! in repo.rs that lacked proper logging and fallback handling.
//!
//! The phase:
//! 1. Takes a git diff as input
//! 2. Runs the commit agent with the diff via the standard pipeline
//! 3. Extracts the commit message from agent output
//! 4. Returns the generated message for use by the caller

#![expect(clippy::trivially_copy_pass_by_ref)]
#![expect(clippy::too_many_arguments)]
use crate::agents::{AgentRegistry, AgentRole};
use crate::colors::Colors;
use crate::config::Config;
use crate::files::llm_output_extraction::{
    extract_llm_output, generate_fallback_commit_message, try_salvage_commit_message,
    validate_commit_message, OutputFormat,
};
use crate::git_helpers::{git_add_all, git_commit, CommitResultFallback};
use crate::logger::Logger;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::prompt_generate_commit_message_with_diff;
use crate::timer::Timer;
use std::fs::{self, File};
use std::io::Read;

/// Result of commit message generation.
pub struct CommitMessageResult {
    /// The generated commit message (may be empty on failure)
    pub message: String,
    /// Whether the generation was successful
    pub success: bool,
    /// Path to the agent log file for debugging (currently unused but kept for API compatibility)
    pub _log_path: String,
}

/// Generate a commit message using the standard agent pipeline with fallback.
///
/// This function uses the same `run_with_fallback()` pipeline as other phases,
/// which provides:
/// - Proper stdout/stderr logging
/// - Configurable fallback chains
/// - Retry logic with exponential backoff
/// - Agent error classification
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message for
/// * `registry` - The agent registry for resolving agents and fallbacks
/// * `runtime` - The pipeline runtime for execution services
/// * `commit_agent` - The primary agent to use for commit generation
///
/// # Returns
///
/// Returns `Ok(CommitMessageResult)` with the generated message and metadata.
pub fn generate_commit_message(
    diff: &str,
    registry: &AgentRegistry,
    runtime: &mut PipelineRuntime,
    commit_agent: &str,
) -> anyhow::Result<CommitMessageResult> {
    let log_dir = ".agent/logs/commit_generation";
    let log_file = format!("{log_dir}/final.log");

    // Ensure log directory exists
    fs::create_dir_all(log_dir)?;

    runtime.logger.info("Generating commit message...");

    // Build the commit message prompt
    let prompt = prompt_generate_commit_message_with_diff(diff);

    // Run the agent through the standard pipeline
    // This handles all the fallback, retry, and logging logic
    let exit_code = run_with_fallback(
        AgentRole::Commit,
        "generate commit message",
        &prompt,
        log_dir,
        runtime,
        registry,
        commit_agent,
    )?;

    // Try to extract the commit message from the agent output
    // We look at the most recent log file
    let result = if exit_code == 0 {
        // Agent succeeded - extract commit message from logs
        match extract_commit_message_from_logs(log_dir, diff, commit_agent, runtime.logger) {
            Ok(Some(message)) => CommitMessageResult {
                message,
                success: true,
                _log_path: log_file,
            },
            Ok(None) => {
                // Agent succeeded but no commit message found
                runtime
                    .logger
                    .warn("Agent succeeded but no commit message was extracted");
                CommitMessageResult {
                    message: String::new(),
                    success: false,
                    _log_path: log_file,
                }
            }
            Err(e) => {
                runtime
                    .logger
                    .error(&format!("Failed to extract commit message: {e}"));
                CommitMessageResult {
                    message: String::new(),
                    success: false,
                    _log_path: log_file,
                }
            }
        }
    } else {
        // Agent failed - check if we can extract a partial result
        runtime
            .logger
            .warn("Commit agent failed, checking logs for partial output...");
        match extract_commit_message_from_logs(log_dir, diff, commit_agent, runtime.logger) {
            Ok(Some(message)) => {
                runtime
                    .logger
                    .warn("Using partially generated commit message from failed agent");
                CommitMessageResult {
                    message,
                    success: false,
                    _log_path: log_file,
                }
            }
            _ => CommitMessageResult {
                message: String::new(),
                success: false,
                _log_path: log_file,
            },
        }
    };

    Ok(result)
}

/// Create a commit with an automatically generated message using the standard pipeline.
///
/// This is a replacement for `commit_with_auto_message_fallback_result` in `git_helpers`
/// that uses the standard agent pipeline with proper logging and fallback support.
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message from
/// * `commit_agent` - The primary agent to use for commit generation
/// * `git_user_name` - Optional git user name
/// * `git_user_email` - Optional git user email
/// * `registry` - The agent registry for resolving fallbacks
/// * `logger` - Logger for output
/// * `colors` - Color formatting
/// * `config` - Configuration
/// * `timer` - Timer for tracking execution time
///
/// # Returns
///
/// Returns `CommitResultFallback` indicating success, no changes, or failure.
pub fn commit_with_generated_message(
    diff: &str,
    commit_agent: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
    registry: &AgentRegistry,
    logger: &Logger,
    colors: &Colors,
    config: &Config,
    timer: &mut Timer,
) -> CommitResultFallback {
    // Stage all changes first
    let staged = match git_add_all() {
        Ok(s) => s,
        Err(e) => {
            return CommitResultFallback::Failed(format!("Failed to stage changes: {e}"));
        }
    };

    if !staged {
        return CommitResultFallback::NoChanges;
    }

    // Set up the runtime
    let mut runtime = PipelineRuntime {
        timer,
        logger,
        colors,
        config,
    };

    // Generate commit message using the standard pipeline
    let result = match generate_commit_message(diff, registry, &mut runtime, commit_agent) {
        Ok(r) => r,
        Err(e) => {
            return CommitResultFallback::Failed(format!("Failed to generate commit message: {e}"));
        }
    };

    // Check if generation succeeded
    if !result.success || result.message.trim().is_empty() {
        return CommitResultFallback::Failed("Commit message generation failed".to_string());
    }

    // Create the commit
    match git_commit(&result.message, git_user_name, git_user_email) {
        Ok(Some(oid)) => CommitResultFallback::Success(oid),
        Ok(None) => CommitResultFallback::NoChanges,
        Err(e) => CommitResultFallback::Failed(format!("Failed to create commit: {e}")),
    }
}

/// Extract a commit message from the agent log files.
///
/// This function reads the most recent agent log file and extracts
/// the commit message using the standard LLM output extraction logic.
///
/// # Arguments
///
/// * `log_dir` - Directory containing the agent log files
/// * `diff` - The original diff (for context/error messages)
/// * `agent_cmd` - The agent command (for format hint detection)
/// * `logger` - Logger for diagnostic output
///
/// # Returns
///
/// * `Ok(Some(message))` - A valid commit message was extracted
/// * `Ok(None)` - No commit message could be extracted
/// * `Err(e)` - An error occurred during extraction
fn extract_commit_message_from_logs(
    log_dir: &str,
    diff: &str,
    agent_cmd: &str,
    logger: &Logger,
) -> anyhow::Result<Option<String>> {
    // Find the most recent log file
    let log_path = find_most_recent_log(log_dir)?;

    let Some(log_file) = log_path else {
        logger.warn("No log files found in commit generation directory");
        return Ok(None);
    };

    logger.info(&format!(
        "Reading commit message from log: {}",
        log_file.display()
    ));

    // Read the log file
    let mut content = String::new();
    let mut file = File::open(&log_file)?;
    file.read_to_string(&mut content)?;

    if content.trim().is_empty() {
        logger.warn("Log file is empty");
        return Ok(None);
    }

    // Detect format hint from agent command
    let format_hint = agent_cmd
        .split_whitespace()
        .find_map(|tok| {
            let tok = tok.to_lowercase();
            if tok.contains("codex") {
                Some("codex")
            } else if tok.contains("claude") || tok.contains("ccs") || tok.contains("qwen") {
                Some("claude")
            } else if tok.contains("gemini") {
                Some("gemini")
            } else if tok.contains("opencode") {
                Some("opencode")
            } else {
                None
            }
        })
        .map(OutputFormat::from_str);

    // Extract the commit message using the standard extraction
    let extraction = extract_llm_output(&content, format_hint);

    // Log extraction metadata for debugging
    logger.info(&format!(
        "LLM output extraction: {:?} format, structured={}",
        extraction.format, extraction.was_structured
    ));

    if let Some(warning) = &extraction.warning {
        logger.warn(&format!("LLM output extraction warning: {warning}"));
    }

    let extracted = extraction.content;

    // Validate the commit message
    match validate_commit_message(&extracted) {
        Ok(()) => {
            logger.info("Successfully extracted and validated commit message");
            Ok(Some(extracted))
        }
        Err(e) => {
            logger.warn(&format!("Commit message validation failed: {e}"));

            // Recovery Layer 1: Attempt to salvage valid commit message from raw content
            logger.info("Attempting to salvage commit message from output...");
            if let Some(salvaged) = try_salvage_commit_message(&content) {
                logger.info("Successfully salvaged commit message");
                return Ok(Some(salvaged));
            }
            logger.warn("Salvage attempt failed");

            // Recovery Layer 2: Generate deterministic fallback from diff metadata
            logger.info("Generating fallback commit message from diff...");
            let fallback = generate_fallback_commit_message(diff);

            // Defensive validation (should always pass, but be safe)
            if validate_commit_message(&fallback).is_ok() {
                logger.info(&format!(
                    "Using fallback: {}",
                    fallback.lines().next().unwrap_or(&fallback)
                ));
                return Ok(Some(fallback));
            }

            logger.error("Fallback commit message failed validation - this is a bug");
            Ok(None)
        }
    }
}

/// Find the most recently modified log file matching a pattern.
///
/// # Arguments
///
/// * `log_prefix` - The prefix of log files to search for (e.g., ".`agent/logs/commit_generation`")
///
/// # Returns
///
/// * `Ok(Some(path))` - Path to the most recent log file
/// * `Ok(None)` - No log files found
/// * `Err(e)` - Error reading directory
fn find_most_recent_log(log_prefix: &str) -> anyhow::Result<Option<std::path::PathBuf>> {
    // Get the parent directory of the log prefix
    let log_path = std::path::PathBuf::from(log_prefix);
    let parent_dir = match log_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    };

    if !parent_dir.exists() {
        return Ok(None);
    }

    let entries = fs::read_dir(parent_dir)?;

    let mut most_recent: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

    // Extract the base name to match (e.g., "commit_generation" from ".agent/logs/commit_generation")
    let base_name = log_path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only look at .log files that start with the base name
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            if !file_name.starts_with(base_name)
                || path.extension().and_then(|s| s.to_str()) != Some("log")
            {
                continue;
            }
        } else {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata.modified()?;

        match &most_recent {
            None => {
                most_recent = Some((path, modified));
            }
            Some((_, prev_modified)) => {
                if modified > *prev_modified {
                    most_recent = Some((path, modified));
                }
            }
        }
    }

    Ok(most_recent.map(|(path, _)| path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_most_recent_log() {
        // Test with non-existent directory
        let result = find_most_recent_log("/nonexistent/path");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
