//! Prompt Templates Module
//!
//! Provides context-controlled prompts for agents.
//! Key design: reviewers get minimal context for "fresh eyes" perspective.
//!
//! Enhanced with language-specific review guidelines based on detected project stack.
//!
//! # Module Structure
//!
//! - [`types`] - Type definitions (`ContextLevel`, Role, Action)
//! - [`developer`] - Developer prompts (iteration, planning)
//! - [`reviewer`] - Reviewer prompts (review, comprehensive, security, incremental)
//! - [`commit`] - Fix and commit message prompts
//! - [`rebase`] - Conflict resolution prompts for auto-rebase
//! - [`partials`] - Shared template partials for composition

mod commit;
mod developer;
pub mod partials;
mod rebase;
pub mod reviewer;
pub mod template_catalog;
pub mod template_context;
mod template_engine;
mod template_macros;
pub mod template_registry;
mod template_validator;
mod types;

// Re-export ResumeContext for use in prompts
pub use crate::checkpoint::restore::ResumeContext;

// Re-export all public items for backward compatibility
pub use commit::{
    prompt_fix_with_context, prompt_generate_commit_message_with_diff_with_context,
    prompt_simplified_commit_with_context, prompt_xsd_retry_with_context,
};
pub use developer::{prompt_developer_iteration_with_context, prompt_plan_with_context};
pub use rebase::{
    build_conflict_resolution_prompt_with_context, collect_conflict_info, FileConflict,
};

#[cfg(any(test, feature = "test-utils"))]
pub use rebase::build_enhanced_conflict_resolution_prompt;

// Types only used in tests
#[cfg(any(test, feature = "test-utils"))]
pub use rebase::{collect_branch_info, BranchInfo};
pub use reviewer::{
    prompt_comprehensive_review_with_diff_with_context,
    prompt_detailed_review_without_guidelines_with_diff_with_context,
    prompt_incremental_review_with_diff_with_context,
    prompt_reviewer_review_with_guidelines_and_diff_with_context,
    prompt_security_focused_review_with_diff_with_context,
    prompt_universal_review_with_diff_with_context,
};

// Re-export non-context variants for test compatibility
#[cfg(test)]
pub use commit::{prompt_fix, prompt_generate_commit_message_with_diff};
#[cfg(test)]
pub use developer::{prompt_developer_iteration, prompt_plan};
pub use template_context::TemplateContext;
pub use template_engine::Template;
pub use template_validator::{
    extract_metadata, extract_partials, extract_variables, validate_template, ValidationError,
    ValidationWarning,
};
pub use types::{Action, ContextLevel, Role};

/// Configuration for prompt generation.
///
/// Groups related parameters to reduce function argument count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[must_use]
pub struct PromptConfig {
    /// The current iteration number (for developer iteration prompts).
    pub iteration: Option<u32>,
    /// The total number of iterations (for developer iteration prompts).
    pub total_iterations: Option<u32>,
    /// PROMPT.md content for planning prompts.
    pub prompt_md_content: Option<String>,
    /// (PROMPT.md, PLAN.md) content tuple for developer iteration prompts.
    pub prompt_and_plan: Option<(String, String)>,
    /// (PROMPT.md, PLAN.md, ISSUES.md) content tuple for fix prompts.
    pub prompt_plan_and_issues: Option<(String, String, String)>,
    /// Whether this is a resumed session (from a checkpoint).
    pub is_resume: bool,
    /// Rich resume context if available.
    pub resume_context: Option<ResumeContext>,
}

impl PromptConfig {
    /// Create a new prompt configuration with default values.
    #[must_use = "configuration is required for prompt generation"]
    pub const fn new() -> Self {
        Self {
            iteration: None,
            total_iterations: None,
            prompt_md_content: None,
            prompt_and_plan: None,
            prompt_plan_and_issues: None,
            is_resume: false,
            resume_context: None,
        }
    }

    /// Set iteration numbers for developer iteration prompts.
    #[must_use = "returns the updated configuration for chaining"]
    pub const fn with_iterations(mut self, iteration: u32, total: u32) -> Self {
        self.iteration = Some(iteration);
        self.total_iterations = Some(total);
        self
    }

