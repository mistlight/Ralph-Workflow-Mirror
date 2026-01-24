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

use super::commit_logging::{
    AttemptOutcome, CommitAttemptLog, CommitLogSession, ExtractionAttempt,
};
use super::context::PhaseContext;
use crate::agents::{AgentRegistry, AgentRole};
use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::files::llm_output_extraction::{
    archive_xml_file, preprocess_raw_content, try_extract_from_file,
    try_extract_xml_commit_with_trace, xml_paths, CommitExtractionResult,
};
use crate::git_helpers::{git_add_all, git_commit, CommitResultFallback};
use crate::logger::Logger;
use crate::pipeline::PipelineRuntime;
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_generate_commit_message_with_diff_with_context,
    prompt_simplified_commit_with_context, prompt_xsd_retry_with_context,
};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

/// Preview a commit message for display (first line, truncated if needed).
fn preview_commit_message(msg: &str) -> String {
    let first_line = msg.lines().next().unwrap_or(msg);
    if first_line.len() > 60 {
        format!("{}...", &first_line[..60])
    } else {
        first_line.to_string()
    }
}

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
pub(crate) const HARDCODED_FALLBACK_COMMIT: &str = "chore: automated commit";

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
///
/// With XSD validation, we now have two strategies. Each strategy supports
/// up to 5 in-session retries with validation feedback.
///
/// The XSD retry mechanism is used internally for in-session retries when
/// validation fails, not as a separate stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitRetryStrategy {
    /// First attempt with normal XML prompt
    Normal,
    /// Simplified XML prompt - more direct instructions
    Simplified,
}

