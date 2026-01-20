//! Review phase execution.
//!
//! This module handles the review and fix phase of the Ralph pipeline. It runs
//! a configurable number of review-fix cycles, where each cycle:
//! 1. Reviews the code and creates ISSUES.md
//! 2. Fixes the issues found
//! 3. Cleans up ISSUES.md (in isolation mode)
//!
//! # Module Structure
//!
//! - [`prompt`] - Review prompt building logic
//! - [`validation`] - Pre-flight and post-flight validation checks

use crate::agents::AgentRole;
use crate::checkpoint::restore::ResumeContext;
use crate::checkpoint::{save_checkpoint, CheckpointBuilder, PipelinePhase};
use crate::files::extract_issues;
use crate::files::{clean_context_for_reviewer, delete_issues_file_for_isolation, update_status};
use crate::git_helpers::{
    get_baseline_summary, git_snapshot, update_review_baseline, CommitResultFallback,
};
use crate::logger::{print_progress, Logger};
use crate::phases::commit::commit_with_generated_message;
use crate::phases::get_primary_commit_agent;
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_with_fallback_and_validator, FallbackConfig, PipelineRuntime};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_for_agent, Action, ContextLevel, PromptConfig, Role,
};
use crate::review_metrics::ReviewMetrics;
use std::fs;
use std::path::Path;

mod prompt;
pub use prompt::{build_review_prompt, should_use_universal_prompt};

mod validation;
pub use validation::{
    post_flight_review_check, pre_flight_review_check, PostflightResult, PreflightResult,
};

use super::context::PhaseContext;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};

use std::time::Instant;

/// Result of the review phase.
pub struct ReviewResult {
    /// Whether the review completed early due to no issues found.
    pub completed_early: bool,
}

