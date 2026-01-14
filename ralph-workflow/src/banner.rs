//! Banner and UI output utilities.
//!
//! This module contains presentation logic for the pipeline's visual output,
//! including the welcome banner and the final summary display.

use crate::colors::Colors;
use crate::config::Config;
use crate::logger::Logger;
use crate::pipeline::Stats;
use crate::review_metrics::ReviewMetrics;
use crate::timer::Timer;

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
/// * `config` - Pipeline configuration
/// * `timer` - Timer tracking the pipeline duration
/// * `stats` - Statistics collected during the pipeline run
/// * `logger` - Logger for final success message
pub fn print_final_summary(
    colors: Colors,
    config: &Config,
    timer: &Timer,
    stats: &Stats,
    logger: &Logger,
) {
    logger.header("Pipeline Complete", super::colors::Colors::green);

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
        timer.elapsed_formatted(),
        colors.reset()
    );
    println!(
        "  {}🔄{}  Dev runs:        {}{}{}/{}",
        colors.blue(),
        colors.reset(),
        colors.bold(),
        stats.developer_runs_completed,
        colors.reset(),
        config.developer_iters
    );
    println!(
        "  {}🔍{}  Review runs:     {}{}{}",
        colors.magenta(),
        colors.reset(),
        colors.bold(),
        stats.reviewer_runs_completed,
        colors.reset()
    );
    println!(
        "  {}📝{}  Changes detected: {}{}{}",
        colors.green(),
        colors.reset(),
        colors.bold(),
        stats.changes_detected,
        colors.reset()
    );

    // Review metrics from ISSUES.md
    if let Ok(metrics) = ReviewMetrics::from_issues_file() {
        print_review_metrics(colors, config, &metrics);
    }
    println!();

    print_output_files(colors, config);

    logger.success("Ralph pipeline completed successfully!");
}

/// Print review metrics summary.
fn print_review_metrics(colors: Colors, config: &Config, metrics: &ReviewMetrics) {
    if !metrics.issues_file_found {
        return;
    }

    if metrics.no_issues_declared && metrics.total_issues == 0 {
        println!(
            "  {}✓{}   Review result:   {}{}{}",
            colors.green(),
            colors.reset(),
            colors.bold(),
            metrics.summary(),
            colors.reset()
        );
    } else if metrics.total_issues > 0 {
        // Use summary() for a concise one-line display
        println!(
            "  {}🔎{}  Review summary:  {}{}{}",
            colors.yellow(),
            colors.reset(),
            colors.bold(),
            metrics.summary(),
            colors.reset()
        );
        // Show unresolved count
        let unresolved = metrics.unresolved_issues();
        if unresolved > 0 {
            println!(
                "  {}⚠{}   Unresolved:      {}{}{} issues remaining",
                colors.red(),
                colors.reset(),
                colors.bold(),
                unresolved,
                colors.reset()
            );
        }
        // Show detailed breakdown in verbose mode
        if config.verbosity.is_verbose() && metrics.total_issues > 1 {
            println!("  {}📊{}  Breakdown:", colors.dim(), colors.reset());
            for line in metrics.detailed_summary().lines() {
                println!("      {}{}{}", colors.dim(), line.trim(), colors.reset());
            }
            // Also show a few unresolved issue summaries (useful when running in verbose mode).
            let samples = metrics.unresolved_issue_summaries(3);
            if !samples.is_empty() {
                println!(
                    "  {}🧾{}  Unresolved samples:",
                    colors.dim(),
                    colors.reset()
                );
                for s in samples {
                    println!("      {}- {}{}", colors.dim(), s, colors.reset());
                }
            }
        }
        // Highlight blocking issues
        if metrics.has_blocking_issues() {
            println!(
                "  {}🚨{}  BLOCKING:        {}{}{} critical/high issues unresolved",
                colors.red(),
                colors.reset(),
                colors.bold(),
                metrics.unresolved_blocking_issues(),
                colors.reset()
            );
        }
    }
}

/// Print the output files list.
fn print_output_files(colors: Colors, config: &Config) {
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
    if !config.isolation_mode {
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