impl CommitRetryStrategy {
    /// Get the description of this retry stage for logging
    const fn description(self) -> &'static str {
        match self {
            Self::Normal => "normal XML prompt",
            Self::Simplified => "simplified XML prompt",
        }
    }

    /// Get the next retry strategy, or None if this is the last stage
    const fn next(self) -> Option<Self> {
        match self {
            Self::Normal => Some(Self::Simplified),
            Self::Simplified => None,
        }
    }

    /// Get the 1-based stage number for this strategy
    const fn stage_number(self) -> usize {
        match self {
            Self::Normal => 1,
            Self::Simplified => 2,
        }
    }

    /// Get the total number of retry stages
    const fn total_stages() -> usize {
        2 // Normal + Simplified
    }

    /// Get the maximum number of in-session retries for this strategy
    const fn max_session_retries(self) -> usize {
        match self {
            Self::Normal => crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS as usize,
            Self::Simplified => crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS as usize,
        }
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
    /// Prompts that were generated during this commit generation (key -> prompt)
    /// This is used for capturing prompts in checkpoints for deterministic resume
    pub generated_prompts: std::collections::HashMap<String, String>,
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

/// Check and pre-truncate diff if it exceeds agent's token limits.
///
/// Returns the (possibly truncated) diff and whether truncation occurred.
fn check_and_pre_truncate_diff(
    diff: &str,
    commit_agent: &str,
    runtime: &PipelineRuntime,
) -> (String, bool) {
    let max_size = max_prompt_size_for_agent(commit_agent);
    if diff.len() > max_size {
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
    }
}

/// Generate the appropriate prompt for the current retry strategy.
///
/// For XSD retry, the xsd_error parameter is used to provide feedback to the agent.
/// Note: XSD retry is handled internally within the session, not as a separate stage.
///
/// For hardened resume, this function uses stored prompts from checkpoint when available
/// to ensure deterministic behavior on resume.
fn generate_prompt_for_strategy(
    strategy: CommitRetryStrategy,
    working_diff: &str,
    template_context: &crate::prompts::TemplateContext,
    xsd_error: Option<&str>,
    prompt_history: &HashMap<String, String>,
    prompt_key: &str,
) -> (String, bool) {
    // Use stored_or_generate pattern for hardened resume
    // The key identifies which prompt variant this is (strategy + retry state)
    let full_prompt_key = if xsd_error.is_some() {
        format!("{}_xsd_retry", prompt_key)
    } else {
        prompt_key.to_string()
    };

    let (prompt, was_replayed) =
        get_stored_or_generate_prompt(&full_prompt_key, prompt_history, || match strategy {
            CommitRetryStrategy::Normal => {
                if let Some(error_msg) = xsd_error {
                    // In-session XSD retry with error feedback
                    prompt_xsd_retry_with_context(template_context, working_diff, error_msg)
                } else {
                    // First attempt with normal XML prompt
                    prompt_generate_commit_message_with_diff_with_context(
                        template_context,
                        working_diff,
                    )
                }
            }
            CommitRetryStrategy::Simplified => {
                if let Some(error_msg) = xsd_error {
                    // In-session XSD retry with error feedback
                    prompt_xsd_retry_with_context(template_context, working_diff, error_msg)
                } else {
                    // Simplified XML prompt
                    prompt_simplified_commit_with_context(template_context, working_diff)
                }
            }
        });

    (prompt, was_replayed)
}

/// Log the current attempt with prompt size information.
fn log_commit_attempt(
    strategy: CommitRetryStrategy,
    prompt_size_kb: usize,
    commit_agent: &str,
    runtime: &PipelineRuntime,
) {
    if strategy == CommitRetryStrategy::Normal {
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
}

/// Handle the extraction result from a commit attempt.
///
/// Returns `Some(result)` if we should return early (success),
/// or `None` if we should continue to the next strategy.
///
/// With XSD validation handling everything, we only check:
/// - Success: Valid commit message extracted
/// - Failure: No valid message (try next strategy)
fn handle_commit_extraction_result(
    extraction_result: anyhow::Result<Option<CommitExtractionResult>>,
    strategy: CommitRetryStrategy,
    log_dir: &str,
    runtime: &PipelineRuntime,
    last_extraction: &mut Option<CommitExtractionResult>,
    attempt_log: &mut CommitAttemptLog,
) -> Option<anyhow::Result<CommitMessageResult>> {
    let log_file = format!("{log_dir}/final.log");

    match extraction_result {
        Ok(Some(extraction)) => {
            // XSD validation already passed - we have a valid commit message
            runtime.logger.info(&format!(
                "Successfully extracted commit message with {strategy}"
            ));
            let message = extraction.clone().into_message();
            attempt_log.set_outcome(AttemptOutcome::Success(message.clone()));
            *last_extraction = Some(extraction);
            // Note: generated_prompts is collected in the context and returned later
            Some(Ok(CommitMessageResult {
                message,
                success: true,
                _log_path: log_file,
                generated_prompts: std::collections::HashMap::new(),
            }))
        }
        Ok(None) => {
            runtime.logger.warn(&format!(
                "No valid commit message extracted with {strategy}, will try next strategy"
            ));
            attempt_log.set_outcome(AttemptOutcome::ExtractionFailed(
                "No valid commit message extracted".to_string(),
            ));
            None // Continue to next strategy
        }
        Err(e) => {
            runtime.logger.error(&format!(
                "Failed to extract commit message with {strategy}: {e}"
            ));
            attempt_log.set_outcome(AttemptOutcome::ExtractionFailed(e.to_string()));
            None // Continue to next strategy
        }
    }
}

/// Build the list of agents to try for commit generation.
///
/// This helper function constructs the ordered list of agents to try,
/// starting with the primary agent and followed by configured fallbacks.
fn build_agents_to_try<'a>(fallbacks: &'a [&'a str], primary_agent: &'a str) -> Vec<&'a str> {
    let mut agents_to_try: Vec<&'a str> = vec![primary_agent];
    for fb in fallbacks {
        if *fb != primary_agent && !agents_to_try.contains(fb) {
            agents_to_try.push(fb);
        }
    }
    agents_to_try
}

/// Context for a commit attempt, bundling related state to avoid too many arguments.
struct CommitAttemptContext<'a> {
    /// The diff being processed
    working_diff: &'a str,
    /// Log directory path
    log_dir: &'a str,
    /// Whether the diff was pre-truncated
    diff_was_truncated: bool,
    /// Template context for user template overrides
    template_context: &'a crate::prompts::TemplateContext,
    /// Prompt history for checkpoint/resume determinism
    prompt_history: &'a HashMap<String, String>,
    /// Unique key for this commit generation attempt
    prompt_key: String,
    /// Output map to capture prompts that were newly generated (not replayed)
    /// This is used for checkpoint/resume determinism
    generated_prompts: &'a mut std::collections::HashMap<String, String>,
}

