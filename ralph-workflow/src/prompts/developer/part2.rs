// Tests for developer prompts (part 1 of test module)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_developer_iteration() {
        let result =
            prompt_developer_iteration(2, 5, ContextLevel::Normal, "test prompt", "test plan");
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(result.contains("IMPLEMENTATION MODE"));
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
        // STATUS.md should NOT be referenced (vague prompts, isolation mode)
        assert!(!result.contains("STATUS.md"));
    }

    #[test]
    fn test_developer_iteration_minimal_context() {
        let result =
            prompt_developer_iteration(1, 5, ContextLevel::Minimal, "test prompt", "test plan");
        // Minimal context should include essential files (not STATUS.md in isolation mode)
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
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
        // Plan is now returned as XML output format
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("<ralph-implementation-steps>"));
        assert!(result.contains("<ralph-critical-files>"));
        assert!(result.contains("<ralph-verification-strategy>"));

        // Ensure strict read-only constraints are present (Claude Code alignment)
        assert!(result.contains("READ-ONLY"));
        assert!(result.contains("STRICTLY PROHIBITED"));

        // Ensure 5-phase workflow structure (Claude Code alignment)
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        assert!(result.contains("PHASE 2: EXPLORATION"));
        assert!(result.contains("PHASE 3: DESIGN"));
        assert!(result.contains("PHASE 4: REVIEW"));
        assert!(result.contains("PHASE 5: WRITE STRUCTURED PLAN"));

        // Ensure XML output format is specified
        assert!(result.contains("<ralph-plan>"));
        assert!(result.contains("<ralph-summary>"));
    }

    #[test]
    fn test_prompt_plan_with_content() {
        let prompt_md = "# Test Prompt\n\nThis is the content.";
        let result = prompt_plan(Some(prompt_md));
        // Should include the content WITHOUT naming PROMPT.md
        assert!(result.contains("USER REQUIREMENTS:"));
        assert!(result.contains("This is the content."));
        // Should NOT mention PROMPT.md file name
        assert!(!result.contains("PROMPT.md"));
        // Should still have the planning structure
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        // Should have XML output format
        assert!(result.contains("<ralph-plan>"));
    }

    #[test]
    fn all_developer_prompts_isolate_agents_from_git() {
        // Verify developer prompts don't tell agents to run git commands
        let prompts = vec![
            prompt_developer_iteration(1, 3, ContextLevel::Minimal, "", ""),
            prompt_developer_iteration(2, 3, ContextLevel::Normal, "", ""),
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

    #[test]
    fn test_prompt_developer_iteration_with_context() {
        let context = TemplateContext::default();
        let result = prompt_developer_iteration_with_context(
            &context,
            2,
            5,
            ContextLevel::Normal,
            "test prompt",
            "test plan",
        );
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(result.contains("IMPLEMENTATION MODE"));
        // Agent should NOT be told to read PROMPT.md (orchestrator handles it)
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
    }

    #[test]
    fn test_prompt_developer_iteration_with_context_minimal() {
        let context = TemplateContext::default();
        let result = prompt_developer_iteration_with_context(
            &context,
            1,
            5,
            ContextLevel::Minimal,
            "test prompt",
            "test plan",
        );
        // Agent should receive PROMPT and PLAN content directly
        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(!result.contains("PROMPT.md"));
        assert!(!result.contains("PLAN.md"));
    }

    #[test]
    fn test_prompt_plan_with_context() {
        let context = TemplateContext::default();
        let result = prompt_plan_with_context(&context, None);
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("<ralph-implementation-steps>"));
        assert!(result.contains("<ralph-critical-files>"));
        assert!(result.contains("<ralph-verification-strategy>"));
        assert!(result.contains("READ-ONLY"));
        assert!(result.contains("STRICTLY PROHIBITED"));
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
        assert!(result.contains("PHASE 2: EXPLORATION"));
        assert!(result.contains("PHASE 3: DESIGN"));
        assert!(result.contains("PHASE 4: REVIEW"));
        assert!(result.contains("PHASE 5: WRITE STRUCTURED PLAN"));
        assert!(result.contains("<ralph-plan>"));
    }

    #[test]
    fn test_prompt_plan_with_context_and_content() {
        let context = TemplateContext::default();
        let prompt_md = "# Test Prompt\n\nThis is the content.";
        let result = prompt_plan_with_context(&context, Some(prompt_md));
        assert!(result.contains("USER REQUIREMENTS:"));
        assert!(result.contains("This is the content."));
        assert!(!result.contains("PROMPT.md"));
        assert!(result.contains("PLANNING MODE"));
        assert!(result.contains("PHASE 1: UNDERSTANDING"));
    }

    #[test]
    fn test_context_based_prompts_isolate_from_git() {
        let context = TemplateContext::default();
        let prompts = vec![
            prompt_developer_iteration_with_context(&context, 1, 3, ContextLevel::Minimal, "", ""),
            prompt_developer_iteration_with_context(&context, 2, 3, ContextLevel::Normal, "", ""),
            prompt_plan_with_context(&context, None),
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

    #[test]
    fn test_context_based_matches_regular_functions() {
        let context = TemplateContext::default();
        let regular = prompt_developer_iteration(1, 3, ContextLevel::Normal, "prompt", "plan");
        let with_context = prompt_developer_iteration_with_context(
            &context,
            1,
            3,
            ContextLevel::Normal,
            "prompt",
            "plan",
        );
        // Both should produce equivalent output
        assert_eq!(regular, with_context);

        let regular_plan = prompt_plan(None);
        let with_context_plan = prompt_plan_with_context(&context, None);
        assert_eq!(regular_plan, with_context_plan);
    }

    #[test]
    fn test_prompt_developer_iteration_xml_with_context_renders_shared_partials() {
        let context = TemplateContext::default();

        let result =
            prompt_developer_iteration_xml_with_context(&context, "test prompt", "test plan");

        assert!(result.contains("test prompt"));
        assert!(result.contains("test plan"));
        assert!(result.contains("IMPLEMENTATION MODE"));

        // Shared partials should be expanded
        assert!(
            result.contains("*** UNATTENDED MODE - NO USER INTERACTION ***"),
            "developer_iteration_xml should render shared/_unattended_mode partial"
        );
        assert!(
            !result.contains("{{>"),
            "developer_iteration_xml should not contain raw partial directives"
        );
    }
}
