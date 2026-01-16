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
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

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

    /// Write this trace to a file.
    ///
    /// # Arguments
    ///
    /// * `log_dir` - Directory to write the trace file to
    ///
    /// # Returns
    ///
    /// Path to the written trace file on success.
    pub fn write_to_file(&self, log_dir: &Path) -> std::io::Result<PathBuf> {
        let trace_path = log_dir.join(format!(
            "attempt_{:03}_parsing_trace.log",
            self.attempt_number
        ));

        let file = File::create(&trace_path)?;
        let mut writer = BufWriter::new(file);

        Self::write_header(&mut writer, self)?;
        Self::write_raw_output(&mut writer, self)?;
        Self::write_parsing_steps(&mut writer, self)?;
        Self::write_final_message(&mut writer, self)?;
        Self::write_footer(&mut writer)?;

        writer.flush()?;
        Ok(trace_path)
    }

    fn write_header(w: &mut impl Write, trace: &Self) -> std::io::Result<()> {
        writeln!(
            w,
            "================================================================================"
        )?;
        writeln!(
            w,
            "PARSING TRACE LOG - Attempt #{:03}",
            trace.attempt_number
        )?;
        writeln!(
            w,
            "================================================================================"
        )?;
        writeln!(w)?;
        writeln!(w, "Agent:     {}", trace.agent)?;
        writeln!(w, "Strategy:  {}", trace.strategy)?;
        writeln!(
            w,
            "Timestamp: {}",
            trace.timestamp.format("%Y-%m-%d %H:%M:%S %Z")
        )?;
        writeln!(w)?;
        Ok(())
    }

    fn write_raw_output(w: &mut impl Write, trace: &Self) -> std::io::Result<()> {
        writeln!(
            w,
            "--------------------------------------------------------------------------------"
        )?;
        writeln!(w, "RAW AGENT OUTPUT")?;
        writeln!(
            w,
            "--------------------------------------------------------------------------------"
        )?;
        writeln!(w)?;
        match &trace.raw_output {
            Some(output) => {
                writeln!(w, "{output}")?;
            }
            None => {
                writeln!(w, "[No raw output captured]")?;
            }
        }
        writeln!(w)?;
        Ok(())
    }

    fn write_parsing_steps(w: &mut impl Write, trace: &Self) -> std::io::Result<()> {
        writeln!(
            w,
            "--------------------------------------------------------------------------------"
        )?;
        writeln!(w, "PARSING STEPS")?;
        writeln!(
            w,
            "--------------------------------------------------------------------------------"
        )?;
        writeln!(w)?;

        if trace.steps.is_empty() {
            writeln!(w, "[No parsing steps recorded]")?;
        } else {
            for step in &trace.steps {
                let status = if step.success {
                    "✓ SUCCESS"
                } else {
                    "✗ FAILED"
                };
                writeln!(w, "{}. {} [{}]", step.step_number, step.description, status)?;
                writeln!(w)?;

                if let Some(input) = &step.input {
                    writeln!(w, "   INPUT:")?;
                    for line in input.lines() {
                        writeln!(w, "   {line}")?;
                    }
                    writeln!(w)?;
                }

                if let Some(result) = &step.result {
                    writeln!(w, "   RESULT:")?;
                    for line in result.lines() {
                        writeln!(w, "   {line}")?;
                    }
                    writeln!(w)?;
                }

                if !step.details.is_empty() {
                    writeln!(w, "   DETAILS: {}", step.details)?;
                    writeln!(w)?;
                }
            }
        }
        writeln!(w)?;
        Ok(())
    }

    fn write_final_message(w: &mut impl Write, trace: &Self) -> std::io::Result<()> {
        writeln!(
            w,
            "--------------------------------------------------------------------------------"
        )?;
        writeln!(w, "FINAL EXTRACTED MESSAGE")?;
        writeln!(
            w,
            "--------------------------------------------------------------------------------"
        )?;
        writeln!(w)?;
        match &trace.final_message {
            Some(message) => {
                writeln!(w, "{message}")?;
            }
            None => {
                writeln!(w, "[No message extracted]")?;
            }
        }
        writeln!(w)?;
        Ok(())
    }

    fn write_footer(w: &mut impl Write) -> std::io::Result<()> {
        writeln!(
            w,
            "================================================================================"
        )?;
        Ok(())
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
    pub const fn pass(name: &'static str) -> Self {
        Self {
            name,
            passed: true,
            error: None,
        }
    }

    /// Create a failing validation check.
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
    /// Extraction produced a fallback message (may trigger re-prompt)
    Fallback(String),
    /// Agent error detected (should trigger fallback)
    AgentError(String),
    /// Extraction failed entirely
    ExtractionFailed(String),
}