/// Run a single commit attempt with the given strategy and agent.
///
/// This function runs a single agent (not using fallback) to allow for
/// per-agent prompt variant cycling with in-session XSD validation retry.
/// Returns Some(result) if we should return early (success or hard error),
/// or None if we should continue to the next strategy.
fn run_commit_attempt_with_agent(
    strategy: CommitRetryStrategy,
    ctx: &mut CommitAttemptContext<'_>,
    runtime: &mut PipelineRuntime,
    registry: &AgentRegistry,
    agent: &str,
    last_extraction: &mut Option<CommitExtractionResult>,
    session: &mut CommitLogSession,
) -> Option<anyhow::Result<CommitMessageResult>> {
    // Get the agent config
    let Some(agent_config) = registry.resolve_config(agent) else {
        runtime
            .logger
            .warn(&format!("Agent '{agent}' not found in registry, skipping"));
        let mut attempt_log = session.new_attempt(agent, strategy.description());
        attempt_log.set_outcome(AttemptOutcome::ExtractionFailed(format!(
            "Agent '{agent}' not found in registry"
        )));
        let _ = attempt_log.write_to_file(session.run_dir());
        return None;
    };

    // Build the command for this agent
    let cmd_str = agent_config.build_cmd(true, true, false);
    let logfile = format!("{}/{}_latest.log", ctx.log_dir, agent.replace('/', "-"));

    // In-session retry loop with XSD validation feedback
    let max_retries = strategy.max_session_retries();
    let mut xsd_error: Option<String> = None;

    for retry_num in 0..max_retries {
        // Before each retry, check if the XML file is writable and clean up if locked
        if retry_num > 0 {
            use crate::files::io::check_and_cleanup_xml_before_retry;
            use std::path::Path;
            let xml_path =
                Path::new(crate::files::llm_output_extraction::xml_paths::COMMIT_MESSAGE_XML);
            let _ = check_and_cleanup_xml_before_retry(xml_path, runtime.logger);
        }

        // For initial attempt, xsd_error is None
        // For retries, we use the XSD error to guide the agent
        // Build prompt key for this attempt (strategy-specific)
        let prompt_key = format!("{}_{}", ctx.prompt_key, strategy.stage_number());
        let (prompt, was_replayed) = generate_prompt_for_strategy(
            strategy,
            ctx.working_diff,
            ctx.template_context,
            xsd_error.as_deref(),
            ctx.prompt_history,
            &prompt_key,
        );
        let prompt_size_kb = prompt.len() / 1024;

        // Log if using stored prompt for determinism
        if was_replayed && retry_num == 0 {
            runtime.logger.info(&format!(
                "Using stored prompt from checkpoint for determinism: {}",
                prompt_key
            ));
        } else if !was_replayed {
            // Capture the newly generated prompt for checkpoint/resume
            ctx.generated_prompts
                .insert(prompt_key.clone(), prompt.clone());
        }

        // Create attempt log
        let mut attempt_log = session.new_attempt(agent, strategy.description());
        attempt_log.set_prompt_size(prompt.len());
        attempt_log.set_diff_info(ctx.working_diff.len(), ctx.diff_was_truncated);

        // Log retry attempt if not first attempt
        if retry_num > 0 {
            runtime.logger.info(&format!(
                "  In-session retry {}/{} for XSD validation",
                retry_num,
                max_retries - 1
            ));
            if let Some(ref error) = xsd_error {
                runtime.logger.info(&format!("  XSD error: {}", error));
            }
        } else {
            log_commit_attempt(strategy, prompt_size_kb, agent, runtime);
        }

        // Run the agent directly (without fallback)
        let exit_code = match crate::pipeline::run_with_prompt(
            &crate::pipeline::PromptCommand {
                label: &format!("generate commit message ({})", strategy.description()),
                display_name: agent,
                cmd_str: &cmd_str,
                prompt: &prompt,
                logfile: &logfile,
                parser_type: agent_config.json_parser,
                env_vars: &agent_config.env_vars,
            },
            runtime,
        ) {
            Ok(result) => result.exit_code,
            Err(e) => {
                runtime.logger.error(&format!("Failed to run agent: {e}"));
                attempt_log.set_outcome(AttemptOutcome::ExtractionFailed(format!(
                    "Agent execution failed: {e}"
                )));
                let _ = attempt_log.write_to_file(session.run_dir());
                return None;
            }
        };

        if exit_code != 0 {
            runtime
                .logger
                .warn("Commit agent failed, checking logs for partial output...");
        }

        let extraction_result = extract_commit_message_from_logs_with_trace(
            ctx.log_dir,
            ctx.working_diff,
            agent,
            runtime.logger,
            &mut attempt_log,
        );

        // Check if we got a valid commit message or need to retry for XSD errors
        match &extraction_result {
            Ok(Some(_)) => {
                // XSD validation passed - we have a valid commit message
                let result = handle_commit_extraction_result(
                    extraction_result,
                    strategy,
                    ctx.log_dir,
                    runtime,
                    last_extraction,
                    &mut attempt_log,
                );

                if let Err(e) = attempt_log.write_to_file(session.run_dir()) {
                    runtime
                        .logger
                        .warn(&format!("Failed to write attempt log: {e}"));
                }

                return result;
            }
            _ => {
                // Extraction failed - continue to check for XSD errors for retry
            }
        };

        // Check extraction attempts for XSD validation errors
        let xsd_error_msg = attempt_log
            .extraction_attempts
            .iter()
            .find(|attempt| attempt.detail.contains("XSD validation failed"))
            .map(|attempt| attempt.detail.clone());

        if let Some(ref error_msg) = xsd_error_msg {
            runtime
                .logger
                .warn(&format!("  XSD validation failed: {}", error_msg));

            if retry_num < max_retries - 1 {
                // Extract just the error message (after "XSD validation failed: ")
                let error = error_msg
                    .strip_prefix("XSD validation failed: ")
                    .unwrap_or(error_msg);

                // Store error for next retry attempt
                xsd_error = Some(error.to_string());

                // Write attempt log but don't return yet
                attempt_log.set_outcome(AttemptOutcome::XsdValidationFailed(error.to_string()));
                let _ = attempt_log.write_to_file(session.run_dir());

                // Continue to next retry iteration
                continue;
            } else {
                // No more retries - fall through to handle as extraction failure
                runtime
                    .logger
                    .warn("  No more in-session retries remaining");
            }
        }

        // Handle extraction result (failure cases)
        let result = handle_commit_extraction_result(
            extraction_result,
            strategy,
            ctx.log_dir,
            runtime,
            last_extraction,
            &mut attempt_log,
        );

        // Write the attempt log
        if let Err(e) = attempt_log.write_to_file(session.run_dir()) {
            runtime
                .logger
                .warn(&format!("Failed to write attempt log: {e}"));
        }

        // If we got a result (success or hard error), return it
        if result.is_some() {
            return result;
        }

        // Otherwise, if this was a retry and we exhausted retries, break out
        if retry_num >= max_retries - 1 {
            break;
        }

        // For non-XSD errors, we don't retry in-session - move to next strategy
        break;
    }

    None
}