/// Run the review and fix phase.
///
/// This phase runs `reviewer_reviews` review-fix cycles. Each cycle:
/// 1. Runs a code review (creates ISSUES.md)
/// 2. Fixes the identified issues
/// 3. Cleans up ISSUES.md in isolation mode
///
/// The phase may exit early if a review finds no issues.
///
/// # Arguments
///
/// * `ctx` - The phase context containing shared state
/// * `start_pass` - The review pass to start from (for resume support)
/// * `resume_context` - Optional resume context for resumed sessions
///
/// # Returns
///
/// Returns `Ok(ReviewResult)` on success, or an error if a critical failure occurs.
pub fn run_review_phase(
    ctx: &mut PhaseContext<'_>,
    start_pass: u32,
    resume_context: Option<&ResumeContext>,
) -> anyhow::Result<ReviewResult> {
    let reviewer_context = ContextLevel::from(ctx.config.reviewer_context);

    // Clean context for reviewer if using minimal context
    if reviewer_context == ContextLevel::Minimal {
        clean_context_for_reviewer(ctx.logger, ctx.config.isolation_mode)?;
    }

    // Skip if no review cycles configured
    if ctx.config.reviewer_reviews == 0 {
        ctx.logger
            .info("Skipping review phase (reviewer_reviews=0)");
        return Ok(ReviewResult {
            completed_early: false,
        });
    }

    ctx.logger.info(&format!(
        "Running {}{}{} review → fix cycles ({})",
        ctx.colors.bold(),
        ctx.config.reviewer_reviews,
        ctx.colors.reset(),
        ctx.reviewer_agent
    ));

    // Track git snapshots for detecting changes during review
    let mut prev_snap = git_snapshot()?;
    // Track how many review cycles were skipped due to diff retrieval failures
    let mut skipped_cycles = 0;

    // Review-Fix iterations
    for j in start_pass..=ctx.config.reviewer_reviews {
        let resuming_into_review = resume_context.is_some() && j == start_pass;
        // Save checkpoint at start of each iteration
        if ctx.config.features.checkpoint_enabled {
            let builder = CheckpointBuilder::new()
                .phase(
                    PipelinePhase::Review,
                    ctx.config.developer_iters,
                    ctx.config.developer_iters,
                )
                .reviewer_pass(j, ctx.config.reviewer_reviews)
                .capture_from_context(
                    ctx.config,
                    ctx.registry,
                    ctx.developer_agent,
                    ctx.reviewer_agent,
                    ctx.logger,
                    &ctx.run_context,
                )
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint(&checkpoint);
            }
        }

        // Record this pass as completed
        ctx.record_reviewer_pass();

        ctx.logger.subheader(&format!(
            "Review-Fix Cycle {} of {}",
            j, ctx.config.reviewer_reviews
        ));
        print_progress(j, ctx.config.reviewer_reviews, "Review-Fix cycles");

        // Display baseline information
        match get_baseline_summary() {
            Ok(summary) => {
                if ctx.config.verbosity.is_debug() {
                    // Show detailed baseline information in verbose mode
                    ctx.logger.info(&summary.format_detailed());
                } else {
                    ctx.logger.info(&summary.format_compact());
                }
                if summary.is_stale {
                    ctx.logger.warn(&format!(
                        "Baseline is stale ({} commits behind). Consider updating the baseline to focus the review on recent changes.",
                        summary.commits_since
                    ));
                }
            }
            Err(e) => {
                ctx.logger
                    .warn(&format!("Unable to retrieve baseline information: {e}"));
            }
        }

        // PRE-FLIGHT VALIDATION: Check environment before running review
        match pre_flight_review_check(
            ctx.logger,
            j,
            ctx.reviewer_agent,
            ctx.config.reviewer_model.as_deref(),
        ) {
            PreflightResult::Ok => {
                // All checks passed, proceed
            }
            PreflightResult::Warning(msg) => {
                ctx.logger.warn(&msg);
                // Continue anyway
            }
            PreflightResult::Error(msg) => {
                ctx.logger.error(&format!("Pre-flight check failed: {msg}"));
                return Err(anyhow::anyhow!(
                    "Review pre-flight validation failed: {msg}"
                ));
            }
        }

        // NOTE: Review baseline is NOT captured here. For the first cycle, we use
        // start_commit (via ReviewBaseline::NotSet fallback). For subsequent cycles,
        // we use the baseline that was updated after the previous fix pass.
        // This ensures the reviewer sees actual changes rather than an empty diff.

        // REVIEW PASS
        update_status("Reviewing code", ctx.config.isolation_mode)?;

        // First, get the review label (for logging and error messages)
        let (review_label, _) = build_review_prompt(
            ctx,
            reviewer_context,
            ctx.review_guidelines,
            if resuming_into_review {
                resume_context
            } else {
                None
            },
        );

        // Use prompt replay if available, otherwise build new review prompt
        let review_prompt_key = format!("review_{}", j);
        let (review_prompt, was_replayed) =
            get_stored_or_generate_prompt(&review_prompt_key, &ctx.prompt_history, || {
                let (_, prompt) = build_review_prompt(
                    ctx,
                    reviewer_context,
                    ctx.review_guidelines,
                    if resuming_into_review {
                        resume_context
                    } else {
                        None
                    },
                );
                prompt
            });

        // Capture the review prompt for checkpoint/resume (only if newly generated)
        if !review_prompt.is_empty() {
            if !was_replayed {
                ctx.capture_prompt(&review_prompt_key, &review_prompt);
            } else {
                ctx.logger.info(&format!(
                    "Using stored prompt from checkpoint for determinism: {}",
                    review_prompt_key
                ));
            }
        }

        // Check if the review prompt is empty (e.g., due to diff retrieval failure)
        // If so, skip the review and fix passes but still check for git changes
        if review_prompt.is_empty() {
            ctx.logger
                .warn(&format!("Skipping review cycle {j} due to: {review_label}"));
            skipped_cycles += 1;

            // Check for external git changes and commit if found
            prev_snap = handle_skipped_cycle(ctx, j, &prev_snap)?;
            continue;
        }

        // Log the specific review prompt variant for debugging (when verbose)
        if ctx.config.verbosity.is_debug() {
            ctx.logger.info(&format!(
                "Review prompt variant: '{}' for agent '{}'",
                review_label, ctx.reviewer_agent
            ));
            ctx.logger.info(&format!(
                "Review prompt length: {} characters",
                review_prompt.len()
            ));
        }

        // Run review pass
        let review_result = run_review_pass(ctx, j, &review_label, &review_prompt)?;

        // Check for early exit (no issues found)
        if review_result.early_exit {
            return Ok(ReviewResult {
                completed_early: true,
            });
        }

        // Run fix pass
        run_fix_pass(
            ctx,
            j,
            reviewer_context,
            if resuming_into_review {
                resume_context
            } else {
                None
            },
        )?;

        // UPDATE REVIEW BASELINE: Move baseline forward after fixes
        // This ensures the next review cycle sees only new changes
        if let Err(e) = update_review_baseline() {
            ctx.logger.warn(&format!(
                "Failed to update review baseline: {e}. Next review may see old changes."
            ));
        }

        // Check for changes and create commit if modified
        prev_snap = handle_post_fix_commit(ctx, j, &prev_snap)?;

        // Save checkpoint after review-fix cycle completes (if enabled)
        // This checkpoint captures the completed cycle so resume won't re-run it
        if ctx.config.features.checkpoint_enabled {
            let next_pass = j + 1;
            let builder = CheckpointBuilder::new()
                .phase(
                    PipelinePhase::Review,
                    ctx.config.developer_iters,
                    ctx.config.developer_iters,
                )
                .reviewer_pass(next_pass, ctx.config.reviewer_reviews)
                .capture_from_context(
                    ctx.config,
                    ctx.registry,
                    ctx.developer_agent,
                    ctx.reviewer_agent,
                    ctx.logger,
                    &ctx.run_context,
                )
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint(&checkpoint);
            }
        }
    }

    // Provide feedback if any review cycles were skipped
    log_skipped_cycles_feedback(ctx, skipped_cycles);

    Ok(ReviewResult {
        completed_early: false,
    })
}

