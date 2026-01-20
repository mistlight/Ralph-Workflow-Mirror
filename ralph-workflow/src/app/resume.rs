//! Resume functionality for pipeline checkpoints.
//!
//! This module handles the --resume flag and checkpoint loading logic,
//! including validation and state restoration.

use crate::agents::AgentRegistry;
use crate::checkpoint::file_state::{FileSystemState, ValidationError};
use crate::checkpoint::{
    checkpoint_exists, load_checkpoint, validate_checkpoint, PipelineCheckpoint, PipelinePhase,
};
use crate::config::Config;
use crate::git_helpers::rebase_in_progress;
use crate::logger::Logger;
use std::fs;
use std::io::{self, IsTerminal};

/// Result of handling resume, containing the checkpoint.
pub struct ResumeResult {
    /// The loaded checkpoint.
    pub checkpoint: PipelineCheckpoint,
}

/// Offer interactive resume prompt when checkpoint exists without --resume flag.
///
/// This function checks if a checkpoint exists when the user did NOT specify
/// the --resume flag, and if so, offers to resume via an interactive prompt.
/// This provides a better user experience by detecting incomplete runs and
/// offering to continue them.
///
/// # Arguments
///
/// * `args` - CLI arguments (to check if --resume was already specified)
/// * `config` - Current configuration (for validation comparison)
/// * `registry` - Agent registry (for agent validation)
/// * `logger` - Logger for output
/// * `developer_agent` - Current developer agent name
/// * `reviewer_agent` - Current reviewer agent name
///
/// # Returns
///
/// `Some(ResumeResult)` if user chose to resume from a valid checkpoint,
/// `None` if no checkpoint exists, not in a TTY, user declined, or validation failed.
pub fn offer_resume_if_checkpoint_exists(
    args: &crate::cli::Args,
    config: &Config,
    registry: &AgentRegistry,
    logger: &Logger,
    developer_agent: &str,
    reviewer_agent: &str,
) -> Option<ResumeResult> {
    // Skip if --resume flag was already specified (handled by handle_resume_with_validation)
    if args.recovery.resume {
        return None;
    }

    // Skip if --no-resume flag is specified
    if args.recovery.no_resume {
        return None;
    }

    // Skip if RALPH_NO_RESUME_PROMPT env var is set (for CI/automation)
    if std::env::var("RALPH_NO_RESUME_PROMPT").is_ok() {
        return None;
    }

    // Skip if not in a TTY (can't prompt user)
    if !can_prompt_user() {
        return None;
    }

    // Check if checkpoint exists
    if !checkpoint_exists() {
        return None;
    }

    // Load checkpoint to display summary
    let checkpoint = match load_checkpoint() {
        Ok(Some(cp)) => cp,
        Ok(None) => return None,
        Err(e) => {
            logger.warn(&format!("Checkpoint exists but failed to load: {e}"));
            return None;
        }
    };

    // Display user-friendly checkpoint summary with time elapsed
    logger.header("FOUND PREVIOUS RUN", crate::logger::Colors::cyan);
    display_user_friendly_checkpoint_summary(&checkpoint, logger);

    // Prompt user to resume
    if !prompt_user_to_resume(logger) {
        // User declined - delete checkpoint and start fresh
        logger.info("Deleting checkpoint and starting fresh...");
        let _ = crate::checkpoint::clear_checkpoint();
        return None;
    }

    // User chose to resume - validate and proceed
    logger.header("RESUME: Loading Checkpoint", crate::logger::Colors::yellow);

    let validation = validate_checkpoint(&checkpoint, config, registry);

    for warning in &validation.warnings {
        logger.warn(warning);
    }
    for error in &validation.errors {
        logger.error(error);
    }

    if !validation.is_valid {
        logger.error("Checkpoint validation failed. Cannot resume.");
        logger.info("Delete .agent/checkpoint.json and start fresh, or fix the issues above.");
        return None;
    }

    if checkpoint.developer_agent != developer_agent {
        logger.warn(&format!(
            "Developer agent changed: {} -> {}",
            checkpoint.developer_agent, developer_agent
        ));
    }
    if checkpoint.reviewer_agent != reviewer_agent {
        logger.warn(&format!(
            "Reviewer agent changed: {} -> {}",
            checkpoint.reviewer_agent, reviewer_agent
        ));
    }

    check_rebase_state_on_resume(&checkpoint, logger);

    let validation_outcome = if let Some(ref file_system_state) = checkpoint.file_system_state {
        validate_file_system_state(
            file_system_state,
            logger,
            args.recovery.recovery_strategy.into(),
        )
    } else {
        ValidationOutcome::Passed
    };

    if matches!(validation_outcome, ValidationOutcome::Failed(_)) {
        return None;
    }

    Some(ResumeResult { checkpoint })
}

