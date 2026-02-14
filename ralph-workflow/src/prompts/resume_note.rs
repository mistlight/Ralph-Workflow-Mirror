//! Resume context note generation.
//!
//! Generates rich context notes for resumed sessions to help agents understand
//! where they are in the pipeline when resuming from a checkpoint.

use crate::checkpoint::execution_history::StepOutcome;
use crate::checkpoint::restore::ResumeContext;
use crate::checkpoint::state::PipelinePhase;

/// Generate a rich resume note from resume context.
///
/// Creates a detailed, context-aware note that helps agents understand
/// where they are in the pipeline when resuming from a checkpoint.
///
/// The note includes:
/// - Phase and iteration information
/// - Recent execution history (files modified, issues found/fixed)
/// - Git commits made during the session
/// - Guidance on what to focus on
pub fn generate_resume_note(context: &ResumeContext) -> String {
    let mut note = String::from("SESSION RESUME CONTEXT\n");
    note.push_str("====================\n\n");

    // Add phase information with specific context based on phase type
    match context.phase {
        PipelinePhase::Development => {
            note.push_str(&format!(
                "Resuming DEVELOPMENT phase (iteration {} of {})\n",
                context.iteration + 1,
                context.total_iterations
            ));
        }
        PipelinePhase::Review => {
            note.push_str(&format!(
                "Resuming REVIEW phase (pass {} of {})\n",
                context.reviewer_pass + 1,
                context.total_reviewer_passes
            ));
        }
        _ => {
            note.push_str(&format!("Resuming from phase: {}\n", context.phase_name()));
        }
    }

    // Add resume count if this has been resumed before
    if context.resume_count > 0 {
        note.push_str(&format!(
            "This session has been resumed {} time(s)\n",
            context.resume_count
        ));
    }

    // Add rebase state if applicable
    if !matches!(
        context.rebase_state,
        crate::checkpoint::state::RebaseState::NotStarted
    ) {
        note.push_str(&format!("Rebase state: {:?}\n", context.rebase_state));
    }

    note.push('\n');

    // Add execution history summary if available
    if let Some(ref history) = context.execution_history {
        if !history.steps.is_empty() {
            note.push_str("RECENT ACTIVITY:\n");
            note.push_str("----------------\n");

            // Show recent execution steps (last 5)
            let recent_steps: Vec<_> = history
                .steps
                .iter()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            for step in &recent_steps {
                note.push_str(&format!(
                    "- [{}] {} (iteration {}): {}\n",
                    step.step_type,
                    step.phase,
                    step.iteration,
                    step.outcome.brief_description()
                ));

                // Add files modified count if available
                if let Some(ref detail) = step.modified_files_detail {
                    let added_count = detail.added.as_ref().map_or(0, |v| v.len());
                    let modified_count = detail.modified.as_ref().map_or(0, |v| v.len());
                    let deleted_count = detail.deleted.as_ref().map_or(0, |v| v.len());
                    let total_files = added_count + modified_count + deleted_count;
                    if total_files > 0 {
                        note.push_str(&format!("  Files: {} changed", total_files));
                        if added_count > 0 {
                            note.push_str(&format!(" ({} added)", added_count));
                        }
                        if modified_count > 0 {
                            note.push_str(&format!(" ({} modified)", modified_count));
                        }
                        if deleted_count > 0 {
                            note.push_str(&format!(" ({} deleted)", deleted_count));
                        }
                        note.push('\n');
                    }
                }

                // Add issues summary if available
                if let Some(ref issues) = step.issues_summary {
                    if issues.found > 0 || issues.fixed > 0 {
                        note.push_str(&format!(
                            "  Issues: {} found, {} fixed",
                            issues.found, issues.fixed
                        ));
                        if let Some(ref desc) = issues.description {
                            note.push_str(&format!(" ({})", desc));
                        }
                        note.push('\n');
                    }
                }

                // Add git commit if available
                if let Some(ref oid) = step.git_commit_oid {
                    note.push_str(&format!("  Commit: {}\n", oid));
                }
            }

            note.push('\n');
        }
    }

    note.push_str("Previous progress is preserved in git history.\n");

    // Add helpful guidance about what the agent should focus on
    note.push_str("\nGUIDANCE:\n");
    note.push_str("--------\n");
    match context.phase {
        PipelinePhase::Development => {
            note.push_str("Continue working on the implementation tasks from your plan.\n");
        }
        PipelinePhase::Review => {
            note.push_str("Review the code changes and provide feedback.\n");
        }
        _ => {}
    }

    note.push('\n');
    note
}

/// Helper trait for brief outcome descriptions.
pub trait BriefDescription {
    fn brief_description(&self) -> String;
}

const PARTIAL_FIELD_MAX_CHARS: usize = 120;

fn one_line_truncated(input: &str, max_chars: usize) -> String {
    let first_line = input.lines().next().unwrap_or("").trim();
    let mut out: String = first_line.chars().take(max_chars).collect();
    if first_line.chars().count() > max_chars {
        out.push_str("...(truncated)");
    }
    out
}

impl BriefDescription for StepOutcome {
    fn brief_description(&self) -> String {
        match self {
            Self::Success {
                files_modified,
                output,
                ..
            } => {
                if let Some(ref out) = output {
                    if !out.is_empty() {
                        format!("Success - {}", out.lines().next().unwrap_or(""))
                    } else if let Some(ref files) = files_modified {
                        if !files.is_empty() {
                            format!("Success - {} files modified", files.len())
                        } else {
                            "Success".to_string()
                        }
                    } else {
                        "Success".to_string()
                    }
                } else if let Some(ref files) = files_modified {
                    if !files.is_empty() {
                        format!("Success - {} files modified", files.len())
                    } else {
                        "Success".to_string()
                    }
                } else {
                    "Success".to_string()
                }
            }
            Self::Failure {
                error, recoverable, ..
            } => {
                if *recoverable {
                    format!("Recoverable error - {}", error.lines().next().unwrap_or(""))
                } else {
                    format!("Failed - {}", error.lines().next().unwrap_or(""))
                }
            }
            Self::Partial {
                completed,
                remaining,
                ..
            } => {
                let completed = one_line_truncated(completed, PARTIAL_FIELD_MAX_CHARS);
                let remaining = one_line_truncated(remaining, PARTIAL_FIELD_MAX_CHARS);
                format!("Partial - {} done, {}", completed, remaining)
            }
            Self::Skipped { reason } => {
                format!("Skipped - {}", reason)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BriefDescription;
    use crate::checkpoint::execution_history::StepOutcome;

    #[test]
    fn test_partial_brief_description_is_single_line_and_truncated() {
        let outcome =
            StepOutcome::partial("done line 1\ndone line 2".to_string(), "x".repeat(1000));

        let desc = outcome.brief_description();
        assert!(
            !desc.contains('\n'),
            "description must be single-line: {desc}"
        );
        assert!(
            desc.contains("truncated"),
            "expected truncation marker for oversized fields: {desc}"
        );
        assert!(desc.len() < 300, "expected bounded output size: {desc}");
    }
}