/// Handle a skipped review cycle by checking for external git changes.
fn handle_skipped_cycle(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    prev_snap: &str,
) -> anyhow::Result<String> {
    let start_time = Instant::now();

    let snap = git_snapshot()?;
    if snap != prev_snap {
        ctx.logger
            .success("Repository modified (external changes detected)");
        ctx.stats.changes_detected += 1;

        // Get the primary commit agent
        let commit_agent = get_primary_commit_agent(ctx);
        if let Some(agent) = commit_agent {
            ctx.logger.info(&format!(
                "Creating commit with auto-generated message (agent: {agent})..."
            ));

            // Get the diff for commit message generation
            let diff = match crate::git_helpers::git_diff() {
                Ok(d) => d,
                Err(e) => {
                    ctx.logger
                        .error(&format!("Failed to get diff for commit: {e}"));
                    return Err(anyhow::anyhow!(e));
                }
            };

            // Get git identity from config
            let git_name = ctx.config.git_user_name.as_deref();
            let git_email = ctx.config.git_user_email.as_deref();

            match commit_with_generated_message(&diff, &agent, git_name, git_email, ctx) {
                CommitResultFallback::Success(oid) => {
                    ctx.logger
                        .success(&format!("Commit created successfully: {oid}"));
                    ctx.stats.commits_created += 1;

                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Review",
                        iteration,
                        "commit",
                        StepOutcome::success(Some(oid.to_string()), vec![]),
                    )
                    .with_agent(&agent)
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }
                CommitResultFallback::NoChanges => {
                    ctx.logger.info("No commit created (no meaningful changes)");

                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Review",
                        iteration,
                        "commit",
                        StepOutcome::skipped("No meaningful changes to commit".to_string()),
                    )
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }
                CommitResultFallback::Failed(err) => {
                    ctx.logger.error(&format!("Failed to create commit: {err}"));

                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Review",
                        iteration,
                        "commit",
                        StepOutcome::failure(err.to_string(), false),
                    )
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);

                    return Err(anyhow::anyhow!(err));
                }
            }
        } else {
            ctx.logger.warn("Unable to get commit agent for commit");

            let duration = start_time.elapsed().as_secs();
            let step = ExecutionStep::new(
                "Review",
                iteration,
                "commit",
                StepOutcome::failure("No commit agent available".to_string(), true),
            )
            .with_duration(duration);
            ctx.execution_history.add_step(step);
        }
    }
    Ok(snap)
}

/// Result of running a review pass.
struct ReviewPassResult {
    /// Whether the review found no issues and should exit early.
    early_exit: bool,
}

/// Result of parsing review output.
#[derive(Debug)]
enum ParseResult {
    /// Successfully parsed with issues found
    IssuesFound,
    /// Successfully parsed with explicit "no issues" declaration
    NoIssuesExplicit,
    /// Failed to parse - includes error description for re-prompting
    ParseFailed(String),
}