    /// Set PROMPT.md content for planning prompts.
    #[must_use = "returns the updated configuration for chaining"]
    pub fn with_prompt_md(mut self, content: String) -> Self {
        self.prompt_md_content = Some(content);
        self
    }

    /// Set (PROMPT.md, PLAN.md) content tuple for developer iteration prompts.
    #[must_use = "returns the updated configuration for chaining"]
    pub fn with_prompt_and_plan(mut self, prompt: String, plan: String) -> Self {
        self.prompt_and_plan = Some((prompt, plan));
        self
    }

    /// Set (PROMPT.md, PLAN.md, ISSUES.md) content tuple for fix prompts.
    pub fn with_prompt_plan_and_issues(
        mut self,
        prompt: String,
        plan: String,
        issues: String,
    ) -> Self {
        self.prompt_plan_and_issues = Some((prompt, plan, issues));
        self
    }

    /// Set whether this is a resumed session.
    #[cfg(test)]
    #[must_use = "returns the updated configuration for chaining"]
    pub const fn with_resume(mut self, is_resume: bool) -> Self {
        self.is_resume = is_resume;
        self
    }

    /// Set rich resume context for resumed sessions.
    #[must_use = "returns the updated configuration for chaining"]
    pub fn with_resume_context(mut self, context: ResumeContext) -> Self {
        self.resume_context = Some(context);
        self.is_resume = true;
        self
    }
}

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
        crate::checkpoint::state::PipelinePhase::Development => {
            note.push_str(&format!(
                "Resuming DEVELOPMENT phase (iteration {} of {})\n",
                context.iteration + 1,
                context.total_iterations
            ));
        }
        crate::checkpoint::state::PipelinePhase::Review => {
            note.push_str(&format!(
                "Resuming REVIEW phase (pass {} of {})\n",
                context.reviewer_pass + 1,
                context.total_reviewer_passes
            ));
        }
        crate::checkpoint::state::PipelinePhase::ReviewAgain => {
            note.push_str(&format!(
                "Resuming VERIFICATION REVIEW phase (pass {} of {})\n",
                context.reviewer_pass + 1,
                context.total_reviewer_passes
            ));
        }
        crate::checkpoint::state::PipelinePhase::Fix => {
            note.push_str("Resuming FIX phase\n");
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
                    let total_files =
                        detail.added.len() + detail.modified.len() + detail.deleted.len();
                    if total_files > 0 {
                        note.push_str(&format!("  Files: {} changed", total_files));
                        if !detail.added.is_empty() {
                            note.push_str(&format!(" ({} added)", detail.added.len()));
                        }
                        if !detail.modified.is_empty() {
                            note.push_str(&format!(" ({} modified)", detail.modified.len()));
                        }
                        if !detail.deleted.is_empty() {
                            note.push_str(&format!(" ({} deleted)", detail.deleted.len()));
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
    note.push_str("Check 'git log' for details about what was done before.\n");

    // Add helpful guidance about what the agent should focus on
    note.push_str("\nGUIDANCE:\n");
    note.push_str("--------\n");
    match context.phase {
        crate::checkpoint::state::PipelinePhase::Development => {
            note.push_str("Continue working on the implementation tasks from your plan.\n");
        }
        crate::checkpoint::state::PipelinePhase::Review
        | crate::checkpoint::state::PipelinePhase::ReviewAgain => {
            note.push_str("Review the code changes and provide feedback.\n");
        }
        crate::checkpoint::state::PipelinePhase::Fix => {
            note.push_str("Focus on addressing the issues identified in the review.\n");
        }
        _ => {}
    }

    note.push('\n');
    note
}

// Helper trait for brief outcome descriptions
trait BriefDescription {
    fn brief_description(&self) -> String;
}

impl BriefDescription for crate::checkpoint::execution_history::StepOutcome {
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
                    } else if !files_modified.is_empty() {
                        format!("Success - {} files modified", files_modified.len())
                    } else {
                        "Success".to_string()
                    }
                } else if !files_modified.is_empty() {
                    format!("Success - {} files modified", files_modified.len())
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
                format!("Partial - {} done, {}", completed, remaining)
            }
            Self::Skipped { reason } => {
                format!("Skipped - {}", reason)
            }
        }
    }
}

