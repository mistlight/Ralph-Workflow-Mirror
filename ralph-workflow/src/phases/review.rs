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
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
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
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_with_fallback_and_validator, FallbackConfig, PipelineRuntime};
use crate::prompts::{
    prompt_fix_xml_with_context, prompt_fix_xsd_retry_with_context,
    prompt_review_xsd_retry_with_context, ContextLevel,
};
use crate::review_metrics::ReviewMetrics;

mod prompt;
pub use prompt::{build_review_prompt, should_use_universal_prompt};

mod validation;
pub use validation::{
    post_flight_review_check, pre_flight_review_check, PostflightResult, PreflightResult,
};

use super::context::PhaseContext;
use std::fs;
use std::path::Path;

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
///
/// # Returns
///
/// Returns `Ok(ReviewResult)` on success, or an error if a critical failure occurs.
pub fn run_review_phase(
    ctx: &mut PhaseContext<'_>,
    start_pass: u32,
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
        // Save checkpoint at start of each iteration
        if ctx.config.features.checkpoint_enabled {
            let _ = save_checkpoint(&PipelineCheckpoint::new(
                PipelinePhase::Review,
                ctx.config.developer_iters,
                ctx.config.developer_iters,
                j,
                ctx.config.reviewer_reviews,
                ctx.developer_agent,
                ctx.reviewer_agent,
            ));
        }

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
        let (review_label, review_prompt) =
            build_review_prompt(ctx, reviewer_context, ctx.review_guidelines);

        // Check if the review prompt is empty (e.g., due to diff retrieval failure)
        // If so, skip the review and fix passes but still check for git changes
        if review_prompt.is_empty() {
            ctx.logger
                .warn(&format!("Skipping review cycle {j} due to: {review_label}"));
            skipped_cycles += 1;

            // Check for external git changes and commit if found
            prev_snap = handle_skipped_cycle(ctx, &prev_snap)?;
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
        let fix_result = run_fix_pass(ctx, j, reviewer_context)?;

        // Check the fix result to determine if we should continue
        match fix_result {
            FixPassResult::NoIssuesFound => {
                ctx.logger
                    .success(&format!("No issues found after cycle {j} - stopping early"));
                return Ok(ReviewResult {
                    completed_early: true,
                });
            }
            FixPassResult::AllIssuesAddressed => {
                ctx.logger.success(&format!(
                    "All issues addressed after cycle {j} - stopping early"
                ));
                return Ok(ReviewResult {
                    completed_early: true,
                });
            }
            FixPassResult::IssuesRemain => {
                // Continue to next review cycle
                ctx.logger.info(&format!(
                    "Issues remain after cycle {j} - continuing review"
                ));
            }
        }

        // UPDATE REVIEW BASELINE: Move baseline forward after fixes
        // This ensures the next review cycle sees only new changes
        if let Err(e) = update_review_baseline() {
            ctx.logger.warn(&format!(
                "Failed to update review baseline: {e}. Next review may see old changes."
            ));
        }

        // Check for changes and create commit if modified
        prev_snap = handle_post_fix_commit(ctx, &prev_snap)?;
    }

    // Provide feedback if any review cycles were skipped
    log_skipped_cycles_feedback(ctx, skipped_cycles);

    Ok(ReviewResult {
        completed_early: false,
    })
}

