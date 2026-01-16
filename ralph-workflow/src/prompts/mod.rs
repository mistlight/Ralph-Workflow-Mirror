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

mod commit;
mod developer;
mod rebase;
mod reviewer;
mod template_engine;
mod types;

// Re-export all public items for backward compatibility
pub use commit::{
    prompt_emergency_commit, prompt_emergency_no_diff_commit, prompt_file_list_only_commit,
    prompt_file_list_summary_only_commit, prompt_fix, prompt_generate_commit_message_with_diff,
    prompt_strict_json_commit, prompt_strict_json_commit_v2, prompt_ultra_minimal_commit,
    prompt_ultra_minimal_commit_v2,
};
pub use developer::{prompt_developer_iteration, prompt_plan};
pub use rebase::{build_conflict_resolution_prompt, collect_conflict_info, FileConflict};
pub use reviewer::{
    prompt_comprehensive_review_with_diff, prompt_detailed_review_without_guidelines_with_diff,
    prompt_incremental_review_with_diff, prompt_reviewer_review_with_guidelines_and_diff,
    prompt_security_focused_review_with_diff, prompt_universal_review_with_diff,
};
pub use template_engine::Template;
pub use types::{Action, ContextLevel, Role};

/// Configuration for prompt generation.
///
/// Groups related parameters to reduce function argument count.
#[derive(Debug, Clone, Default)]
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
}

impl PromptConfig {
    /// Create a new prompt configuration with default values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            iteration: None,
            total_iterations: None,
            prompt_md_content: None,
            prompt_and_plan: None,
            prompt_plan_and_issues: None,
        }
    }

    /// Set iteration numbers for developer iteration prompts.
    #[must_use]
    pub const fn with_iterations(mut self, iteration: u32, total: u32) -> Self {
        self.iteration = Some(iteration);
        self.total_iterations = Some(total);
        self
    }

    /// Set PROMPT.md content for planning prompts.
    #[must_use]
    pub fn with_prompt_md(mut self, content: String) -> Self {
        self.prompt_md_content = Some(content);
        self
    }

    /// Set (PROMPT.md, PLAN.md) content tuple for developer iteration prompts.
    #[must_use]
    pub fn with_prompt_and_plan(mut self, prompt: String, plan: String) -> Self {
        self.prompt_and_plan = Some((prompt, plan));
        self
    }

    /// Set (PROMPT.md, PLAN.md, ISSUES.md) content tuple for fix prompts.
    #[must_use]
    pub fn with_prompt_plan_and_issues(
        mut self,
        prompt: String,
        plan: String,
        issues: String,
    ) -> Self {
        self.prompt_plan_and_issues = Some((prompt, plan, issues));
        self
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
pub fn prompt_for_agent(
    role: Role,
    action: Action,
    context: ContextLevel,
    config: PromptConfig,
) -> String {
    match (role, action) {
        (_, Action::Plan) => prompt_plan(config.prompt_md_content.as_deref()),
        (Role::Developer | Role::Reviewer, Action::Iterate) => {
            let (prompt_content, plan_content) = config
                .prompt_and_plan
                .unwrap_or((String::new(), String::new()));
            prompt_developer_iteration(
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
            prompt_fix(&prompt_content, &plan_content, &issues_content)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_for_agent_developer() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            ContextLevel::Normal,
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
        assert!(result.contains("fresh eyes"));
        assert!(result.contains("DETAILED REVIEW MODE"));
    }

    #[test]
    fn test_prompt_for_agent_plan() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Plan,
            ContextLevel::Normal,
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

        let prompts_to_check = vec![
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
        let result = prompt_for_agent(
            Role::Developer,
            Action::Fix,
            ContextLevel::Normal,
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
        let result = prompt_for_agent(
            Role::Developer,
            Action::Fix,
            ContextLevel::Normal,
            PromptConfig::new(),
        );
        assert!(result.contains("FIX MODE"));
        // Should still work with empty context
        assert!(!result.is_empty());
    }

    #[test]
    fn test_reviewer_can_use_iterate_action() {
        // Edge case: Reviewer using Iterate action (fallback behavior)
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Iterate,
            ContextLevel::Normal,
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
        let git_command_patterns = [
            "git diff HEAD",
            "git status",
            "git commit",
            "git add",
            "git log",
            "git show",
            "git reset",
            "git checkout",
            "git branch",
            "Run `git",
            "execute git",
        ];

        let prompts_to_check = vec![
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
            prompt_generate_commit_message_with_diff("diff --git a/a b/b\n"),
        ];

        for prompt in prompts_to_check {
            for pattern in git_command_patterns {
                assert!(
                    !prompt.contains(pattern),
                    "Prompt contains git command pattern '{}': {}",
                    pattern,
                    &prompt[..prompt.len().min(100)]
                );
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
        for pattern in git_command_patterns {
            assert!(
                !orchestrator_prompt.contains(pattern),
                "Orchestrator prompt contains git command pattern '{pattern}'"
            );
        }
    }
}
