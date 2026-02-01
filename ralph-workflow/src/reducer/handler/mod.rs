//
// This module implements the EffectHandler trait to execute pipeline side effects
// through the reducer architecture. Effect handlers perform actual work (agent
// invocation, git operations, file I/O) and emit events.
//
// Handler responsibilities vs reducer responsibilities:
// - Reducer: pure state transitions, policy decisions, phase progression
// - Handler: effect execution, I/O, cleanup, validation
//
// Handlers execute exactly one effect and emit events. They must not perform
// hidden cleanup, fallback, or retry logic beyond the effect being executed.
// XML `.processed` files are archives only and are never read as inputs.

mod agent;
mod chain;
mod checkpoint;
mod commit;
mod context;
mod development;
mod planning;
mod rebase;
mod review;
mod util;

#[cfg(test)]
mod tests;

use crate::phases::PhaseContext;
use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
use crate::reducer::event::{PipelineEvent, PipelinePhase};
use crate::reducer::state::PipelineState;
use crate::reducer::ui_event::UIEvent;
use anyhow::Result;

/// Main effect handler implementation.
///
/// This handler executes effects by calling pipeline subsystems and emitting reducer events.
pub struct MainEffectHandler {
    /// Current pipeline state
    pub state: PipelineState,
    /// Event log for replay/debugging
    pub event_log: Vec<PipelineEvent>,
}

impl MainEffectHandler {
    /// Create a new effect handler.
    pub fn new(state: PipelineState) -> Self {
        Self {
            state,
            event_log: Vec::new(),
        }
    }
}

impl<'ctx> EffectHandler<'ctx> for MainEffectHandler {
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        let result = self.execute_effect(effect, ctx)?;
        self.event_log.push(result.event.clone());
        Ok(result)
    }
}

impl crate::app::event_loop::StatefulHandler for MainEffectHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}

impl MainEffectHandler {
    /// Helper to create phase transition UI event.
    fn phase_transition_ui(&self, to: PipelinePhase) -> UIEvent {
        UIEvent::PhaseTransition {
            from: Some(self.state.phase),
            to,
        }
    }

    fn execute_effect(
        &mut self,
        effect: Effect,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        match effect {
            Effect::AgentInvocation {
                role,
                agent,
                model,
                prompt,
            } => self.invoke_agent(ctx, role, agent, model, prompt),

            Effect::InitializeAgentChain { role } => self.initialize_agent_chain(ctx, role),

            Effect::GeneratePlan { iteration } => self.generate_plan(ctx, iteration),

            Effect::RunDevelopmentIteration { iteration } => {
                self.run_development_iteration(ctx, iteration)
            }

            Effect::PrepareReviewContext { pass } => self.prepare_review_context(ctx, pass),

            Effect::PrepareReviewPrompt { pass } => self.prepare_review_prompt(ctx, pass),

            Effect::InvokeReviewAgent { pass } => self.invoke_review_agent(ctx, pass),

            Effect::ExtractReviewIssuesXml { pass } => self.extract_review_issues_xml(ctx, pass),

            Effect::ValidateReviewIssuesXml { pass } => self.validate_review_issues_xml(ctx, pass),

            Effect::WriteIssuesMarkdown { pass } => self.write_issues_markdown(ctx, pass),

            Effect::ArchiveReviewIssuesXml { pass } => self.archive_review_issues_xml(ctx, pass),

            Effect::ApplyReviewOutcome {
                pass,
                issues_found,
                clean_no_issues,
            } => self.apply_review_outcome(ctx, pass, issues_found, clean_no_issues),

            Effect::PrepareFixPrompt { pass } => self.prepare_fix_prompt(ctx, pass),

            Effect::InvokeFixAgent { pass } => self.invoke_fix_agent(ctx, pass),

            Effect::ExtractFixResultXml { pass } => self.extract_fix_result_xml(ctx, pass),

            Effect::ValidateFixResultXml { pass } => self.validate_fix_result_xml(ctx, pass),

            Effect::ApplyFixOutcome { pass } => self.apply_fix_outcome(ctx, pass),

            Effect::ArchiveFixResultXml { pass } => self.archive_fix_result_xml(ctx, pass),

            Effect::RunRebase {
                phase,
                target_branch,
            } => self.run_rebase(ctx, phase, target_branch),

            Effect::ResolveRebaseConflicts { strategy } => {
                self.resolve_rebase_conflicts(ctx, strategy)
            }

            Effect::GenerateCommitMessage => self.generate_commit_message(ctx),

            Effect::CreateCommit { message } => self.create_commit(ctx, message),

            Effect::SkipCommit { reason } => self.skip_commit(ctx, reason),

            Effect::BackoffWait {
                role,
                cycle,
                duration_ms,
            } => {
                use std::time::Duration;
                ctx.registry
                    .retry_timer()
                    .sleep(Duration::from_millis(duration_ms));
                Ok(EffectResult::event(
                    PipelineEvent::agent_retry_cycle_started(role, cycle),
                ))
            }

            Effect::AbortPipeline { reason } => {
                Ok(EffectResult::event(PipelineEvent::pipeline_aborted(reason)))
            }

            Effect::ValidateFinalState => self.validate_final_state(ctx),

            Effect::SaveCheckpoint { trigger } => self.save_checkpoint(ctx, trigger),

            Effect::CleanupContext => self.cleanup_context(ctx),

            Effect::RestorePromptPermissions => self.restore_prompt_permissions(ctx),

            Effect::WriteContinuationContext(ref data) => {
                development::write_continuation_context_to_workspace(
                    ctx.workspace,
                    ctx.logger,
                    data,
                )?;
                Ok(EffectResult::event(
                    PipelineEvent::development_continuation_context_written(
                        data.iteration,
                        data.attempt,
                    ),
                ))
            }

            Effect::CleanupContinuationContext => self.cleanup_continuation_context(ctx),
        }
    }

    fn is_auth_failure(err: &anyhow::Error) -> bool {
        development::is_auth_failure(err)
    }
}