/// Check if we can prompt the user (stdin/stdout is a TTY).
fn can_prompt_user() -> bool {
    io::stdin().is_terminal() && (io::stdout().is_terminal() || io::stderr().is_terminal())
}

/// Display a user-friendly checkpoint summary with time elapsed.
fn display_user_friendly_checkpoint_summary(checkpoint: &PipelineCheckpoint, logger: &Logger) {
    use chrono::{DateTime, Local, NaiveDateTime};

    logger.info(&format!(
        "You were in the middle of: {}",
        checkpoint.description()
    ));

    // Calculate and display time elapsed
    // Parse the timestamp string which is in "YYYY-MM-DD HH:MM:SS" format
    let checkpoint_time =
        match NaiveDateTime::parse_from_str(&checkpoint.timestamp, "%Y-%m-%d %H:%M:%S") {
            Ok(dt) => {
                DateTime::<Local>::from_naive_utc_and_offset(dt, Local::now().offset().to_owned())
            }
            Err(_) => {
                // If parsing fails, just show the timestamp string
                logger.info(&format!(
                    "Session was interrupted at: {}",
                    checkpoint.timestamp
                ));
                return;
            }
        };
    let now = Local::now();
    let duration = now.signed_duration_since(checkpoint_time);

    let time_str = if duration.num_days() > 0 {
        format!("{} day(s) ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{} hour(s) ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{} minute(s) ago", duration.num_minutes())
    } else {
        "just now".to_string()
    };

    logger.info(&format!("Session was interrupted: {}", time_str));

    // Show rebase conflict information if applicable
    if matches!(
        checkpoint.rebase_state,
        crate::checkpoint::RebaseState::HasConflicts { .. }
    ) {
        if let crate::checkpoint::RebaseState::HasConflicts { files } = &checkpoint.rebase_state {
            logger.warn(&format!(
                "Rebase conflicts detected in {} file(s)",
                files.len()
            ));
            // Show up to 5 conflicted files
            let display_files: Vec<_> = files.iter().take(5).cloned().collect();
            for file in display_files {
                logger.info(&format!("  - {}", file));
            }
            if files.len() > 5 {
                logger.info(&format!("  ... and {} more", files.len() - 5));
            }
        }
    }

    // Show progress
    if checkpoint.total_iterations > 0 {
        logger.info(&format!(
            "Progress: {} of {} development iteration(s) completed",
            checkpoint.actual_developer_runs, checkpoint.total_iterations
        ));
    }
    if checkpoint.total_reviewer_passes > 0 {
        logger.info(&format!(
            "Progress: {} of {} review pass(es) completed",
            checkpoint.actual_reviewer_runs, checkpoint.total_reviewer_passes
        ));
    }

    // Show resume count if this is a resumed session
    if checkpoint.resume_count > 0 {
        logger.info(&format!(
            "This session has been resumed {} time(s) before",
            checkpoint.resume_count
        ));
    }

    // Show the reconstructed command that was used
    if let Some(reconstructed_command) = reconstruct_command(checkpoint) {
        logger.info(&format!("Original command: {}", reconstructed_command));
    }

    // Show agent configuration details
    logger.info(&format!("Developer agent: {}", checkpoint.developer_agent));
    logger.info(&format!("Reviewer agent: {}", checkpoint.reviewer_agent));

    // Show model overrides if present
    if let Some(ref model) = checkpoint.developer_agent_config.model_override {
        logger.info(&format!("Developer model: {}", model));
    }
    if let Some(ref model) = checkpoint.reviewer_agent_config.model_override {
        logger.info(&format!("Reviewer model: {}", model));
    }

    // Show provider overrides if present
    if let Some(ref provider) = checkpoint.developer_agent_config.provider_override {
        logger.info(&format!("Developer provider: {}", provider));
    }
    if let Some(ref provider) = checkpoint.reviewer_agent_config.provider_override {
        logger.info(&format!("Reviewer provider: {}", provider));
    }

    // Show execution history info if available
    if let Some(ref history) = checkpoint.execution_history {
        if !history.steps.is_empty() {
            logger.info(&format!(
                "Execution history: {} step(s) recorded",
                history.steps.len()
            ));
        }
    }

    // Show helpful next step based on current phase
    if let Some(next_step) = suggest_next_step(checkpoint) {
        logger.info(&format!("Next: {}", next_step));
    }

    // Show example commands for inspecting state
    logger.info("");
    logger.info("To inspect the current state, you can run:");
    logger.info("  git status        - See current changes");
    logger.info("  git log --oneline -5 - See recent commits");
}