/// Handle a skipped review cycle by checking for external git changes.
fn handle_skipped_cycle(ctx: &mut PhaseContext<'_>, prev_snap: &str) -> anyhow::Result<String> {
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
                }
                CommitResultFallback::NoChanges => {
                    ctx.logger.info("No commit created (no meaningful changes)");
                }
                CommitResultFallback::Failed(err) => {
                    ctx.logger.error(&format!("Failed to create commit: {err}"));
                    return Err(anyhow::anyhow!(err));
                }
            }
        } else {
            ctx.logger.warn("Unable to get commit agent for commit");
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

/// Run the review pass for a single cycle with XSD retry loop.
///
/// This function implements the XSD retry loop pattern similar to the planning phase,
/// ensuring that the review agent produces valid XML output that conforms to the XSD schema.
fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    review_prompt: &str,
) -> anyhow::Result<ReviewPassResult> {
    let issues_path = Path::new(".agent/ISSUES.md");

    // Ensure .agent directory exists
    if let Some(parent) = issues_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Read PROMPT.md and PLAN.md for XSD retry context
    let prompt_content = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_content = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();

    // Get the diff content for the CHANGES variable
    let diff_content = match crate::git_helpers::get_git_diff_from_review_baseline() {
        Ok(d) if !d.trim().is_empty() => Some(d),
        _ => None,
    };

    let changes_content = diff_content.unwrap_or_else(|| {
        // Fallback if diff retrieval failed
        "No diff available".to_string()
    });

    // In-session retry loop with XSD validation feedback (similar to planning phase)
    let max_retries = 10;
    let mut xsd_error: Option<String> = None;

    for retry_num in 0..max_retries {
        // For initial attempt, use the provided review prompt (XML-based)
        // For retries, use XSD retry prompt with error feedback
        let review_prompt = if retry_num == 0 {
            review_prompt.to_string()
        } else {
            ctx.logger.info(&format!(
                "  In-session retry {}/{} for XSD validation",
                retry_num,
                max_retries - 1
            ));
            if let Some(ref error) = xsd_error {
                ctx.logger.info(&format!("  XSD error: {}", error));
            }

            // Read the last output for retry context
            let log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_{}", retry_num - 1);
            let last_output = read_last_review_output(Path::new(&log_dir));

            prompt_review_xsd_retry_with_context(
                ctx.template_context,
                &prompt_content,
                &plan_content,
                &changes_content,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
            )
        };

        let log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_{retry_num}");

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
                        Ok(Some(_)) => Ok(true),
                        Ok(None) => Ok(false),
                        Err(_) => Ok(true),
                    }
                };

            let mut fallback_config = FallbackConfig {
                role: AgentRole::Reviewer,
                base_label: &format!(
                    "{review_label} #{j}{}",
                    if retry_num > 0 {
                        format!(" (retry {retry_num})")
                    } else {
                        String::new()
                    }
                ),
                prompt: &review_prompt,
                logfile_prefix: &log_dir,
                runtime: &mut runtime,
                registry: ctx.registry,
                primary_agent: ctx.reviewer_agent,
                output_validator: Some(validate_output),
            };
            run_with_fallback_and_validator(&mut fallback_config)
        };
        ctx.stats.reviewer_runs_completed += 1;

        // Extract and validate the review output
        let log_dir_path = Path::new(&log_dir);
        let review_content = read_last_review_output(log_dir_path);

        // Try to extract XML
        if let Some(xml_content) = extract_issues_xml(&review_content) {
            // Try to validate against XSD
            match validate_issues_xml(&xml_content) {
                Ok(issues_elements) => {
                    // XSD validation passed - format and write the issues
                    let formatted_xml = format_xml_for_display(&xml_content);

                    // Convert XML to markdown format for ISSUES.md
                    let markdown = format_issues_as_markdown(&issues_elements);
                    fs::write(issues_path, &markdown)?;

                    // Check if no issues were found
                    if issues_elements.is_empty() || issues_elements.no_issues_found.is_some() {
                        if retry_num > 0 {
                            ctx.logger
                                .success(&format!("Review validated after {} retries", retry_num));
                        } else {
                            ctx.logger
                                .success("Review completed - no issues found (validated XML)");
                        }

                        // Display the formatted XML
                        ctx.logger.info(&format!("\n{}", formatted_xml));

                        // Clean up ISSUES.md before early exit in isolation mode
                        if ctx.config.isolation_mode {
                            delete_issues_file_for_isolation(ctx.logger)?;
                        }
                        return Ok(ReviewPassResult { early_exit: true });
                    }

                    if retry_num > 0 {
                        ctx.logger
                            .success(&format!("Review validated after {} retries", retry_num));
                    } else {
                        ctx.logger.success(&format!(
                            "Issues extracted: {} total (validated XML)",
                            issues_elements.issue_count()
                        ));
                    }

                    // Display the formatted XML
                    ctx.logger.info(&format!("\n{}", formatted_xml));

                    // POST-FLIGHT VALIDATION: Check review output after agent completes
                    handle_postflight_validation(ctx, j);
                    return Ok(ReviewPassResult { early_exit: false });
                }
                Err(xsd_err) => {
                    // XSD validation failed - check if we can retry
                    let error_msg = format_xsd_error_for_review(&xsd_err);
                    ctx.logger
                        .warn(&format!("  XSD validation failed: {}", error_msg));

                    if retry_num < max_retries - 1 {
                        // Store error for next retry attempt
                        xsd_error = Some(error_msg);
                        // Continue to next retry iteration
                        continue;
                    } else {
                        ctx.logger.warn("  No more in-session retries remaining");
                        // Fall through to legacy extraction
                    }
                }
            }
        } else {
            // No XML found - log and try legacy extraction
            ctx.logger
                .info("No XML found in output, trying legacy extraction...");
        }

        // If we get here, either no XML or XSD validation failed after retries
        // Try legacy extraction as fallback
        let parse_result =
            extract_and_validate_review_output_legacy(ctx, log_dir_path, issues_path)?;

        match parse_result {
            ParseResult::IssuesFound => {
                // POST-FLIGHT VALIDATION: Check review output after agent completes
                handle_postflight_validation(ctx, j);
                return Ok(ReviewPassResult { early_exit: false });
            }
            ParseResult::NoIssuesExplicit => {
                ctx.logger
                    .success(&format!("No issues found after cycle {j} - stopping early"));
                // Clean up ISSUES.md before early exit in isolation mode
                if ctx.config.isolation_mode {
                    delete_issues_file_for_isolation(ctx.logger)?;
                }
                return Ok(ReviewPassResult { early_exit: true });
            }
            ParseResult::ParseFailed(error_description) => {
                // If this is the last retry attempt, write failure marker
                if retry_num == max_retries - 1 {
                    ctx.logger.error(&format!(
                        "Failed to get parseable review output after {} attempts. Last error: {}",
                        max_retries, error_description
                    ));
                    // Write a marker file indicating the failure
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
                // Continue to next retry iteration
                xsd_error = Some(format!("No valid XML output: {}", error_description));
            }
        }
    }

    // Should not reach here, but handle the case
    Ok(ReviewPassResult { early_exit: false })
}