/// Log prefix-based file search results.
fn log_prefix_search_results(logger: &Logger, parent: &Path, prefix: &str) {
    use crate::files::result_extraction::file_finder::{
        find_log_files_with_prefix, find_subdirs_with_prefix,
    };

    logger.info(&format!("Debug: Parent directory: {}", parent.display()));
    logger.info(&format!("Debug: Log prefix: '{prefix}'"));

    // Check for prefix-based log files (PRIMARY mode)
    let prefix_files_result: std::io::Result<Vec<std::path::PathBuf>> =
        find_log_files_with_prefix(parent, prefix);
    match prefix_files_result {
        Ok(files) if !files.is_empty() => {
            logger.info(&format!(
                "Debug: Found {} prefix-matched file(s)",
                files.len()
            ));
            for file in &files {
                logger.info(&format!("Debug:   - {}", file.display()));
            }
        }
        Ok(_) => {
            logger.info("Debug: No prefix-matched log files found");
        }
        Err(e) => {
            logger.info(&format!("Debug: Error searching for prefix files: {e}"));
        }
    }

    // Check for subdirectory fallback
    let subdirs_result: std::io::Result<Vec<std::path::PathBuf>> =
        find_subdirs_with_prefix(parent, prefix);
    match subdirs_result {
        Ok(subdirs) if !subdirs.is_empty() => {
            logger.info(&format!(
                "Debug: Found {} subdirectory(s) matching prefix",
                subdirs.len()
            ));
            for subdir in &subdirs {
                logger.info(&format!("Debug:   - {}", subdir.display()));
            }
        }
        Ok(_) => {
            logger.info("Debug: No matching subdirectories found");
        }
        Err(e) => {
            logger.info(&format!("Debug: Error searching for subdirs: {e}"));
        }
    }
}

