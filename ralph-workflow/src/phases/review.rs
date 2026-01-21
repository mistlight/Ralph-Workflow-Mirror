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
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    extract_fix_result_xml, extract_issues_xml, format_xml_for_display, validate_fix_result_xml,
    validate_issues_xml, IssuesElements,
};
use crate::files::{clean_context_for_reviewer, delete_issues_file_for_isolation, update_status};
use crate::git_helpers::{
    get_baseline_summary, git_snapshot, update_review_baseline, CommitResultFallback,
};
use crate::logger::{print_progress, Logger};
use crate::phases::commit::commit_with_generated_message;
use crate::phases::get_primary_commit_agent;
use crate::pipeline::{run_xsd_retry_with_session, PipelineRuntime, XsdRetryConfig};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_fix_xml_with_context, prompt_fix_xsd_retry_with_context,
    prompt_review_xml_with_context, prompt_review_xsd_retry_with_context, ContextLevel,
};
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
    IssuesFound { issues: Vec<String> },
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
///
/// This function implements a nested loop structure similar to fix:
/// - **Outer loop (continuation)**: Not used for review (single pass)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback (max 10)
fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    _review_prompt: &str, // Unused - we build XML prompt internally
) -> anyhow::Result<ReviewPassResult> {
    let issues_path = Path::new(".agent/ISSUES.md");
    let max_xsd_retries = 10;

    // Read PROMPT.md, PLAN.md for context
    let prompt_content = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_content = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();

    // Get the diff for review context
    let changes_content = match crate::git_helpers::git_diff() {
        Ok(diff) => diff,
        Err(e) => {
            ctx.logger
                .warn(&format!("Failed to get diff for review: {e}"));
            String::new()
        }
    };

    // Session info for potential session continuation on XSD retries
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    // Inner loop: XSD validation retry with error feedback
    for retry_num in 0..max_xsd_retries {
        let is_retry = retry_num > 0;
        let log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_{retry_num}");

        // For initial attempt, use XML prompt
        // For retries, use XSD retry prompt with error feedback
        let review_prompt_xml = if !is_retry {
            // First attempt - use initial XML prompt
            let prompt_key = format!("review_{}", j);
            let (prompt, was_replayed) =
                get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                    prompt_review_xml_with_context(
                        ctx.template_context,
                        &prompt_content,
                        &plan_content,
                        &changes_content,
                    )
                });

            if !was_replayed {
                ctx.capture_prompt(&prompt_key, &prompt);
            } else {
                ctx.logger.info(&format!(
                    "Using stored prompt from checkpoint for determinism: {}",
                    prompt_key
                ));
            }

            prompt
        } else {
            // XSD retry - use retry prompt with error feedback
            ctx.logger.info(&format!(
                "  In-session retry {}/{} for XSD validation",
                retry_num,
                max_xsd_retries - 1
            ));

            let last_output = read_last_review_output(Path::new(&log_dir));

            // Get XSD error from previous iteration
            let xsd_error = get_last_xsd_error(ctx, Path::new(&log_dir));

            if let Some(ref error) = xsd_error {
                ctx.logger.info(&format!("  XSD error: {}", error));
            }

            prompt_review_xsd_retry_with_context(
                ctx.template_context,
                &prompt_content,
                &plan_content,
                &changes_content,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
            )
        };

        // Log the review prompt details for debugging (when verbose)
        if ctx.config.verbosity.is_debug() && !is_retry {
            ctx.logger.info(&format!(
                "Review prompt length: {} characters",
                review_prompt_xml.len()
            ));
        }

        let attempt_start = Instant::now();

        // Run the agent with session continuation for XSD retries
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
            let validate_output: crate::pipeline::OutputValidator =
                |log_dir_path: &Path, _logger: &crate::logger::Logger| -> std::io::Result<bool> {
                    use crate::files::result_extraction::extract_last_result;
                    match extract_last_result(log_dir_path) {
                        Ok(Some(_)) => Ok(true), // Valid JSON output exists
                        Ok(None) => Ok(false),   // No JSON output found
                        Err(_) => Ok(true), // On error, assume success (let extraction handle validation)
                    }
                };

            let base_label = format!(
                "{review_label} #{j}{}",
                if is_retry {
                    format!(" (retry {retry_num})")
                } else {
                    String::new()
                }
            );

            let mut xsd_retry_config = XsdRetryConfig {
                role: AgentRole::Reviewer,
                base_label: &base_label,
                prompt: &review_prompt_xml,
                logfile_prefix: &log_dir,
                runtime: &mut runtime,
                registry: ctx.registry,
                primary_agent: ctx.reviewer_agent,
                session_info: session_info.as_ref(),
                retry_num,
                output_validator: Some(validate_output),
            };
            run_xsd_retry_with_session(&mut xsd_retry_config)
        };
        ctx.stats.reviewer_runs_completed += 1;

        // Extract session info for potential retry (only if we don't have it yet)
        let log_dir_path = Path::new(&log_dir);
        if session_info.is_none() {
            if let Some(agent_config) = ctx.registry.resolve_config(ctx.reviewer_agent) {
                session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                    log_dir_path,
                    agent_config.json_parser,
                );
            }
        }

        let attempt_duration = attempt_start.elapsed().as_secs();

        // Extract and validate the review output using XML extraction
        let parse_result = extract_and_validate_review_output_xml(ctx, &log_dir, issues_path)?;

        match parse_result {
            ParseResult::IssuesFound { issues } => {
                // POST-FLIGHT VALIDATION: Check review output after agent completes
                handle_postflight_validation(ctx, j);

                ctx.logger
                    .success(&format!("Issues extracted: {} total", issues.len()));

                // Display formatted XML for user
                if let Ok(xml_content) = fs::read_to_string(issues_path) {
                    if let Some(extracted_xml) = extract_issues_xml(&xml_content) {
                        ctx.logger.info(&format!(
                            "Review output:\n{}",
                            format_xml_for_display(&extracted_xml)
                        ));
                    }
                }

                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::success(
                        Some(format!("{} issues found", issues.len())),
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
                    StepOutcome::failure(
                        format!("XSD validation failed: {error_description}"),
                        true,
                    ),
                )
                .with_agent(ctx.reviewer_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                // Store XSD error for next retry
                store_xsd_error_for_retry(Path::new(&log_dir), &error_description);

                // Last retry failed - write marker and continue
                if retry_num >= max_xsd_retries - 1 {
                    ctx.logger.error(&format!(
                        "Failed to get valid XML review output after {} XSD retries. Last error: {}",
                        max_xsd_retries, error_description
                    ));
                    // Write a marker file indicating the failure
                    let failure_marker = format!(
                        "# Review Output XSD Validation Failure\n\n\
                        The reviewer agent's output failed XSD validation after {} attempts.\n\n\
                        Last validation error: {}\n\n\
                        This does NOT mean there are no issues - it means the XML format was invalid.\n\n\
                        Please check the logs in .agent/logs/ for the raw reviewer output.\n",
                        max_xsd_retries, error_description
                    );
                    fs::write(issues_path, failure_marker)?;
                    // Continue with fix pass anyway - the fix agent will see the failure message
                    return Ok(ReviewPassResult { early_exit: false });
                }

                ctx.logger.warn(&format!(
                    "Review XSD validation failed (attempt {}/{}): {}",
                    retry_num + 1,
                    max_xsd_retries,
                    error_description
                ));
                // Continue to next retry with XSD error feedback
            }
        }
    }

    // Should not reach here, but handle the case
    Ok(ReviewPassResult { early_exit: false })
}

