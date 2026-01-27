//! Per-attempt logging infrastructure for commit message generation.
//!
//! This module provides detailed logging for each commit generation attempt,
//! creating a clear audit trail for debugging parsing failures. Each attempt
//! produces a unique numbered log file that captures:
//! - Prompt information
//! - Raw agent output
//! - All extraction attempts with reasons
//! - Validation results
//! - Final outcome
//!
//! Log files are organized by session to prevent overwrites and allow
//! comparison across multiple attempts.

use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

use crate::common::truncate_text;
use crate::workspace::Workspace;

/// Maximum length for log line preview in commit logging.
const LOG_LINE_PREVIEW_LENGTH: usize = 60;

/// Maximum length for agent name in filenames (to avoid path length issues).
const MAX_AGENT_NAME_LENGTH: usize = 20;

/// Represents a single step in the parsing trace log.
#[derive(Debug, Clone)]
pub struct ParsingTraceStep {
    /// Step number in the trace
    pub step_number: usize,
    /// Description of what was attempted
    pub description: String,
    /// Input content for this step
    pub input: Option<String>,
    /// Result/output of this step
    pub result: Option<String>,
    /// Whether this step succeeded
    pub success: bool,
    /// Additional details or error message
    pub details: String,
}

impl ParsingTraceStep {
    /// Create a new parsing trace step.
    pub fn new(step_number: usize, description: &str) -> Self {
        Self {
            step_number,
            description: description.to_string(),
            input: None,
            result: None,
            success: false,
            details: String::new(),
        }
    }

    /// Set the input for this step.
    pub fn with_input(mut self, input: &str) -> Self {
        // Truncate input if too large
        const MAX_INPUT_SIZE: usize = 10_000;
        self.input = if input.len() > MAX_INPUT_SIZE {
            Some(format!(
                "{}\n\n[... input truncated {} bytes ...]",
                &input[..MAX_INPUT_SIZE / 2],
                input.len() - MAX_INPUT_SIZE
            ))
        } else {
            Some(input.to_string())
        };
        self
    }

    /// Set the result for this step.
    pub fn with_result(mut self, result: &str) -> Self {
        // Truncate result if too large
        const MAX_RESULT_SIZE: usize = 10_000;
        self.result = if result.len() > MAX_RESULT_SIZE {
            Some(format!(
                "{}\n\n[... result truncated {} bytes ...]",
                &result[..MAX_RESULT_SIZE / 2],
                result.len() - MAX_RESULT_SIZE
            ))
        } else {
            Some(result.to_string())
        };
        self
    }

    /// Set whether this step succeeded.
    pub const fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    /// Set additional details.
    pub fn with_details(mut self, details: &str) -> Self {
        self.details = details.to_string();
        self
    }
}

/// Detailed parsing trace log for commit message extraction.
///
/// This log captures each step of the extraction process, showing:
/// - What extraction method was tried (XML, JSON, pattern-based)
/// - The exact content being processed at each step
/// - Validation results and why they passed/failed
/// - The final extracted message
///
/// This is separate from the attempt log and written to `parsing_trace.log`.
#[derive(Debug, Clone)]
pub struct ParsingTraceLog {
    /// Attempt number this trace belongs to
    pub attempt_number: usize,
    /// Agent that generated the output
    pub agent: String,
    /// Strategy used
    pub strategy: String,
    /// Raw output from the agent
    pub raw_output: Option<String>,
    /// Individual parsing steps
    pub steps: Vec<ParsingTraceStep>,
    /// Final extracted message (if any)
    pub final_message: Option<String>,
    /// Timestamp when trace started
    pub timestamp: DateTime<Local>,
}

impl ParsingTraceLog {
    /// Create a new parsing trace log.
    pub fn new(attempt_number: usize, agent: &str, strategy: &str) -> Self {
        Self {
            attempt_number,
            agent: agent.to_string(),
            strategy: strategy.to_string(),
            raw_output: None,
            steps: Vec::new(),
            final_message: None,
            timestamp: Local::now(),
        }
    }

    /// Set the raw output from the agent.
    pub fn set_raw_output(&mut self, output: &str) {
        const MAX_OUTPUT_SIZE: usize = 50_000;
        self.raw_output = if output.len() > MAX_OUTPUT_SIZE {
            Some(format!(
                "{}\n\n[... raw output truncated {} bytes ...]\n\n{}",
                &output[..MAX_OUTPUT_SIZE / 2],
                output.len() - MAX_OUTPUT_SIZE,
                &output[output.len() - MAX_OUTPUT_SIZE / 2..]
            ))
        } else {
            Some(output.to_string())
        };
    }

