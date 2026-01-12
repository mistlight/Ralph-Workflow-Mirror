//! Prompt Templates Module
//!
//! Provides context-controlled prompts for agents.
//! Key design: reviewers get minimal context for "fresh eyes" perspective.
//!
//! Enhanced with language-specific review guidelines based on detected project stack.
//!
//! # Module Structure
//!
//! - [`types`] - Type definitions (ContextLevel, Role, Action)
//! - [`developer`] - Developer prompts (iteration, planning)
//! - [`reviewer`] - Reviewer prompts (review, comprehensive, security, incremental)
//! - [`commit`] - Fix and commit message prompts

mod commit;
mod developer;
mod reviewer;
mod types;

// Re-export all public items for backward compatibility
pub(crate) use commit::{prompt_fix, prompt_generate_commit_message};
pub(crate) use developer::{prompt_developer_iteration, prompt_plan};
pub(crate) use reviewer::{
    prompt_comprehensive_review, prompt_detailed_review_without_guidelines,
    prompt_incremental_review, prompt_reviewer_review, prompt_reviewer_review_with_guidelines,
    prompt_security_focused_review,
};
pub(crate) use types::{Action, ContextLevel, Role};

use crate::guidelines::ReviewGuidelines;

/// Generate a prompt for any agent type.
///
/// This is the main dispatcher function that routes to the appropriate
/// prompt generator based on role and action.
///
/// The optional `guidelines` parameter allows providing language-specific review
/// guidance when the project stack has been detected. When provided, review prompts
/// will include tailored checks for the detected language and frameworks.
pub(crate) fn prompt_for_agent(
    role: Role,
    action: Action,
    context: ContextLevel,
    iteration: Option<u32>,
    total_iterations: Option<u32>,
    guidelines: Option<&ReviewGuidelines>,
) -> String {
    match (role, action) {
        (_, Action::Plan) => prompt_plan(),
        (Role::Developer, Action::Iterate) => prompt_developer_iteration(
            iteration.unwrap_or(1),
            total_iterations.unwrap_or(1),
            context,
        ),
        (Role::Reviewer, Action::Review) => {
            // Use guidelines-enhanced prompt if guidelines are available
            if let Some(g) = guidelines {
                prompt_reviewer_review_with_guidelines(context, g)
            } else {
                prompt_reviewer_review(context)
            }
        }
        (_, Action::Fix) => prompt_fix(),
        (_, Action::GenerateCommitMessage) => prompt_generate_commit_message(),
        // Fallback for Reviewer + Iterate (shouldn't happen but be safe)
        (Role::Reviewer, Action::Iterate) => prompt_developer_iteration(
            iteration.unwrap_or(1),
            total_iterations.unwrap_or(1),
            context,
        ),
        // Fallback for Developer + Review (shouldn't happen but be safe)
        (Role::Developer, Action::Review) => prompt_reviewer_review(context),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_detector::ProjectStack;

    #[test]
    fn test_prompt_for_agent_developer() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            ContextLevel::Normal,
            Some(3),
            Some(10),
            None,
        );
        assert!(result.contains("PROMPT.md"));
    }

    #[test]
    fn test_prompt_for_agent_reviewer() {
        let result = prompt_reviewer_review(ContextLevel::Minimal);
        assert!(result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_for_agent_plan() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Plan,
            ContextLevel::Normal,
            None,
            None,
            None,
        );
        assert!(result.contains("PLAN.md"));
    }

    #[test]
    fn test_prompt_for_agent_generate_commit_message() {
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::GenerateCommitMessage,
            ContextLevel::Normal,
            None,
            None,
            None,
        );
        assert!(result.contains("commit-message.txt"));
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
            prompt_developer_iteration(1, 5, ContextLevel::Normal),
            prompt_developer_iteration(1, 5, ContextLevel::Minimal),
            prompt_reviewer_review(ContextLevel::Normal),
            prompt_reviewer_review(ContextLevel::Minimal),
            prompt_fix(),
            prompt_plan(),
            prompt_generate_commit_message(),
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
            None,
            None,
            None,
        );
        assert!(result.contains("FIX MODE"));
        assert!(result.contains("ISSUES.md"));
    }

    #[test]
    fn test_reviewer_can_use_iterate_action() {
        // Edge case: Reviewer using Iterate action (fallback behavior)
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Iterate,
            ContextLevel::Normal,
            Some(1),
            Some(3),
            None,
        );
        // Should fall back to developer iteration prompt
        assert!(result.contains("IMPLEMENTATION MODE"));
    }

    #[test]
    fn test_prompt_for_agent_with_guidelines() {
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Django".to_string()],
            has_tests: true,
            test_framework: Some("pytest".to_string()),
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Review,
            ContextLevel::Minimal,
            None,
            None,
            Some(&guidelines),
        );

        // Should use the enhanced prompt with guidelines
        assert!(result.contains("Language-Specific"));
        assert!(result.contains("SECURITY"));
    }

    #[test]
    fn test_prompt_for_agent_without_guidelines() {
        // When no guidelines are provided, should use the standard prompt
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Review,
            ContextLevel::Minimal,
            None,
            None,
            None,
        );

        // Should use standard prompt
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("fresh eyes"));
        // Should NOT contain language-specific section header
        assert!(!result.contains("Language-Specific"));
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
            prompt_developer_iteration(1, 5, ContextLevel::Normal),
            prompt_fix(),
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
        let developer_prompt = prompt_developer_iteration(1, 5, ContextLevel::Normal);
        assert!(
            !developer_prompt.contains("NOTES.md"),
            "Developer prompt should not reference NOTES.md in isolation mode"
        );
    }
}
