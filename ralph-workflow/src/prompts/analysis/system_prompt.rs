// Analysis agent system prompt generation.
//
// Generates prompts for the analysis agent to produce objective assessment
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
    format!(
        r#"You are an independent code analysis agent. Your task is to objectively assess
whether the code changes align with the original plan.

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

NOTE: You are analyzing COMPLETED work, not doing the work yourself. Base your
assessment on the git diff vs the plan if possible. 
If git diff does not exist, broken or does not provide enough context, use the
current working tree to determine the status against the plan. 
This is iteration {} (for reference only).
"#,
        plan_content, diff_content, iteration
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
