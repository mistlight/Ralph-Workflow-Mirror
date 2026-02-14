// Detailed display functions for checkpoint inspection.
// This module handles comprehensive checkpoint inspection output.

/// Display detailed checkpoint information for inspection.
///
/// This function shows comprehensive checkpoint details when the user
/// runs with the --inspect-checkpoint flag.
fn display_detailed_checkpoint_info(checkpoint: &PipelineCheckpoint, logger: &Logger) {
    use chrono::Local;

    logger.info(&format!("Phase: {}", checkpoint.phase));
    logger.info(&format!("Timestamp: {}", checkpoint.timestamp));

    // Calculate and display time elapsed
    if let Some(checkpoint_time) = parse_checkpoint_timestamp_as_local(&checkpoint.timestamp) {
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
