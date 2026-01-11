//! Prompt Templates Module
//!
//! Provides context-controlled prompts for agents.
//! Key design: reviewers get minimal context for "fresh eyes" perspective.

/// Context level for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextLevel {
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
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
pub(crate) fn prompt_developer_iteration(
    _iteration: u32,
    _total: u32,
    context: ContextLevel,
) -> String {
    match context {
        ContextLevel::Minimal | ContextLevel::Normal => {
            r#"You are in IMPLEMENTATION MODE. Execute the plan and make progress.

INPUTS TO READ:
1. .agent/PLAN.md - The implementation plan (execute these steps)
2. PROMPT.md - The original requirements (for reference)

YOUR TASK:
Execute the next steps from .agent/PLAN.md that haven't been completed yet.
Work toward satisfying all acceptance checks in PROMPT.md.

GUIDELINES:
- Make meaningful progress in each iteration
- Write clean, idiomatic code following project patterns
- Add tests where appropriate"#
                .to_string()
        }
    }
}

/// Generate reviewer review prompt with minimal context
/// Reviewer should NOT see what was done - just evaluate the code against requirements
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Follows best practices for unbiased code review:
/// - Fresh eyes perspective (minimal context mode)
/// - Output format is intentionally vague to avoid contaminating future runs
pub(crate) fn prompt_reviewer_review(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in REVIEW MODE with fresh eyes perspective.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)

YOUR TASK:
Evaluate the codebase against the requirements in PROMPT.md.

1. GOAL ALIGNMENT - Does the implementation achieve the stated goal?
2. ACCEPTANCE CHECKS - Verify each check in PROMPT.md passes or fails
3. CODE QUALITY - Check for bugs, error handling, tests, security issues

OUTPUT:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "No issues found." (if everything looks good), OR
- "Issues found." (if you have any concerns)
Do not include any details or additional lines."#
            .to_string(),
        ContextLevel::Normal => r#"You are in REVIEW MODE.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)

YOUR TASK:
Review the repository against PROMPT.md requirements.

1. Check goal alignment - does implementation achieve the stated goal?
2. Verify each acceptance check passes
3. Examine code quality (bugs, error handling, tests, security)

OUTPUT:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "No issues found." (if everything looks good), OR
- "Issues found." (if you have any concerns)
Do not include any details or additional lines."#
            .to_string(),
    }
}

/// Generate fix prompt (applies to either role)
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
pub(crate) fn prompt_fix() -> String {
    r#"You are in FIX MODE. Address issues found during review.

INPUTS TO READ:
- .agent/ISSUES.md - Issues to address (if it exists)
- PROMPT.md - Original requirements for context

YOUR TASK:
1. Read .agent/ISSUES.md to understand any issues (if it exists)
2. Fix issues found, prioritizing by severity
3. Verify fixes work

AFTER FIXING:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "Issues addressed." (if you believe everything is fixed), OR
- "Issues remain." (if you believe issues still exist)
If .agent/NOTES.md exists, OVERWRITE it with exactly ONE vague sentence (no details).

GUIDELINES:
- Fix issues properly, don't just suppress warnings
- Ensure fixes don't introduce new issues"#
        .to_string()
}

/// Generate reviewer re-review prompt with minimal context
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions are intentionally vague to avoid assumptions about previous
/// iterations and to prevent context contamination.
pub(crate) fn prompt_review_again(context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal => r#"You are in VERIFICATION MODE with fresh eyes.

INPUTS TO READ:
- PROMPT.md - The requirements (Goal and Acceptance checks)
- .agent/ISSUES.md - Previous issues (if it exists)

YOUR TASK:
1. Verify all acceptance checks in PROMPT.md are satisfied
2. Check current state of the codebase
3. Address any issues found

OUTPUT:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "No issues found." (if everything is satisfied), OR
- "Issues remain." (if something still fails)
If .agent/NOTES.md exists, OVERWRITE it with exactly ONE vague sentence (no details).

Be thorough but efficient - focus on verification."#
            .to_string(),
        ContextLevel::Normal => r#"You are in VERIFICATION MODE.

INPUTS TO READ:
- PROMPT.md - Requirements to verify against
- .agent/ISSUES.md - Previous issues (if it exists)

YOUR TASK:
Verify all acceptance checks pass.
If issues remain, fix them.

OUTPUT:
If .agent/ISSUES.md exists, OVERWRITE it with exactly ONE vague sentence:
- "No issues found." (if everything is satisfied), OR
- "Issues remain." (if something still fails)
Do not include any details or additional lines."#
            .to_string(),
    }
}