/// Reconstruct the original command from checkpoint data.
///
/// This function attempts to reconstruct the exact command that was used
/// to create the checkpoint, including all relevant flags and options.
fn reconstruct_command(checkpoint: &PipelineCheckpoint) -> Option<String> {
    let cli = &checkpoint.cli_args;
    let mut parts = vec!["ralph".to_string()];

    // Add -D flag
    if cli.developer_iters > 0 {
        parts.push(format!("-D {}", cli.developer_iters));
    }

    // Add -R flag
    if cli.reviewer_reviews > 0 {
        parts.push(format!("-R {}", cli.reviewer_reviews));
    }

    // Add --commit-msg if specified
    if !cli.commit_msg.is_empty() {
        parts.push(format!("--commit-msg \"{}\"", cli.commit_msg));
    }

    // Add --review-depth if specified
    if let Some(ref depth) = cli.review_depth {
        parts.push(format!("--review-depth {}", depth));
    }

    // Add --skip-rebase if true
    if cli.skip_rebase {
        parts.push("--skip-rebase".to_string());
    }

    // Add --no-isolation if false (isolation_mode defaults to true)
    if !cli.isolation_mode {
        parts.push("--no-isolation".to_string());
    }

    // Add verbosity flags
    match cli.verbosity {
        0 => parts.push("--quiet".to_string()),
        1 => {} // Normal is default
        2 => parts.push("--verbose".to_string()),
        3 => parts.push("--full".to_string()),
        4 => parts.push("--debug".to_string()),
        _ => {}
    }

    // Add --show-streaming-metrics if true
    if cli.show_streaming_metrics {
        parts.push("--show-streaming-metrics".to_string());
    }

    // Add --reviewer-json-parser if specified
    if let Some(ref parser) = cli.reviewer_json_parser {
        parts.push(format!("--reviewer-json-parser {}", parser));
    }

    // Add --agent flags if agents differ from defaults
    // Note: We can't determine defaults here, so we always show them
    parts.push(format!("--agent {}", checkpoint.developer_agent));
    parts.push(format!("--reviewer-agent {}", checkpoint.reviewer_agent));

    // Add model overrides if present
    if let Some(ref model) = checkpoint.developer_agent_config.model_override {
        parts.push(format!("--model \"{}\"", model));
    }
    if let Some(ref model) = checkpoint.reviewer_agent_config.model_override {
        parts.push(format!("--reviewer-model \"{}\"", model));
    }

    // Add provider overrides if present
    if let Some(ref provider) = checkpoint.developer_agent_config.provider_override {
        parts.push(format!("--provider \"{}\"", provider));
    }
    if let Some(ref provider) = checkpoint.reviewer_agent_config.provider_override {
        parts.push(format!("--reviewer-provider \"{}\"", provider));
    }

    if parts.len() > 1 {
        Some(parts.join(" "))
    } else {
        None
    }
}