/// Log directory contents and file details.
fn log_directory_details(logger: &Logger, log_dir_path: &Path) {
    // Count log files in the directory
    match std::fs::read_dir(log_dir_path) {
        Ok(entries) => {
            let files: Vec<_> = entries.filter_map(Result::ok).collect();
            let file_count = files.len();
            logger.info(&format!(
                "Debug: Log directory exists with {file_count} file(s)"
            ));
            // List files for diagnosis
            for entry in &files {
                logger.info(&format!("Debug:   - {}", entry.path().display()));
            }
        }
        Err(e) => {
            logger.info(&format!("Debug: Error reading log directory: {e}"));
        }
    }

    // Try to read first log file content for diagnosis
    if let Ok(mut entries) = std::fs::read_dir(log_dir_path) {
        if let Some(Ok(first_entry)) = entries.next() {
            logger.info(&format!(
                "Debug: Reading first file for diagnosis: {}",
                first_entry.path().display()
            ));
            match std::fs::read_to_string(first_entry.path()) {
                Ok(content) => {
                    let preview: String = content.chars().take(300).collect();
                    logger.info(&format!(
                        "Debug: First log file preview (300 chars):\n{preview}"
                    ));
                    let line_count = content.lines().count();
                    logger.info(&format!("Debug: Log file has {line_count} line(s)"));

                    // Check if file contains JSON events
                    let json_count = content
                        .lines()
                        .filter(|line| line.trim().starts_with('{'))
                        .count();
                    logger.info(&format!("Debug: Found {json_count} JSON line(s)"));

                    // Check for result events
                    let result_count = content
                        .lines()
                        .filter(|line| {
                            line.contains(r#""type":"result""#)
                                || line.contains(r#""type": "result""#)
                        })
                        .count();
                    logger.info(&format!("Debug: Found {result_count} result event line(s)"));
                }
                Err(e) => {
                    logger.info(&format!("Debug: Error reading file content: {e}"));
                }
            }
        }
    }
}

/// Log diagnostic information when JSON extraction fails.
///
/// Provides detailed debug logging about log file search strategies,
/// file contents, and why extraction might have failed.
fn log_extraction_diagnostics(logger: &Logger, log_dir: &str) {
    let log_dir_path = Path::new(log_dir);

    // Show the exact log path being searched
    logger.info(&format!("Debug: Log path searched: {log_dir}"));

    // Extract parent and prefix for prefix-mode search info
    let parent = log_dir_path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = log_dir_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if !prefix.is_empty() {
        log_prefix_search_results(logger, parent, prefix);
    }

    // Check if log path exists as directory
    if log_dir_path.exists() {
        if log_dir_path.is_dir() {
            log_directory_details(logger, log_dir_path);
        } else {
            logger.info(&format!(
                "Debug: Path exists but is not a directory: {}",
                log_dir_path.display()
            ));
        }
    } else {
        logger.info(&format!("Debug: Log path does not exist: {log_dir}"));
    }
}

/// Run the review pass for a single cycle.
fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    review_prompt: &str,
) -> anyhow::Result<ReviewPassResult> {
    let issues_path = Path::new(".agent/ISSUES.md");

    // Get the configurable maximum retry count
    let max_retries = ctx.config.review_format_retries;

    // Run initial review
    let mut current_prompt = review_prompt.to_string();
    let mut retry_count = 0;

    loop {
        let attempt_start = Instant::now();
        let log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_{retry_count}");

        let _ = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
                #[cfg(any(test, feature = "test-utils"))]
                agent_executor: None,
            };

            // Output validator: checks if reviewer produced valid JSON output
            // This is critical for GLM agents which may exit with code 1 even when successful
            let validate_output: crate::pipeline::OutputValidator =
                |log_dir_path: &Path, _logger: &crate::logger::Logger| -> std::io::Result<bool> {
                    use crate::files::result_extraction::extract_last_result;
                    match extract_last_result(log_dir_path) {
                        Ok(Some(_)) => Ok(true), // Valid JSON output exists
                        Ok(None) => Ok(false),   // No JSON output found
                        Err(_) => Ok(true), // On error, assume success (let extraction handle validation)
                    }
                };

            let mut fallback_config = FallbackConfig {
                role: AgentRole::Reviewer,
                base_label: &format!(
                    "{review_label} #{j}{}",
                    if retry_count > 0 {
                        format!(" (retry {retry_count})")
                    } else {
                        String::new()
                    }
                ),
                prompt: &current_prompt,
                logfile_prefix: &log_dir,
                runtime: &mut runtime,
                registry: ctx.registry,
                primary_agent: ctx.reviewer_agent,
                output_validator: Some(validate_output),
            };
            run_with_fallback_and_validator(&mut fallback_config)
        };
        ctx.stats.reviewer_runs_completed += 1;

        let attempt_duration = attempt_start.elapsed().as_secs();

        // ORCHESTRATOR-CONTROLLED FILE I/O:
        // Ensure .agent directory exists
        if let Some(parent) = issues_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Extract and validate the review output
        let parse_result = extract_and_validate_review_output(ctx, &log_dir, issues_path)?;

        match parse_result {
            ParseResult::IssuesFound => {
                // POST-FLIGHT VALIDATION: Check review output after agent completes
                handle_postflight_validation(ctx, j);

                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::success(
                        Some("Issues found".to_string()),
                        vec![".agent/ISSUES.md".to_string()],
                    ),
                )
                .with_agent(ctx.reviewer_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                return Ok(ReviewPassResult { early_exit: false });
            }
            ParseResult::NoIssuesExplicit => {
                ctx.logger
                    .success(&format!("No issues found after cycle {j} - stopping early"));
                // Clean up ISSUES.md before early exit in isolation mode
                if ctx.config.isolation_mode {
                    delete_issues_file_for_isolation(ctx.logger)?;
                }

                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::success(Some("No issues found".to_string()), vec![]),
                )
                .with_agent(ctx.reviewer_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                return Ok(ReviewPassResult { early_exit: true });
            }
            ParseResult::ParseFailed(error_description) => {
                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::failure(format!("Parse failed: {error_description}"), true),
                )
                .with_agent(ctx.reviewer_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                retry_count += 1;

                if retry_count > max_retries {
                    ctx.logger.error(&format!(
                        "Failed to get parseable review output after {} retries. Last error: {}",
                        max_retries, error_description
                    ));
                    // Write a marker file indicating the failure - do NOT assume no issues
                    let failure_marker = format!(
                        "# Review Output Parse Failure\n\n\
                        The reviewer agent's output could not be parsed after {} attempts.\n\n\
                        Last parsing error: {}\n\n\
                        This does NOT mean there are no issues - it means the output format was not recognized.\n\n\
                        Please check the logs in .agent/logs/ for the raw reviewer output.\n",
                        max_retries, error_description
                    );
                    fs::write(issues_path, failure_marker)?;
                    // Continue with fix pass anyway - the fix agent will see the failure message
                    return Ok(ReviewPassResult { early_exit: false });
                }

                ctx.logger.warn(&format!(
                    "Review output format not recognized (attempt {}/{}): {}",
                    retry_count,
                    max_retries + 1,
                    error_description
                ));
                ctx.logger
                    .info("Re-prompting agent with format correction...");

                // Build a format correction prompt
                current_prompt = build_format_correction_prompt(&error_description);
            }
        }
    }
}

