//! Checkpoint validation for resume functionality.
//!
//! This module provides validation for checkpoint state before resuming,
//! ensuring the environment matches the checkpoint and detecting configuration changes.

use crate::agents::AgentRegistry;
use crate::checkpoint::state::{calculate_file_checksum, AgentConfigSnapshot, PipelineCheckpoint};
use crate::config::Config;
use std::path::Path;

/// Result of checkpoint validation.
#[derive(Debug)]
pub struct ValidationResult {
    /// Whether the checkpoint is valid for resume.
    pub is_valid: bool,
    /// Warnings that don't prevent resume but should be shown.
    pub warnings: Vec<String>,
    /// Errors that prevent resume.
    pub errors: Vec<String>,
}

impl ValidationResult {
    /// Create a successful validation result with no issues.
    pub fn ok() -> Self {
        Self {
            is_valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Create a validation result with a single error.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            warnings: Vec::new(),
            errors: vec![msg.into()],
        }
    }

    /// Add a warning to the result.
    pub fn with_warning(mut self, msg: impl Into<String>) -> Self {
        self.warnings.push(msg.into());
        self
    }

    /// Merge another validation result into this one.
    pub fn merge(mut self, other: ValidationResult) -> Self {
        if !other.is_valid {
            self.is_valid = false;
        }
        self.warnings.extend(other.warnings);
        self.errors.extend(other.errors);
        self
    }
}

/// Validate a checkpoint before resuming.
///
/// Performs comprehensive validation to ensure the checkpoint can be safely resumed:
/// - Working directory matches
/// - PROMPT.md hasn't changed (if checksum available)
/// - Agent configurations are compatible
///
/// # Arguments
///
/// * `checkpoint` - The checkpoint to validate
/// * `current_config` - Current configuration to compare against
/// * `registry` - Agent registry for agent validation
///
/// # Returns
///
/// A `ValidationResult` with any warnings or errors found.
pub fn validate_checkpoint(
    checkpoint: &PipelineCheckpoint,
    current_config: &Config,
    registry: &AgentRegistry,
) -> ValidationResult {
    let mut result = ValidationResult::ok();

    // Validate working directory
    result = result.merge(validate_working_directory(checkpoint));

    // Validate PROMPT.md checksum
    result = result.merge(validate_prompt_md(checkpoint));

    // Validate agent configurations
    result = result.merge(validate_agent_config(
        &checkpoint.developer_agent_config,
        &checkpoint.developer_agent,
        registry,
    ));
    result = result.merge(validate_agent_config(
        &checkpoint.reviewer_agent_config,
        &checkpoint.reviewer_agent,
        registry,
    ));

    // Check for iteration count mismatches (warning only)
    result = result.merge(validate_iteration_counts(checkpoint, current_config));

    result
}

/// Validate that the working directory matches the checkpoint.
pub fn validate_working_directory(checkpoint: &PipelineCheckpoint) -> ValidationResult {
    if checkpoint.working_dir.is_empty() {
        return ValidationResult::ok().with_warning(
            "Checkpoint has no working directory recorded (legacy checkpoint)".to_string(),
        );
    }

    let current_dir = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if current_dir != checkpoint.working_dir {
        return ValidationResult::error(format!(
            "Working directory mismatch: checkpoint was created in '{}', but current directory is '{}'",
            checkpoint.working_dir, current_dir
        ));
    }

    ValidationResult::ok()
}

/// Validate that PROMPT.md hasn't changed since checkpoint.
pub fn validate_prompt_md(checkpoint: &PipelineCheckpoint) -> ValidationResult {
    let Some(ref saved_checksum) = checkpoint.prompt_md_checksum else {
        return ValidationResult::ok()
            .with_warning("Checkpoint has no PROMPT.md checksum (legacy checkpoint)");
    };

    let current_checksum = calculate_file_checksum(Path::new("PROMPT.md"));

    match current_checksum {
        Some(current) if current == *saved_checksum => ValidationResult::ok(),
        Some(current) => ValidationResult::ok().with_warning(format!(
            "PROMPT.md has changed since checkpoint was created (checksum: {} -> {})",
            &saved_checksum[..8],
            &current[..8]
        )),
        None => ValidationResult::ok()
            .with_warning("PROMPT.md not found or unreadable - cannot verify integrity"),
    }
}