/// Extract review output using XML extraction and validate with XSD.
///
/// Returns a `ParseResult` indicating whether the output was successfully parsed,
/// explicitly declared no issues, or failed to parse (with an error description).
fn extract_and_validate_review_output_xml(
    ctx: &mut PhaseContext<'_>,
    log_dir: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    let extraction = extract_issues(Path::new(log_dir))?;

    // First, try to get content from extraction or legacy file
    let raw_content = if let Some(content) = extraction.raw_content {
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
                        "Agent wrote an empty ISSUES.md file. Expected XML output with <ralph-issues> tags."
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

    // Extract XML from the raw content
    let xml_content = match extract_issues_xml(&raw_content) {
        Some(xml) => xml,
        None => {
            // No XML found - assume entire output is XML and validate to get specific error
            ctx.logger
                .warn("No XML tags found in output, assuming entire output is XML for validation");

            // Try to validate the raw content as XML to get specific error message
            match validate_issues_xml(&raw_content) {
                Ok(_) => {
                    // Unexpectedly valid - might be a bug in extraction, but accept it
                    ctx.logger.info(
                        "Raw content validated as XML despite no tags found (extraction bug?)",
                    );
                    raw_content
                }
                Err(e) => {
                    // Return the specific XSD error
                    return Ok(ParseResult::ParseFailed(format!(
                        "XSD validation failed: {}",
                        e.format_for_ai_retry()
                    )));
                }
            }
        }
    };

    // Validate the extracted XML against XSD
    let validated: Result<IssuesElements, XsdValidationError> = validate_issues_xml(&xml_content);

    match validated {
        Ok(elements) => {
            // Write the validated XML to ISSUES.md
            fs::write(issues_path, &xml_content)?;

            if elements.no_issues_found.is_some() {
                return Ok(ParseResult::NoIssuesExplicit);
            }

            if !elements.issues.is_empty() {
                return Ok(ParseResult::IssuesFound {
                    issues: elements.issues,
                });
            }

            Ok(ParseResult::ParseFailed(
                "XML validated but contains no issues or no-issues-found element.".to_string(),
            ))
        }
        Err(xsd_error) => {
            // Return the specific XSD error for retry
            Ok(ParseResult::ParseFailed(xsd_error.format_for_ai_retry()))
        }
    }
}

/// Read the last review output from logs.
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/reviewer_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/reviewer_1_ccs-glm_0.log`
fn read_last_review_output(log_prefix: &Path) -> String {
    read_last_output_from_prefix(log_prefix)
}

/// Get the last XSD error from the log directory for retry feedback.
fn get_last_xsd_error(_ctx: &PhaseContext<'_>, log_dir: &Path) -> Option<String> {
    let error_file = log_dir.join("xsd_error.txt");
    if let Ok(content) = fs::read_to_string(&error_file) {
        if !content.trim().is_empty() {
            return Some(content);
        }
    }
    None
}

/// Store XSD error for the next retry attempt.
fn store_xsd_error_for_retry(log_dir: &Path, error: &str) {
    let error_file = log_dir.join("xsd_error.txt");
    let _ = fs::write(&error_file, error);
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

/// Read the last fix output from logs.
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/fix_1_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/fix_1_1_ccs-glm_0.log`
fn read_last_fix_output(log_prefix: &Path) -> String {
    read_last_output_from_prefix(log_prefix)
}

/// Read the most recent log file matching a prefix pattern.
///
/// This is a shared helper for reading log output. Truncation of large prompts
/// is handled centrally in `build_agent_command` to prevent E2BIG errors.
fn read_last_output_from_prefix(log_prefix: &Path) -> String {
    let parent = log_prefix.parent().unwrap_or(Path::new("."));
    let prefix_str = log_prefix
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    // Find all log files matching the prefix pattern and get the most recently modified one
    let mut best_file: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                // Match files that start with our prefix and end with .log
                if filename.starts_with(prefix_str)
                    && filename.len() > prefix_str.len()
                    && filename.ends_with(".log")
                {
                    // Get modification time for this file
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            match &best_file {
                                None => best_file = Some((path.clone(), modified)),
                                Some((_, best_time)) if modified > *best_time => {
                                    best_file = Some((path.clone(), modified));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    // Read the most recently modified matching log file
    if let Some((path, _)) = best_file {
        if let Ok(content) = fs::read_to_string(&path) {
            return content;
        }
    }

    String::new()
}

/// Format XSD error for display (for fix result).
fn format_xsd_error_for_fix(error: &XsdValidationError) -> String {
    format!(
        "{} - expected: {}, found: {}",
        error.element_path, error.expected, error.found
    )
}

/// Run the fix pass for a single cycle.
///
/// This function implements a nested loop structure similar to development:
/// - **Outer loop (continuation)**: Continue while status != "all_issues_addressed" (max 100)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback (max 10)
fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    _reviewer_context: ContextLevel,
    _resume_context: Option<&ResumeContext>,
) -> anyhow::Result<()> {
    let fix_start_time = Instant::now();

    update_status("Applying fixes", ctx.config.isolation_mode)?;

    // Read PROMPT.md, PLAN.md, and ISSUES.md for context
    let prompt_content = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_content = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();
    let issues_content = fs::read_to_string(".agent/ISSUES.md").unwrap_or_default();

    let log_dir = format!(".agent/logs/reviewer_fix_{j}");

    let max_xsd_retries = 10;
    let max_continuations = 100; // Safety limit to prevent infinite loops
    let mut _had_any_error = false; // Tracked for potential future use

    // Session info for potential session continuation on XSD retries
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    // Outer loop: Continue until agent returns status="all_issues_addressed" or "no_issues_found"
    'continuation: for continuation_num in 0..max_continuations {
        let is_continuation = continuation_num > 0;
        if is_continuation {
            ctx.logger.info(&format!(
                "Fix continuation {} of {} (status was not complete)",
                continuation_num, max_continuations
            ));
        }

        let mut xsd_error: Option<String> = None;

        // Inner loop: XSD validation retry with error feedback
        for retry_num in 0..max_xsd_retries {
            let is_retry = retry_num > 0;
            let total_attempts = continuation_num * max_xsd_retries + retry_num + 1;

            // For initial attempt, use XML prompt
            // For retries, use XSD retry prompt with error feedback
            let fix_prompt = if !is_retry && !is_continuation {
                // First attempt ever - use initial XML prompt
                let prompt_key = format!("fix_{}", j);
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_fix_xml_with_context(
                            ctx.template_context,
                            &prompt_content,
                            &plan_content,
                            &issues_content,
                        )
                    });

                if !was_replayed {
                    ctx.capture_prompt(&prompt_key, &prompt);
                } else {
                    ctx.logger.info(&format!(
                        "Using stored prompt from checkpoint for determinism: {}",
                        prompt_key
                    ));
                }

                prompt
            } else if !is_continuation {
                // XSD retry only (no continuation yet)
                ctx.logger.info(&format!(
                    "  In-session retry {}/{} for XSD validation (total attempt: {})",
                    retry_num,
                    max_xsd_retries - 1,
                    total_attempts
                ));
                if let Some(ref error) = xsd_error {
                    ctx.logger.info(&format!("  XSD error: {}", error));
                }

                let last_output = read_last_fix_output(Path::new(&log_dir));

                prompt_fix_xsd_retry_with_context(
                    ctx.template_context,
                    &issues_content,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                )
            } else if !is_retry {
                // Continuation only (first XSD attempt after continuation)
                ctx.logger.info(&format!(
                    "  Continuation attempt {} (XSD validation attempt {}/{})",
                    total_attempts, 1, max_xsd_retries
                ));

                prompt_fix_xml_with_context(
                    ctx.template_context,
                    &prompt_content,
                    &plan_content,
                    &issues_content,
                )
            } else {
                // Both continuation and XSD retry
                ctx.logger.info(&format!(
                    "  Continuation retry {}/{} for XSD validation (total attempt: {})",
                    retry_num,
                    max_xsd_retries - 1,
                    total_attempts
                ));
                if let Some(ref error) = xsd_error {
                    ctx.logger.info(&format!("  XSD error: {}", error));
                }

                let last_output = read_last_fix_output(Path::new(&log_dir));

                prompt_fix_xsd_retry_with_context(
                    ctx.template_context,
                    &issues_content,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                )
            };

            // Log the fix prompt details for debugging (when verbose)
            if ctx.config.verbosity.is_debug() && !is_continuation && !is_retry {
                ctx.logger.info(&format!(
                    "Fix prompt length: {} characters",
                    fix_prompt.len()
                ));
            }

            // Run the agent with session continuation for XSD retries
            let exit_code = {
                let mut runtime = PipelineRuntime {
                    timer: ctx.timer,
                    logger: ctx.logger,
                    colors: ctx.colors,
                    config: ctx.config,
                    #[cfg(any(test, feature = "test-utils"))]
                    agent_executor: None,
                };

                // Output validator: checks if fixer produced valid JSON output
                let validate_output: crate::pipeline::OutputValidator =
                    |log_dir_path: &Path,
                     _logger: &crate::logger::Logger|
                     -> std::io::Result<bool> {
                        use crate::files::result_extraction::extract_last_result;
                        match extract_last_result(log_dir_path) {
                            Ok(Some(_)) => Ok(true),
                            Ok(None) => Ok(false),
                            Err(_) => Ok(true),
                        }
                    };

                let base_label = format!(
                    "fix #{}{}",
                    j,
                    if is_continuation {
                        format!(" (continuation {})", continuation_num)
                    } else {
                        String::new()
                    }
                );

                let mut xsd_retry_config = XsdRetryConfig {
                    role: AgentRole::Reviewer,
                    base_label: &base_label,
                    prompt: &fix_prompt,
                    logfile_prefix: &log_dir,
                    runtime: &mut runtime,
                    registry: ctx.registry,
                    primary_agent: ctx.reviewer_agent,
                    session_info: session_info.as_ref(),
                    retry_num,
                    output_validator: Some(validate_output),
                };
                run_xsd_retry_with_session(&mut xsd_retry_config)
            };

            ctx.stats.reviewer_runs_completed += 1;

            // Extract session info for potential retry (only if we don't have it yet)
            let log_dir_path = Path::new(&log_dir);
            if session_info.is_none() {
                if let Some(agent_config) = ctx.registry.resolve_config(ctx.reviewer_agent) {
                    session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                        log_dir_path,
                        agent_config.json_parser,
                    );
                }
            }

            // Track if any agent run had an error
            if exit_code.is_err() || exit_code.ok() != Some(0) {
                _had_any_error = true;
            }
            let fix_content = read_last_fix_output(log_dir_path);

            // Try to extract XML - if extraction fails, assume entire output is XML
            // and validate it to get specific XSD errors for retry
            let xml_to_validate = if let Some(xml_content) = extract_fix_result_xml(&fix_content) {
                xml_content
            } else {
                // No XML tags found - assume the entire content is XML for validation
                // This allows us to get specific XSD errors to send back to the agent
                fix_content.clone()
            };

            // Try to validate against XSD
            match validate_fix_result_xml(&xml_to_validate) {
                Ok(result_elements) => {
                    // XSD validation passed - format and log the result
                    let formatted_xml = format_xml_for_display(&xml_to_validate);

                    if is_retry {
                        ctx.logger
                            .success(&format!("Fix validated after {} retries", retry_num));
                    } else {
                        ctx.logger
                            .success("Fix status extracted and validated (XML)");
                    }

                    // Display the formatted status
                    ctx.logger.info(&format!("\n{}", formatted_xml));

                    // Check the status to determine if we should continue
                    if result_elements.is_complete() || result_elements.is_no_issues() {
                        // Status is "all_issues_addressed" or "no_issues_found" - we're done
                        let duration = fix_start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            j,
                            "fix",
                            StepOutcome::success(result_elements.summary, vec![]),
                        )
                        .with_agent(ctx.reviewer_agent)
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);

                        return Ok(());
                    } else if result_elements.has_remaining_issues() {
                        // Status is "issues_remain" - continue the outer loop
                        ctx.logger
                            .info("Status is 'issues_remain' - continuing with same fix pass");
                        continue 'continuation;
                    }
                }
                Err(xsd_err) => {
                    // XSD validation failed - check if we can retry
                    let error_msg = format_xsd_error_for_fix(&xsd_err);
                    ctx.logger
                        .warn(&format!("  XSD validation failed: {}", error_msg));

                    if retry_num < max_xsd_retries - 1 {
                        // Store error for next retry attempt
                        xsd_error = Some(error_msg);
                        // Continue to next XSD retry iteration
                        continue;
                    } else {
                        ctx.logger
                            .warn("  No more in-session XSD retries remaining");
                        // Fall through to return what we have
                        break 'continuation;
                    }
                }
            }
        }

        // If we've exhausted XSD retries, break the continuation loop
        ctx.logger
            .warn("XSD retry loop exhausted - stopping continuation");
        break;
    }

    // If we get here, we exhausted the continuation limit
    let duration = fix_start_time.elapsed().as_secs();
    let step = ExecutionStep::new(
        "Review",
        j,
        "fix",
        StepOutcome::failure(
            format!(
                "Continuation stopped after {} attempts",
                max_continuations * max_xsd_retries
            ),
            true,
        ),
    )
    .with_agent(ctx.reviewer_agent)
    .with_duration(duration);
    ctx.execution_history.add_step(step);

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
