//! Developer prompts.
//!
//! Prompts for developer agent actions including iteration and planning.

use super::types::ContextLevel;

/// Generate developer iteration prompt.
///
/// Note: We do NOT tell the agent how many total iterations exist.
/// This prevents "context pollution" - the agent should complete their task fully
/// without knowing when the loop ends.
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Instructions for NOTES.md are intentionally vague to avoid creating
/// overly-specific context that could contaminate future runs.
pub fn prompt_developer_iteration(_iteration: u32, _total: u32, context: ContextLevel) -> String {
    match context {
        ContextLevel::Minimal | ContextLevel::Normal => {
            r#"You are in IMPLEMENTATION MODE. Execute the plan and make progress.

INPUTS TO READ:
1. .agent/PLAN.md - The implementation plan (execute these steps)

YOUR TASK:
Execute the next steps from .agent/PLAN.md that haven't been completed yet.

GUIDELINES:
- Make meaningful progress in each iteration
- Write clean, idiomatic code following project patterns
- Add tests where appropriate"#
                .to_string()
        }
    }
}

/// Generate prompt for planning phase.
///
/// The orchestrator provides requirements via the planning task context.
/// The plan content is returned as structured output (captured by JSON parser)
/// and the orchestrator writes it to .agent/PLAN.md.
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
///
/// # Arguments
///
/// * `prompt_content` - Optional PROMPT.md content to include directly in the prompt.
///   When provided, the agent doesn't need to discover PROMPT.md through file exploration,
///   which prevents accidental deletion.
pub fn prompt_plan(prompt_content: Option<&str>) -> String {
    let mut prompt = r#"You are in PLANNING MODE. Create a detailed implementation plan.

CRITICAL: This is a READ-ONLY planning task. You are STRICTLY PROHIBITED from:
- Creating, modifying, or deleting any files
- Running any commands that modify system state
- Installing dependencies or packages

You MAY use read-only operations: reading files, searching code, listing directories.

═══════════════════════════════════════════════════════════════════════════════
PHASE 1: UNDERSTANDING
═══════════════════════════════════════════════════════════════════════════════"#
        .to_string();

    // If prompt content is provided, include it directly in the prompt
    // without naming the source file. This prevents agents from discovering
    // the file through exploration, reducing the risk of accidental deletion.
    if let Some(content) = prompt_content {
        prompt.push_str(&format!(
            r#"

REQUIREMENTS FROM PROJECT TASK:
───────────────────────────────────────────────────────────────────────────────
{}
───────────────────────────────────────────────────────────────────────────────
"#,
            content
        ));
    } else {
        prompt.push_str(
            r#"

The orchestrator has provided requirements to you via the planning task.
"#,
        );
    }

    prompt.push_str(
        r#"
Understand:
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
- Does the approach satisfy ALL acceptance checks?
- Are there edge cases or error scenarios to handle?
- Is the plan specific enough to implement without ambiguity?
- Have you identified the correct files to modify?

═══════════════════════════════════════════════════════════════════════════════
PHASE 5: WRITE PLAN
═══════════════════════════════════════════════════════════════════════════════
OUTPUT your plan with this exact structure:

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

CRITICAL OUTPUT INSTRUCTIONS:
- Output your COMPLETE plan above as a single response
- Ensure ALL sections (Summary, Implementation Steps, Critical Files, Risks & Mitigations, Verification Strategy) are included
- Do NOT truncate or shorten your plan
- Do NOT write to any files"#,
    );

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_developer_iteration() {
        let result = prompt_developer_iteration(2, 5, ContextLevel::Normal);
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        assert!(result.contains("IMPLEMENTATION MODE"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_developer_iteration_minimal_context() {
        let result = prompt_developer_iteration(1, 5, ContextLevel::Minimal);
        // Minimal context should include essential files (not STATUS.md in isolation mode)
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(result.contains("PLAN.md"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_prompt_plan() {
        let result = prompt_plan(None);
        // Prompt should NOT explicitly mention PROMPT.md file name
        // Agents receive content directly without knowing the source file
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("NEVER read, write, or delete this file"));
        // Plan is now returned as structured output, not written to file
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
    fn test_prompt_plan_with_content() {
        let prompt_md = "# Test Prompt\n\nThis is the content.";
        let result = prompt_plan(Some(prompt_md));
        // Should include the content WITHOUT naming PROMPT.md
        assert!(result.contains("REQUIREMENTS FROM PROJECT TASK:"));
        assert!(result.contains("This is the content."));
        // Should NOT mention PROMPT.md file name
        assert!(!result.contains("PROMPT.md"));
        // Should still have the planning structure
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
    }

    #[test]
    fn all_developer_prompts_isolate_agents_from_git() {
        // Verify developer prompts don't tell agents to run git commands
        let prompts = vec![
            prompt_developer_iteration(1, 3, ContextLevel::Minimal),
            prompt_developer_iteration(2, 3, ContextLevel::Normal),
            prompt_plan(None),
        ];

        for prompt in prompts {
            assert!(
                !prompt.contains("git diff"),
                "Developer prompt should not tell agent to run git diff"
            );
            assert!(
                !prompt.contains("git status"),
                "Developer prompt should not tell agent to run git status"
            );
            assert!(
                !prompt.contains("git commit"),
                "Developer prompt should not tell agent to run git commit"
            );
            assert!(
                !prompt.contains("git add"),
                "Developer prompt should not tell agent to run git add"
            );
        }
    }
}