    /// Add a parsing step to the trace.
    pub fn add_step(&mut self, step: ParsingTraceStep) {
        self.steps.push(step);
    }

    /// Set the final extracted message.
    pub fn set_final_message(&mut self, message: &str) {
        self.final_message = Some(message.to_string());
    }

    /// Write this trace to a file using workspace abstraction.
    ///
    /// This is the architecture-conformant version that uses the workspace trait
    /// instead of direct filesystem access.
    ///
    /// # Arguments
    ///
    /// * `log_dir` - Directory to write the trace file to (relative to workspace)
    /// * `workspace` - The workspace to use for filesystem operations
    ///
    /// # Returns
    ///
    /// Path to the written trace file on success.
    pub fn write_to_workspace(
        &self,
        log_dir: &Path,
        workspace: &dyn Workspace,
    ) -> std::io::Result<PathBuf> {
        let trace_path = log_dir.join(format!(
            "attempt_{:03}_parsing_trace.log",
            self.attempt_number
        ));

        // Build the content in memory first
        let mut content = String::new();
        Self::write_header_to_string(&mut content, self);
        Self::write_raw_output_to_string(&mut content, self);
        Self::write_parsing_steps_to_string(&mut content, self);
        Self::write_final_message_to_string(&mut content, self);
        Self::write_footer_to_string(&mut content);

        // Write using workspace
        workspace.create_dir_all(log_dir)?;
        workspace.write(&trace_path, &content)?;

        Ok(trace_path)
    }

    // String-based write helpers for workspace support
    fn write_header_to_string(s: &mut String, trace: &Self) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "================================================================================"
        );
        let _ = writeln!(
            s,
            "PARSING TRACE LOG - Attempt #{:03}",
            trace.attempt_number
        );
        let _ = writeln!(
            s,
            "================================================================================"
        );
        let _ = writeln!(s);
        let _ = writeln!(s, "Agent:     {}", trace.agent);
        let _ = writeln!(s, "Strategy:  {}", trace.strategy);
        let _ = writeln!(
            s,
            "Timestamp: {}",
            trace.timestamp.format("%Y-%m-%d %H:%M:%S %Z")
        );
        let _ = writeln!(s);
    }

    fn write_raw_output_to_string(s: &mut String, trace: &Self) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "--------------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "RAW AGENT OUTPUT");
        let _ = writeln!(
            s,
            "--------------------------------------------------------------------------------"
        );
        let _ = writeln!(s);
        match &trace.raw_output {
            Some(output) => {
                let _ = writeln!(s, "{output}");
            }
            None => {
                let _ = writeln!(s, "[No raw output captured]");
            }
        }
        let _ = writeln!(s);
    }

    fn write_parsing_steps_to_string(s: &mut String, trace: &Self) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "--------------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "PARSING STEPS");
        let _ = writeln!(
            s,
            "--------------------------------------------------------------------------------"
        );
        let _ = writeln!(s);

        if trace.steps.is_empty() {
            let _ = writeln!(s, "[No parsing steps recorded]");
        } else {
            for step in &trace.steps {
                let status = if step.success {
                    "✓ SUCCESS"
                } else {
                    "✗ FAILED"
                };
                let _ = writeln!(s, "{}. {} [{}]", step.step_number, step.description, status);
                let _ = writeln!(s);

                if let Some(input) = &step.input {
                    let _ = writeln!(s, "   INPUT:");
                    for line in input.lines() {
                        let _ = writeln!(s, "   {line}");
                    }
                    let _ = writeln!(s);
                }

                if let Some(result) = &step.result {
                    let _ = writeln!(s, "   RESULT:");
                    for line in result.lines() {
                        let _ = writeln!(s, "   {line}");
                    }
                    let _ = writeln!(s);
                }

                if !step.details.is_empty() {
                    let _ = writeln!(s, "   DETAILS: {}", step.details);
                    let _ = writeln!(s);
                }
            }
        }
        let _ = writeln!(s);
    }

    fn write_final_message_to_string(s: &mut String, trace: &Self) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "--------------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "FINAL EXTRACTED MESSAGE");
        let _ = writeln!(
            s,
            "--------------------------------------------------------------------------------"
        );
        let _ = writeln!(s);
        match &trace.final_message {
            Some(message) => {
                let _ = writeln!(s, "{message}");
            }
            None => {
                let _ = writeln!(s, "[No message extracted]");
            }
        }
        let _ = writeln!(s);
    }

    fn write_footer_to_string(s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "================================================================================"
        );
    }
}