/// Generate prompt for planning phase
/// Agent does a deep dive on PROMPT.md and creates a detailed PLAN.md
///
/// This prompt is designed to be agent-agnostic and follows best practices
/// from Claude Code's plan mode implementation:
/// - Multi-phase workflow (Understanding → Exploration → Design → Review → Final Plan)
/// - Strict read-only constraints during planning
/// - Critical files identification (3-5 files with justifications)
/// - Verification strategy
/// - Clear exit criteria
///
/// Reference: <https://github.com/Piebald-AI/claude-code-system-prompts>
pub(crate) fn prompt_plan() -> String {
    r#"You are in PLANNING MODE. Create a detailed implementation plan.

CRITICAL: This is a READ-ONLY planning task. You are STRICTLY PROHIBITED from:
- Creating, modifying, or deleting any files (except .agent/PLAN.md)
- Running any commands that modify system state
- Making commits or staging changes
- Installing dependencies or packages

You MAY use read-only operations: reading files, searching code, listing directories,
running `git status`, `git log`, `git diff`, or similar read-only commands.

═══════════════════════════════════════════════════════════════════════════════
PHASE 1: UNDERSTANDING
═══════════════════════════════════════════════════════════════════════════════
Read PROMPT.md thoroughly to understand:
- The Goal: What is the desired end state?
- Acceptance Checks: What specific conditions must be satisfied?
- Constraints: Any requirements, limitations, or quality standards mentioned?

If requirements are ambiguous, note them for clarification in the plan.

═══════════════════════════════════════════════════════════════════════════════
PHASE 2: EXPLORATION
═══════════════════════════════════════════════════════════════════════════════
Explore the codebase using read-only tools to understand:
- Current architecture and patterns used
- Relevant existing code that will need changes
- Dependencies and potential impacts
- Similar implementations that can serve as reference
- Test patterns and coverage expectations

Be thorough. Quality exploration leads to better plans.

═══════════════════════════════════════════════════════════════════════════════
PHASE 3: DESIGN
═══════════════════════════════════════════════════════════════════════════════
Design your implementation approach:
- What changes need to be made and in what order?
- Are there multiple valid approaches? Evaluate trade-offs.
- What are the potential risks or challenges?
- How does this integrate with existing code patterns?

═══════════════════════════════════════════════════════════════════════════════
PHASE 4: REVIEW
═══════════════════════════════════════════════════════════════════════════════
Validate your plan against the requirements:
- Does the approach satisfy ALL acceptance checks in PROMPT.md?
- Are there edge cases or error scenarios to handle?
- Is the plan specific enough to implement without ambiguity?
- Have you identified the correct files to modify?

═══════════════════════════════════════════════════════════════════════════════
PHASE 5: WRITE PLAN
═══════════════════════════════════════════════════════════════════════════════
Create .agent/PLAN.md with this structure:

## Summary
One paragraph explaining what will be done and why.

## Implementation Steps
Numbered, actionable steps. Be specific about:
- What each step accomplishes
- Which files are affected
- Dependencies between steps

## Critical Files for Implementation
List 3-5 key files that will be created or modified:
- `path/to/file.rs` - Brief justification for why this file needs changes

## Risks & Mitigations
Challenges identified during exploration and how to handle them.

## Verification Strategy
How to verify acceptance checks are met:
- Specific tests to run
- Manual verification steps
- Success criteria

REMEMBER: Do NOT implement anything. Your only output is .agent/PLAN.md."#
        .to_string()
}

/// Generate prompt for agent to generate a commit message
/// Agent writes the commit message to .agent/commit-message.txt
/// NOTES.md reference is explicitly optional since it may not exist in isolation mode.
pub(crate) fn prompt_generate_commit_message() -> String {
    r#"Generate a commit message for all changes made.

FIRST, gather context:
1. Run `git diff HEAD` to see exactly what changed
2. Read PROMPT.md to understand the original goal
3. Optionally read .agent/NOTES.md for additional context (if it exists)

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
pub(crate) enum Role {
    Developer,
    Reviewer,
}

/// Action types for prompts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Plan,
    Iterate,
    Review,
    Fix,
    ReviewAgain,
    GenerateCommitMessage,
}