/// Extract review output and validate its format.
///
/// Returns a `ParseResult` indicating whether the output was successfully parsed,
/// explicitly declared no issues, or failed to parse (with an error description).
fn extract_and_validate_review_output(
    ctx: &mut PhaseContext<'_>,
    log_dir: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    let extraction = extract_issues(Path::new(log_dir))?;

    // First, try to get content from extraction or legacy file
    let content = if let Some(content) = extraction.raw_content {
        content
    } else {
        // JSON extraction failed - check for legacy agent-written file
        if ctx.config.verbosity.is_debug() {
            ctx.logger
                .info("No JSON result event found in reviewer logs");
            log_extraction_diagnostics(ctx.logger, log_dir);
        }

        // Check if agent wrote the file directly (legacy fallback)
        if issues_path.exists() {
            if let Ok(content) = fs::read_to_string(issues_path) {
                if !content.trim().is_empty() {
                    ctx.logger
                        .info("Using agent-written ISSUES.md (legacy mode)");
                    content
                } else {
                    return Ok(ParseResult::ParseFailed(
                        "Agent wrote an empty ISSUES.md file. Expected either issues in checkbox format \
                        (e.g., '- [ ] Critical: description') or explicit 'No issues found.' declaration."
                            .to_string(),
                    ));
                }
            } else {
                return Ok(ParseResult::ParseFailed(
                    "No review output captured. The agent may have failed to produce any output."
                        .to_string(),
                ));
            }
        } else {
            return Ok(ParseResult::ParseFailed(
                "No review output captured. The agent did not write ISSUES.md and no JSON result was found in logs."
                    .to_string(),
            ));
        }
    };

    // Write the content to ISSUES.md
    fs::write(issues_path, &content)?;

    // Parse the content to determine the result
    let metrics = ReviewMetrics::from_issues_content(&content);

    if metrics.total_issues > 0 {
        ctx.logger.success(&format!(
            "Issues extracted: {} total ({} critical, {} high, {} medium, {} low)",
            metrics.total_issues,
            metrics.critical_issues,
            metrics.high_issues,
            metrics.medium_issues,
            metrics.low_issues
        ));
        return Ok(ParseResult::IssuesFound);
    }

    if metrics.no_issues_declared {
        return Ok(ParseResult::NoIssuesExplicit);
    }

    // Content exists but we couldn't parse any issues AND no explicit "no issues" declaration
    // This is an ambiguous state - we need to re-prompt
    let preview: String = content.lines().take(10).collect::<Vec<_>>().join("\n");
    let content_len = content.len();

    Ok(ParseResult::ParseFailed(format!(
        "Review output ({} bytes) could not be parsed. No issues found in expected format \
        (checkbox format like '- [ ] Critical: description' or header format like '#### [ ] Critical: description'), \
        and no explicit 'No issues found.' declaration. Content preview:\n{}\n\n\
        If there are no issues, the output must explicitly state 'No issues found.' on its own line.",
        content_len,
        preview
    )))
}

/// Build a prompt to ask the agent to reformat its output.
fn build_format_correction_prompt(error_description: &str) -> String {
    format!(
        r#"Your previous review output could not be parsed. Please reformat your response.

## Parsing Error

{error_description}

## Required Output Format

You MUST output your review in ONE of these two formats:

### Format A: If you found issues

List each issue using checkbox format with severity:

```
- [ ] Critical: `file.rs:line` - Description of the critical issue
- [ ] High: `file.rs:line` - Description of the high priority issue  
- [ ] Medium: `file.rs:line` - Description of the medium priority issue
- [ ] Low: `file.rs:line` - Description of the low priority issue
```

OR using header format:

```
#### [ ] Critical: `file.rs:line` - Description of the critical issue
#### [ ] High: `file.rs:line` - Description of the high priority issue
```

### Format B: If you found NO issues

You MUST explicitly write this exact line:

```
No issues found.
```

## Important

- Do NOT write prose about findings without the checkbox/header format
- Do NOT assume the reader knows there are no issues - be EXPLICIT
- Every issue MUST have a severity level (Critical, High, Medium, or Low)
- If you found issues in your previous review, please restate them in the correct format

Please provide your review output now in the correct format."#
    )
}