/// Represents an extraction attempt with its method and outcome.
#[derive(Debug, Clone)]
pub struct ExtractionAttempt {
    /// Name of the extraction method (e.g., "XML", "JSON", "Salvage")
    pub method: &'static str,
    /// Whether this method succeeded
    pub success: bool,
    /// Detailed reason/description of what happened
    pub detail: String,
}

impl ExtractionAttempt {
    /// Create a successful extraction attempt.
    pub const fn success(method: &'static str, detail: String) -> Self {
        Self {
            method,
            success: true,
            detail,
        }
    }

    /// Create a failed extraction attempt.
    pub const fn failure(method: &'static str, detail: String) -> Self {
        Self {
            method,
            success: false,
            detail,
        }
    }
}

/// Represents a single validation check result.
#[derive(Debug, Clone)]
pub struct ValidationCheck {
    /// Name of the validation check
    pub name: &'static str,
    /// Whether this check passed
    pub passed: bool,
    /// Error message if check failed
    pub error: Option<String>,
}

impl ValidationCheck {
    /// Create a passing validation check.
    #[cfg(test)]
    pub const fn pass(name: &'static str) -> Self {
        Self {
            name,
            passed: true,
            error: None,
        }
    }

    /// Create a failing validation check.
    #[cfg(test)]
    pub const fn fail(name: &'static str, error: String) -> Self {
        Self {
            name,
            passed: false,
            error: Some(error),
        }
    }
}

/// Outcome of a commit generation attempt.
#[derive(Debug, Clone)]
pub enum AttemptOutcome {
    /// Successfully extracted a valid commit message
    Success(String),
    /// XSD validation failed with specific error message
    XsdValidationFailed(String),
    /// Extraction failed entirely
    ExtractionFailed(String),
}

impl std::fmt::Display for AttemptOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success(msg) => write!(f, "SUCCESS: {}", preview_message(msg)),
            Self::XsdValidationFailed(err) => write!(f, "XSD_VALIDATION_FAILED: {err}"),
            Self::ExtractionFailed(err) => write!(f, "EXTRACTION_FAILED: {err}"),
        }
    }
}

/// Preview a message, truncating if too long.
///
/// Uses character-based truncation to avoid panics on UTF-8 multi-byte characters.
fn preview_message(msg: &str) -> String {
    let first_line = msg.lines().next().unwrap_or(msg);
    // truncate_text handles the ellipsis, so we use 63 to get ~60 chars + "..."
    truncate_text(first_line, 63)
}

/// Per-attempt log for commit message generation.
///
/// Captures all details about a single attempt to generate a commit message,
/// providing a complete audit trail for debugging.
#[derive(Debug, Clone)]
pub struct CommitAttemptLog {
    /// Attempt number within this session
    pub attempt_number: usize,
    /// Agent being used (e.g., "claude", "glm")
    pub agent: String,
    /// Retry strategy (e.g., "initial", "`strict_json`")
    pub strategy: String,
    /// Timestamp when attempt started
    pub timestamp: DateTime<Local>,
    /// Size of the prompt in bytes
    pub prompt_size_bytes: usize,
    /// Size of the diff in bytes
    pub diff_size_bytes: usize,
    /// Whether the diff was pre-truncated
    pub diff_was_truncated: bool,
    /// Raw output from the agent (truncated if very large)
    pub raw_output: Option<String>,
    /// Extraction attempts with their results
    pub extraction_attempts: Vec<ExtractionAttempt>,
    /// Validation checks that were run
    pub validation_checks: Vec<ValidationCheck>,
    /// Final outcome of this attempt
    pub outcome: Option<AttemptOutcome>,
}

impl CommitAttemptLog {
    /// Create a new attempt log.
    pub fn new(attempt_number: usize, agent: &str, strategy: &str) -> Self {
        Self {
            attempt_number,
            agent: agent.to_string(),
            strategy: strategy.to_string(),
            timestamp: Local::now(),
            prompt_size_bytes: 0,
            diff_size_bytes: 0,
            diff_was_truncated: false,
            raw_output: None,
            extraction_attempts: Vec::new(),
            validation_checks: Vec::new(),
            outcome: None,
        }
    }

    /// Set the prompt size.
    pub const fn set_prompt_size(&mut self, size: usize) {
        self.prompt_size_bytes = size;
    }

    /// Set the diff information.
    pub const fn set_diff_info(&mut self, size: usize, was_truncated: bool) {
        self.diff_size_bytes = size;
        self.diff_was_truncated = was_truncated;
    }

