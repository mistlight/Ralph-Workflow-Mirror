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
use crate::agents::{AgentErrorKind, AgentRegistry, AgentRole};
use crate::files::llm_output_extraction::{
    detect_agent_errors_in_output, extract_llm_output, generate_fallback_commit_message,
    try_extract_structured_commit, try_salvage_commit_message, validate_commit_message,
    CommitExtractionResult, OutputFormat,
};
use crate::git_helpers::{git_add_all, git_commit, CommitResultFallback};
use crate::logger::Logger;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{
    prompt_emergency_commit, prompt_emergency_no_diff_commit, prompt_file_list_only_commit,
    prompt_file_list_summary_only_commit, prompt_generate_commit_message_with_diff,
    prompt_strict_json_commit, prompt_strict_json_commit_v2, prompt_ultra_minimal_commit,
    prompt_ultra_minimal_commit_v2,
};
use std::fmt;
use std::fs::{self, File};
use std::io::Read;

/// Maximum safe prompt size in bytes before pre-truncation.
///
/// This is a conservative limit to prevent agents from failing with "prompt too long"
/// errors. Different agents have different token limits:
/// - GLM: ~100KB effective limit
/// - Claude CCS: ~300KB effective limit
/// - Others: vary by model
///
/// We use 200KB as a safe middle ground that works for most agents while still
/// allowing substantial diffs to be processed without truncation.
const MAX_SAFE_PROMPT_SIZE: usize = 200_000;

/// Absolute last resort fallback commit message.
///
/// This is used ONLY when all other methods fail:
/// - All 8 prompt variants exhausted
/// - All agents in fallback chain exhausted
/// - All truncation stages failed
/// - Emergency no-diff prompt failed
/// - Deterministic fallback from diff failed
///
/// This ensures the commit process NEVER fails completely.
const HARDCODED_FALLBACK_COMMIT: &str = "chore: automated commit";

/// Get the maximum safe prompt size for a specific agent.
///
/// Different agents have different token limits. This function returns a
/// conservative max size for the given agent to prevent "prompt too long" errors.
///
/// # Arguments
///
/// * `commit_agent` - The commit agent command string
///
/// # Returns
///
/// Maximum safe prompt size in bytes
fn max_prompt_size_for_agent(commit_agent: &str) -> usize {
    let agent_lower = commit_agent.to_lowercase();

    // GLM and similar agents have smaller effective limits
    if agent_lower.contains("glm")
        || agent_lower.contains("zhipuai")
        || agent_lower.contains("zai")
        || agent_lower.contains("qwen")
        || agent_lower.contains("deepseek")
    {
        100_000 // 100KB for GLM-like agents
    } else if agent_lower.contains("claude")
        || agent_lower.contains("ccs")
        || agent_lower.contains("anthropic")
    {
        300_000 // 300KB for Claude-based agents
    } else {
        MAX_SAFE_PROMPT_SIZE // Default 200KB
    }
}

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
    /// Ultra-minimal V2 - even shorter
    UltraMinimalV2,
    /// File list only - no diff content
    FileListOnly,
    /// File list summary only - just file counts and categories
    FileListSummaryOnly,
    /// Emergency prompt - maximum strictness
    Emergency,
    /// Emergency no-diff - absolute last resort
    EmergencyNoDiff,
}

