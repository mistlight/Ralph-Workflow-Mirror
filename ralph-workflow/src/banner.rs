//! Banner and UI output utilities.
//!
//! This module contains presentation logic for the pipeline's visual output,
//! including the welcome banner and the final summary display.

use crate::logger::Colors;
use crate::logger::Logger;

/// Summary data for pipeline completion display.
///
/// Decouples the banner presentation logic from the actual pipeline types.
pub struct PipelineSummary {
    /// Total elapsed time formatted as "Xm YYs"
    pub total_time: String,
    /// Number of developer runs completed
    pub dev_runs_completed: usize,
    /// Total configured developer iterations
    pub dev_runs_total: usize,
    /// Number of reviewer runs completed
    pub review_runs: usize,
    /// Number of changes detected during pipeline
    pub changes_detected: usize,
    /// Whether isolation mode is enabled
    pub isolation_mode: bool,
    /// Whether to show verbose output
    pub verbose: bool,
    /// Optional review metrics summary
    pub review_summary: Option<ReviewSummary>,
}

/// Review metrics summary for display.
pub struct ReviewSummary {
    /// One-line summary of review results
    pub summary: String,
    /// Number of unresolved issues
    pub unresolved_count: usize,
    /// Number of unresolved blocking issues
    pub blocking_count: usize,
    /// Optional detailed breakdown (for verbose mode)
    pub detailed_breakdown: Option<String>,
    /// Optional sample unresolved issues (for verbose mode)
    pub samples: Vec<String>,
}

/// Print the welcome banner for the Ralph pipeline.
///
/// Displays a styled ASCII box with the pipeline name and agent information.
///
/// # Arguments
///
/// * `colors` - Color configuration for terminal output
/// * `developer_agent` - Name of the developer agent
/// * `reviewer_agent` - Name of the reviewer agent
pub fn print_welcome_banner(colors: Colors, developer_agent: &str, reviewer_agent: &str) {
    println!();
    println!(
        "{}{}╭────────────────────────────────────────────────────────────╮{}",
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!(
        "{}{}│{}  {}{}🤖 Ralph{} {}─ PROMPT-driven agent orchestrator{}              {}{}│{}",
        colors.bold(),
        colors.cyan(),
        colors.reset(),
        colors.bold(),
        colors.white(),
        colors.reset(),
        colors.dim(),
        colors.reset(),
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!(
        "{}{}│{}  {}{} × {} pipeline for autonomous development{}                 {}{}│{}",
        colors.bold(),
        colors.cyan(),
        colors.reset(),
        colors.dim(),
        developer_agent,
        reviewer_agent,
        colors.reset(),
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!(
        "{}{}╰────────────────────────────────────────────────────────────╯{}",
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!();
}

/// Print the final summary after pipeline completion.
///
/// Displays statistics about the pipeline run including timing, run counts,
/// and review metrics if available.
///
/// # Arguments
///
/// * `colors` - Color configuration for terminal output
/// * `summary` - Pipeline summary data
/// * `logger` - Logger for final success message
pub fn print_final_summary(colors: Colors, summary: &PipelineSummary, logger: &Logger) {
    logger.header("Pipeline Complete", crate::logger::Colors::green);

    println!();
    println!(
        "{}{}📊 Summary{}",
        colors.bold(),
        colors.white(),
        colors.reset()
    );
    println!(
        "{}──────────────────────────────────{}",
        colors.dim(),
        colors.reset()
    );
    println!(
        "  {}⏱{}  Total time:      {}{}{}",
        colors.cyan(),
        colors.reset(),
        colors.bold(),
        summary.total_time,
        colors.reset()
    );
    println!(
        "  {}🔄{}  Dev runs:        {}{}{}/{}",
        colors.blue(),
        colors.reset(),
        colors.bold(),
        summary.dev_runs_completed,
        colors.reset(),
        summary.dev_runs_total
    );
    println!(
        "  {}🔍{}  Review runs:     {}{}{}",
        colors.magenta(),
        colors.reset(),
        colors.bold(),
        summary.review_runs,
        colors.reset()
    );
    println!(
        "  {}📝{}  Changes detected: {}{}{}",
        colors.green(),
        colors.reset(),
        colors.bold(),
        summary.changes_detected,
        colors.reset()
    );

    // Review metrics
    if let Some(ref review) = summary.review_summary {
        print_review_summary(colors, summary.verbose, review);
    }
    println!();

    print_output_files(colors, summary.isolation_mode);

    logger.success("Ralph pipeline completed successfully!");
}

/// Print review metrics summary.
fn print_review_summary(colors: Colors, verbose: bool, review: &ReviewSummary) {
    // No issues case
    if review.unresolved_count == 0 && review.blocking_count == 0 {
        println!(
            "  {}✓{}   Review result:   {}{}{}",
            colors.green(),
            colors.reset(),
            colors.bold(),
            review.summary,
            colors.reset()
        );
        return;
    }

    // Issues present
    println!(
        "  {}🔎{}  Review summary:  {}{}{}",
        colors.yellow(),
        colors.reset(),
        colors.bold(),
        review.summary,
        colors.reset()
    );

    // Show unresolved count
    if review.unresolved_count > 0 {
        println!(
            "  {}⚠{}   Unresolved:      {}{}{} issues remaining",
            colors.red(),
            colors.reset(),
            colors.bold(),
            review.unresolved_count,
            colors.reset()
        );
    }

    // Show detailed breakdown in verbose mode
    if verbose {
        if let Some(ref breakdown) = review.detailed_breakdown {
            println!("  {}📊{}  Breakdown:", colors.dim(), colors.reset());
            for line in breakdown.lines() {
                println!("      {}{}{}", colors.dim(), line.trim(), colors.reset());
            }
        }
        // Show sample unresolved issues
        if !review.samples.is_empty() {
            println!(
                "  {}🧾{}  Unresolved samples:",
                colors.dim(),
                colors.reset()
            );
            for s in &review.samples {
                println!("      {}- {}{}", colors.dim(), s, colors.reset());
            }
        }
    }

    // Highlight blocking issues
    if review.blocking_count > 0 {
        println!(
            "  {}🚨{}  BLOCKING:        {}{}{} critical/high issues unresolved",
            colors.red(),
            colors.reset(),
            colors.bold(),
            review.blocking_count,
            colors.reset()
        );
    }
}

/// Print the output files list.
fn print_output_files(colors: Colors, isolation_mode: bool) {
    println!(
        "{}{}📁 Output Files{}",
        colors.bold(),
        colors.white(),
        colors.reset()
    );
    println!(
        "{}──────────────────────────────────{}",
        colors.dim(),
        colors.reset()
    );
    println!(
        "  → {}PROMPT.md{}           Goal definition",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  → {}.agent/STATUS.md{}    Current status",
        colors.cyan(),
        colors.reset()
    );
    // Only show ISSUES.md and NOTES.md when NOT in isolation mode
    if !isolation_mode {
        println!(
            "  → {}.agent/ISSUES.md{}    Review findings",
            colors.cyan(),
            colors.reset()
        );
        println!(
            "  → {}.agent/NOTES.md{}     Progress notes",
            colors.cyan(),
            colors.reset()
        );
    }
    println!(
        "  → {}.agent/logs/{}        Detailed logs",
        colors.cyan(),
        colors.reset()
    );
    println!();
}