    /// Set the raw output from the agent.
    ///
    /// Truncates very large outputs to prevent log file bloat.
    pub fn set_raw_output(&mut self, output: &str) {
        const MAX_OUTPUT_SIZE: usize = 50_000;
        if output.len() > MAX_OUTPUT_SIZE {
            self.raw_output = Some(format!(
                "{}\n\n[... truncated {} bytes ...]\n\n{}",
                &output[..MAX_OUTPUT_SIZE / 2],
                output.len() - MAX_OUTPUT_SIZE,
                &output[output.len() - MAX_OUTPUT_SIZE / 2..]
            ));
        } else {
            self.raw_output = Some(output.to_string());
        }
    }

    /// Record an extraction attempt.
    pub fn add_extraction_attempt(&mut self, attempt: ExtractionAttempt) {
        self.extraction_attempts.push(attempt);
    }

    /// Record validation check results.
    #[cfg(test)]
    pub fn set_validation_checks(&mut self, checks: Vec<ValidationCheck>) {
        self.validation_checks = checks;
    }

    /// Set the final outcome.
    pub fn set_outcome(&mut self, outcome: AttemptOutcome) {
        self.outcome = Some(outcome);
    }

    /// Write this log to a file using workspace abstraction.
    ///
    /// This is the architecture-conformant version that uses the workspace trait
    /// instead of direct filesystem access.
    ///
    /// # Arguments
    ///
    /// * `log_dir` - Directory to write the log file to (relative to workspace)
    /// * `workspace` - The workspace to use for filesystem operations
    ///
    /// # Returns
    ///
    /// Path to the written log file on success.
    pub fn write_to_workspace(
        &self,
        log_dir: &Path,
        workspace: &dyn Workspace,
    ) -> std::io::Result<PathBuf> {
        // Create the log directory if needed
        workspace.create_dir_all(log_dir)?;

        // Generate filename
        let filename = format!(
            "attempt_{:03}_{}_{}_{}.log",
            self.attempt_number,
            sanitize_agent_name(&self.agent),
            self.strategy.replace(' ', "_"),
            self.timestamp.format("%Y%m%dT%H%M%S")
        );
        let log_path = log_dir.join(filename);

        // Build content in memory
        let mut content = String::new();
        self.write_header_to_string(&mut content);
        self.write_context_to_string(&mut content);
        self.write_raw_output_to_string(&mut content);
        self.write_extraction_attempts_to_string(&mut content);
        self.write_validation_to_string(&mut content);
        self.write_outcome_to_string(&mut content);

        // Write using workspace
        workspace.write(&log_path, &content)?;
        Ok(log_path)
    }

    // String-based write helpers for workspace support
    fn write_header_to_string(&self, s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "========================================================================"
        );
        let _ = writeln!(s, "COMMIT GENERATION ATTEMPT LOG");
        let _ = writeln!(
            s,
            "========================================================================"
        );
        let _ = writeln!(s);
        let _ = writeln!(s, "Attempt:   #{}", self.attempt_number);
        let _ = writeln!(s, "Agent:     {}", self.agent);
        let _ = writeln!(s, "Strategy:  {}", self.strategy);
        let _ = writeln!(
            s,
            "Timestamp: {}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S %Z")
        );
        let _ = writeln!(s);
    }

    fn write_context_to_string(&self, s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "CONTEXT");
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "Prompt size: {} bytes ({} KB)",
            self.prompt_size_bytes,
            self.prompt_size_bytes / 1024
        );
        let _ = writeln!(
            s,
            "Diff size:   {} bytes ({} KB)",
            self.diff_size_bytes,
            self.diff_size_bytes / 1024
        );
        let _ = writeln!(
            s,
            "Diff truncated: {}",
            if self.diff_was_truncated { "YES" } else { "NO" }
        );
        let _ = writeln!(s);
    }

    fn write_raw_output_to_string(&self, s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "RAW AGENT OUTPUT");
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s);
        match &self.raw_output {
            Some(output) => {
                let _ = writeln!(s, "{output}");
            }
            None => {
                let _ = writeln!(s, "[No output captured]");
            }
        }
        let _ = writeln!(s);
    }

    fn write_extraction_attempts_to_string(&self, s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "EXTRACTION ATTEMPTS");
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s);

        if self.extraction_attempts.is_empty() {
            let _ = writeln!(s, "[No extraction attempts recorded]");
        } else {
            for (i, attempt) in self.extraction_attempts.iter().enumerate() {
                let status = if attempt.success {
                    "✓ SUCCESS"
                } else {
                    "✗ FAILED"
                };
                let _ = writeln!(s, "{}. {} [{}]", i + 1, attempt.method, status);
                let _ = writeln!(s, "   Detail: {}", attempt.detail);
                let _ = writeln!(s);
            }
        }
        let _ = writeln!(s);
    }

    fn write_validation_to_string(&self, s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "VALIDATION RESULTS");
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s);

        if self.validation_checks.is_empty() {
            let _ = writeln!(s, "[No validation checks recorded]");
        } else {
            for check in &self.validation_checks {
                let status = if check.passed { "✓ PASS" } else { "✗ FAIL" };
                let _ = write!(s, "  [{status}] {}", check.name);
                if let Some(error) = &check.error {
                    let _ = writeln!(s, ": {error}");
                } else {
                    let _ = writeln!(s);
                }
            }
        }
        let _ = writeln!(s);
    }

    fn write_outcome_to_string(&self, s: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s, "OUTCOME");
        let _ = writeln!(
            s,
            "------------------------------------------------------------------------"
        );
        let _ = writeln!(s);
        match &self.outcome {
            Some(outcome) => {
                let _ = writeln!(s, "{outcome}");
            }
            None => {
                let _ = writeln!(s, "[Outcome not recorded]");
            }
        }
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "========================================================================"
        );
    }
}