/// Extract review output and validate its format (legacy fallback).
///
/// Returns a `ParseResult` indicating whether the output was successfully parsed,
/// explicitly declared no issues, or failed to parse (with an error description).
fn extract_and_validate_review_output_legacy(
    ctx: &mut PhaseContext<'_>,
    log_dir: &Path,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    // First, try XML extraction (new format)
    let log_content = read_last_review_output(log_dir);
    if let Some(xml_content) = extract_issues_xml(&log_content) {
        match validate_issues_xml(&xml_content) {
            Ok(issues_elements) => {
                // XSD validation passed - convert to markdown and write
                let markdown = format_issues_as_markdown(&issues_elements);
                fs::write(issues_path, &markdown)?;

                if issues_elements.is_empty() {
                    ctx.logger.success("No issues found (validated XML)");
                    return Ok(ParseResult::NoIssuesExplicit);
                }

                ctx.logger.success(&format!(
                    "Issues extracted: {} total (validated XML)",
                    issues_elements.issue_count()
                ));
                return Ok(ParseResult::IssuesFound);
            }
            Err(xsd_err) => {
                // XSD validation failed - log error and fall through to legacy extraction
                ctx.logger.warn(&format!(
                    "XML extraction found but XSD validation failed: {} - expected: {}, found: {}",
                    xsd_err.element_path, xsd_err.expected, xsd_err.found
                ));
            }
        }
    }

    // Fall back to existing JSON/text extraction
    let extraction = extract_issues(Path::new(log_dir))?;

    // First, try to get content from extraction or legacy file
    let content = if let Some(content) = extraction.raw_content {
        content
    } else {
        // JSON extraction failed - check for legacy agent-written file
        if ctx.config.verbosity.is_debug() {
            ctx.logger
                .info("No JSON result event found in reviewer logs");
            log_extraction_diagnostics(ctx.logger, &log_dir.to_string_lossy());
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

/// Read the last review output from logs.
fn read_last_review_output(log_dir: &Path) -> String {
    // Try to read from the latest log file
    let log_path = log_dir.join("latest.log");
    if let Ok(content) = fs::read_to_string(&log_path) {
        return content;
    }

    // Fallback to reading all .log files in the directory
    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                if let Ok(content) = fs::read_to_string(&path) {
                    return content;
                }
            }
        }
    }

    String::new()
}