impl std::fmt::Display for AttemptOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success(msg) => write!(f, "SUCCESS: {}", preview_message(msg)),
            Self::Fallback(msg) => write!(f, "FALLBACK: {}", preview_message(msg)),
            Self::AgentError(err) => write!(f, "AGENT_ERROR: {err}"),
            Self::ExtractionFailed(err) => write!(f, "EXTRACTION_FAILED: {err}"),
        }
    }
}

/// Preview a message, truncating if too long.
fn preview_message(msg: &str) -> String {
    let first_line = msg.lines().next().unwrap_or(msg);
    if first_line.len() > 60 {
        format!("{}...", &first_line[..60])
    } else {
        first_line.to_string()
    }
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
    pub fn set_validation_checks(&mut self, checks: Vec<ValidationCheck>) {
        self.validation_checks = checks;
    }

    /// Set the final outcome.
    pub fn set_outcome(&mut self, outcome: AttemptOutcome) {
        self.outcome = Some(outcome);
    }

    /// Write this log to a file.
    ///
    /// # Arguments
    ///
    /// * `log_dir` - Directory to write the log file to
    ///
    /// # Returns
    ///
    /// Path to the written log file on success.
    pub fn write_to_file(&self, log_dir: &Path) -> std::io::Result<PathBuf> {
        // Create the log directory if needed
        fs::create_dir_all(log_dir)?;

        // Generate filename
        let filename = format!(
            "attempt_{:03}_{}_{}_{}.log",
            self.attempt_number,
            sanitize_agent_name(&self.agent),
            self.strategy.replace(' ', "_"),
            self.timestamp.format("%Y%m%dT%H%M%S")
        );
        let log_path = log_dir.join(filename);

        // Write the log
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)?;
        let mut writer = BufWriter::new(file);

        self.write_header(&mut writer)?;
        self.write_context(&mut writer)?;
        self.write_raw_output(&mut writer)?;
        self.write_extraction_attempts(&mut writer)?;
        self.write_validation(&mut writer)?;
        self.write_outcome(&mut writer)?;

        writer.flush()?;
        Ok(log_path)
    }

    fn write_header(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(
            w,
            "========================================================================"
        )?;
        writeln!(w, "COMMIT GENERATION ATTEMPT LOG")?;
        writeln!(
            w,
            "========================================================================"
        )?;
        writeln!(w)?;
        writeln!(w, "Attempt:   #{}", self.attempt_number)?;
        writeln!(w, "Agent:     {}", self.agent)?;
        writeln!(w, "Strategy:  {}", self.strategy)?;
        writeln!(
            w,
            "Timestamp: {}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S %Z")
        )?;
        writeln!(w)?;
        Ok(())
    }

    fn write_context(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w, "CONTEXT")?;
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w)?;
        writeln!(
            w,
            "Prompt size: {} bytes ({} KB)",
            self.prompt_size_bytes,
            self.prompt_size_bytes / 1024
        )?;
        writeln!(
            w,
            "Diff size:   {} bytes ({} KB)",
            self.diff_size_bytes,
            self.diff_size_bytes / 1024
        )?;
        writeln!(
            w,
            "Diff truncated: {}",
            if self.diff_was_truncated { "YES" } else { "NO" }
        )?;
        writeln!(w)?;
        Ok(())
    }

    fn write_raw_output(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w, "RAW AGENT OUTPUT")?;
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w)?;
        match &self.raw_output {
            Some(output) => {
                writeln!(w, "{output}")?;
            }
            None => {
                writeln!(w, "[No output captured]")?;
            }
        }
        writeln!(w)?;
        Ok(())
    }

    fn write_extraction_attempts(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w, "EXTRACTION ATTEMPTS")?;
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w)?;

        if self.extraction_attempts.is_empty() {
            writeln!(w, "[No extraction attempts recorded]")?;
        } else {
            for (i, attempt) in self.extraction_attempts.iter().enumerate() {
                let status = if attempt.success {
                    "✓ SUCCESS"
                } else {
                    "✗ FAILED"
                };
                writeln!(w, "{}. {} [{}]", i + 1, attempt.method, status)?;
                writeln!(w, "   Detail: {}", attempt.detail)?;
                writeln!(w)?;
            }
        }
        writeln!(w)?;
        Ok(())
    }

    fn write_validation(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w, "VALIDATION RESULTS")?;
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w)?;

        if self.validation_checks.is_empty() {
            writeln!(w, "[No validation checks recorded]")?;
        } else {
            for check in &self.validation_checks {
                let status = if check.passed { "✓ PASS" } else { "✗ FAIL" };
                write!(w, "  [{status}] {}", check.name)?;
                if let Some(error) = &check.error {
                    writeln!(w, ": {error}")?;
                } else {
                    writeln!(w)?;
                }
            }
        }
        writeln!(w)?;
        Ok(())
    }

    fn write_outcome(&self, w: &mut impl Write) -> std::io::Result<()> {
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w, "OUTCOME")?;
        writeln!(
            w,
            "------------------------------------------------------------------------"
        )?;
        writeln!(w)?;
        match &self.outcome {
            Some(outcome) => {
                writeln!(w, "{outcome}")?;
            }
            None => {
                writeln!(w, "[Outcome not recorded]")?;
            }
        }
        writeln!(w)?;
        writeln!(
            w,
            "========================================================================"
        )?;
        Ok(())
    }
}

