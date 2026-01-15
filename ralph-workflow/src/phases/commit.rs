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

use super::context::PhaseContext;
use crate::agents::{AgentRegistry, AgentRole};
use crate::files::llm_output_extraction::{
    extract_llm_output, generate_fallback_commit_message, try_extract_structured_commit,
    try_salvage_commit_message, validate_commit_message, CommitExtractionResult, OutputFormat,
};
use crate::git_helpers::{git_add_all, git_commit, CommitResultFallback};
use crate::logger::Logger;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{
    prompt_emergency_commit, prompt_generate_commit_message_with_diff, prompt_strict_json_commit,
    prompt_strict_json_commit_v2, prompt_ultra_minimal_commit,
};
use std::fmt;
use std::fs::{self, File};
use std::io::Read;

/// Retry strategy for commit message generation.
///
/// Tracks which stage of re-prompting we're in, allowing for progressive
/// degradation from detailed prompts to minimal ones before falling back
/// to the next agent in the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitRetryStrategy {
    /// First attempt with normal prompt
    Initial,
    /// Re-prompt with strict JSON requirement
    StrictJson,
    /// Even stricter prompt with negative examples
    StrictJsonV2,
    /// Ultra-minimal prompt, no context
    UltraMinimal,
    /// Final attempt, maximum strictness
    Emergency,
}

impl CommitRetryStrategy {
    /// Get the description of this retry stage for logging
    const fn description(self) -> &'static str {
        match self {
            Self::Initial => "initial prompt",
            Self::StrictJson => "strict JSON prompt",
            Self::StrictJsonV2 => "strict JSON V2 prompt",
            Self::UltraMinimal => "ultra-minimal prompt",
            Self::Emergency => "emergency prompt",
        }
    }

    /// Get the next retry strategy, or None if this is the last stage
    const fn next(self) -> Option<Self> {
        match self {
            Self::Initial => Some(Self::StrictJson),
            Self::StrictJson => Some(Self::StrictJsonV2),
            Self::StrictJsonV2 => Some(Self::UltraMinimal),
            Self::UltraMinimal => Some(Self::Emergency),
            Self::Emergency => None,
        }
    }

    /// Get the total number of retry stages
    const fn total_stages() -> usize {
        5 // Initial + 4 re-prompt variants
    }
}

