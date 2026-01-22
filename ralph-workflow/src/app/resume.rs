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

    // Display phase with emoji indicator
    let phase_emoji = get_phase_emoji(checkpoint.phase);
    logger.info(&format!("{} {}", phase_emoji, checkpoint.description()));

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

    // Show progress with visual bar
    if checkpoint.total_iterations > 0 {
        let progress_bar = create_progress_bar(
            checkpoint.actual_developer_runs,
            checkpoint.total_iterations,
        );
        logger.info(&format!(
            "Development: {} {}/{} completed",
            progress_bar, checkpoint.actual_developer_runs, checkpoint.total_iterations
        ));
    }
    if checkpoint.total_reviewer_passes > 0 {
        let progress_bar = create_progress_bar(
            checkpoint.actual_reviewer_runs,
            checkpoint.total_reviewer_passes,
        );
        logger.info(&format!(
            "Review: {} {}/{} completed",
            progress_bar, checkpoint.actual_reviewer_runs, checkpoint.total_reviewer_passes
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

            // Show recent activity (last 5 steps) with user-friendly details
            let recent_steps: Vec<_> = history
                .steps
                .iter()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            logger.info("");
            logger.info("Recent Activity:");

            for step in &recent_steps {
                let outcome_emoji = match step.outcome {
                    crate::checkpoint::execution_history::StepOutcome::Success { .. } => "✓",
                    crate::checkpoint::execution_history::StepOutcome::Failure { .. } => "✗",
                    crate::checkpoint::execution_history::StepOutcome::Partial { .. } => "◐",
                    crate::checkpoint::execution_history::StepOutcome::Skipped { .. } => "○",
                };

                logger.info(&format!(
                    "  {} {} ({})",
                    outcome_emoji, step.step_type, step.phase
                ));

                // Add files modified count if available
                if let Some(ref detail) = step.modified_files_detail {
                    let total_files =
                        detail.added.len() + detail.modified.len() + detail.deleted.len();
                    if total_files > 0 {
                        let mut file_summary = String::from("    Files: ");
                        let mut parts = Vec::new();
                        if !detail.added.is_empty() {
                            parts.push(format!("{} added", detail.added.len()));
                        }
                        if !detail.modified.is_empty() {
                            parts.push(format!("{} modified", detail.modified.len()));
                        }
                        if !detail.deleted.is_empty() {
                            parts.push(format!("{} deleted", detail.deleted.len()));
                        }
                        file_summary.push_str(&parts.join(", "));
                        logger.info(&file_summary);
                    }
                }

                // Add issues summary if available
                if let Some(ref issues) = step.issues_summary {
                    if issues.found > 0 || issues.fixed > 0 {
                        logger.info(&format!(
                            "    Issues: {} found, {} fixed",
                            issues.found, issues.fixed
                        ));
                    }
                }

                // Add git commit if available (shortened)
                if let Some(ref oid) = step.git_commit_oid {
                    let short_oid = if oid.len() > 8 { &oid[..8] } else { oid };
                    logger.info(&format!("    Commit: {}", short_oid));
                }
            }
        }
    }

    // Show helpful next step based on current phase
    if let Some(next_step) = suggest_next_step(checkpoint) {
        logger.info("");
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
        PipelinePhase::Interrupted => {
            // Provide more detailed information for interrupted state
            // The interrupted phase can occur at any point, so we need to describe
            // what the user was doing when interrupted
            let mut context = vec!["resume from interrupted state".to_string()];

            // Add context about what was being worked on
            if checkpoint.iteration > 0 {
                context.push(format!(
                    "(development iteration {}/{})",
                    checkpoint.iteration, checkpoint.total_iterations
                ));
            }
            if checkpoint.reviewer_pass > 0 {
                context.push(format!(
                    "(review pass {}/{})",
                    checkpoint.reviewer_pass, checkpoint.total_reviewer_passes
                ));
            }

            // Explain what will happen on resume
            context.push("full pipeline will run from interrupted point".to_string());

            Some(context.join(" - "))
        }
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
    // Handle --inspect-checkpoint flag
    if args.recovery.inspect_checkpoint {
        match load_checkpoint() {
            Ok(Some(checkpoint)) => {
                logger.header("CHECKPOINT INSPECTION", crate::logger::Colors::cyan);
                display_detailed_checkpoint_info(&checkpoint, logger);
                std::process::exit(0);
            }
            Ok(None) => {
                logger.error("No checkpoint found to inspect.");
                std::process::exit(1);
            }
            Err(e) => {
                logger.error(&format!("Failed to load checkpoint: {}", e));
                std::process::exit(1);
            }
        }
    }

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
        let (problem, commands) = error.recovery_commands();
        logger.warn(&format!("  - {}", error));
        logger.info(&format!("    What's wrong: {}", problem));
        logger.info("    How to fix:");
        for cmd in commands {
            logger.info(&format!("      {}", cmd));
        }
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
                if let Some(content) = snapshot.get_content() {
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
        ValidationError::GitWorkingTreeChanged { .. } => {
            // Working tree changes are not automatically recoverable
            Err("Git working tree changes require manual intervention".to_string())
        }
        ValidationError::FileMissing { path } => {
            // Can't recover a missing file unless we have content
            if let Some(snapshot) = file_system_state.files.get(path) {
                if let Some(content) = snapshot.get_content() {
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
/// Create a visual progress bar for checkpoint summary display.
fn create_progress_bar(current: u32, total: u32) -> String {
    if total == 0 {
        return "[----]".to_string();
    }

    let width = 20; // Total width of progress bar
    let filled = ((current as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.min(width);

    let mut bar = String::from("[");
    for i in 0..width {
        if i < filled {
            bar.push('=');
        } else {
            bar.push('-');
        }
    }
    bar.push(']');

    let percentage = ((current as f64 / total as f64) * 100.0).round() as u32;
    format!("{} {}%", bar, percentage)
}

/// Display detailed checkpoint information for inspection.
///
/// This function shows comprehensive checkpoint details when the user
/// runs with the --inspect-checkpoint flag.
fn display_detailed_checkpoint_info(checkpoint: &PipelineCheckpoint, logger: &Logger) {
    use chrono::{DateTime, Local, NaiveDateTime};

    logger.info(&format!("Phase: {}", checkpoint.phase));
    logger.info(&format!("Timestamp: {}", checkpoint.timestamp));

    // Calculate and display time elapsed
    if let Ok(dt) = NaiveDateTime::parse_from_str(&checkpoint.timestamp, "%Y-%m-%d %H:%M:%S") {
        let checkpoint_time =
            DateTime::<Local>::from_naive_utc_and_offset(dt, Local::now().offset().to_owned());
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
        logger.info(&format!("Time elapsed: {}", time_str));
    }

    logger.info("");
    logger.info("Configuration:");

    // Show iterations and reviews
    if checkpoint.total_iterations > 0 {
        let progress_bar = create_progress_bar(
            checkpoint.actual_developer_runs,
            checkpoint.total_iterations,
        );
        logger.info(&format!(
            "  Development: {} {}/{}",
            progress_bar, checkpoint.actual_developer_runs, checkpoint.total_iterations
        ));
    }
    if checkpoint.total_reviewer_passes > 0 {
        let progress_bar = create_progress_bar(
            checkpoint.actual_reviewer_runs,
            checkpoint.total_reviewer_passes,
        );
        logger.info(&format!(
            "  Review: {} {}/{}",
            progress_bar, checkpoint.actual_reviewer_runs, checkpoint.total_reviewer_passes
        ));
    }

    logger.info("");
    logger.info("Agents:");
    logger.info(&format!("  Developer: {}", checkpoint.developer_agent));
    logger.info(&format!("  Reviewer: {}", checkpoint.reviewer_agent));

    // Show model overrides
    if let Some(ref model) = checkpoint.developer_agent_config.model_override {
        logger.info(&format!("  Developer model: {}", model));
    }
    if let Some(ref model) = checkpoint.reviewer_agent_config.model_override {
        logger.info(&format!("  Reviewer model: {}", model));
    }
    if let Some(ref provider) = checkpoint.developer_agent_config.provider_override {
        logger.info(&format!("  Developer provider: {}", provider));
    }
    if let Some(ref provider) = checkpoint.reviewer_agent_config.provider_override {
        logger.info(&format!("  Reviewer provider: {}", provider));
    }

    // Show CLI args
    if let Some(ref cmd) = reconstruct_command(checkpoint) {
        logger.info("");
        logger.info(&format!("Command: {}", cmd));
    }

    // Show resume count
    if checkpoint.resume_count > 0 {
        logger.info("");
        logger.info(&format!(
            "Resumed {} time(s) before",
            checkpoint.resume_count
        ));
    }

    // Show run ID
    logger.info("");
    logger.info(&format!("Run ID: {}", checkpoint.run_id));
    if let Some(ref parent_id) = checkpoint.parent_run_id {
        logger.info(&format!("Parent Run ID: {}", parent_id));
    }

    // Show rebase state if applicable
    if matches!(
        checkpoint.rebase_state,
        crate::checkpoint::RebaseState::HasConflicts { .. }
    ) {
        logger.info("");
        logger.warn("Rebase conflicts detected:");
        if let crate::checkpoint::RebaseState::HasConflicts { files } = &checkpoint.rebase_state {
            for file in files.iter().take(10) {
                logger.info(&format!("  - {}", file));
            }
            if files.len() > 10 {
                logger.info(&format!("  ... and {} more", files.len() - 10));
            }
        }
    }

    // Show execution history if available
    if let Some(ref history) = checkpoint.execution_history {
        if !history.steps.is_empty() {
            logger.info("");
            logger.info(&format!(
                "Execution History: {} step(s)",
                history.steps.len()
            ));
            for (i, step) in history.steps.iter().take(10).enumerate() {
                let outcome_str = match &step.outcome {
                    crate::checkpoint::execution_history::StepOutcome::Success { .. } => "✓",
                    crate::checkpoint::execution_history::StepOutcome::Failure { .. } => "✗",
                    crate::checkpoint::execution_history::StepOutcome::Partial { .. } => "◐",
                    crate::checkpoint::execution_history::StepOutcome::Skipped { .. } => "○",
                };
                logger.info(&format!(
                    "  {}. {} {} ({})",
                    i + 1,
                    outcome_str,
                    step.step_type,
                    step.phase
                ));
            }
            if history.steps.len() > 10 {
                logger.info(&format!(
                    "  ... and {} more steps",
                    history.steps.len() - 10
                ));
            }
        }
    }

    // Show file system state if available
    if let Some(ref fs_state) = checkpoint.file_system_state {
        logger.info("");
        logger.info(&format!(
            "File System State: {} file(s) tracked",
            fs_state.files.len()
        ));

        // Show git state
        if let Some(ref branch) = fs_state.git_branch {
            logger.info(&format!("  Git branch: {}", branch));
        }
        if let Some(ref head) = fs_state.git_head_oid {
            logger.info(&format!("  Git HEAD: {}", head));
        }
        if let Some(ref status) = fs_state.git_status {
            if !status.is_empty() {
                logger.warn("  Git working tree has changes:");
                for line in status.lines().take(5) {
                    logger.info(&format!("    {}", line));
                }
            }
        }
    }

    // Show environment snapshot if available
    if let Some(ref env_snap) = checkpoint.env_snapshot {
        if !env_snap.ralph_vars.is_empty() {
            logger.info("");
            logger.info(&format!(
                "Environment Variables: {} RALPH_* var(s)",
                env_snap.ralph_vars.len()
            ));
            for (key, value) in env_snap.ralph_vars.iter().take(10) {
                logger.info(&format!("  {}={}", key, value));
            }
            if env_snap.ralph_vars.len() > 10 {
                logger.info(&format!(
                    "  ... and {} more",
                    env_snap.ralph_vars.len() - 10
                ));
            }
        }
    }

    // Show working directory
    logger.info("");
    logger.info(&format!("Working directory: {}", checkpoint.working_dir));
}

/// Get an emoji indicator for a pipeline phase.
fn get_phase_emoji(phase: PipelinePhase) -> &'static str {
    match phase {
        PipelinePhase::Rebase => "🔄",
        PipelinePhase::Planning => "📋",
        PipelinePhase::Development => "🔨",
        PipelinePhase::Review => "👀",
        PipelinePhase::Fix => "🔧",
        PipelinePhase::ReviewAgain => "🔍",
        PipelinePhase::CommitMessage => "📝",
        PipelinePhase::FinalValidation => "✅",
        PipelinePhase::Complete => "🎉",
        PipelinePhase::PreRebase => "⏪",
        PipelinePhase::PreRebaseConflict => "⚠️",
        PipelinePhase::PostRebase => "⏩",
        PipelinePhase::PostRebaseConflict => "⚠️",
        PipelinePhase::Interrupted => "⏸️",
    }
}