/// Sanitize agent name for use in filename.
fn sanitize_agent_name(agent: &str) -> String {
    agent
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .chars()
        .take(MAX_AGENT_NAME_LENGTH)
        .collect()
}

/// Session tracker for commit generation logging.
///
/// Manages a unique run directory for a commit generation session,
/// ensuring log files are organized and don't overwrite each other.
#[derive(Debug)]
pub struct CommitLogSession {
    /// Base log directory
    run_dir: PathBuf,
    /// Current attempt counter
    attempt_counter: usize,
}

impl CommitLogSession {
    /// Create a new logging session using workspace abstraction.
    ///
    /// Creates a unique run directory under the base log path.
    ///
    /// # Arguments
    ///
    /// * `base_log_dir` - Base directory for commit logs (e.g., `.agent/logs/commit_generation`)
    /// * `workspace` - The workspace to use for filesystem operations
    pub fn new(base_log_dir: &str, workspace: &dyn Workspace) -> std::io::Result<Self> {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let run_dir = PathBuf::from(base_log_dir).join(format!("run_{timestamp}"));
        workspace.create_dir_all(&run_dir)?;

        Ok(Self {
            run_dir,
            attempt_counter: 0,
        })
    }

    /// Create a no-op logging session that discards all writes.
    ///
    /// This is used as a fallback when all log directories fail to be created.
    /// The session will still track attempt numbers and provide a dummy run_dir,
    /// but writes will silently succeed without actually writing anything.
    ///
    /// # Returns
    ///
    /// A `CommitLogSession` that uses `/dev/null` equivalent as its run directory.
    pub fn noop() -> Self {
        // Use a path that indicates this is a noop session
        // The path won't be created or written to by noop session
        Self {
            run_dir: PathBuf::from("/dev/null/ralph-noop-session"),
            attempt_counter: 0,
        }
    }

    /// Check if this is a no-op session.
    pub fn is_noop(&self) -> bool {
        self.run_dir.starts_with("/dev/null")
    }

    /// Get the path to the run directory.
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    /// Get the next attempt number and increment the counter.
    pub const fn next_attempt_number(&mut self) -> usize {
        self.attempt_counter += 1;
        self.attempt_counter
    }

    /// Create a new attempt log for this session.
    ///
    /// # Arguments
    ///
    /// * `agent` - The agent being used
    /// * `strategy` - The retry strategy being used
    pub fn new_attempt(&mut self, agent: &str, strategy: &str) -> CommitAttemptLog {
        let attempt_number = self.next_attempt_number();
        CommitAttemptLog::new(attempt_number, agent, strategy)
    }

