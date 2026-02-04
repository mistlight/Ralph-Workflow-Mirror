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
///
/// # Returns
///
/// Returns the complete prompt for the analysis agent.
pub fn generate_analysis_prompt(plan_content: &str, diff_content: &str, iteration: u32) -> String {
    use crate::prompts::content_reference::{DiffContentReference, PlanContentReference};
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

    let plan_rendered = plan_ref.render_for_template();
    let diff_rendered = diff_ref.render_for_template();

    // TODO THIS NEEDS to be migrated to .txt this is not conforming, prompt strings should almost
    // never be allowed since users won't be able to edit them when we add prompt editing in the
    // future
    //
    // *important* we need to get the status no matter what, if working tree is analysis is needed,
    // we NEED to do so
    //
    // *important* working tree is not off limits for a reason btw if diff is not available to ensure
    // continuity
    format!(
        r#"You are an independent code analysis agent. Your task is to objectively assess
whether the code changes align with the original plan. 

You are in read only mode EXCEPT for outputting the required xml at .agent/tmp/development_result.xml

IMPORTANT: Base your assessment strictly on the explicit PLAN + DIFF inputs below.
If Diff is unavailable use the current working tree

PLAN (from PLAN.md):
{}

ACTUAL CHANGES (git diff since start):
{}

Analyze the diff and determine:
1. Which planned items were completed
2. Which planned items are missing
3. Any changes not mentioned in the plan
4. Overall status: completed, partial, or failed

IMPORTANT - If git diff is EMPTY:
- Check if the PLAN required any code changes
- If no changes were needed (plan already satisfied): status="completed"
- If changes were expected but not made (dev agent failed): status="failed"
- Always explain in summary WHY there are no changes

IMPORTANT - If you cannot access the DIFF content (inline or referenced file), use the current working tree

Output your analysis using the development_result.xml format:
<ralph-development-result>
  <ralph-status>completed|partial|failed</ralph-status>
  <ralph-summary>Brief summary of what was accomplished vs the plan</ralph-summary>
  <ralph-files-changed>List of files modified (optional)</ralph-files-changed>
  <ralph-next-steps>What remains to be done, if status is partial (optional)</ralph-next-steps>
</ralph-development-result>

Write the XML to .agent/tmp/development_result.xml

IMPORTANT: Your XML MUST conform to the XSD schema at .agent/tmp/development_result.xsd
to ensure it can be parsed correctly. The schema is available for reference if needed.

This is iteration {} (for reference only).
"#,
        plan_rendered, diff_rendered, iteration
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::content_reference::MAX_INLINE_CONTENT_SIZE;

    #[test]
    fn test_generate_analysis_prompt_includes_all_parts() {
        let plan = "Step 1: Add feature X\nStep 2: Add tests";
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+fn feature_x() {}";
        let iteration = 0;

        let prompt = generate_analysis_prompt(plan, diff, iteration);

        assert!(prompt.contains("Step 1: Add feature X"));
        assert!(prompt.contains("Step 2: Add tests"));
        assert!(prompt.contains("diff --git"));
        assert!(prompt.contains("iteration 0"));
        assert!(prompt.contains("development_result.xml"));
    }

    #[test]
    fn test_generate_analysis_prompt_handles_empty_diff() {
        let plan = "Verify feature exists";
        let diff = "";
        let iteration = 0;

        let prompt = generate_analysis_prompt(plan, diff, iteration);

        assert!(prompt.contains("Verify feature exists"));
        assert!(prompt.contains("If git diff is EMPTY"));
        assert!(prompt.contains("no changes were needed"));
        assert!(prompt.contains("changes were expected but not made"));
    }

    #[test]
    fn test_generate_analysis_prompt_uses_materialized_references_when_plan_is_oversize() {
        let plan = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let diff = "small diff";
        let prompt = generate_analysis_prompt(&plan, diff, 0);

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
        let plan = "small plan";
        let diff = "d".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let prompt = generate_analysis_prompt(plan, &diff, 0);

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
        let plan = "Plan content";
        let diff = "Diff content";
        let iteration = 1;

        let prompt = generate_analysis_prompt(plan, diff, iteration);

        assert!(prompt.contains("<ralph-development-result>"));
        assert!(prompt.contains("<ralph-status>"));
        assert!(prompt.contains("<ralph-summary>"));
        assert!(prompt.contains("completed|partial|failed"));
    }
}