/// Generate a prompt for any agent type.
///
/// This is the main dispatcher function that routes to the appropriate
/// prompt generator based on role and action.
///
/// The config parameter allows providing:
/// - Language-specific review guidance when the project stack has been detected
/// - PROMPT.md content for planning prompts
/// - PROMPT.md and PLAN.md content for developer iteration prompts
///
/// # Arguments
///
/// * `role` - The agent role (Developer, Reviewer, etc.)
/// * `action` - The action to perform (Plan, Iterate, Fix, etc.)
/// * `context` - The context level (minimal or normal)
/// * `template_context` - Template context for user template overrides
/// * `config` - Prompt configuration with content variables
pub fn prompt_for_agent(
    role: Role,
    action: Action,
    context: ContextLevel,
    template_context: &TemplateContext,
    config: PromptConfig,
) -> String {
    let resume_note = if let Some(resume_ctx) = &config.resume_context {
        generate_resume_note(resume_ctx)
    } else if config.is_resume {
        // Fallback for backward compatibility when no rich context is available
        "\nNOTE: This session is resuming from a previous run. Previous progress is preserved in git history. You can check 'git log' for context about what was done before.\n\n".to_string()
    } else {
        String::new()
    };

    let base_prompt = match (role, action) {
        (_, Action::Plan) => {
            prompt_plan_with_context(template_context, config.prompt_md_content.as_deref())
        }
        (Role::Developer | Role::Reviewer, Action::Iterate) => {
            let (prompt_content, plan_content) = config
                .prompt_and_plan
                .unwrap_or((String::new(), String::new()));
            prompt_developer_iteration_with_context(
                template_context,
                config.iteration.unwrap_or(1),
                config.total_iterations.unwrap_or(1),
                context,
                &prompt_content,
                &plan_content,
            )
        }
        (_, Action::Fix) => {
            let (prompt_content, plan_content, issues_content) = config
                .prompt_plan_and_issues
                .unwrap_or((String::new(), String::new(), String::new()));
            prompt_fix_with_context(
                template_context,
                &prompt_content,
                &plan_content,
                &issues_content,
            )
        }
    };

    // Prepend resume note if applicable
    if config.is_resume {
        format!("{}{}", resume_note, base_prompt)
    } else {
        base_prompt
    }
}