/// Handle post-flight validation after a review pass.
fn handle_postflight_validation(ctx: &PhaseContext<'_>, j: u32) {
    let postflight_result = post_flight_review_check(ctx.logger, j);
    match postflight_result {
        PostflightResult::Valid => {
            // ISSUES.md found and valid, continue
        }
        PostflightResult::Missing(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. Proceeding with fix pass anyway."
            ));
            // If using a problematic agent, suggest alternatives
            if should_use_universal_prompt(
                ctx.reviewer_agent,
                ctx.config.reviewer_model.as_deref(),
                ctx.config.features.force_universal_prompt,
            ) {
                ctx.logger.info(&format!(
                    "{}Tip:{} Review with this agent may be unreliable. Consider:",
                    ctx.colors.bold(),
                    ctx.colors.reset()
                ));
                ctx.logger
                    .info("  1. Use Claude/Codex as reviewer: ralph --reviewer-agent codex");
                ctx.logger
                    .info("  2. Try generic parser: ralph --reviewer-json-parser generic");
                ctx.logger
                    .info("  3. Skip review: RALPH_REVIEWER_REVIEWS=0 ralph");
            }
        }
        PostflightResult::Malformed(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. The fix pass may not work correctly."
            ));
            ctx.logger.info(&format!(
                "{}Tip:{} Try with generic parser: {}RALPH_REVIEWER_JSON_PARSER=generic ralph{}",
                ctx.colors.bold(),
                ctx.colors.reset(),
                ctx.colors.bold(),
                ctx.colors.reset()
            ));
        }
    }
}

/// Run the fix pass for a single cycle.
fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    reviewer_context: ContextLevel,
    resume_context: Option<&ResumeContext>,
) -> anyhow::Result<()> {
    let fix_start_time = Instant::now();

    update_status("Applying fixes", ctx.config.isolation_mode)?;

    // Read PROMPT.md, PLAN.md, and ISSUES.md for context
    let prompt_content = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_content = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();
    let issues_content = fs::read_to_string(".agent/ISSUES.md").unwrap_or_default();

    let mut prompt_config = PromptConfig::new().with_prompt_plan_and_issues(
        prompt_content.clone(),
        plan_content,
        issues_content,
    );

    // Set resume context if this is the first pass of a resumed session
    if let Some(resume_ctx) = resume_context {
        prompt_config = prompt_config.with_resume_context(resume_ctx.clone());
    }

    // Use prompt replay if available, otherwise generate new fix prompt
    let prompt_key = format!("fix_{}", j);
    let (fix_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            prompt_for_agent(
                Role::Reviewer,
                Action::Fix,
                reviewer_context,
                ctx.template_context,
                prompt_config.clone(),
            )
        });

    // Capture the fix prompt for checkpoint/resume (only if newly generated)
    if !was_replayed {
        ctx.capture_prompt(&prompt_key, &fix_prompt);
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    // Log the fix prompt details for debugging (when verbose)
    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Fix prompt length: {} characters",
            fix_prompt.len()
        ));
        ctx.logger.info(&format!(
            "Fix prompt contains constraints: {}",
            fix_prompt.contains("MUST NOT") && fix_prompt.contains("CRITICAL CONSTRAINTS")
        ));
    }

    let _ = {
        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            #[cfg(any(test, feature = "test-utils"))]
            agent_executor: None,
        };

        // Output validator: checks if fixer produced valid JSON output
        // This is critical for GLM agents which may exit with code 1 even when successful
        let validate_output: crate::pipeline::OutputValidator =
            |log_dir_path: &Path, _logger: &crate::logger::Logger| -> std::io::Result<bool> {
                use crate::files::result_extraction::extract_last_result;
                match extract_last_result(log_dir_path) {
                    Ok(Some(_)) => Ok(true), // Valid JSON output exists
                    Ok(None) => Ok(false),   // No JSON output found
                    Err(_) => Ok(true), // On error, assume success (let later validation handle it)
                }
            };

        let log_dir = format!(".agent/logs/reviewer_fix_{j}");
        let mut fallback_config = FallbackConfig {
            role: AgentRole::Reviewer,
            base_label: &format!("fix #{j}"),
            prompt: &fix_prompt,
            logfile_prefix: &log_dir,
            runtime: &mut runtime,
            registry: ctx.registry,
            primary_agent: ctx.reviewer_agent,
            output_validator: Some(validate_output),
        };
        run_with_fallback_and_validator(&mut fallback_config)
    };
    ctx.stats.reviewer_runs_completed += 1;

    let duration = fix_start_time.elapsed().as_secs();
    let step = ExecutionStep::new("Review", j, "fix", StepOutcome::success(None, vec![]))
        .with_agent(ctx.reviewer_agent)
        .with_duration(duration);
    ctx.execution_history.add_step(step);

    // Clean up ISSUES.md after each fix cycle in isolation mode
    if ctx.config.isolation_mode {
        delete_issues_file_for_isolation(ctx.logger)?;
    }

    // Periodic restoration check - ensure PROMPT.md still exists
    ensure_prompt_integrity(ctx.logger, "review", j);

    Ok(())
}

