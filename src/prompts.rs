//! Prompt Templates Module
//!
//! Provides context-controlled prompts for agents.
//! Key design: reviewers get minimal context for "fresh eyes" perspective.

/// Context level for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextLevel {
    /// Minimal context (fresh eyes) - only essential info
    Minimal = 0,
    /// Normal context - includes status information
    Normal = 1,
}

impl From<u8> for ContextLevel {
    fn from(v: u8) -> Self {
        if v == 0 {
            ContextLevel::Minimal
        } else {
            ContextLevel::Normal
        }
    }
}

/// Generate developer iteration prompt
/// Note: We do NOT tell the agent how many total iterations exist.
/// This prevents "context pollution" - the agent should complete their task fully
/// without knowing when the loop ends.
pub fn prompt_developer_iteration(_iteration: u32, _total: u32, context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal | ContextLevel::Normal => r#"Read PROMPT.md and .agent/STATUS.md.
Work toward PROMPT.md's Goal and Acceptance checks until all are satisfied.
Update .agent/STATUS.md (last action, blockers, next action).
Append brief bullets to .agent/NOTES.md."#
            .to_string(),
    }
}

/// Generate reviewer review prompt with minimal context
/// Reviewer should NOT see what was done - just evaluate the code against requirements
pub fn prompt_reviewer_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are reviewing this repository with fresh eyes.

Read ONLY PROMPT.md to understand the Goal and Acceptance checks.
DO NOT read .agent/STATUS.md or .agent/NOTES.md - you need an unbiased perspective.

Evaluate the codebase against the requirements:
1. Does the code meet the Goal?
2. Do all Acceptance checks pass?
3. Are there quality issues (bugs, code smells, missing tests)?

Write your findings into .agent/ISSUES.md as a prioritized checklist.
Be specific about file paths and line numbers."#
            .to_string(),
        ContextLevel::Normal => {
            r#"Review the repository against PROMPT.md (Goal + Acceptance checks).
Write findings into .agent/ISSUES.md as a prioritized checklist."#
                .to_string()
        }
    }
}

/// Generate fix prompt (applies to either role)
pub fn prompt_fix() -> String {
    r#"Fix everything in .agent/ISSUES.md.
Update .agent/ISSUES.md to mark items resolved.
Append brief bullets to .agent/NOTES.md."#
        .to_string()
}

/// Generate reviewer re-review prompt with minimal context
pub fn prompt_codex_review_again(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"Re-review the repository with fresh eyes.

Read ONLY PROMPT.md to verify the Goal and Acceptance checks are met.
DO NOT assume previous issues were fixed - verify independently.

If issues remain:
1. Fix them directly
2. Update .agent/ISSUES.md with what was found and fixed

Be thorough but efficient."#
            .to_string(),
        ContextLevel::Normal => r#"Re-review the repository after fixes against PROMPT.md.
If issues remain, fix them and update .agent/ISSUES.md."#
            .to_string(),
    }
}

/// Generate commit prompt for reviewer
pub fn prompt_commit(message: &str) -> String {
    format!(
        r#"All work is complete. Create a git commit with all changes.

Run:
  git add -A
  git commit -m "{}"

If commit hooks fail, fix the issues and try again.
Report success or failure."#,
        message
    )
}

/// Role types for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Developer,
    Reviewer,
}

/// Action types for prompts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Iterate,
    Review,
    Fix,
    ReviewAgain,
    Commit,
}

/// Generate a prompt for any agent type
pub fn prompt_for_agent(
    role: Role,
    action: Action,
    context: ContextLevel,
    iteration: Option<u32>,
    total_iterations: Option<u32>,
    commit_msg: Option<&str>,
) -> String {
    match (role, action) {
        (Role::Developer, Action::Iterate) => prompt_developer_iteration(
            iteration.unwrap_or(1),
            total_iterations.unwrap_or(1),
            context,
        ),
        (Role::Reviewer, Action::Review) => prompt_reviewer_review(context),
        (Role::Reviewer, Action::Fix) | (Role::Developer, Action::Fix) => prompt_fix(),
        (Role::Reviewer, Action::ReviewAgain) => prompt_codex_review_again(context),
        (_, Action::Commit) => prompt_commit(commit_msg.unwrap_or("chore: apply changes")),
        _ => "Unknown prompt action".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_claude_iteration() {
        let result = prompt_developer_iteration(2, 5, ContextLevel::Normal);
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("STATUS.md"));
        assert!(result.contains("until all are satisfied"));
    }

    #[test]
    fn test_prompt_codex_review_fresh_eyes() {
        let result = prompt_reviewer_review(ContextLevel::Minimal);
        assert!(result.contains("fresh eyes"));
        assert!(result.contains("DO NOT read"));
    }

    #[test]
    fn test_prompt_codex_review_normal() {
        let result = prompt_reviewer_review(ContextLevel::Normal);
        assert!(result.contains("PROMPT.md"));
        assert!(!result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_codex_fix() {
        let result = prompt_fix();
        assert!(result.contains("ISSUES.md"));
        assert!(result.contains("NOTES.md"));
    }

    #[test]
    fn test_prompt_codex_review_again_fresh_eyes() {
        let result = prompt_codex_review_again(ContextLevel::Minimal);
        assert!(result.contains("fresh eyes"));
        assert!(result.contains("DO NOT assume"));
    }

    #[test]
    fn test_prompt_commit() {
        let result = prompt_commit("feat: test commit");
        assert!(result.contains("git add -A"));
        assert!(result.contains("git commit"));
        assert!(result.contains("feat: test commit"));
    }

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
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::Review,
            ContextLevel::Minimal,
            None,
            None,
            None,
        );
        assert!(result.contains("fresh eyes"));
    }
}