/// Suggest the next step based on the current checkpoint phase.
///
/// Returns a detailed, actionable description of what will happen next
/// when the user resumes from this checkpoint.
fn suggest_next_step(checkpoint: &PipelineCheckpoint) -> Option<String> {
    match checkpoint.phase {
        PipelinePhase::Planning => {
            Some("continue creating implementation plan from PROMPT.md".to_string())
        }
        PipelinePhase::PreRebase => Some("complete rebase before starting development".to_string()),
        PipelinePhase::PreRebaseConflict => {
            Some("resolve rebase conflicts then continue to development".to_string())
        }
        PipelinePhase::Development => {
            if checkpoint.iteration < checkpoint.total_iterations {
                Some(format!(
                    "continue development iteration {} of {} (will use same prompts as before)",
                    checkpoint.iteration + 1,
                    checkpoint.total_iterations
                ))
            } else {
                Some("move to review phase".to_string())
            }
        }
        PipelinePhase::Review => {
            if checkpoint.reviewer_pass < checkpoint.total_reviewer_passes {
                Some(format!(
                    "continue review pass {} of {} (will review recent changes)",
                    checkpoint.reviewer_pass + 1,
                    checkpoint.total_reviewer_passes
                ))
            } else {
                Some("complete review cycle".to_string())
            }
        }
        PipelinePhase::Fix => Some("address issues from code review".to_string()),
        PipelinePhase::ReviewAgain => Some("complete verification review".to_string()),
        PipelinePhase::PostRebase => Some("complete post-development rebase".to_string()),
        PipelinePhase::PostRebaseConflict => Some("resolve post-rebase conflicts".to_string()),
        PipelinePhase::CommitMessage => Some("finalize commit message".to_string()),
        PipelinePhase::FinalValidation => Some("complete final validation".to_string()),
        PipelinePhase::Complete => Some("pipeline complete!".to_string()),
        PipelinePhase::Rebase => Some("complete rebase operation".to_string()),
    }
}

/// Prompt user to decide whether to resume or start fresh.
///
/// Returns `true` if user wants to resume, `false` if they want to start fresh.
fn prompt_user_to_resume(logger: &Logger) -> bool {
    use std::io::Write;

    println!();
    logger.info("Would you like to resume from where you left off?");

    let prompt = "Resume? [y/N] ";
    let colors = crate::logger::Colors::new();

    let mut input = String::new();
    // Print prompt directly to stdout for better UX
    print!("{}", colors.yellow());
    let _ = io::stdout().write_all(prompt.as_bytes());
    let _ = io::stdout().flush();
    print!("{}", colors.reset());

    match io::stdin().read_line(&mut input) {
        Ok(0) => {
            // EOF
            println!();
            false
        }
        Ok(_) => {
            let response = input.trim().to_lowercase();
            println!();

            matches!(response.as_str(), "y" | "yes" | "Y" | "YES")
        }
        Err(_) => false,
    }
}

/// Result of file system validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationOutcome {
    /// Validation passed, safe to resume
    Passed,
    /// Validation failed, cannot resume
    Failed(String),
}

/// Handles the --resume flag and loads checkpoint if applicable.
///
/// This function loads and validates the checkpoint, providing detailed
/// feedback about what state is being restored and any configuration changes.
///
/// # Arguments
///
/// * `args` - CLI arguments
/// * `config` - Current configuration (for validation comparison)
/// * `registry` - Agent registry (for agent validation)
/// * `logger` - Logger for output
/// * `developer_agent` - Current developer agent name
/// * `reviewer_agent` - Current reviewer agent name
///
/// # Returns
///
/// `Some(ResumeResult)` if a valid checkpoint was found and loaded,
/// `None` if no checkpoint exists or --resume was not specified.
pub fn handle_resume_with_validation(
    args: &crate::cli::Args,
    config: &Config,
    registry: &AgentRegistry,
    logger: &Logger,
    developer_agent: &str,
    reviewer_agent: &str,
) -> Option<ResumeResult> {
    if !args.recovery.resume {
        return None;
    }

    match load_checkpoint() {
        Ok(Some(checkpoint)) => {
            logger.header("RESUME: Loading Checkpoint", crate::logger::Colors::yellow);
            display_checkpoint_summary(&checkpoint, logger);

            // Validate checkpoint
            let validation = validate_checkpoint(&checkpoint, config, registry);

            // Display validation results
            for warning in &validation.warnings {
                logger.warn(warning);
            }
            for error in &validation.errors {
                logger.error(error);
            }

            if !validation.is_valid {
                logger.error("Checkpoint validation failed. Cannot resume.");
                logger.info(
                    "Delete .agent/checkpoint.json and start fresh, or fix the issues above.",
                );
                return None;
            }

            // Verify agents match (additional agent-specific warnings)
            if checkpoint.developer_agent != developer_agent {
                logger.warn(&format!(
                    "Developer agent changed: {} -> {}",
                    checkpoint.developer_agent, developer_agent
                ));
            }
            if checkpoint.reviewer_agent != reviewer_agent {
                logger.warn(&format!(
                    "Reviewer agent changed: {} -> {}",
                    checkpoint.reviewer_agent, reviewer_agent
                ));
            }

            // Check for in-progress git rebase
            check_rebase_state_on_resume(&checkpoint, logger);

            // Perform file system state validation
            let validation_outcome = if let Some(file_system_state) = &checkpoint.file_system_state
            {
                validate_file_system_state(
                    file_system_state,
                    logger,
                    args.recovery.recovery_strategy.into(),
                )
            } else {
                ValidationOutcome::Passed
            };

            if matches!(validation_outcome, ValidationOutcome::Failed(_)) {
                return None;
            }

            Some(ResumeResult { checkpoint })
        }
        Ok(None) => {
            logger.warn("No checkpoint found. Starting fresh pipeline...");
            None
        }
        Err(e) => {
            logger.warn(&format!("Failed to load checkpoint (starting fresh): {e}"));
            None
        }
    }
}