    /// Write summary file at end of session.
    ///
    /// For noop sessions, this silently succeeds without writing anything.
    ///
    /// # Arguments
    ///
    /// * `total_attempts` - Total number of attempts made
    /// * `final_outcome` - Description of the final outcome
    /// * `workspace` - The workspace to use for filesystem operations
    pub fn write_summary(
        &self,
        total_attempts: usize,
        final_outcome: &str,
        workspace: &dyn Workspace,
    ) -> std::io::Result<()> {
        // Skip writing for noop sessions
        if self.is_noop() {
            return Ok(());
        }

        use std::fmt::Write;

        let summary_path = self.run_dir.join("SUMMARY.txt");
        let mut content = String::new();

        let _ = writeln!(content, "COMMIT GENERATION SESSION SUMMARY");
        let _ = writeln!(content, "=================================");
        let _ = writeln!(content);
        let _ = writeln!(content, "Run directory: {}", self.run_dir.display());
        let _ = writeln!(content, "Total attempts: {total_attempts}");
        let _ = writeln!(content, "Final outcome: {final_outcome}");
        let _ = writeln!(content);
        let _ = writeln!(content, "Individual attempt logs are in this directory.");

        workspace.write(&summary_path, &content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;

    // =========================================================================
    // Tests using MemoryWorkspace (architecture-conformant)
    // =========================================================================

    #[test]
    fn test_attempt_log_write_to_workspace() {
        let workspace = MemoryWorkspace::new_test();
        let log_dir = Path::new(".agent/logs/commit_generation/run_test");

        let mut log = CommitAttemptLog::new(1, "claude", "initial");
        log.set_prompt_size(5000);
        log.set_diff_info(10000, false);
        log.set_raw_output("raw agent output here");
        log.add_extraction_attempt(ExtractionAttempt::failure(
            "XML",
            "No <ralph-commit> tag found".to_string(),
        ));
        log.set_outcome(AttemptOutcome::Success("feat: add feature".to_string()));

        let log_path = log.write_to_workspace(log_dir, &workspace).unwrap();
        assert!(workspace.exists(&log_path));

        let content = workspace.read(&log_path).unwrap();
        assert!(content.contains("COMMIT GENERATION ATTEMPT LOG"));
        assert!(content.contains("Attempt:   #1"));
        assert!(content.contains("claude"));
    }

    #[test]
    fn test_attempt_log_write_with_all_fields() {
        let workspace = MemoryWorkspace::new_test();
        let log_dir = Path::new(".agent/logs/commit_generation/run_test");

        let mut log = CommitAttemptLog::new(1, "claude", "initial");
        log.set_prompt_size(5000);
        log.set_diff_info(10000, false);
        log.set_raw_output("raw agent output here");
        log.add_extraction_attempt(ExtractionAttempt::failure(
            "XML",
            "No <ralph-commit> tag found".to_string(),
        ));
        log.add_extraction_attempt(ExtractionAttempt::success(
            "JSON",
            "Extracted from JSON".to_string(),
        ));
        log.set_validation_checks(vec![
            ValidationCheck::pass("basic_length"),
            ValidationCheck::fail("no_bad_patterns", "File list pattern detected".to_string()),
        ]);
        log.set_outcome(AttemptOutcome::ExtractionFailed("bad pattern".to_string()));

        let log_path = log.write_to_workspace(log_dir, &workspace).unwrap();
        assert!(workspace.exists(&log_path));

        let content = workspace.read(&log_path).unwrap();
        assert!(content.contains("COMMIT GENERATION ATTEMPT LOG"));
        assert!(content.contains("Attempt:   #1"));
        assert!(content.contains("claude"));
        assert!(content.contains("EXTRACTION ATTEMPTS"));
        assert!(content.contains("VALIDATION RESULTS"));
        assert!(content.contains("OUTCOME"));
    }

    #[test]
    fn test_parsing_trace_write_to_workspace() {
        let workspace = MemoryWorkspace::new_test();
        let log_dir = Path::new(".agent/logs/commit_generation/run_test");

        let mut trace = ParsingTraceLog::new(1, "claude", "initial");
        trace.set_raw_output("raw agent output");
        trace.add_step(
            ParsingTraceStep::new(1, "XML extraction")
                .with_input("input")
                .with_success(true),
        );
        trace.set_final_message("feat: add feature");

        let trace_path = trace.write_to_workspace(log_dir, &workspace).unwrap();
        assert!(workspace.exists(&trace_path));

        let content = workspace.read(&trace_path).unwrap();
        assert!(content.contains("PARSING TRACE LOG"));
        assert!(content.contains("Attempt #001"));
    }

    #[test]
    fn test_parsing_trace_write_with_steps() {
        let workspace = MemoryWorkspace::new_test();
        let log_dir = Path::new(".agent/logs/commit_generation/run_test");

        let mut trace = ParsingTraceLog::new(1, "claude", "initial");
        trace.set_raw_output("raw agent output");
        trace.add_step(
            ParsingTraceStep::new(1, "XML extraction")
                .with_input("input")
                .with_result("result")
                .with_success(true)
                .with_details("success"),
        );
        trace.add_step(
            ParsingTraceStep::new(2, "Validation")
                .with_success(false)
                .with_details("failed"),
        );
        trace.set_final_message("feat: add feature");

        let trace_path = trace.write_to_workspace(log_dir, &workspace).unwrap();
        assert!(workspace.exists(&trace_path));
        assert!(trace_path.to_string_lossy().contains("parsing_trace"));

        let content = workspace.read(&trace_path).unwrap();
        assert!(content.contains("PARSING TRACE LOG"));
        assert!(content.contains("Attempt #001"));
        assert!(content.contains("RAW AGENT OUTPUT"));
        assert!(content.contains("PARSING STEPS"));
        assert!(content.contains("FINAL EXTRACTED MESSAGE"));
    }

    #[test]
    fn test_session_creates_run_directory() {
        let workspace = MemoryWorkspace::new_test();

        let session = CommitLogSession::new(".agent/logs/commit_generation", &workspace).unwrap();
        assert!(workspace.exists(session.run_dir()));
        assert!(session.run_dir().to_string_lossy().contains("run_"));
    }

    #[test]
    fn test_session_increments_attempt_number() {
        let workspace = MemoryWorkspace::new_test();

        let mut session =
            CommitLogSession::new(".agent/logs/commit_generation", &workspace).unwrap();

        assert_eq!(session.next_attempt_number(), 1);
        assert_eq!(session.next_attempt_number(), 2);
        assert_eq!(session.next_attempt_number(), 3);
    }

    #[test]
    fn test_session_new_attempt() {
        let workspace = MemoryWorkspace::new_test();

        let mut session =
            CommitLogSession::new(".agent/logs/commit_generation", &workspace).unwrap();

        let log1 = session.new_attempt("claude", "initial");
        assert_eq!(log1.attempt_number, 1);

        let log2 = session.new_attempt("glm", "strict_json");
        assert_eq!(log2.attempt_number, 2);
    }

    #[test]
    fn test_session_write_summary() {
        let workspace = MemoryWorkspace::new_test();

        let session = CommitLogSession::new(".agent/logs/commit_generation", &workspace).unwrap();
        session
            .write_summary(5, "SUCCESS: feat: add feature", &workspace)
            .unwrap();

        let summary_path = session.run_dir().join("SUMMARY.txt");
        assert!(workspace.exists(&summary_path));

        let content = workspace.read(&summary_path).unwrap();
        assert!(content.contains("Total attempts: 5"));
        assert!(content.contains("SUCCESS"));
    }

    #[test]
    fn test_noop_session_creation() {
        let session = CommitLogSession::noop();
        assert!(session.is_noop());
        assert!(session.run_dir().starts_with("/dev/null"));
    }

    #[test]
    fn test_noop_session_write_summary_succeeds_silently() {
        let workspace = MemoryWorkspace::new_test();
        let session = CommitLogSession::noop();

        // Should succeed without error
        session
            .write_summary(5, "SUCCESS: feat: add feature", &workspace)
            .unwrap();

        // Should not create any files
        let summary_path = session.run_dir().join("SUMMARY.txt");
        assert!(!workspace.exists(&summary_path));
    }

    #[test]
    fn test_noop_session_attempt_counter() {
        let mut session = CommitLogSession::noop();
        assert_eq!(session.next_attempt_number(), 1);
        assert_eq!(session.next_attempt_number(), 2);
        assert_eq!(session.next_attempt_number(), 3);
    }

    #[test]
    fn test_sanitize_agent_name() {
        assert_eq!(sanitize_agent_name("claude"), "claude");
        assert_eq!(sanitize_agent_name("agent/commit"), "agent_commit");
        assert_eq!(sanitize_agent_name("my-agent-v2"), "my_agent_v2");
        // Long names are truncated
        let long_name = "a".repeat(50);
        assert_eq!(sanitize_agent_name(&long_name).len(), 20);
    }

    #[test]
    fn test_large_output_truncation() {
        let mut log = CommitAttemptLog::new(1, "test", "test");
        let large_output = "x".repeat(100_000);
        log.set_raw_output(&large_output);

        let output = log.raw_output.unwrap();
        assert!(output.len() < large_output.len());
        assert!(output.contains("[... truncated"));
    }

    #[test]
    fn test_parsing_trace_step_creation() {
        let step = ParsingTraceStep::new(1, "XML extraction");
        assert_eq!(step.step_number, 1);
        assert_eq!(step.description, "XML extraction");
        assert!(!step.success);
        assert!(step.input.is_none());
        assert!(step.result.is_none());
    }

    #[test]
    fn test_parsing_trace_step_builder() {
        let step = ParsingTraceStep::new(1, "XML extraction")
            .with_input("input content")
            .with_result("result content")
            .with_success(true)
            .with_details("extraction successful");

        assert!(step.success);
        assert_eq!(step.input.as_deref(), Some("input content"));
        assert_eq!(step.result.as_deref(), Some("result content"));
        assert_eq!(step.details, "extraction successful");
    }

    #[test]
    fn test_parsing_trace_step_truncation() {
        let large_input = "x".repeat(100_000);
        let step = ParsingTraceStep::new(1, "test").with_input(&large_input);

        assert!(step.input.is_some());
        let input = step.input.as_ref().unwrap();
        assert!(input.len() < large_input.len());
        assert!(input.contains("[... input truncated"));
    }

    #[test]
    fn test_parsing_trace_log_creation() {
        let trace = ParsingTraceLog::new(1, "claude", "initial");
        assert_eq!(trace.attempt_number, 1);
        assert_eq!(trace.agent, "claude");
        assert_eq!(trace.strategy, "initial");
        assert!(trace.raw_output.is_none());
        assert!(trace.steps.is_empty());
        assert!(trace.final_message.is_none());
    }

    #[test]
    fn test_parsing_trace_log_set_raw_output() {
        let mut trace = ParsingTraceLog::new(1, "claude", "initial");
        trace.set_raw_output("test output");

        assert_eq!(trace.raw_output.as_deref(), Some("test output"));
    }

    #[test]
    fn test_parsing_trace_raw_output_truncation() {
        let mut trace = ParsingTraceLog::new(1, "claude", "initial");
        let large_output = "x".repeat(100_000);
        trace.set_raw_output(&large_output);

        let output = trace.raw_output.unwrap();
        assert!(output.len() < large_output.len());
        assert!(output.contains("[... raw output truncated"));
    }

    #[test]
    fn test_parsing_trace_add_step() {
        let mut trace = ParsingTraceLog::new(1, "claude", "initial");
        let step = ParsingTraceStep::new(1, "XML extraction");
        trace.add_step(step);

        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].description, "XML extraction");
    }