/// Handle post-fix commit creation.
fn handle_post_fix_commit(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    prev_snap: &str,
) -> anyhow::Result<String> {
    let start_time = Instant::now();

    let snap = git_snapshot()?;
    if snap != prev_snap {
        ctx.logger.success("Repository modified during fix pass");
        ctx.stats.changes_detected += 1;

        // Get the primary commit agent
        let commit_agent = get_primary_commit_agent(ctx);
        if let Some(agent) = commit_agent {
            ctx.logger.info(&format!(
                "Creating commit with auto-generated message (agent: {agent})..."
            ));

            // Get the diff for commit message generation
            let diff = match crate::git_helpers::git_diff() {
                Ok(d) => d,
                Err(e) => {
                    ctx.logger
                        .error(&format!("Failed to get diff for commit: {e}"));
                    return Err(anyhow::anyhow!(e));
                }
            };

            // Get git identity from config
            let git_name = ctx.config.git_user_name.as_deref();
            let git_email = ctx.config.git_user_email.as_deref();

            match commit_with_generated_message(&diff, &agent, git_name, git_email, ctx) {
                CommitResultFallback::Success(oid) => {
                    ctx.logger
                        .success(&format!("Commit created successfully: {oid}"));
                    ctx.stats.commits_created += 1;

                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Review",
                        iteration,
                        "commit",
                        StepOutcome::success(Some(oid.to_string()), vec![]),
                    )
                    .with_agent(&agent)
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }
                CommitResultFallback::NoChanges => {
                    ctx.logger.info("No commit created (no meaningful changes)");

                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Review",
                        iteration,
                        "commit",
                        StepOutcome::skipped("No meaningful changes to commit".to_string()),
                    )
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);
                }
                CommitResultFallback::Failed(err) => {
                    ctx.logger.error(&format!(
                        "Failed to create commit (git operation failed): {err}"
                    ));

                    let duration = start_time.elapsed().as_secs();
                    let step = ExecutionStep::new(
                        "Review",
                        iteration,
                        "commit",
                        StepOutcome::failure(err.to_string(), false),
                    )
                    .with_duration(duration);
                    ctx.execution_history.add_step(step);

                    return Err(anyhow::anyhow!(err));
                }
            }
        } else {
            ctx.logger
                .warn("Unable to get commit agent chain for commit");

            let duration = start_time.elapsed().as_secs();
            let step = ExecutionStep::new(
                "Review",
                iteration,
                "commit",
                StepOutcome::failure("No commit agent available".to_string(), true),
            )
            .with_duration(duration);
            ctx.execution_history.add_step(step);
        }
    }
    Ok(snap)
}

/// Log feedback about skipped review cycles.
fn log_skipped_cycles_feedback(ctx: &PhaseContext<'_>, skipped_cycles: u32) {
    if skipped_cycles > 0 {
        let total_cycles = ctx.config.reviewer_reviews;
        ctx.logger.warn(&format!(
            "{skipped_cycles} of {total_cycles} review cycle(s) were skipped due to diff retrieval failures."
        ));
        ctx.logger.info(
            "This may indicate a git repository issue or that no changes have been made yet.",
        );
        if skipped_cycles == total_cycles {
            ctx.logger.warn(
                "No review cycles were completed. Consider checking your git repository state.",
            );
        }
    }
}