/// Generate a prompt for any agent type
pub(crate) fn prompt_for_agent(
    role: Role,
    action: Action,
    context: ContextLevel,
    iteration: Option<u32>,
    total_iterations: Option<u32>,
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
        (_, Action::ReviewAgain) => prompt_review_again(context),
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
    fn test_prompt_developer_iteration() {
        let result = prompt_developer_iteration(2, 5, ContextLevel::Normal);
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        assert!(result.contains("IMPLEMENTATION MODE"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_prompt_reviewer_review_fresh_eyes() {
        let result = prompt_reviewer_review(ContextLevel::Minimal);
        assert!(result.contains("fresh eyes"));
        assert!(result.contains("REVIEW MODE"));
        assert!(result.contains("ISSUES.md"));

        // Should have evaluation sections (now more vague)
        assert!(result.contains("GOAL ALIGNMENT"));
        assert!(result.contains("ACCEPTANCE CHECKS"));
        assert!(result.contains("CODE QUALITY"));

        // Should NOT have detailed priority guide (vague prompts)
        assert!(!result.contains("Priority Guide"));
        // Should NOT reference STATUS.md (isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_prompt_reviewer_review_normal() {
        let result = prompt_reviewer_review(ContextLevel::Normal);
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("REVIEW MODE"));
        assert!(!result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_fix() {
        let result = prompt_fix();
        assert!(result.contains("ISSUES.md"));
        // NOTES.md/ISSUES.md should be constrained to vague overwrite semantics
        assert!(result.contains("OVERWRITE"));
        assert!(result.contains("exactly ONE vague sentence"));
        assert!(result.contains("FIX MODE"));
    }

    #[test]
    fn test_prompt_review_again_fresh_eyes() {
        let result = prompt_review_again(ContextLevel::Minimal);
        assert!(result.contains("fresh eyes"));
        // Removed detailed assumptions about previous iterations (vague prompts)
        assert!(!result.contains("DO NOT assume"));
        assert!(result.contains("VERIFICATION MODE"));
    }

    #[test]
    fn test_prompt_for_agent_developer() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            ContextLevel::Normal,
            Some(3),
            Some(10),
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
        );
        assert!(result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_plan() {
        let result = prompt_plan();
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("Implementation Steps"));
        assert!(result.contains("Critical Files"));
        assert!(result.contains("Verification Strategy"));

        // Ensure strict read-only constraints are present (Claude Code alignment)
        assert!(result.contains("READ-ONLY"));
        assert!(result.contains("STRICTLY PROHIBITED"));

        // Ensure 5-phase workflow structure (Claude Code alignment)
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        assert!(result.contains("PHASE 2: EXPLORATION"));
        assert!(result.contains("PHASE 3: DESIGN"));
        assert!(result.contains("PHASE 4: REVIEW"));
        assert!(result.contains("PHASE 5: WRITE PLAN"));
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
            prompt_review_again(ContextLevel::Normal),
            prompt_review_again(ContextLevel::Minimal),
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
    fn test_context_level_from_u8() {
        assert_eq!(ContextLevel::from(0), ContextLevel::Minimal);
        assert_eq!(ContextLevel::from(1), ContextLevel::Normal);
        assert_eq!(ContextLevel::from(2), ContextLevel::Normal);
        assert_eq!(ContextLevel::from(255), ContextLevel::Normal);
    }

    #[test]
    fn test_developer_iteration_minimal_context() {
        let result = prompt_developer_iteration(1, 5, ContextLevel::Minimal);
        // Minimal context should include essential files (not STATUS.md in isolation mode)
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_review_again_normal_context() {
        let result = prompt_review_again(ContextLevel::Normal);
        assert!(result.contains("VERIFICATION MODE"));
        assert!(result.contains("PROMPT.md"));
        assert!(result.contains("ISSUES.md"));
        // Normal context doesn't need "fresh eyes" restriction
        assert!(!result.contains("fresh eyes"));
    }

    #[test]
    fn test_prompt_for_agent_fix() {
        let result = prompt_for_agent(
            Role::Developer,
            Action::Fix,
            ContextLevel::Normal,
            None,
            None,
        );
        assert!(result.contains("FIX MODE"));
        assert!(result.contains("ISSUES.md"));
    }

    #[test]
    fn test_prompt_for_agent_review_again() {
        let result = prompt_for_agent(
            Role::Reviewer,
            Action::ReviewAgain,
            ContextLevel::Minimal,
            None,
            None,
        );
        assert!(result.contains("VERIFICATION MODE"));
        assert!(result.contains("fresh eyes"));
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
        );
        // Should fall back to developer iteration prompt
        assert!(result.contains("IMPLEMENTATION MODE"));
    }

    // =========================================================================
    // Vague Prompt Tests (Context Contamination Prevention)
    // =========================================================================

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
            prompt_review_again(ContextLevel::Normal),
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
    fn test_reviewer_review_is_vague() {
        // Reviewer review prompt should NOT have detailed priority levels
        let result = prompt_reviewer_review(ContextLevel::Minimal);

        // Should NOT have structured priority format
        assert!(!result.contains("Priority Guide"));
        assert!(!result.contains("- [ ] Critical:"));
        assert!(!result.contains("- [ ] High:"));
        assert!(!result.contains("[file:line]"));
    }

    #[test]
    fn test_notes_md_references_are_minimal_or_absent() {
        // NOTES.md references should be minimal or absent (isolation mode removes these files)
        let developer_prompt = prompt_developer_iteration(1, 5, ContextLevel::Normal);
        let fix_prompt = prompt_fix();
        let review_again_prompt = prompt_review_again(ContextLevel::Minimal);

        // Developer prompt should NOT mention NOTES.md at all (isolation mode)
        assert!(
            !developer_prompt.contains("NOTES.md"),
            "Developer prompt should not reference NOTES.md in isolation mode"
        );

        // Fix and review-again prompts may have optional language or no reference
        // They use "(if it exists)" when they do reference NOTES.md
        if fix_prompt.contains("NOTES.md") {
            assert!(
                fix_prompt.contains("if it exists") || fix_prompt.contains("Optionally"),
                "Fix prompt NOTES.md reference should be optional"
            );
        }
        if review_again_prompt.contains("NOTES.md") {
            assert!(
                review_again_prompt.contains("if it exists")
                    || review_again_prompt.contains("Optionally"),
                "Review again prompt NOTES.md reference should be optional"
            );
        }
    }
}