    #[test]
    fn test_parsing_trace_set_final_message() {
        let mut trace = ParsingTraceLog::new(1, "claude", "initial");
        trace.set_final_message("feat: add feature");

        assert_eq!(trace.final_message.as_deref(), Some("feat: add feature"));
    }

    #[test]
    fn test_attempt_log_creation() {
        let log = CommitAttemptLog::new(1, "claude", "initial");
        assert_eq!(log.attempt_number, 1);
        assert_eq!(log.agent, "claude");
        assert_eq!(log.strategy, "initial");
        assert!(log.raw_output.is_none());
        assert!(log.extraction_attempts.is_empty());
        assert!(log.validation_checks.is_empty());
        assert!(log.outcome.is_none());
    }

    #[test]
    fn test_attempt_log_set_values() {
        let mut log = CommitAttemptLog::new(2, "glm", "strict_json");

        log.set_prompt_size(10_000);
        log.set_diff_info(50_000, true);
        log.set_raw_output("test output");

        assert_eq!(log.prompt_size_bytes, 10_000);
        assert_eq!(log.diff_size_bytes, 50_000);
        assert!(log.diff_was_truncated);
        assert_eq!(log.raw_output.as_deref(), Some("test output"));
    }

    #[test]
    fn test_extraction_attempt_creation() {
        let success =
            ExtractionAttempt::success("XML", "Found <ralph-commit> at pos 0".to_string());
        assert!(success.success);
        assert_eq!(success.method, "XML");

        let failure = ExtractionAttempt::failure("JSON", "No JSON found".to_string());
        assert!(!failure.success);
        assert_eq!(failure.method, "JSON");
    }

    #[test]
    fn test_validation_check_creation() {
        let pass = ValidationCheck::pass("basic_length");
        assert!(pass.passed);
        assert!(pass.error.is_none());

        let fail = ValidationCheck::fail("no_json_artifacts", "Found JSON in message".to_string());
        assert!(!fail.passed);
        assert!(fail.error.is_some());
    }

    #[test]
    fn test_outcome_display() {
        let success = AttemptOutcome::Success("feat: add feature".to_string());
        assert!(format!("{success}").contains("SUCCESS"));

        let error = AttemptOutcome::ExtractionFailed("extraction failed".to_string());
        assert!(format!("{error}").contains("EXTRACTION_FAILED"));
    }
}