/// Validate file system state when resuming.
///
/// This function validates that the current file system state matches
/// the state captured in the checkpoint. This is part of the hardened
/// resume feature that ensures idempotent recovery.
///
/// Returns a `ValidationOutcome` indicating whether validation passed
/// or failed with a reason.
fn validate_file_system_state(
    file_system_state: &FileSystemState,
    logger: &Logger,
    strategy: crate::checkpoint::recovery::RecoveryStrategy,
) -> ValidationOutcome {
    let errors = file_system_state.validate();

    if errors.is_empty() {
        logger.info("File system state validation passed.");
        return ValidationOutcome::Passed;
    }

    logger.warn("File system state validation detected changes:");

    for error in &errors {
        logger.warn(&format!("  - {}", error));
        logger.info(&format!("    Suggestion: {}", error.recovery_suggestion()));
    }

    // Handle based on the recovery strategy
    match strategy {
        crate::checkpoint::recovery::RecoveryStrategy::Fail => {
            logger.error("File system state validation failed (strategy: fail).");
            logger.info("Use --recovery-strategy=auto to attempt automatic recovery.");
            logger.info("Use --recovery-strategy=force to proceed anyway (not recommended).");
            ValidationOutcome::Failed(
                "File system state changed - see errors above or use --recovery-strategy=force to proceed anyway".to_string()
            )
        }
        crate::checkpoint::recovery::RecoveryStrategy::Force => {
            logger.warn("Proceeding with resume despite file changes (strategy: force).");
            logger.info("Note: Pipeline behavior may be unpredictable.");
            ValidationOutcome::Passed
        }
        crate::checkpoint::recovery::RecoveryStrategy::Auto => {
            // Attempt automatic recovery for recoverable errors
            let (_recovered, remaining) = attempt_auto_recovery(file_system_state, &errors, logger);

            if remaining.is_empty() {
                logger.success("Automatic recovery completed successfully.");
                ValidationOutcome::Passed
            } else {
                logger.warn("Some issues could not be automatically recovered:");
                for error in &remaining {
                    logger.warn(&format!("  - {}", error));
                }
                logger.warn("Proceeding with resume despite unrecovered issues (strategy: auto).");
                logger.info("Note: Pipeline behavior may be unpredictable.");
                ValidationOutcome::Passed
            }
        }
    }
}