impl CommitRetryStrategy {
    /// Get the description of this retry stage for logging
    const fn description(self) -> &'static str {
        match self {
            Self::Initial => "initial prompt",
            Self::StrictJson => "strict JSON prompt",
            Self::StrictJsonV2 => "strict JSON V2 prompt",
            Self::UltraMinimal => "ultra-minimal prompt",
            Self::UltraMinimalV2 => "ultra-minimal V2 prompt",
            Self::FileListOnly => "file list only prompt",
            Self::FileListSummaryOnly => "file list summary only prompt",
            Self::Emergency => "emergency prompt",
            Self::EmergencyNoDiff => "emergency no-diff prompt",
        }
    }

    /// Get the next retry strategy, or None if this is the last stage
    const fn next(self) -> Option<Self> {
        match self {
            Self::Initial => Some(Self::StrictJson),
            Self::StrictJson => Some(Self::StrictJsonV2),
            Self::StrictJsonV2 => Some(Self::UltraMinimal),
            Self::UltraMinimal => Some(Self::UltraMinimalV2),
            Self::UltraMinimalV2 => Some(Self::FileListOnly),
            Self::FileListOnly => Some(Self::FileListSummaryOnly),
            Self::FileListSummaryOnly => Some(Self::Emergency),
            Self::Emergency => Some(Self::EmergencyNoDiff),
            Self::EmergencyNoDiff => None,
        }
    }

    /// Get the total number of retry stages
    const fn total_stages() -> usize {
        9 // Initial + 8 re-prompt variants
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

/// Truncate diff if it's too large for agents with small context windows.
///
/// This is a defensive measure when agents report "prompt too long" errors.
/// Returns a truncated diff with a summary of omitted content.
///
/// # Semantic Awareness
///
/// The improved truncation:
/// 1. Preserves file structure - truncates at file boundaries (after `diff --git` blocks)
/// 2. Prioritizes important files - keeps files from `src/` over `tests/`, `.md` files, etc.
/// 3. Preserves last N files - shows what changed at the end
/// 4. Adds a summary header - includes "First M files shown, N files truncated"
fn truncate_diff_if_large(diff: &str, max_size: usize) -> String {
    if diff.len() <= max_size {
        return diff.to_string();
    }

    // Parse the diff into individual file blocks
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file = DiffFile::default();
    let mut in_file = false;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            // Save previous file if any
            if in_file && !current_file.lines.is_empty() {
                files.push(std::mem::take(&mut current_file));
            }
            in_file = true;
            current_file.lines.push(line.to_string());

            // Extract and prioritize the file path
            if let Some(path) = line.split(" b/").nth(1) {
                current_file.path = path.to_string();
                current_file.priority = prioritize_file_path(path);
            }
        } else if in_file {
            current_file.lines.push(line.to_string());
        }
    }

    // Don't forget the last file
    if in_file && !current_file.lines.is_empty() {
        files.push(current_file);
    }

    let total_files = files.len();

    // Sort files by priority (highest first) to keep important files
    files.sort_by_key(|f| std::cmp::Reverse(f.priority));

    // Greedily select files that fit within max_size
    let mut selected_files = Vec::new();
    let mut current_size = 0;

    for file in files {
        let file_size: usize = file.lines.iter().map(|l| l.len() + 1).sum(); // +1 for newline

        if current_size + file_size <= max_size {
            current_size += file_size;
            selected_files.push(file);
        } else if current_size > 0 {
            // We have at least one file and this one would exceed the limit
            // Stop adding more files
            break;
        } else {
            // Even the first (highest priority) file is too large
            // Take at least the first part of it
            let truncated_lines = truncate_lines_to_fit(&file.lines, max_size);
            selected_files.push(DiffFile {
                path: file.path,
                priority: file.priority,
                lines: truncated_lines,
            });
            break;
        }
    }

    let selected_count = selected_files.len();
    let omitted_count = total_files.saturating_sub(selected_count);

    // Build the truncated diff
    let mut result = String::new();

    // Add summary header at the top
    if omitted_count > 0 {
        use std::fmt::Write;
        let _ = write!(
            result,
            "[Diff truncated: Showing first {selected_count} of {total_files} files. {omitted_count} files omitted due to size constraints.]\n\n"
        );
    }

    for file in selected_files {
        for line in &file.lines {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

/// Represents a single file's diff chunk.
#[derive(Debug, Default, Clone)]
struct DiffFile {
    /// File path (extracted from diff header)
    path: String,
    /// Priority for selection (higher = more important)
    priority: i32,
    /// Lines in this file's diff
    lines: Vec<String>,
}

/// Assign a priority score to a file path for truncation selection.
///
/// Higher priority files are kept first when truncating:
/// - src/*.rs: +100 (source code is most important)
/// - src/*: +80 (other src files)
/// - tests/*: +40 (tests are important but secondary)
/// - Cargo.toml, package.json, etc.: +60 (config files)
/// - docs/*, *.md: +20 (docs are least important)
/// - Other: +50 (default)
fn prioritize_file_path(path: &str) -> i32 {
    use std::path::Path;
    let path_lower = path.to_lowercase();

    // Helper function for case-insensitive extension check
    let has_ext = |ext: &str| -> bool {
        Path::new(path)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    };

    // Helper function for case-insensitive file extension check on path_lower
    let has_ext_lower = |ext: &str| -> bool {
        Path::new(&path_lower)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    };

    // Source code files (highest priority)
    if path_lower.contains("src/") && has_ext_lower("rs") {
        100
    } else if path_lower.contains("src/") {
        80
    }
    // Test files
    else if path_lower.contains("test") {
        40
    }
    // Config files - use case-insensitive extension check
    else if has_ext("toml")
        || has_ext("json")
        || path_lower.ends_with("cargo.toml")
        || path_lower.ends_with("package.json")
        || path_lower.ends_with("tsconfig.json")
    {
        60
    }
    // Documentation files (lowest priority)
    else if path_lower.contains("doc") || has_ext("md") {
        20
    }
    // Default priority
    else {
        50
    }
}

/// Truncate a slice of lines to fit within a maximum size.
///
/// This is a fallback for when even a single file is too large.
/// Returns as many complete lines as will fit.
fn truncate_lines_to_fit(lines: &[String], max_size: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_size = 0;

    for line in lines {
        let line_size = line.len() + 1; // +1 for newline
        if current_size + line_size <= max_size {
            current_size += line_size;
            result.push(line.clone());
        } else {
            break;
        }
    }

    // Add truncation marker to the last line
    if let Some(last) = result.last_mut() {
        last.push_str(" [truncated...]");
    }

    result
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

    // Step 1: Proactive prompt size checking
    // Before calling run_with_fallback(), check if the prompt will exceed
    // typical agent token limits and pre-truncate if needed
    let max_size = max_prompt_size_for_agent(commit_agent);
    let (working_diff, diff_was_pre_truncated) = if diff.len() > max_size {
        runtime.logger.warn(&format!(
            "Diff size ({} KB) exceeds agent limit ({} KB). Pre-truncating to avoid token errors.",
            diff.len() / 1024,
            max_size / 1024
        ));
        (truncate_diff_if_large(diff, max_size), true)
    } else {
        runtime.logger.info(&format!(
            "Diff size ({} KB) is within safe limit ({} KB).",
            diff.len() / 1024,
            max_size / 1024
        ));
        (diff.to_string(), false)
    };

    // Try each prompt variant in sequence
    let mut strategy = CommitRetryStrategy::Initial;
    let mut last_extraction: Option<CommitExtractionResult> = None;

    while let Some(current_strategy) = Some(strategy) {
        // Generate the appropriate prompt for this retry stage
        let prompt = match current_strategy {
            CommitRetryStrategy::Initial => prompt_generate_commit_message_with_diff(&working_diff),
            CommitRetryStrategy::StrictJson => prompt_strict_json_commit(&working_diff),
            CommitRetryStrategy::StrictJsonV2 => prompt_strict_json_commit_v2(&working_diff),
            CommitRetryStrategy::UltraMinimal => prompt_ultra_minimal_commit(&working_diff),
            CommitRetryStrategy::UltraMinimalV2 => prompt_ultra_minimal_commit_v2(&working_diff),
            CommitRetryStrategy::FileListOnly => prompt_file_list_only_commit(&working_diff),
            CommitRetryStrategy::FileListSummaryOnly => {
                prompt_file_list_summary_only_commit(&working_diff)
            }
            CommitRetryStrategy::Emergency => prompt_emergency_commit(&working_diff),
            CommitRetryStrategy::EmergencyNoDiff => prompt_emergency_no_diff_commit(&working_diff),
        };

        // Log prompt size for diagnostic purposes
        let prompt_size_kb = prompt.len() / 1024;

        // Log the current attempt with enhanced diagnostics
        if strategy == CommitRetryStrategy::Initial {
            runtime.logger.info(&format!(
                "Attempt 1/{}: Using {} (prompt size: {} KB, agent: {})",
                CommitRetryStrategy::total_stages(),
                strategy,
                prompt_size_kb,
                commit_agent
            ));
        } else {
            runtime.logger.warn(&format!(
                "Attempt {}/{}: Re-prompting with {} (prompt size: {} KB, agent: {})...",
                strategy as usize + 1,
                CommitRetryStrategy::total_stages(),
                strategy,
                prompt_size_kb,
                commit_agent
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
            extract_commit_message_from_logs(log_dir, &working_diff, commit_agent, runtime.logger);

        match extraction_result {
            Ok(Some(extraction)) => {
                // Check if we got a valid extraction or a fallback
                if extraction.is_agent_error() {
                    // Agent error detected - check if it's TokenExhausted
                    // For TokenExhausted, we should continue trying smaller prompt variants
                    // rather than immediately breaking to agent fallback
                    if extraction.error_kind() == Some(AgentErrorKind::TokenExhausted) {
                        runtime.logger.warn(&format!(
                            "TokenExhausted detected with {}. Trying smaller prompt variant.",
                            strategy.description()
                        ));
                        last_extraction = Some(extraction);

                        // Move to next (smaller) strategy instead of breaking
                        if let Some(next) = strategy.next() {
                            strategy = next;
                        } else {
                            // All strategies exhausted with TokenExhausted - will try truncation
                            runtime.logger.warn(&format!(
                                "All {} prompt variants failed with TokenExhausted.",
                                CommitRetryStrategy::total_stages()
                            ));
                            break;
                        }
                    } else {
                        // Other agent errors - log and break to fallback
                        runtime.logger.warn(&format!(
                            "Agent error detected: {}. Skipping remaining prompt variants.",
                            extraction
                                .error_kind()
                                .map_or("unknown", AgentErrorKind::description)
                        ));
                        last_extraction = Some(extraction);
                        // Break out of the strategy loop - we've hit a hard error
                        // that requires switching agents or using fallback
                        break;
                    }
                } else if extraction.is_fallback() {
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
                // Extraction completely failed - generate fallback and continue
                runtime.logger.warn(&format!(
                    "No valid commit message extracted with {strategy}, will use fallback"
                ));
                // Move to next strategy (they all generate fallbacks, so last one will be used)
                if let Some(next) = strategy.next() {
                    strategy = next;
                } else {
                    // All strategies exhausted - generate final fallback
                    runtime
                        .logger
                        .warn("All prompt variants exhausted, generating fallback from diff...");
                    let fallback = generate_fallback_commit_message(diff);
                    return Ok(CommitMessageResult {
                        message: fallback,
                        success: true,
                        _log_path: log_file,
                    });
                }
            }
            Err(e) => {
                // Extraction error - log and continue to next strategy
                runtime.logger.error(&format!(
                    "Failed to extract commit message with {strategy}: {e}"
                ));

                if let Some(next) = strategy.next() {
                    strategy = next;
                } else {
                    // All strategies failed with error - generate fallback
                    runtime
                        .logger
                        .warn("All prompt variants failed with error, generating fallback...");
                    let fallback = generate_fallback_commit_message(diff);
                    return Ok(CommitMessageResult {
                        message: fallback,
                        success: true,
                        _log_path: log_file,
                    });
                }
            }
        }
    }

    // If we have a fallback from the last attempt, use it
    if let Some(extraction) = last_extraction {
        // If we have an AgentError, generate a deterministic fallback from diff
        if extraction.is_agent_error() {
            runtime.logger.warn(&format!(
                "Agent error ({}) - generating fallback commit message from diff...",
                extraction
                    .error_kind()
                    .map_or("unknown", AgentErrorKind::description)
            ));
            let fallback = generate_fallback_commit_message(diff);
            return Ok(CommitMessageResult {
                message: fallback,
                success: true, // We have a valid (though generic) message
                _log_path: log_file,
            });
        }

        // Otherwise use the fallback message from the extraction
        runtime
            .logger
            .warn("Using fallback commit message from final attempt");
        return Ok(CommitMessageResult {
            message: extraction.into_message(),
            success: true, // We still have a valid (though generic) message
            _log_path: log_file,
        });
    }

    // If all strategies failed with TokenExhausted error, try with progressively truncated diff
    // Skip if we already pre-truncated (avoid double truncation)
    if last_extraction
        == Some(CommitExtractionResult::AgentError(
            AgentErrorKind::TokenExhausted,
        ))
        && !diff_was_pre_truncated
    {
        runtime
            .logger
            .warn("TokenExhausted detected: All agents failed due to token limits.");
        runtime
            .logger
            .warn("Attempting progressive diff truncation...");

        // Progressive truncation stages: 50KB -> 25KB -> 10KB -> file-list-only -> emergency
        let truncation_stages = [
            (50_000, "50KB"),
            (25_000, "25KB"),
            (10_000, "10KB"),
            (1_000, "file-list-only"),
        ];

        for (size_kb, label) in truncation_stages {
            runtime.logger.warn(&format!(
                "Truncation retry: Trying {} limit ({})...",
                label,
                size_kb / 1024
            ));

            let truncated_diff = truncate_diff_if_large(diff, size_kb);
            let prompt = prompt_emergency_commit(&truncated_diff);

            runtime.logger.info(&format!(
                "Truncated diff attempt ({}): prompt size {} KB",
                label,
                prompt.len() / 1024
            ));

            let exit_code = run_with_fallback(
                AgentRole::Commit,
                &format!("generate commit message (truncated {label})"),
                &prompt,
                log_dir,
                runtime,
                registry,
                commit_agent,
            )?;

            if exit_code == 0 {
                if let Ok(Some(extraction)) = extract_commit_message_from_logs(
                    log_dir,
                    &truncated_diff,
                    commit_agent,
                    runtime.logger,
                ) {
                    // Check if we got a valid result (not an agent error)
                    if extraction.is_agent_error() {
                        // TokenExhausted - continue to next truncation stage
                        runtime.logger.warn(&format!(
                            "{label} truncation still hit token limits, trying smaller size..."
                        ));
                        continue;
                    }

                    // Not an agent error - try to use the message
                    let message = extraction.into_message();
                    if !message.is_empty() {
                        runtime.logger.info(&format!(
                            "Successfully generated commit message with {label} truncation"
                        ));
                        return Ok(CommitMessageResult {
                            message,
                            success: true,
                            _log_path: log_file,
                        });
                    }
                    // Empty message - break out and try fallback
                    break;
                }
            }
        }

        // All truncation stages failed - try emergency no-diff as last resort
        runtime
            .logger
            .warn("All truncation stages failed. Trying emergency no-diff prompt...");
        let no_diff_prompt = prompt_emergency_no_diff_commit(&working_diff);

        let exit_code = run_with_fallback(
            AgentRole::Commit,
            "generate commit message (emergency no-diff)",
            &no_diff_prompt,
            log_dir,
            runtime,
            registry,
            commit_agent,
        )?;

        if exit_code == 0 {
            if let Ok(Some(extraction)) = extract_commit_message_from_logs(
                log_dir,
                &working_diff,
                commit_agent,
                runtime.logger,
            ) {
                if !extraction.is_agent_error() {
                    let message = extraction.into_message();
                    if !message.is_empty() {
                        return Ok(CommitMessageResult {
                            message,
                            success: true,
                            _log_path: log_file,
                        });
                    }
                }
            }
        }
        // Emergency no-diff failed - generate fallback from diff metadata
        runtime
            .logger
            .warn("Emergency no-diff failed. Generating fallback from diff metadata...");
        let fallback = generate_fallback_commit_message(diff);
        return Ok(CommitMessageResult {
            message: fallback,
            success: true,
            _log_path: log_file,
        });
    } else if last_extraction
        == Some(CommitExtractionResult::AgentError(
            AgentErrorKind::TokenExhausted,
        ))
        && diff_was_pre_truncated
    {
        // Already pre-truncated and still got TokenExhausted - try progressive truncation
        runtime
            .logger
            .warn("Already pre-truncated but still hit token limits. Trying further truncation...");

        // Start with smaller sizes since we already pre-truncated
        let further_truncation_stages = [
            (25_000, "25KB"),
            (10_000, "10KB"),
            (1_000, "file-list-only"),
        ];

        for (size_kb, label) in further_truncation_stages {
            runtime.logger.warn(&format!(
                "Further truncation: Trying {} limit ({})...",
                label,
                size_kb / 1024
            ));

            let truncated_diff = truncate_diff_if_large(diff, size_kb);
            let prompt = prompt_emergency_commit(&truncated_diff);

            let exit_code = run_with_fallback(
                AgentRole::Commit,
                &format!("generate commit message (further truncated {label})"),
                &prompt,
                log_dir,
                runtime,
                registry,
                commit_agent,
            )?;

            if exit_code == 0 {
                if let Ok(Some(extraction)) = extract_commit_message_from_logs(
                    log_dir,
                    &truncated_diff,
                    commit_agent,
                    runtime.logger,
                ) {
                    if extraction.is_agent_error() {
                        // Still TokenExhausted - continue to next stage
                        continue;
                    }
                    // Not an agent error - try to use the message
                    let message = extraction.into_message();
                    if !message.is_empty() {
                        return Ok(CommitMessageResult {
                            message,
                            success: true,
                            _log_path: log_file,
                        });
                    }
                    // Empty message - break out
                    break;
                }
            }
        }

        // All further truncation failed - generate fallback directly
        runtime
            .logger
            .warn("All further truncation stages failed. Generating fallback from diff...");
        let fallback = generate_fallback_commit_message(diff);
        return Ok(CommitMessageResult {
            message: fallback,
            success: true,
            _log_path: log_file,
        });
    }

    // All automated recovery exhausted - use hardcoded fallback as last resort
    runtime.logger.warn("");
    runtime.logger.warn("All recovery methods failed:");
    runtime.logger.warn("  - All 8 prompt variants exhausted");
    runtime
        .logger
        .warn("  - All agents in fallback chain exhausted");
    runtime.logger.warn("  - All truncation stages failed");
    runtime.logger.warn("  - Emergency prompts failed");
    runtime.logger.warn("");
    runtime
        .logger
        .warn("Using hardcoded fallback commit message as last resort.");
    runtime.logger.warn(&format!(
        "Fallback message: \"{HARDCODED_FALLBACK_COMMIT}\""
    ));
    runtime.logger.warn("");

    Ok(CommitMessageResult {
        message: HARDCODED_FALLBACK_COMMIT.to_string(),
        success: true,
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
        // This should never happen after our fixes, but add defensive fallback
        ctx.logger
            .warn("Commit generation returned empty message, using hardcoded fallback...");
        let fallback_message = HARDCODED_FALLBACK_COMMIT.to_string();
        match git_commit(&fallback_message, git_user_name, git_user_email) {
            Ok(Some(oid)) => CommitResultFallback::Success(oid),
            Ok(None) => CommitResultFallback::NoChanges,
            Err(e) => CommitResultFallback::Failed(format!("Failed to create commit: {e}")),
        }
    } else {
        // Create the commit with the generated message
        match git_commit(&result.message, git_user_name, git_user_email) {
            Ok(Some(oid)) => CommitResultFallback::Success(oid),
            Ok(None) => CommitResultFallback::NoChanges,
            Err(e) => CommitResultFallback::Failed(format!("Failed to create commit: {e}")),
        }
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

    // FIRST: Detect agent errors in the output stream BEFORE attempting extraction
    // This handles cases where agents output errors in their result field instead of stderr
    if let Some(error_kind) = detect_agent_errors_in_output(&content) {
        logger.warn(&format!(
            "Detected agent error in output: {}. This should trigger fallback.",
            error_kind.description()
        ));
        return Ok(Some(CommitExtractionResult::AgentError(error_kind)));
    }

    // SECOND: Try structured JSON extraction (new primary method)
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

    #[test]
    fn test_truncate_diff_if_large() {
        let large_diff = "a".repeat(100_000);
        let truncated = truncate_diff_if_large(&large_diff, 10_000);

        // Should be truncated
        assert!(truncated.len() < large_diff.len());
    }

    #[test]
    fn test_truncate_preserves_small_diffs() {
        let small_diff = "a".repeat(100);
        let truncated = truncate_diff_if_large(&small_diff, 10_000);

        // Should not be modified
        assert_eq!(truncated, small_diff);
    }

    #[test]
    fn test_truncate_exactly_at_limit() {
        let diff = "a".repeat(10_000);
        let truncated = truncate_diff_if_large(&diff, 10_000);

        // Should not be modified when at exact limit
        assert_eq!(truncated, diff);
    }

    #[test]
    fn test_truncate_preserves_file_boundaries() {
        let diff = "diff --git a/file1.rs b/file1.rs\n\
            +line1\n\
            +line2\n\
            diff --git a/file2.rs b/file2.rs\n\
            +line3\n\
            +line4\n";
        let large_diff = format!("{}{}", diff, "x".repeat(100_000));
        let truncated = truncate_diff_if_large(&large_diff, 50);

        // Should preserve complete file blocks
        assert!(truncated.contains("diff --git"));
        // Should contain truncation summary
        assert!(truncated.contains("Diff truncated"));
    }

    #[test]
    fn test_prioritize_file_path() {
        // Source files get highest priority
        assert!(prioritize_file_path("src/main.rs") > prioritize_file_path("tests/test.rs"));
        assert!(prioritize_file_path("src/lib.rs") > prioritize_file_path("README.md"));

        // Tests get lower priority than src
        assert!(prioritize_file_path("src/main.rs") > prioritize_file_path("test/test.rs"));

        // Config files get medium priority
        assert!(prioritize_file_path("Cargo.toml") > prioritize_file_path("docs/guide.md"));

        // Docs get lowest priority
        assert!(prioritize_file_path("README.md") < prioritize_file_path("src/main.rs"));
    }

    #[test]
    fn test_truncate_keeps_high_priority_files() {
        let diff = "diff --git a/README.md b/README.md\n\
            +doc change\n\
            diff --git a/src/main.rs b/src/main.rs\n\
            +important change\n\
            diff --git a/tests/test.rs b/tests/test.rs\n\
            +test change\n";

        // With a very small limit, should keep src/main.rs first
        let truncated = truncate_diff_if_large(diff, 80);

        // Should include the high priority src file
        assert!(truncated.contains("src/main.rs"));
    }

    #[test]
    fn test_truncate_lines_to_fit() {
        let lines = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
            "line4".to_string(),
        ];

        // Only fit first 3 lines (each 5 chars + newline = 6)
        let truncated = truncate_lines_to_fit(&lines, 18);

        assert_eq!(truncated.len(), 3);
        // Last line should have truncation marker
        assert!(truncated[2].ends_with("[truncated...]"));
    }

    #[test]
    fn test_hardcoded_fallback_commit() {
        // The hardcoded fallback should always be valid
        let result = validate_commit_message(HARDCODED_FALLBACK_COMMIT);
        assert!(result.is_ok(), "Hardcoded fallback must pass validation");
        assert!(!HARDCODED_FALLBACK_COMMIT.is_empty());
        assert!(HARDCODED_FALLBACK_COMMIT.len() >= 5);
    }
}