/// Return the hardcoded fallback commit message as last resort.
fn return_hardcoded_fallback(
    log_file: &str,
    runtime: &PipelineRuntime,
    generated_prompts: std::collections::HashMap<String, String>,
) -> CommitMessageResult {
    runtime.logger.warn("");
    runtime.logger.warn("All recovery methods failed:");
    runtime.logger.warn("  - All 9 prompt variants exhausted");
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

    CommitMessageResult {
        message: HARDCODED_FALLBACK_COMMIT.to_string(),
        success: true,
        _log_path: log_file.to_string(),
        generated_prompts,
    }
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
/// # Agent Cycling Behavior
///
/// This function implements proper strategy-first cycling by trying each strategy
/// with all agents before moving to the next strategy:
/// - Strategy 1 (initial): Agent 1 → Agent 2 → Agent 3
/// - Strategy 2 (strict JSON): Agent 1 → Agent 2 → Agent 3
/// - Strategy 3 (strict JSON V2): Agent 1 → Agent 2 → Agent 3
/// - etc.
///
/// This approach is more efficient because if a particular strategy works well
/// with any agent, we succeed quickly rather than exhausting all strategies
/// on the first agent before trying others.
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message for
/// * `registry` - The agent registry for resolving agents and fallbacks
/// * `runtime` - The pipeline runtime for execution services
/// * `commit_agent` - The primary agent to use for commit generation
/// * `template_context` - Template context for user template overrides
/// * `prompt_history` - Prompt history for checkpoint/resume determinism
///
/// # Returns
///
/// Returns `Ok(CommitMessageResult)` with the generated message and metadata.
pub fn generate_commit_message(
    diff: &str,
    registry: &AgentRegistry,
    runtime: &mut PipelineRuntime,
    commit_agent: &str,
    template_context: &crate::prompts::TemplateContext,
    prompt_history: &HashMap<String, String>,
) -> anyhow::Result<CommitMessageResult> {
    let log_dir = ".agent/logs/commit_generation";
    let log_file = format!("{log_dir}/final.log");

    fs::create_dir_all(log_dir)?;
    runtime.logger.info("Generating commit message...");

    // Create a logging session for this commit generation run
    let mut session = create_commit_log_session(log_dir, runtime);
    let (working_diff, diff_was_pre_truncated) =
        check_and_pre_truncate_diff(diff, commit_agent, runtime);

    let fallbacks = registry.available_fallbacks(AgentRole::Commit);
    let agents_to_try = build_agents_to_try(&fallbacks, commit_agent);

    let mut last_extraction: Option<CommitExtractionResult> = None;
    let mut total_attempts = 0;

    // Generate a unique prompt key for this commit generation attempt
    // Use timestamp-based key to ensure uniqueness across different commit generations
    let prompt_key = format!(
        "commit_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    // Map to capture newly generated prompts for checkpoint/resume
    let mut generated_prompts = std::collections::HashMap::new();

    let mut attempt_ctx = CommitAttemptContext {
        working_diff: &working_diff,
        log_dir,
        diff_was_truncated: diff_was_pre_truncated,
        template_context,
        prompt_history,
        prompt_key,
        generated_prompts: &mut generated_prompts,
    };

    // Try each agent with all prompt variants
    if let Some(result) = try_agents_with_strategies(
        &agents_to_try,
        &mut attempt_ctx,
        runtime,
        registry,
        &mut last_extraction,
        &mut session,
        &mut total_attempts,
    ) {
        log_completion(runtime, &session, total_attempts, &result);
        // Include generated prompts in the result
        return result.map(|mut r| {
            r.generated_prompts = generated_prompts;
            r
        });
    }

    // Handle fallback cases
    let fallback_ctx = CommitFallbackContext {
        log_file: &log_file,
    };
    handle_commit_fallbacks(
        &fallback_ctx,
        runtime,
        &session,
        total_attempts,
        last_extraction.as_ref(),
        generated_prompts,
    )
}

/// Create a commit log session, with fallback.
fn create_commit_log_session(log_dir: &str, runtime: &mut PipelineRuntime) -> CommitLogSession {
    match CommitLogSession::new(log_dir) {
        Ok(s) => {
            runtime.logger.info(&format!(
                "Commit logs will be written to: {}",
                s.run_dir().display()
            ));
            s
        }
        Err(e) => {
            runtime
                .logger
                .warn(&format!("Failed to create log session: {e}"));
            CommitLogSession::new(log_dir).unwrap_or_else(|_| {
                CommitLogSession::new("/tmp/ralph-commit-logs").expect("fallback session")
            })
        }
    }
}

/// Try all agents with their strategy variants.
///
/// This function implements strategy-first cycling:
/// - Outer loop: Iterate through strategies
/// - Inner loop: Try all agents with the current strategy
/// - Only advance to next strategy if ALL agents failed with current strategy
///
/// This ensures each strategy gets the best chance to succeed with all
/// available agents before we try degraded fallback prompts.
fn try_agents_with_strategies(
    agents: &[&str],
    ctx: &mut CommitAttemptContext<'_>,
    runtime: &mut PipelineRuntime,
    registry: &AgentRegistry,
    last_extraction: &mut Option<CommitExtractionResult>,
    session: &mut CommitLogSession,
    total_attempts: &mut usize,
) -> Option<anyhow::Result<CommitMessageResult>> {
    let mut strategy = CommitRetryStrategy::Normal;
    loop {
        runtime.logger.info(&format!(
            "Trying strategy {}/{}: {}",
            strategy.stage_number(),
            CommitRetryStrategy::total_stages(),
            strategy.description()
        ));

        for (agent_idx, agent) in agents.iter().enumerate() {
            runtime.logger.info(&format!(
                "  - Agent {}/{}: {agent}",
                agent_idx + 1,
                agents.len()
            ));

            *total_attempts += 1;
            if let Some(result) = run_commit_attempt_with_agent(
                strategy,
                ctx,
                runtime,
                registry,
                agent,
                last_extraction,
                session,
            ) {
                return Some(result);
            }
        }

        runtime.logger.warn(&format!(
            "All agents failed for strategy: {}",
            strategy.description()
        ));

        match strategy.next() {
            Some(next) => strategy = next,
            None => break,
        }
    }
    None
}

/// Log completion info and write session summary on success.
fn log_completion(
    runtime: &mut PipelineRuntime,
    session: &CommitLogSession,
    total_attempts: usize,
    result: &anyhow::Result<CommitMessageResult>,
) {
    if let Ok(ref commit_result) = result {
        let _ = session.write_summary(
            total_attempts,
            &format!(
                "SUCCESS: {}",
                preview_commit_message(&commit_result.message)
            ),
        );
    }
    runtime.logger.info(&format!(
        "Commit generation complete after {total_attempts} attempts. Logs: {}",
        session.run_dir().display()
    ));
}

/// Context for commit fallback handling.
struct CommitFallbackContext<'a> {
    log_file: &'a str,
}

/// Handle fallback cases after all agents exhausted.
///
/// With XSD validation handling everything, the fallback logic is simple:
/// - If we have a last extraction with a valid message, use it
/// - Otherwise, use the hardcoded fallback
fn handle_commit_fallbacks(
    ctx: &CommitFallbackContext<'_>,
    runtime: &mut PipelineRuntime,
    session: &CommitLogSession,
    total_attempts: usize,
    last_extraction: Option<&CommitExtractionResult>,
    generated_prompts: std::collections::HashMap<String, String>,
) -> anyhow::Result<CommitMessageResult> {
    // Use message from last extraction if available
    // (XSD validation already passed if we have an extraction)
    if let Some(extraction) = last_extraction {
        let message = extraction.clone().into_message();
        let _ = session.write_summary(
            total_attempts,
            &format!("LAST_EXTRACTION: {}", preview_commit_message(&message)),
        );
        runtime.logger.info(&format!(
            "Commit generation complete after {total_attempts} attempts. Logs: {}",
            session.run_dir().display()
        ));
        return Ok(CommitMessageResult {
            message,
            success: true,
            _log_path: ctx.log_file.to_string(),
            generated_prompts,
        });
    }

    // Hardcoded fallback as last resort
    let _ = session.write_summary(
        total_attempts,
        &format!("HARDCODED_FALLBACK: {HARDCODED_FALLBACK_COMMIT}"),
    );
    runtime.logger.info(&format!(
        "Commit generation complete after {total_attempts} attempts (hardcoded fallback). Logs: {}",
        session.run_dir().display()
    ));
    Ok(return_hardcoded_fallback(
        ctx.log_file,
        runtime,
        generated_prompts,
    ))
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

    // Track execution start for commit generation
    let start_time = std::time::Instant::now();

    // Set up the runtime
    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        executor: ctx.executor,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Generate commit message using the standard pipeline
    let result = match generate_commit_message(
        diff,
        ctx.registry,
        &mut runtime,
        commit_agent,
        ctx.template_context,
        &ctx.prompt_history,
    ) {
        Ok(r) => r,
        Err(e) => {
            // Record failed generation in execution history
            ctx.execution_history.add_step(
                ExecutionStep::new(
                    "commit",
                    0,
                    "commit_generation",
                    StepOutcome::failure(format!("Failed to generate commit message: {e}"), false),
                )
                .with_agent(commit_agent)
                .with_duration(start_time.elapsed().as_secs()),
            );
            return CommitResultFallback::Failed(format!("Failed to generate commit message: {e}"));
        }
    };

    // Capture generated prompts for checkpoint/resume
    for (key, prompt) in result.generated_prompts {
        ctx.capture_prompt(&key, &prompt);
    }

    // Check if generation succeeded
    if !result.success || result.message.trim().is_empty() {
        // This should never happen after our fixes, but add defensive fallback
        ctx.logger
            .warn("Commit generation returned empty message, using hardcoded fallback...");
        let fallback_message = HARDCODED_FALLBACK_COMMIT.to_string();
        let commit_result = match git_commit(&fallback_message, git_user_name, git_user_email) {
            Ok(Some(oid)) => CommitResultFallback::Success(oid),
            Ok(None) => CommitResultFallback::NoChanges,
            Err(e) => CommitResultFallback::Failed(format!("Failed to create commit: {e}")),
        };
        // Record completion with fallback in execution history
        let outcome = match &commit_result {
            CommitResultFallback::Success(oid) => StepOutcome::success(
                Some(format!("Commit created: {oid}")),
                vec![".".to_string()],
            ),
            CommitResultFallback::NoChanges => {
                StepOutcome::skipped("No changes to commit".to_string())
            }
            CommitResultFallback::Failed(e) => StepOutcome::failure(e.clone(), false),
        };
        ctx.execution_history.add_step(
            ExecutionStep::new("commit", 0, "commit_generation", outcome)
                .with_agent(commit_agent)
                .with_duration(start_time.elapsed().as_secs()),
        );
        commit_result
    } else {
        // Create the commit with the generated message
        let commit_result = match git_commit(&result.message, git_user_name, git_user_email) {
            Ok(Some(oid)) => CommitResultFallback::Success(oid),
            Ok(None) => CommitResultFallback::NoChanges,
            Err(e) => CommitResultFallback::Failed(format!("Failed to create commit: {e}")),
        };
        // Record completion in execution history
        let outcome = match &commit_result {
            CommitResultFallback::Success(oid) => StepOutcome::success(
                Some(format!("Commit created: {oid}")),
                vec![".".to_string()],
            ),
            CommitResultFallback::NoChanges => {
                StepOutcome::skipped("No changes to commit".to_string())
            }
            CommitResultFallback::Failed(e) => StepOutcome::failure(e.clone(), false),
        };
        let oid_for_history = match &commit_result {
            CommitResultFallback::Success(oid) => Some(oid.to_string()),
            _ => None,
        };
        let mut step = ExecutionStep::new("commit", 0, "commit_generation", outcome)
            .with_agent(commit_agent)
            .with_duration(start_time.elapsed().as_secs());
        if let Some(oid) = &oid_for_history {
            step = step.with_git_commit_oid(oid);
        }
        ctx.execution_history.add_step(step);
        commit_result
    }
}

/// Import types needed for parsing trace helpers.
use crate::phases::commit_logging::{ParsingTraceLog, ParsingTraceStep};

/// Write parsing trace log to file with error handling.
fn write_parsing_trace_with_logging(
    parsing_trace: &ParsingTraceLog,
    log_dir: &str,
    logger: &Logger,
) {
    if let Err(e) = parsing_trace.write_to_file(std::path::Path::new(log_dir)) {
        logger.warn(&format!("Failed to write parsing trace log: {e}"));
    }
}

/// Try XML extraction and record in parsing trace.
/// Returns `Some(result)` if extraction succeeded (XSD validation passed), `None` otherwise.
fn try_xml_extraction_traced(
    content: &str,
    step_number: &mut usize,
    parsing_trace: &mut ParsingTraceLog,
    logger: &Logger,
    attempt_log: &mut CommitAttemptLog,
    log_dir: &str,
) -> Option<CommitExtractionResult> {
    // Try file-based extraction first - allows agents to write XML to .agent/tmp/commit_message.xml
    let xml_file_path = Path::new(xml_paths::COMMIT_MESSAGE_XML);
    let (xml_result, xml_detail) = if let Some(file_xml) = try_extract_from_file(xml_file_path) {
        // Found XML in file - validate it
        let (validated, detail) = try_extract_xml_commit_with_trace(&file_xml);
        let detail = format!("file-based: {}", detail);
        (validated, detail)
    } else {
        // Fall back to log content extraction
        try_extract_xml_commit_with_trace(content)
    };
    logger.info(&format!("  ✓ XML extraction: {xml_detail}"));

    parsing_trace.add_step(
        ParsingTraceStep::new(*step_number, "XML Extraction")
            .with_input(&content[..content.len().min(1000)])
            .with_result(xml_result.as_deref().unwrap_or("[No XML found]"))
            .with_success(xml_result.is_some())
            .with_details(&xml_detail),
    );
    *step_number += 1;

    if let Some(message) = xml_result {
        // XSD validation already passed inside try_extract_xml_commit_with_trace
        // Archive the XML file now that it's been successfully processed
        archive_xml_file(xml_file_path);

        attempt_log.add_extraction_attempt(ExtractionAttempt::success("XML", xml_detail));
        parsing_trace.set_final_message(&message);
        write_parsing_trace_with_logging(parsing_trace, log_dir, logger);
        return Some(CommitExtractionResult::new(message));
    }

    // XML extraction or XSD validation failed - file stays in place for agent to edit
    attempt_log.add_extraction_attempt(ExtractionAttempt::failure("XML", xml_detail));
    logger.info("  ✗ XML extraction failed");
    None
}

/// Extract a commit message from agent logs with full tracing for diagnostics.
///
/// Records all extraction attempts in the provided `CommitAttemptLog` for debugging.
///
/// This function also creates and writes a `ParsingTraceLog` that captures
/// detailed information about each extraction step, including the exact
/// content being processed and XSD validation results.
fn extract_commit_message_from_logs_with_trace(
    log_dir: &str,
    _diff: &str,
    _agent_cmd: &str,
    logger: &Logger,
    attempt_log: &mut CommitAttemptLog,
) -> anyhow::Result<Option<CommitExtractionResult>> {
    // Create parsing trace log
    let mut parsing_trace = ParsingTraceLog::new(
        attempt_log.attempt_number,
        &attempt_log.agent,
        &attempt_log.strategy,
    );

    // Read and preprocess log content
    let Some(content) = read_log_content_with_trace(log_dir, logger, attempt_log)? else {
        return Ok(None);
    };

    // Set raw output in parsing trace
    parsing_trace.set_raw_output(&content);

    let mut step_number = 1;

    // XML-only extraction with XSD validation
    // The XML extraction includes flexible parsing with 4 strategies and XSD validation
    // If XSD validation fails, the error is returned for in-session retry
    if let Some(result) = try_xml_extraction_traced(
        &content,
        &mut step_number,
        &mut parsing_trace,
        logger,
        attempt_log,
        log_dir,
    ) {
        return Ok(Some(result));
    }

    // XML extraction failed - add final failure step to parsing trace
    parsing_trace.add_step(
        ParsingTraceStep::new(step_number, "XML Extraction Failed")
            .with_input(&content[..content.len().min(1000)])
            .with_success(false)
            .with_details("No valid XML found or XSD validation failed"),
    );

    write_parsing_trace_with_logging(&parsing_trace, log_dir, logger);

    // Return None to trigger next strategy/agent fallback
    // The in-session retry loop will have already attempted XSD validation retries
    // if the error was an XSD validation failure (detected in attempt_log)
    Ok(None)
}

/// Read and preprocess log content for extraction.
fn read_log_content_with_trace(
    log_dir: &str,
    logger: &Logger,
    attempt_log: &mut CommitAttemptLog,
) -> anyhow::Result<Option<String>> {
    let log_path = find_most_recent_log(log_dir)?;
    let Some(log_file) = log_path else {
        logger.warn("No log files found in commit generation directory");
        attempt_log.add_extraction_attempt(ExtractionAttempt::failure(
            "File",
            "No log files found".to_string(),
        ));
        return Ok(None);
    };

    logger.info(&format!(
        "Reading commit message from log: {}",
        log_file.display()
    ));

    let mut content = String::new();
    let mut file = File::open(&log_file)?;
    file.read_to_string(&mut content)?;
    attempt_log.set_raw_output(&content);

    if content.trim().is_empty() {
        logger.warn("Log file is empty");
        attempt_log.add_extraction_attempt(ExtractionAttempt::failure(
            "File",
            "Log file is empty".to_string(),
        ));
        return Ok(None);
    }

    // Apply preprocessing
    Ok(Some(preprocess_raw_content(&content)))
}

/// Find the most recently modified log file in a directory or matching a prefix pattern.
///
/// Supports two modes:
/// 1. **Directory mode**: If `log_path` is a directory, find the most recent `.log` file in it
/// 2. **Prefix mode**: If `log_path` is not a directory, treat it as a prefix pattern and
///    search for files matching `{prefix}*.log` in the parent directory
///
/// # Arguments
///
/// * `log_path` - Either a directory path or a prefix pattern for log files
///
/// # Returns
///
/// * `Ok(Some(path))` - Path to the most recent log file
/// * `Ok(None)` - No log files found
/// * `Err(e)` - Error reading directory
fn find_most_recent_log(log_path: &str) -> anyhow::Result<Option<std::path::PathBuf>> {
    let path = std::path::PathBuf::from(log_path);

    // Mode 1: If path is a directory, search for .log files with empty prefix (matches all)
    if path.is_dir() {
        return find_most_recent_log_with_prefix(&path, "");
    }

    // Mode 2: Prefix pattern mode - search parent directory for files starting with the prefix
    let parent_dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    };

    if !parent_dir.exists() {
        return Ok(None);
    }

    let base_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    find_most_recent_log_with_prefix(&parent_dir, base_name)
}