/// Attempt automatic recovery from file system state changes.
///
/// This function attempts to automatically fix recoverable issues:
/// - Restores small files from content stored in snapshot
/// - Warns about unrecoverable issues (large files, git changes)
///
/// # Arguments
///
/// * `file_system_state` - The file system state from checkpoint
/// * `errors` - Validation errors that were detected
/// * `logger` - Logger for output
///
/// # Returns
///
/// A tuple of (number of issues recovered, remaining errors)
fn attempt_auto_recovery(
    file_system_state: &FileSystemState,
    errors: &[ValidationError],
    logger: &Logger,
) -> (usize, Vec<ValidationError>) {
    let mut recovered = 0;
    let mut remaining = Vec::new();

    for error in errors {
        match attempt_recovery_for_error(file_system_state, error, logger) {
            Ok(()) => {
                recovered += 1;
                logger.success(&format!("Recovered: {}", error));
            }
            Err(e) => {
                remaining.push(error.clone());
                logger.warn(&format!("Could not recover: {} - {}", error, e));
            }
        }
    }

    (recovered, remaining)
}

/// Attempt to recover from a single validation error.
///
/// # Returns
///
/// `Ok(())` if recovery succeeded, `Err(reason)` if it failed.
fn attempt_recovery_for_error(
    file_system_state: &FileSystemState,
    error: &ValidationError,
    logger: &Logger,
) -> Result<(), String> {
    match error {
        ValidationError::FileContentChanged { path } => {
            // Try to restore from snapshot if content is available
            if let Some(snapshot) = file_system_state.files.get(path) {
                if let Some(content) = &snapshot.content {
                    fs::write(path, content).map_err(|e| format!("Failed to write file: {}", e))?;
                    logger.info(&format!("Restored {} from checkpoint content.", path));
                    return Ok(());
                }
            }
            Err("No content available in snapshot".to_string())
        }
        ValidationError::GitHeadChanged { .. } => {
            // Git state changes are not automatically recoverable
            // They require user intervention to reset or accept the new state
            Err("Git HEAD changes require manual intervention".to_string())
        }
        ValidationError::GitStateInvalid { .. } => {
            Err("Git state validation requires manual intervention".to_string())
        }
        ValidationError::FileMissing { path } => {
            // Can't recover a missing file unless we have content
            if let Some(snapshot) = file_system_state.files.get(path) {
                if let Some(content) = &snapshot.content {
                    fs::write(path, content).map_err(|e| format!("Failed to write file: {}", e))?;
                    logger.info(&format!("Restored missing {} from checkpoint.", path));
                    return Ok(());
                }
            }
            Err(format!("Cannot recover missing file {}", path))
        }
        ValidationError::FileUnexpectedlyExists { path } => {
            // Unexpected files should be removed by user
            Err(format!(
                "File {} should not exist - requires manual removal",
                path
            ))
        }
    }
}

/// Check for in-progress git rebase when resuming.
///
/// This function detects if a git rebase is in progress and provides
/// appropriate guidance to the user.
fn check_rebase_state_on_resume(checkpoint: &PipelineCheckpoint, logger: &Logger) {
    // Only check for rebase if we're resuming from a rebase-related phase
    let is_rebase_phase = matches!(
        checkpoint.phase,
        PipelinePhase::PreRebase
            | PipelinePhase::PreRebaseConflict
            | PipelinePhase::PostRebase
            | PipelinePhase::PostRebaseConflict
    );

    if !is_rebase_phase {
        return;
    }

    match rebase_in_progress() {
        Ok(true) => {
            logger.warn("A git rebase is currently in progress.");
            logger.info("The checkpoint indicates you were in a rebase phase.");
            logger.info("Options:");
            logger.info("  - Continue: Let ralph complete the rebase process");
            logger.info("  - Abort manually: Run 'git rebase --abort' then use --resume");
        }
        Ok(false) => {
            // No rebase in progress - this is expected if rebase completed
            // but checkpoint wasn't cleared (e.g., pipeline was interrupted)
            logger.info("No git rebase is currently in progress.");
        }
        Err(e) => {
            logger.warn(&format!("Could not check rebase state: {e}"));
        }
    }
}

