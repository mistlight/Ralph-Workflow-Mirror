//! Analysis agent effect handlers.

use crate::agents::AgentRole;
use crate::files::write_diff_backup_with_workspace;
use crate::git_helpers::get_git_diff_from_start_with_workspace;
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentEvent, DevelopmentEvent, PipelineEvent};
use crate::reducer::handler::MainEffectHandler;
use anyhow::Result;
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
        // Read PLAN.md (non-fatal if missing).
        // Requirements: analysis should still run and report missing inputs.
        let plan_path = Path::new(".agent/PLAN.md");
        let plan_content = match ctx.workspace.read(plan_path) {
            Ok(s) => s,
            Err(err) => {
                // Best-effort fallback: older checkpoints or interrupted runs may still have
                // the XML plan available.
                let xml_fallback = Path::new(".agent/tmp/plan.xml");
                match ctx.workspace.read(xml_fallback) {
                    Ok(xml) => format!(
                        "[PLAN unavailable: failed to read .agent/PLAN.md ({err}); using fallback .agent/tmp/plan.xml]\n\n{xml}"
                    ),
                    Err(fallback_err) => format!(
                        "[PLAN unavailable: failed to read .agent/PLAN.md ({err}); also failed to read .agent/tmp/plan.xml ({fallback_err})]"
                    ),
                }
            }
        };

        // Generate git diff since pipeline start (non-fatal if it fails).
        // If generation fails, fall back to `.agent/DIFF.backup` if present; otherwise
        // provide a placeholder with recovery commands.
        let diff_content = match get_git_diff_from_start_with_workspace(ctx.workspace) {
            Ok(diff) => {
                // Best-effort: persist diff for prompt materialization fallbacks.
                // Missing `.agent/DIFF.backup` must not be fatal.
                let _ = write_diff_backup_with_workspace(ctx.workspace, &diff);
                diff
            }
            Err(err) => match ctx.workspace.read(Path::new(".agent/DIFF.backup")) {
                Ok(backup) => backup,
                Err(backup_err) => format!(
                    "[DIFF unavailable: failed to generate git diff ({err}); and failed to read .agent/DIFF.backup ({backup_err})]\n\n\
Fallback commands (last resort):\n\
- Unstaged changes: git diff\n\
- Staged changes:   git diff --cached\n\
- Untracked files:  git ls-files --others --exclude-standard\n"
                ),
            },
        };

        // Generate analysis prompt
        let mut prompt = crate::prompts::analysis::generate_analysis_prompt(
            &plan_content,
            &diff_content,
            iteration,
        );

        // XSD retry context: if the last analysis XML was invalid, instruct the agent to
        // read the schema error and previous invalid output from workspace files.
        if self.state.continuation.xsd_retry_pending {
            let xsd_error_path = ".agent/tmp/development_xsd_error.txt";
            let last_output_path = ".agent/tmp/development_result.xml";
            prompt = format!(
                "## XSD Retry Note\n\n\
Your previous XML output failed XSD validation.\n\
- Read the validation error: {xsd_error_path}\n\
- Read your previous invalid output: {last_output_path}\n\
Then produce a corrected development_result.xml that conforms to the schema.\n\n\
{prompt}"
            );
        }

        // Get current agent from chain
        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.developer_agent.to_string());

        // Invoke agent with analysis role
        let mut result = self.invoke_agent(ctx, AgentRole::Analysis, agent, None, prompt)?;

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
