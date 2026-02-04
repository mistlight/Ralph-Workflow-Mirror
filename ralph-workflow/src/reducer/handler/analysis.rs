//! Analysis agent effect handlers.

use crate::agents::AgentRole;
use crate::git_helpers::get_git_diff_from_start_with_workspace;
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentEvent, DevelopmentEvent, PipelineEvent};
use crate::reducer::handler::MainEffectHandler;
use anyhow::{Context, Result};
use std::path::Path;

impl MainEffectHandler {
    /// Invoke analysis agent to verify development results.
    ///
    /// TIMING: This handler runs after EVERY development iteration where
    /// InvokeDevelopmentAgent completed, regardless of iteration count.
    ///
    /// This handler:
    /// 1. Reads PLAN.md content
    /// 2. Generates git diff since pipeline start
    /// 3. Builds analysis prompt with both inputs
    /// 4. Invokes agent to produce development_result.xml
    /// 5. Emits AnalysisAgentInvoked event
    ///
    /// The analysis agent has NO context from development execution,
    /// ensuring an objective assessment based purely on observable changes.
    ///
    /// Empty diff handling: The analysis agent receives empty diff and must
    /// determine if this means "no changes needed" (status=completed) or
    /// "dev agent failed to execute" (status=failed) based on PLAN.md context.
    pub(super) fn invoke_analysis_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        // Read PLAN.md
        let plan_path = Path::new(".agent/PLAN.md");
        let plan_content = ctx
            .workspace
            .read(plan_path)
            .context("Failed to read PLAN.md for analysis")?;

        // Generate git diff since pipeline start
        let diff_content = get_git_diff_from_start_with_workspace(ctx.workspace)
            .context("Failed to generate git diff for analysis")?;

        // Generate analysis prompt
        let prompt = crate::prompts::analysis::generate_analysis_prompt(
            &plan_content,
            &diff_content,
            iteration,
        );

        // Get current agent from chain
        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.developer_agent.to_string());

        // Invoke agent with analysis role
        let mut result = self.invoke_agent(ctx, AgentRole::Developer, agent, None, prompt)?;

        // Emit AnalysisAgentInvoked event if agent invocation succeeded
        if result.additional_events.iter().any(|e| {
            matches!(
                e,
                PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
            )
        }) {
            result = result.with_additional_event(PipelineEvent::Development(
                DevelopmentEvent::AnalysisAgentInvoked { iteration },
            ));
        }

        Ok(result)
    }
}
