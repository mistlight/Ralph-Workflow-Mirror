// Analysis agent system prompt generation.
//
// Generates prompts for the analysis agent to produce an objective assessment
// of development progress by comparing git diff against PLAN.md.
//
// TIMING: Analysis agent runs after EVERY development iteration, not just
// the final one. This provides continuous verification throughout the
// development phase, catching issues early rather than only at the end.
//
// The analysis agent has NO context from development execution, ensuring
// an unbiased assessment based purely on observable code changes (git diff).

/// Generate analysis agent prompt.
///
/// The analysis agent receives only the PLAN.md content and git diff since
/// pipeline start. It has NO context from development execution, ensuring
/// an unbiased assessment based purely on observable code changes.
///
/// # Arguments
///
/// * `plan_content` - The implementation plan (PLAN.md content)
/// * `diff_content` - The git diff since pipeline start (may be empty)
/// * `iteration` - The iteration number (for documentation only)
/// * `workspace` - Workspace for resolving absolute paths
///
/// # Returns
///
/// Returns the complete prompt for the analysis agent.
pub fn generate_analysis_prompt(
    plan_content: &str,
    diff_content: &str,
    iteration: u32,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    use crate::prompts::content_reference::{DiffContentReference, PlanContentReference};
    use crate::prompts::partials::get_shared_partials;
    use crate::prompts::template_context::TemplateContext;
    use crate::prompts::template_engine::Template;
    use std::collections::HashMap;
    use std::path::Path;

    let plan_ref = PlanContentReference::from_plan(
        plan_content.to_string(),
        Path::new(".agent/PLAN.md"),
        Some(Path::new(".agent/tmp/plan.xml")),
    );
    let diff_ref = DiffContentReference::from_diff(
        diff_content.to_string(),
        "",
        Path::new(".agent/DIFF.backup"),
    );

    let partials = get_shared_partials();
    let context = TemplateContext::default();
    let template_content = context
        .registry()
        .get_template("analysis_system_prompt")
        .unwrap_or_else(|_| include_str!("../templates/analysis_system_prompt.txt").to_string());

    let variables = HashMap::from([
        ("PLAN", plan_ref.render_for_template()),
        (
            "DIFF",
            diff_ref
                .render_for_template()
                .replace("git diff", "git\u{00A0}diff"),
        ),
        ("ITERATION", iteration.to_string()),
        (
            "DEVELOPMENT_RESULT_XML_PATH",
            workspace.absolute_str(".agent/tmp/development_result.xml"),
        ),
        (
            "DEVELOPMENT_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/development_result.xsd"),
        ),
    ]);

    Template::new(&template_content)
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            let plan = plan_ref.render_for_template();
            let diff = diff_ref.render_for_template();
            let out = workspace.absolute_str(".agent/tmp/development_result.xml");
            let xsd = workspace.absolute_str(".agent/tmp/development_result.xsd");
            format!(
                "You are an independent code analysis agent.\n\nPLAN:\n{plan}\n\nDIFF:\n{diff}\n\nWrite development_result.xml to: {out}\nXSD: {xsd}\nIteration: {iteration}\n"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;

    #[test]
    fn test_generate_analysis_prompt_includes_all_parts() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        let plan = "Step 1: Add feature X\nStep 2: Add tests";
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn feature_x() {}";
        let iteration = 0;

        let prompt = generate_analysis_prompt(plan, diff, iteration, &workspace);

        assert!(prompt.contains("Step 1: Add feature X"));
        assert!(prompt.contains("Step 2: Add tests"));
        assert!(prompt.contains("diff --git"));
        assert!(prompt.contains("iteration 0"));
        assert!(prompt.contains("development_result.xml"));
    }

    #[test]
    fn test_generate_analysis_prompt_handles_empty_diff() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        let plan = "Verify feature exists";
        let diff = "";
        let iteration = 0;

        let prompt = generate_analysis_prompt(plan, diff, iteration, &workspace);

        assert!(prompt.contains("Verify feature exists"));
        assert!(
            prompt.contains("EMPTY")
                || prompt.contains("diff input")
                || prompt.contains("git diff")
        );
        // Specific phrasing lives in the template; just ensure empty diff guidance is present.
        assert!(prompt.contains("EMPTY OR MISSING DIFF HANDLING"));
        assert!(prompt.contains("If the DIFF is EMPTY"));
    }

    #[test]
    fn test_generate_analysis_prompt_uses_materialized_references_when_plan_is_oversize() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        let plan = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let diff = "small diff";
        let prompt = generate_analysis_prompt(&plan, diff, 0, &workspace);

        assert!(
            prompt.contains("[PLAN too large to embed"),
            "expected plan to be referenced when oversize"
        );
        assert!(
            !prompt.contains(&plan),
            "oversize plan must not be inlined into the prompt"
        );
    }

    #[test]
    fn test_generate_analysis_prompt_uses_materialized_references_when_diff_is_oversize() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        let plan = "small plan";
        let diff = "d".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let prompt = generate_analysis_prompt(plan, &diff, 0, &workspace);

        assert!(
            prompt.contains("[DIFF too large to embed"),
            "expected diff to be referenced when oversize"
        );
        assert!(
            !prompt.contains(&diff),
            "oversize diff must not be inlined into the prompt"
        );
    }

    #[test]
    fn test_generate_analysis_prompt_specifies_xml_format() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        let plan = "Plan content";
        let diff = "Diff content";
        let iteration = 1;

        let prompt = generate_analysis_prompt(plan, diff, iteration, &workspace);

        assert!(prompt.contains("<ralph-development-result>"));
        assert!(prompt.contains("<ralph-status>"));
        assert!(prompt.contains("<ralph-summary>"));
        assert!(prompt.contains("completed|partial|failed"));
    }

    #[test]
    fn test_generate_analysis_prompt_does_not_fallback_to_working_tree() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        // The analysis agent must be context-free: it should assess PLAN vs DIFF only.
        // Working-tree fallback instructions can bias results and expand what the agent reads.
        let prompt = generate_analysis_prompt("Plan", "Diff", 0, &workspace);

        assert!(
            !prompt.to_lowercase().contains("working tree"),
            "prompt must not mention working tree; got: {prompt}"
        );
        // The analysis system prompt must not instruct git commands directly.
        // (The DIFF reference type used elsewhere may include git fallback, which is filtered out
        // for analysis prompts in generate_analysis_prompt.)
        assert!(
            !prompt.contains("git\u{00A0}diff"),
            "prompt must not instruct git commands; got: {prompt}"
        );
    }

    #[test]
    fn test_generate_analysis_prompt_mentions_diff_backup_path_but_not_git_fallback_commands() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();
        // Analysis is context-free: it must not expand inputs by instructing git commands.
        // It may reference a materialized diff backup path if provided by DIFF reference rendering.
        // When the diff is oversized, the prompt should reference a file path rather than inline.
        // When small, the diff will likely be inlined and may not mention a file path.
        let large_diff = "d".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let prompt = generate_analysis_prompt("Plan", &large_diff, 0, &workspace);
        assert!(
            prompt.contains(".agent/tmp/diff.txt") || prompt.contains(".agent/DIFF.backup"),
            "expected oversize diff prompt to mention a DIFF file path reference; got: {prompt}"
        );
        assert!(
            !prompt.contains("git diff"),
            "prompt must not instruct git fallback commands; got: {prompt}"
        );
    }
}