impl fmt::Display for CommitRetryStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

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
/// Multi-stage retry logic:
/// 1. Try initial prompt
/// 2. On fallback/empty result, try strict JSON prompt
/// 3. On failure, try V2 strict prompt (with negative examples)
/// 4. On failure, try ultra-minimal prompt
/// 5. On failure, try emergency prompt
/// 6. Only use hardcoded fallback after all prompt variants exhausted
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

    // Try each prompt variant in sequence
    let mut strategy = CommitRetryStrategy::Initial;
    let mut last_extraction: Option<CommitExtractionResult> = None;
    let mut last_error: Option<anyhow::Error> = None;

    while let Some(current_strategy) = Some(strategy) {
        // Generate the appropriate prompt for this retry stage
        let prompt = match current_strategy {
            CommitRetryStrategy::Initial => prompt_generate_commit_message_with_diff(diff),
            CommitRetryStrategy::StrictJson => prompt_strict_json_commit(diff),
            CommitRetryStrategy::StrictJsonV2 => prompt_strict_json_commit_v2(diff),
            CommitRetryStrategy::UltraMinimal => prompt_ultra_minimal_commit(diff),
            CommitRetryStrategy::Emergency => prompt_emergency_commit(diff),
        };

        // Log the current attempt
        if strategy == CommitRetryStrategy::Initial {
            runtime.logger.info(&format!(
                "Attempt 1/{}: Using {}",
                CommitRetryStrategy::total_stages(),
                strategy
            ));
        } else {
            runtime.logger.warn(&format!(
                "Attempt {}/{}: Re-prompting with {}...",
                strategy as usize + 1,
                CommitRetryStrategy::total_stages(),
                strategy
            ));
        }

        // Run the agent through the standard pipeline
        let exit_code = run_with_fallback(
            AgentRole::Commit,
            &format!("generate commit message ({})", strategy.description()),
            &prompt,
            log_dir,
            runtime,
            registry,
            commit_agent,
        )?;

        // Try to extract the commit message from the agent output
        if exit_code != 0 {
            // Agent failed - check if we can extract a partial result
            runtime
                .logger
                .warn("Commit agent failed, checking logs for partial output...");
        }
        let extraction_result =
            extract_commit_message_from_logs(log_dir, diff, commit_agent, runtime.logger);

        match extraction_result {
            Ok(Some(extraction)) => {
                // Check if we got a valid extraction or a fallback
                if extraction.is_fallback() {
                    // Fallback was generated - log and continue to next prompt variant
                    runtime.logger.warn(&format!(
                        "Extraction produced fallback message with {strategy}"
                    ));
                    last_extraction = Some(extraction);

                    // Move to next strategy
                    if let Some(next) = strategy.next() {
                        strategy = next;
                    } else {
                        // No more strategies - use the last fallback we got
                        runtime.logger.warn(&format!(
                            "All {} prompt variants exhausted, using fallback message",
                            CommitRetryStrategy::total_stages()
                        ));
                        break;
                    }
                } else {
                    // Got a valid extraction (Extracted or Salvaged) - use it
                    runtime.logger.info(&format!(
                        "Successfully extracted commit message with {strategy}"
                    ));
                    return Ok(CommitMessageResult {
                        message: extraction.into_message(),
                        success: true,
                        _log_path: log_file,
                    });
                }
            }
            Ok(None) => {
                // Extraction completely failed - log and continue
                runtime.logger.warn(&format!(
                    "No valid commit message extracted with {strategy}"
                ));

                // Move to next strategy
                if let Some(next) = strategy.next() {
                    strategy = next;
                } else {
                    // No more strategies - return failure
                    runtime.logger.error(&format!(
                        "All {} prompt variants failed",
                        CommitRetryStrategy::total_stages()
                    ));
                    return Ok(CommitMessageResult {
                        message: String::new(),
                        success: false,
                        _log_path: log_file,
                    });
                }
            }
            Err(e) => {
                // Extraction error - log and continue
                runtime.logger.error(&format!(
                    "Failed to extract commit message with {strategy}: {e}"
                ));
                last_error = Some(e);

                // Move to next strategy
                if let Some(next) = strategy.next() {
                    strategy = next;
                } else {
                    // No more strategies - return failure
                    runtime.logger.error(&format!(
                        "All {} prompt variants failed with errors",
                        CommitRetryStrategy::total_stages()
                    ));
                    return Ok(CommitMessageResult {
                        message: String::new(),
                        success: false,
                        _log_path: log_file,
                    });
                }
            }
        }
    }

    // If we have a fallback from the last attempt, use it
    if let Some(extraction) = last_extraction {
        runtime
            .logger
            .warn("Using fallback commit message from final attempt");
        return Ok(CommitMessageResult {
            message: extraction.into_message(),
            success: true, // We still have a valid (though generic) message
            _log_path: log_file,
        });
    }

    // Complete failure - no valid message could be generated
    let error_msg = last_error.as_ref().map_or_else(
        || "Failed to generate commit message".to_string(),
        std::string::ToString::to_string,
    );
    runtime.logger.error(&error_msg);
    Ok(CommitMessageResult {
        message: String::new(),
        success: false,
        _log_path: log_file,
    })
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
/// * `ctx` - The phase context containing registry, logger, colors, config, and timer
///
/// # Returns
///
/// Returns `CommitResultFallback` indicating success, no changes, or failure.
pub fn commit_with_generated_message(
    diff: &str,
    commit_agent: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
    ctx: &mut PhaseContext<'_>,
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
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
    };

    // Generate commit message using the standard pipeline
    let result = match generate_commit_message(diff, ctx.registry, &mut runtime, commit_agent) {
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
/// * `Ok(CommitExtractionResult)` - A result indicating how the message was obtained:
///   - `Extracted` - Successfully extracted from structured output
///   - `Salvaged` - Recovered from mixed output via salvage mechanism
///   - `Fallback` - Using deterministic fallback (caller should consider re-prompt)
/// * `Err(e)` - An error occurred during extraction (e.g., file I/O error)
fn extract_commit_message_from_logs(
    log_dir: &str,
    diff: &str,
    agent_cmd: &str,
    logger: &Logger,
) -> anyhow::Result<Option<CommitExtractionResult>> {
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

    // FIRST: Try structured JSON extraction (new primary method)
    // This is the preferred method when the agent outputs JSON schema format
    if let Some(message) = try_extract_structured_commit(&content) {
        logger.info("Successfully extracted commit message from JSON schema");
        return Ok(Some(CommitExtractionResult::Extracted(message)));
    }

    logger.info("JSON schema extraction failed, falling back to pattern-based extraction");

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
            Ok(Some(CommitExtractionResult::Extracted(extracted)))
        }
        Err(e) => {
            logger.warn(&format!("Commit message validation failed: {e}"));

            // Recovery Layer 1: Attempt to salvage valid commit message from raw content
            logger.info("Attempting to salvage commit message from output...");
            if let Some(salvaged) = try_salvage_commit_message(&content) {
                logger.info("Successfully salvaged commit message");
                return Ok(Some(CommitExtractionResult::Salvaged(salvaged)));
            }
            logger.warn("Salvage attempt failed");

            // Recovery Layer 2: Generate deterministic fallback from diff metadata
            // Note: We return Fallback variant to signal the caller should try re-prompting
            logger.info("Generating fallback commit message from diff...");
            let fallback = generate_fallback_commit_message(diff);

            // Defensive validation (should always pass, but be safe)
            if validate_commit_message(&fallback).is_ok() {
                logger.info(&format!(
                    "Generated fallback: {}",
                    fallback.lines().next().unwrap_or(&fallback)
                ));
                return Ok(Some(CommitExtractionResult::Fallback(fallback)));
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