/// Get a stored prompt from history or generate a new one.
///
/// This function implements prompt replay for hardened resume functionality.
/// When resuming from a checkpoint, it checks if a prompt was already used
/// and returns the stored prompt for deterministic behavior. Otherwise, it
/// generates a new prompt using the provided generator function.
///
/// # Arguments
///
/// * `prompt_key` - Unique key identifying this prompt (e.g., "development_1", "review_2")
/// * `prompt_history` - The prompt history from the checkpoint (if available)
/// * `generator` - Function to generate the prompt if not found in history
///
/// # Returns
///
/// A tuple of (prompt, was_replayed) where:
/// - `prompt` is the prompt string (either replayed or newly generated)
/// - `was_replayed` is true if the prompt came from history, false if newly generated
///
/// # Example
///
/// ```rust
/// let (prompt, was_replayed) = get_stored_or_generate_prompt(
///     "development_1",
///     &ctx.prompt_history,
///     || prompt_for_agent(role, action, context, template_context, config),
/// );
/// if was_replayed {
///     logger.info("Using stored prompt from checkpoint for determinism");
/// }
/// ```
pub fn get_stored_or_generate_prompt<F>(
    prompt_key: &str,
    prompt_history: &std::collections::HashMap<String, String>,
    generator: F,
) -> (String, bool)
where
    F: FnOnce() -> String,
{
    if let Some(stored_prompt) = prompt_history.get(prompt_key) {
        (stored_prompt.clone(), true)
    } else {
        (generator(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::template_context::TemplateContext;

    // Import non-context variants for test compatibility
    use crate::prompts::reviewer::prompt_detailed_review_without_guidelines_with_diff;

    #[test]
    fn test_prompt_for_agent_developer() {
        let template_context = TemplateContext::default();
        let result = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new()
                .with_iterations(3, 10)
                .with_prompt_and_plan("test prompt".to_string(), "test plan".to_string()),
        );
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
    }

    #[test]
    fn test_prompt_for_agent_reviewer() {
        let result = prompt_detailed_review_without_guidelines_with_diff(
            ContextLevel::Minimal,
            "sample diff",
            "",
            "",
        );
        // NOTE: The detailed_review template has been deprecated and now uses standard_review
        // The test should verify the new template behavior
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("CRITICAL CONSTRAINTS"));
    }

    #[test]
    fn test_prompt_for_agent_plan() {
        let template_context = TemplateContext::default();
        let result = prompt_for_agent(
            Role::Developer,
            Action::Plan,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new().with_prompt_md("test requirements".to_string()),
        );
        // Plan is now returned as structured output, not written to file
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("Implementation Steps"));
    }

    #[test]
    fn test_prompts_are_agent_agnostic() {
        // All prompts should be free of agent-specific references
        // to ensure they work with any AI coding assistant
        let agent_specific_terms = [
            "claude", "codex", "opencode", "gemini", "aider", "goose", "cline", "continue",
            "amazon-q", "gpt", "copilot",
        ];

        let prompts_to_check: Vec<String> = vec![
            prompt_developer_iteration(1, 5, ContextLevel::Normal, "", ""),
            prompt_developer_iteration(1, 5, ContextLevel::Minimal, "", ""),
            prompt_detailed_review_without_guidelines_with_diff(
                ContextLevel::Normal,
                "sample diff",
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff(
                ContextLevel::Minimal,
                "sample diff",
                "",
                "",
            ),
            prompt_fix("", "", ""),
            prompt_plan(None),
            prompt_generate_commit_message_with_diff("diff --git a/a b/b"),
        ];

        for prompt in prompts_to_check {
            let prompt_lower = prompt.to_lowercase();
            for term in agent_specific_terms {
                assert!(
                    !prompt_lower.contains(term),
                    "Prompt contains agent-specific term '{}': {}",
                    term,
                    &prompt[..prompt.len().min(100)]
                );
            }
        }
    }

    #[test]
    fn test_prompt_for_agent_fix() {
        let template_context = TemplateContext::default();
        let result = prompt_for_agent(
            Role::Developer,
            Action::Fix,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new().with_prompt_plan_and_issues(
                "test prompt".to_string(),
                "test plan".to_string(),
                "test issues".to_string(),
            ),
        );
        assert!(result.contains("FIX MODE"));
        assert!(result.contains("test issues"));
        // Should include PROMPT and PLAN context
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
    }

    #[test]
    fn test_prompt_for_agent_fix_with_empty_context() {
        let template_context = TemplateContext::default();
        let result = prompt_for_agent(
            Role::Developer,
            Action::Fix,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new(),
        );
        assert!(result.contains("FIX MODE"));
        // Should still work with empty context
        assert!(!result.is_empty());
    }

    #[test]
    fn test_reviewer_can_use_iterate_action() {
        // Edge case: Reviewer using Iterate action (fallback behavior)
        let template_context = TemplateContext::default();
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Iterate,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new()
                .with_iterations(1, 3)
                .with_prompt_and_plan(String::new(), String::new()),
        );
        // Should fall back to developer iteration prompt
        assert!(result.contains("IMPLEMENTATION MODE"));
    }

    #[test]
    fn test_prompts_do_not_have_detailed_tracking_language() {
        // Prompts should NOT contain detailed history tracking language
        // to prevent context contamination in future runs
        let detailed_tracking_terms = [
            "iteration number",
            "phase completed",
            "previous iteration",
            "history of",
            "detailed log",
        ];

        let prompts_to_check = vec![
            prompt_developer_iteration(1, 5, ContextLevel::Normal, "", ""),
            prompt_fix("", "", ""),
        ];

        for prompt in prompts_to_check {
            let prompt_lower = prompt.to_lowercase();
            for term in detailed_tracking_terms {
                assert!(
                    !prompt_lower.contains(term),
                    "Prompt contains detailed tracking language '{}': {}",
                    term,
                    &prompt[..prompt.len().min(100)]
                );
            }
        }
    }

    #[test]
    fn test_developer_notes_md_not_referenced() {
        // Developer prompt should NOT mention NOTES.md at all (isolation mode)
        let developer_prompt = prompt_developer_iteration(1, 5, ContextLevel::Normal, "", "");
        assert!(
            !developer_prompt.contains("NOTES.md"),
            "Developer prompt should not reference NOTES.md in isolation mode"
        );
    }

    #[test]
    fn test_all_prompts_isolate_agents_from_git() {
        // AC3: "AI agent does not know that we have previous committed change"
        // All prompts should NOT tell agents to run git commands
        // Git operations are handled by the orchestrator via libgit2

        // These patterns indicate the agent is being instructed to RUN git commands
        // We exclude patterns that are part of constraint lists (like "MUST NOT run X, Y, Z")
        let instructive_git_patterns = [
            "Run `git",
            "run git",
            "execute git",
            "Try: git",
            "you can git",
            "should run git",
            "please run git",
            "\ngit ", // Command starting at line beginning after newline
        ];

        // Context patterns that indicate the command is being FORBIDDEN, not instructed
        // These should be excluded from the check
        let forbid_contexts = [
            "MUST NOT run",
            "DO NOT run",
            "must not run",
            "do not run",
            "NOT run commands",
            "commands (",
            "commands:",
            "including:",
            "such as",
        ];

        // Special case: "Use git" is allowed in fix_mode.txt for fault tolerance
        // when issue descriptions lack file context - the fixer needs to find the relevant code
        // This is part of the recovery mechanism for vague issues

        let prompts_to_check: Vec<String> = vec![
            prompt_developer_iteration(1, 5, ContextLevel::Normal, "", ""),
            prompt_developer_iteration(1, 5, ContextLevel::Minimal, "", ""),
            prompt_detailed_review_without_guidelines_with_diff(
                ContextLevel::Normal,
                "sample diff",
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff(
                ContextLevel::Minimal,
                "sample diff",
                "",
                "",
            ),
            // Note: fix_mode.txt is intentionally excluded from "Use git" check
            // because it contains "Use git grep/rg ONLY when issue descriptions lack file context"
            // which is part of the fault tolerance design
            prompt_fix("", "", ""),
            prompt_plan(None),
            prompt_generate_commit_message_with_diff("diff --git a/a b/b\n"),
        ];

        for prompt in prompts_to_check {
            for pattern in instructive_git_patterns {
                if prompt.contains(pattern) {
                    // Check if this is in a "forbidden" context
                    let is_forbidden = forbid_contexts.iter().any(|ctx| {
                        if let Some(pos) = prompt.find(ctx) {
                            // Check if the pattern appears after the forbid context
                            if let Some(pattern_pos) = prompt[pos..].find(pattern) {
                                // Pattern is within reasonable proximity (200 chars) of forbid context
                                pattern_pos < 200
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    });

                    if !is_forbidden {
                        panic!(
                            "Prompt contains instructive git command pattern '{}': {}",
                            pattern,
                            &prompt[..prompt.len().min(150)]
                        );
                    }
                }
            }
        }

        // Verify the orchestrator-specific function for commit message generation
        // DOES contain the diff content (orchestrator receives diff, not git commands).
        // The orchestrator uses this function to pass diff to the LLM via stdin.
        let orchestrator_prompt = prompt_generate_commit_message_with_diff("some diff");
        assert!(
            orchestrator_prompt.contains("DIFF:") || orchestrator_prompt.contains("diff"),
            "Orchestrator prompt should contain the diff content for commit message generation"
        );
        // But the prompt should NOT tell the agent to run git commands (orchestrator handles git)
        for pattern in instructive_git_patterns {
            if orchestrator_prompt.contains(pattern) {
                // Check if this is in a "forbidden" context
                let is_forbidden = forbid_contexts.iter().any(|ctx| {
                    if let Some(pos) = orchestrator_prompt.find(ctx) {
                        if let Some(pattern_pos) = orchestrator_prompt[pos..].find(pattern) {
                            pattern_pos < 200
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });

                assert!(
                    is_forbidden,
                    "Orchestrator prompt contains instructive git command pattern '{pattern}'"
                );
            }
        }
    }

    #[test]
    fn test_prompt_with_resume_context() {
        let template_context = TemplateContext::default();
        let result = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new()
                .with_resume(true)
                .with_iterations(2, 5)
                .with_prompt_and_plan("test prompt".to_string(), "test plan".to_string()),
        );
        // Should include resume note
        assert!(result.contains("resuming from a previous run"));
        assert!(result.contains("git log"));
    }

    #[test]
    fn test_prompt_with_rich_resume_context_development() {
        use crate::checkpoint::state::{PipelinePhase, RebaseState};

        let template_context = TemplateContext::default();

        // Create a resume context for development phase
        let resume_context = ResumeContext {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 3,
            resume_count: 1,
            rebase_state: RebaseState::NotStarted,
            run_id: "test-run-id".to_string(),
            prompt_history: None,
            execution_history: None,
        };

        let result = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new()
                .with_resume_context(resume_context)
                .with_iterations(3, 5)
                .with_prompt_and_plan("test prompt".to_string(), "test plan".to_string()),
        );

        // Should include rich resume context
        assert!(result.contains("SESSION RESUME CONTEXT"));
        assert!(result.contains("DEVELOPMENT phase"));
        assert!(result.contains("iteration 3 of 5"));
        assert!(result.contains("has been resumed 1 time"));
        assert!(result.contains("Continue working on the implementation"));
    }

    #[test]
    fn test_prompt_with_rich_resume_context_review() {
        use crate::checkpoint::state::{PipelinePhase, RebaseState};

        let template_context = TemplateContext::default();

        // Create a resume context for review phase
        let resume_context = ResumeContext {
            phase: PipelinePhase::Review,
            iteration: 5,
            total_iterations: 5,
            reviewer_pass: 1,
            total_reviewer_passes: 3,
            resume_count: 2,
            rebase_state: RebaseState::NotStarted,
            run_id: "test-run-id".to_string(),
            prompt_history: None,
            execution_history: None,
        };

        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Fix,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new()
                .with_resume_context(resume_context)
                .with_prompt_plan_and_issues(
                    "test prompt".to_string(),
                    "test plan".to_string(),
                    "test issues".to_string(),
                ),
        );

        // Should include rich resume context for review
        assert!(result.contains("SESSION RESUME CONTEXT"));
        assert!(result.contains("REVIEW phase"));
        assert!(result.contains("pass 2 of 3"));
        assert!(result.contains("has been resumed 2 time"));
    }

    #[test]
    fn test_prompt_with_rich_resume_context_fix() {
        use crate::checkpoint::state::{PipelinePhase, RebaseState};

        let template_context = TemplateContext::default();

        // Create a resume context for fix phase
        let resume_context = ResumeContext {
            phase: PipelinePhase::Fix,
            iteration: 5,
            total_iterations: 5,
            reviewer_pass: 1,
            total_reviewer_passes: 3,
            resume_count: 0,
            rebase_state: RebaseState::NotStarted,
            run_id: "test-run-id".to_string(),
            prompt_history: None,
            execution_history: None,
        };

        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Fix,
            ContextLevel::Normal,
            &template_context,
            PromptConfig::new()
                .with_resume_context(resume_context)
                .with_prompt_plan_and_issues(
                    "test prompt".to_string(),
                    "test plan".to_string(),
                    "test issues".to_string(),
                ),
        );

        // Should include rich resume context for fix
        assert!(result.contains("SESSION RESUME CONTEXT"));
        assert!(result.contains("FIX phase"));
        assert!(result.contains("Focus on addressing the issues"));
    }

    #[test]
    fn test_get_stored_or_generate_prompt_replays_when_available() {
        let mut history = std::collections::HashMap::new();
        history.insert("test_key".to_string(), "stored prompt".to_string());

        let (prompt, was_replayed) =
            get_stored_or_generate_prompt("test_key", &history, || "generated prompt".to_string());

        assert_eq!(prompt, "stored prompt");
        assert!(was_replayed, "Should have replayed the stored prompt");
    }

    #[test]
    fn test_get_stored_or_generate_prompt_generates_when_not_available() {
        let history = std::collections::HashMap::new();

        let (prompt, was_replayed) = get_stored_or_generate_prompt("missing_key", &history, || {
            "generated prompt".to_string()
        });

        assert_eq!(prompt, "generated prompt");
        assert!(!was_replayed, "Should have generated a new prompt");
    }

    #[test]
    fn test_get_stored_or_generate_prompt_with_empty_history() {
        let history = std::collections::HashMap::new();

        let (prompt, was_replayed) =
            get_stored_or_generate_prompt("any_key", &history, || "fresh prompt".to_string());

        assert_eq!(prompt, "fresh prompt");
        assert!(
            !was_replayed,
            "Should have generated a new prompt for empty history"
        );
    }
}