/// Find the most recently modified log file matching a prefix pattern.
///
/// Only matches `.log` files that start with `prefix`. If `prefix` is empty,
/// matches any `.log` file in the directory.
fn find_most_recent_log_with_prefix(
    dir: &std::path::Path,
    prefix: &str,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }

    let entries = fs::read_dir(dir)?;
    let mut most_recent: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

    for entry in entries.flatten() {
        let path = entry.path();

        // Only look at .log files that start with the prefix (or any .log file if prefix is empty)
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            if !file_name.starts_with(prefix)
                || path.extension().and_then(|s| s.to_str()) != Some("log")
            {
                continue;
            }
        } else {
            continue;
        }

        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                match &most_recent {
                    None => {
                        most_recent = Some((path, modified));
                    }
                    Some((_, prev_modified)) if modified > *prev_modified => {
                        most_recent = Some((path, modified));
                    }
                    _ => {}
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
        // The hardcoded fallback should always be a valid conventional commit
        use crate::files::llm_output_extraction::is_conventional_commit_subject;
        assert!(
            is_conventional_commit_subject(HARDCODED_FALLBACK_COMMIT),
            "Hardcoded fallback must be a valid conventional commit"
        );
        assert!(!HARDCODED_FALLBACK_COMMIT.is_empty());
        assert!(HARDCODED_FALLBACK_COMMIT.len() >= 5);
    }
}
