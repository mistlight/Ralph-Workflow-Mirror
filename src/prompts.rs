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

/// Generate commit prompt for reviewer (DEPRECATED: prefer prompt_generate_commit_message)
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

/// Generate prompt for planning phase
/// Agent does a deep dive on PROMPT.md and creates a detailed PLAN.md
pub fn prompt_plan() -> String {
    r#"Do a deep dive on PROMPT.md to understand the Goal and Acceptance checks.

Create .agent/PLAN.md with:
1. A clear understanding of what needs to be done
2. Step-by-step implementation plan
3. Identified risks or challenges
4. Specific files that will need changes
5. Testing strategy to verify acceptance checks

Be thorough but concise. This plan will guide the implementation."#
        .to_string()
}

/// Generate prompt for agent to generate a commit message
/// Agent writes the commit message to .agent/commit-message.txt
pub fn prompt_generate_commit_message() -> String {
    r#"Generate a commit message for all changes made.

FIRST, gather context:
1. Run `git diff HEAD` to see exactly what changed
2. Read PROMPT.md to understand the original goal
3. Read .agent/NOTES.md for a summary of work done (if it exists)

THEN: Write a Conventional Commits message to .agent/commit-message.txt

FORMAT:
<type>[optional scope][!]: <subject>

[optional body]

[optional footer]

RULES:
- type: feat|fix|docs|refactor|test|chore|perf|build|ci (required)
- scope: area affected in parentheses, e.g., feat(parser): (optional)
- !: add before colon for breaking changes, e.g., feat!: or feat(api)!:
- subject: imperative mood ("add" not "added"), lowercase, no period, max 50 chars
- body: wrap at 72 chars, explain what/why not how (optional, for complex changes)
- footer: BREAKING CHANGE: description, or Fixes #123, Refs #456 (optional)

GOOD EXAMPLES:
feat(auth): add OAuth2 login flow
fix: prevent null pointer in user lookup
refactor(api): extract validation into middleware

feat!: drop Python 3.7 support

BREAKING CHANGE: Minimum Python version is now 3.8.

feat: add CSV export for reports

Add ability to export analytics reports as CSV files.
Supports filtering by date range and custom column selection.

Fixes #42

BAD EXAMPLES (avoid these patterns):
- "chore: apply changes" (too vague - what changes?)
- "chore: update code" (meaningless)
- "Updated the code" (no type, not imperative)
- "feat: Add new feature." (capitalized, has period, vague)

Write ONLY the commit message to .agent/commit-message.txt (no markdown fences, no extra text)."#
        .to_string()
}

/// Role types for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Developer,
    Reviewer,
}

/// Action types for prompts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Action {
    Plan,
    Iterate,
    Review,
    Fix,
    ReviewAgain,
    Commit,
    GenerateCommitMessage,
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
        (_, Action::Plan) => prompt_plan(),
        (Role::Developer, Action::Iterate) => prompt_developer_iteration(
            iteration.unwrap_or(1),
            total_iterations.unwrap_or(1),
            context,
        ),
        (_, Action::Review) => prompt_reviewer_review(context),
        (_, Action::Fix) => prompt_fix(),
        (_, Action::ReviewAgain) => prompt_codex_review_again(context),
        (_, Action::Commit) => prompt_commit(commit_msg.unwrap_or("chore: apply changes")),
        (_, Action::GenerateCommitMessage) => prompt_generate_commit_message(),
        // Fallback for Reviewer + Iterate (shouldn't happen but be safe)
        (Role::Reviewer, Action::Iterate) => prompt_developer_iteration(
            iteration.unwrap_or(1),
            total_iterations.unwrap_or(1),
            context,
        ),
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

    #[test]
    fn test_prompt_plan() {
        let result = prompt_plan();
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        assert!(result.contains("implementation plan"));
    }

    #[test]
    fn test_prompt_generate_commit_message() {
        let result = prompt_generate_commit_message();
        // Basic structure
        assert!(result.contains("commit-message.txt"));
        assert!(result.contains("Conventional Commits"));

        // Context gathering instructions
        assert!(result.contains("git diff HEAD"));
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("NOTES.md"));

        // Type prefixes
        assert!(result.contains("feat"));
        assert!(result.contains("fix"));
        assert!(result.contains("docs"));
        assert!(result.contains("refactor"));
        assert!(result.contains("test"));
        assert!(result.contains("chore"));
        assert!(result.contains("perf"));

        // Scope support
        assert!(result.contains("scope"));
        assert!(result.contains("feat(parser):"));

        // Breaking change notation
        assert!(result.contains("!:"));
        assert!(result.contains("BREAKING CHANGE"));

        // Imperative mood guidance
        assert!(result.contains("imperative"));
        assert!(result.contains("\"add\" not \"added\""));

        // Character limits
        assert!(result.contains("max 50 chars"));
        assert!(result.contains("72 chars"));

        // Issue references
        assert!(result.contains("Fixes #"));

        // Good examples
        assert!(result.contains("feat(auth): add OAuth2 login flow"));
        assert!(result.contains("fix: prevent null pointer"));

        // Bad examples (anti-patterns to avoid)
        assert!(result.contains("BAD EXAMPLES"));
        assert!(result.contains("chore: apply changes"));
        assert!(result.contains("too vague"));
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
}