/// Validate that an agent configuration matches the current registry.
pub fn validate_agent_config(
    saved_config: &AgentConfigSnapshot,
    agent_name: &str,
    registry: &AgentRegistry,
) -> ValidationResult {
    // Skip validation if the saved config has empty command (legacy/minimal checkpoint)
    if saved_config.cmd.is_empty() {
        return ValidationResult::ok();
    }

    let Some(current_config) = registry.resolve_config(agent_name) else {
        return ValidationResult::ok().with_warning(format!(
            "Agent '{}' not found in current registry (may have been removed)",
            agent_name
        ));
    };

    let mut result = ValidationResult::ok();

    // Check command
    if current_config.cmd != saved_config.cmd {
        result = result.with_warning(format!(
            "Agent '{}' command changed: '{}' -> '{}'",
            agent_name, saved_config.cmd, current_config.cmd
        ));
    }

    // Check output flag
    if current_config.output_flag != saved_config.output_flag {
        result = result.with_warning(format!(
            "Agent '{}' output flag changed: '{}' -> '{}'",
            agent_name, saved_config.output_flag, current_config.output_flag
        ));
    }

    // Check can_commit flag
    if current_config.can_commit != saved_config.can_commit {
        result = result.with_warning(format!(
            "Agent '{}' can_commit flag changed: {} -> {}",
            agent_name, saved_config.can_commit, current_config.can_commit
        ));
    }

    result
}

/// Validate iteration counts between checkpoint and current config.
///
/// This is a soft validation - mismatches generate warnings but don't block resume.
/// The checkpoint values take precedence during resume.
pub fn validate_iteration_counts(
    checkpoint: &PipelineCheckpoint,
    current_config: &Config,
) -> ValidationResult {
    let mut result = ValidationResult::ok();

    // Check developer iterations
    let saved_dev_iters = checkpoint.cli_args.developer_iters;
    if saved_dev_iters > 0 && saved_dev_iters != current_config.developer_iters {
        result = result.with_warning(format!(
            "Developer iterations changed: {} (checkpoint) vs {} (current config). Using checkpoint value.",
            saved_dev_iters, current_config.developer_iters
        ));
    }

    // Check reviewer reviews
    let saved_rev_reviews = checkpoint.cli_args.reviewer_reviews;
    if saved_rev_reviews > 0 && saved_rev_reviews != current_config.reviewer_reviews {
        result = result.with_warning(format!(
            "Reviewer reviews changed: {} (checkpoint) vs {} (current config). Using checkpoint value.",
            saved_rev_reviews, current_config.reviewer_reviews
        ));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::state::{CheckpointParams, CliArgsSnapshot, PipelinePhase, RebaseState};

    fn make_test_checkpoint() -> PipelineCheckpoint {
        let cli_args = CliArgsSnapshot::new(5, 2, "test".to_string(), None, false, true);
        let dev_config =
            AgentConfigSnapshot::new("claude".into(), "claude".into(), "-p".into(), None, true);
        let rev_config =
            AgentConfigSnapshot::new("codex".into(), "codex".into(), "-p".into(), None, true);
        let run_id = uuid::Uuid::new_v4().to_string();

        PipelineCheckpoint::from_params(CheckpointParams {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args,
            developer_agent_config: dev_config,
            reviewer_agent_config: rev_config,
            rebase_state: RebaseState::default(),
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: 2,
            actual_reviewer_runs: 0,
        })
    }

    #[test]
    fn test_validation_result_ok() {
        let result = ValidationResult::ok();
        assert!(result.is_valid);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validation_result_error() {
        let result = ValidationResult::error("test error");
        assert!(!result.is_valid);
        assert!(result.warnings.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0], "test error");
    }

    #[test]
    fn test_validation_result_with_warning() {
        let result = ValidationResult::ok().with_warning("test warning");
        assert!(result.is_valid);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0], "test warning");
    }

    #[test]
    fn test_validation_result_merge() {
        let result1 = ValidationResult::ok().with_warning("warning 1");
        let result2 = ValidationResult::ok().with_warning("warning 2");

        let merged = result1.merge(result2);
        assert!(merged.is_valid);
        assert_eq!(merged.warnings.len(), 2);
    }

    #[test]
    fn test_validation_result_merge_with_error() {
        let result1 = ValidationResult::ok();
        let result2 = ValidationResult::error("error");

        let merged = result1.merge(result2);
        assert!(!merged.is_valid);
        assert_eq!(merged.errors.len(), 1);
    }

    #[test]
    fn test_validate_working_directory_empty() {
        let mut checkpoint = make_test_checkpoint();
        checkpoint.working_dir = String::new();

        let result = validate_working_directory(&checkpoint);
        assert!(result.is_valid);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("legacy checkpoint"));
    }

    #[test]
    fn test_validate_working_directory_mismatch() {
        let mut checkpoint = make_test_checkpoint();
        checkpoint.working_dir = "/some/other/directory".to_string();

        let result = validate_working_directory(&checkpoint);
        assert!(
            !result.is_valid,
            "Should fail validation on working_dir mismatch"
        );
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("Working directory mismatch"));
    }

    #[test]
    fn test_validate_prompt_md_no_checksum() {
        let mut checkpoint = make_test_checkpoint();
        checkpoint.prompt_md_checksum = None;

        let result = validate_prompt_md(&checkpoint);
        assert!(result.is_valid);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("legacy checkpoint"));
    }
}