/// Format XSD error for display in review context.
fn format_xsd_error_for_review(error: &XsdValidationError) -> String {
    format!(
        "{} - expected: {}, found: {}",
        error.element_path, error.expected, error.found
    )
}

/// Format issues elements as markdown for ISSUES.md.
fn format_issues_as_markdown(elements: &IssuesElements) -> String {
    let mut result = String::new();

    if let Some(ref no_issues_msg) = elements.no_issues_found {
        result.push_str(no_issues_msg);
        return result;
    }

    for (i, issue) in elements.issues.iter().enumerate() {
        result.push_str(&format!("{}. {}\n\n", i + 1, issue));
    }

    result
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

/// Run the fix pass for a single cycle with XSD retry loop.
///
/// This function implements the XSD retry loop pattern similar to the review pass,
/// ensuring that the fix agent produces valid XML output that conforms to the XSD schema.
fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    _reviewer_context: ContextLevel,
) -> anyhow::Result<FixPassResult> {
    update_status("Applying fixes", ctx.config.isolation_mode)?;

    // Read PROMPT.md, PLAN.md, and ISSUES.md for context
    let prompt_content = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_content = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();
    let issues_content = fs::read_to_string(".agent/ISSUES.md").unwrap_or_default();

    // In-session retry loop with XSD validation feedback
    let max_retries = 10;
    let mut xsd_error: Option<String> = None;

    for retry_num in 0..max_retries {
        // For initial attempt, use XML fix prompt
        // For retries, use XSD retry prompt with error feedback
        let fix_prompt = if retry_num == 0 {
            prompt_fix_xml_with_context(
                ctx.template_context,
                &prompt_content,
                &plan_content,
                &issues_content,
            )
        } else {
            ctx.logger.info(&format!(
                "  In-session retry {}/{} for XSD validation",
                retry_num,
                max_retries - 1
            ));
            if let Some(ref error) = xsd_error {
                ctx.logger.info(&format!("  XSD error: {}", error));
            }

            // Read the last output for retry context
            let log_dir = format!(".agent/logs/reviewer_fix_{j}_attempt_{}", retry_num - 1);
            let last_output = read_last_review_output(Path::new(&log_dir));

            prompt_fix_xsd_retry_with_context(
                ctx.template_context,
                &issues_content,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
            )
        };

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

        let log_dir = format!(".agent/logs/reviewer_fix_{j}_attempt_{retry_num}");

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
            let validate_output: crate::pipeline::OutputValidator =
                |log_dir_path: &Path, _logger: &crate::logger::Logger| -> std::io::Result<bool> {
                    use crate::files::result_extraction::extract_last_result;
                    match extract_last_result(log_dir_path) {
                        Ok(Some(_)) => Ok(true),
                        Ok(None) => Ok(false),
                        Err(_) => Ok(true),
                    }
                };

            let mut fallback_config = FallbackConfig {
                role: AgentRole::Reviewer,
                base_label: &format!(
                    "fix #{j}{}",
                    if retry_num > 0 {
                        format!(" (retry {retry_num})")
                    } else {
                        String::new()
                    }
                ),
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

        // Extract and validate the fix result output
        let log_dir_path = Path::new(&log_dir);
        let fix_content = read_last_review_output(log_dir_path);

        // Try to extract XML
        if let Some(xml_content) = extract_fix_result_xml(&fix_content) {
            // Try to validate against XSD
            match validate_fix_result_xml(&xml_content) {
                Ok(fix_result) => {
                    // XSD validation passed - determine the result
                    let formatted_xml = format_xml_for_display(&xml_content);

                    if retry_num > 0 {
                        ctx.logger
                            .success(&format!("Fix result validated after {} retries", retry_num));
                    } else {
                        ctx.logger.success("Fix result validated (XML)");
                    }

                    // Display the formatted XML
                    ctx.logger.info(&format!("\n{}", formatted_xml));

                    // Determine the result based on ralph-status
                    let result = match fix_result.status.as_str() {
                        "all_issues_addressed" => FixPassResult::AllIssuesAddressed,
                        "no_issues_found" => FixPassResult::NoIssuesFound,
                        "issues_remain" => FixPassResult::IssuesRemain,
                        _ => FixPassResult::IssuesRemain, // Default to issues remain for unknown status
                    };

                    // Clean up ISSUES.md after each fix cycle in isolation mode
                    if ctx.config.isolation_mode {
                        delete_issues_file_for_isolation(ctx.logger)?;
                    }

                    // Periodic restoration check - ensure PROMPT.md still exists
                    ensure_prompt_integrity(ctx.logger, "review", j);

                    return Ok(result);
                }
                Err(xsd_err) => {
                    // XSD validation failed - check if we can retry
                    let error_msg = format_xsd_error_for_review(&xsd_err);
                    ctx.logger
                        .warn(&format!("  XSD validation failed: {}", error_msg));

                    if retry_num < max_retries - 1 {
                        // Store error for next retry attempt
                        xsd_error = Some(error_msg);
                        // Continue to next retry iteration
                        continue;
                    } else {
                        ctx.logger.warn("  No more in-session retries remaining");
                        // Fall through to legacy handling
                    }
                }
            }
        } else {
            // No XML found - log warning
            ctx.logger.info("No XML found in fix output, continuing...");
        }

        // If we get here, either no XML or XSD validation failed after retries
        // Continue to the next retry iteration with a generic error
        if retry_num < max_retries - 1 {
            xsd_error = Some("No valid XML output found".to_string());
            continue;
        }
    }

    // After all retries, clean up and return with issues remain status
    ctx.logger
        .warn("Fix pass completed without valid XML status - assuming issues remain");

    // Clean up ISSUES.md after each fix cycle in isolation mode
    if ctx.config.isolation_mode {
        delete_issues_file_for_isolation(ctx.logger)?;
    }

    // Periodic restoration check - ensure PROMPT.md still exists
    ensure_prompt_integrity(ctx.logger, "review", j);

    Ok(FixPassResult::IssuesRemain)
}

/// Result of the fix pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixPassResult {
    /// All issues have been addressed
    AllIssuesAddressed,
    /// No issues were found
    NoIssuesFound,
    /// Issues remain and need further review cycles
    IssuesRemain,
}

/// Handle post-fix commit creation.
fn handle_post_fix_commit(ctx: &mut PhaseContext<'_>, prev_snap: &str) -> anyhow::Result<String> {
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
                }
                CommitResultFallback::NoChanges => {
                    ctx.logger.info("No commit created (no meaningful changes)");
                }
                CommitResultFallback::Failed(err) => {
                    ctx.logger.error(&format!(
                        "Failed to create commit (git operation failed): {err}"
                    ));
                    return Err(anyhow::anyhow!(err));
                }
            }
        } else {
            ctx.logger
                .warn("Unable to get commit agent chain for commit");
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