/// Sanitize agent name for use in filename.
fn sanitize_agent_name(agent: &str) -> String {
    agent
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .chars()
        .take(20)
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
    /// Create a new logging session.
    ///
    /// Creates a unique run directory under the base log path.
    ///
    /// # Arguments
    ///
    /// * `base_log_dir` - Base directory for commit logs (e.g., `.agent/logs/commit_generation`)
    pub fn new(base_log_dir: &str) -> std::io::Result<Self> {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let run_dir = PathBuf::from(base_log_dir).join(format!("run_{timestamp}"));
        fs::create_dir_all(&run_dir)?;

        Ok(Self {
            run_dir,
            attempt_counter: 0,
        })
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
    /// # Arguments
    ///
    /// * `total_attempts` - Total number of attempts made
    /// * `final_outcome` - Description of the final outcome
    pub fn write_summary(&self, total_attempts: usize, final_outcome: &str) -> std::io::Result<()> {
        let summary_path = self.run_dir.join("SUMMARY.txt");
        let mut file = File::create(summary_path)?;

        writeln!(file, "COMMIT GENERATION SESSION SUMMARY")?;
        writeln!(file, "=================================")?;
        writeln!(file)?;
        writeln!(file, "Run directory: {}", self.run_dir.display())?;
        writeln!(file, "Total attempts: {total_attempts}")?;
        writeln!(file, "Final outcome: {final_outcome}")?;
        writeln!(file)?;
        writeln!(file, "Individual attempt logs are in this directory.")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

        let error = AttemptOutcome::AgentError("token limit exceeded".to_string());
        assert!(format!("{error}").contains("AGENT_ERROR"));
    }

    #[test]
    fn test_write_log_to_file() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path();

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

        let log_path = log.write_to_file(log_dir).unwrap();
        assert!(log_path.exists());

        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("COMMIT GENERATION ATTEMPT LOG"));
        assert!(content.contains("Attempt:   #1"));
        assert!(content.contains("claude"));
        assert!(content.contains("EXTRACTION ATTEMPTS"));
        assert!(content.contains("✗ FAILED"));
        assert!(content.contains("✓ SUCCESS"));
        assert!(content.contains("VALIDATION RESULTS"));
        assert!(content.contains("OUTCOME"));
    }

    #[test]
    fn test_session_creates_run_directory() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("logs");

        let session = CommitLogSession::new(base_dir.to_str().unwrap()).unwrap();
        assert!(session.run_dir().exists());
        assert!(session.run_dir().to_string_lossy().contains("run_"));
    }

    #[test]
    fn test_session_increments_attempt_number() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("logs");

        let mut session = CommitLogSession::new(base_dir.to_str().unwrap()).unwrap();

        assert_eq!(session.next_attempt_number(), 1);
        assert_eq!(session.next_attempt_number(), 2);
        assert_eq!(session.next_attempt_number(), 3);
    }

    #[test]
    fn test_session_new_attempt() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("logs");

        let mut session = CommitLogSession::new(base_dir.to_str().unwrap()).unwrap();

        let log1 = session.new_attempt("claude", "initial");
        assert_eq!(log1.attempt_number, 1);

        let log2 = session.new_attempt("glm", "strict_json");
        assert_eq!(log2.attempt_number, 2);
    }

    #[test]
    fn test_session_write_summary() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("logs");

        let session = CommitLogSession::new(base_dir.to_str().unwrap()).unwrap();
        session
            .write_summary(5, "SUCCESS: feat: add feature")
            .unwrap();

        let summary_path = session.run_dir().join("SUMMARY.txt");
        assert!(summary_path.exists());

        let content = fs::read_to_string(&summary_path).unwrap();
        assert!(content.contains("Total attempts: 5"));
        assert!(content.contains("SUCCESS"));
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
    fn test_parsing_trace_write_to_file() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path();

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

        let trace_path = trace.write_to_file(log_dir).unwrap();
        assert!(trace_path.exists());
        assert!(trace_path.to_string_lossy().contains("parsing_trace"));

        let content = fs::read_to_string(&trace_path).unwrap();
        assert!(content.contains("PARSING TRACE LOG"));
        assert!(content.contains("Attempt #001"));
        assert!(content.contains("RAW AGENT OUTPUT"));
        assert!(content.contains("PARSING STEPS"));
        assert!(content.contains("✓ SUCCESS"));
        assert!(content.contains("✗ FAILED"));
        assert!(content.contains("FINAL EXTRACTED MESSAGE"));
    }
}