/// Display a summary of the checkpoint being loaded.
fn display_checkpoint_summary(checkpoint: &PipelineCheckpoint, logger: &Logger) {
    logger.info(&format!("Resuming from: {}", checkpoint.description()));
    logger.info(&format!("Checkpoint saved at: {}", checkpoint.timestamp));
    logger.info(&format!("Checkpoint version: {}", checkpoint.version));

    // Show run ID and resume count
    logger.info(&format!("Run ID: {}", checkpoint.run_id));
    if checkpoint.resume_count > 0 {
        logger.info(&format!(
            "Resume count: {} (this is resume #{} of this session)",
            checkpoint.resume_count,
            checkpoint.resume_count + 1
        ));
    }
    if let Some(ref parent_id) = checkpoint.parent_run_id {
        logger.info(&format!("Parent run ID: {}", parent_id));
    }

    // Show actual execution counts vs configured counts
    logger.info(&format!(
        "Development: {} iteration(s) configured, {} completed",
        checkpoint.total_iterations, checkpoint.actual_developer_runs
    ));
    logger.info(&format!(
        "Review: {} pass(es) configured, {} completed",
        checkpoint.total_reviewer_passes, checkpoint.actual_reviewer_runs
    ));

    // Show iteration progress
    if checkpoint.total_iterations > 0 {
        logger.info(&format!(
            "Current position: iteration {}/{}",
            checkpoint.iteration, checkpoint.total_iterations
        ));
    }
    if checkpoint.total_reviewer_passes > 0 {
        logger.info(&format!(
            "Current position: pass {}/{}",
            checkpoint.reviewer_pass, checkpoint.total_reviewer_passes
        ));
    }

    // Show CLI args if available
    let cli = &checkpoint.cli_args;
    if cli.developer_iters > 0 || cli.reviewer_reviews > 0 {
        logger.info(&format!(
            "Original config: -D {} -R {}",
            cli.developer_iters, cli.reviewer_reviews
        ));
    }

    // Show agent configs
    logger.info(&format!("Developer agent: {}", checkpoint.developer_agent));
    logger.info(&format!("Reviewer agent: {}", checkpoint.reviewer_agent));

    // Show model overrides if present
    if let Some(ref model) = checkpoint.developer_agent_config.model_override {
        logger.info(&format!("Developer model override: {}", model));
    }
    if let Some(ref model) = checkpoint.reviewer_agent_config.model_override {
        logger.info(&format!("Reviewer model override: {}", model));
    }

    // Show provider overrides if present
    if let Some(ref provider) = checkpoint.developer_agent_config.provider_override {
        logger.info(&format!("Developer provider: {}", provider));
    }
    if let Some(ref provider) = checkpoint.reviewer_agent_config.provider_override {
        logger.info(&format!("Reviewer provider: {}", provider));
    }

    // Show rebase state if applicable
    match &checkpoint.rebase_state {
        crate::checkpoint::RebaseState::PreRebaseInProgress { upstream_branch } => {
            logger.warn(&format!("Pre-rebase in progress to: {}", upstream_branch));
        }
        crate::checkpoint::RebaseState::HasConflicts { files } => {
            logger.warn(&format!("Rebase has conflicts in {} files", files.len()));
            for file in files.iter().take(3) {
                logger.warn(&format!("  - {}", file));
            }
            if files.len() > 3 {
                logger.warn(&format!("  ... and {} more", files.len() - 3));
            }
        }
        _ => {}
    }
}

/// Helper to get phase rank for resume logic.
///
/// Lower ranks = earlier in pipeline. Used to determine which phases
/// should run when resuming from a checkpoint.
pub const fn phase_rank(p: PipelinePhase) -> u8 {
    match p {
        PipelinePhase::Rebase => 0, // Backward compatibility with old checkpoints
        PipelinePhase::Planning => 1,
        PipelinePhase::PreRebase => 2,
        PipelinePhase::PreRebaseConflict => 3,
        PipelinePhase::Development => 4,
        PipelinePhase::Review => 5,
        PipelinePhase::Fix => 6,
        PipelinePhase::ReviewAgain => 7,
        PipelinePhase::PostRebase => 8,
        PipelinePhase::PostRebaseConflict => 9,
        PipelinePhase::CommitMessage => 10,
        PipelinePhase::FinalValidation => 11,
        PipelinePhase::Complete => 12,
    }
}

/// Determines if a phase should run based on resume checkpoint.
pub const fn should_run_from(
    phase: PipelinePhase,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> bool {
    match resume_checkpoint {
        None => true,
        Some(checkpoint) => phase_rank(phase) >= phase_rank(checkpoint.phase),
    }
}
