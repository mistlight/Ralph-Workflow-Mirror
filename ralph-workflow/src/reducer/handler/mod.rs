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
mod retry_guidance;
mod review;

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
        self.event_log
            .extend(result.additional_events.iter().cloned());
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

            Effect::PreparePlanningPrompt {
                iteration,
                prompt_mode,
            } => self.prepare_planning_prompt(ctx, iteration, prompt_mode),

            Effect::MaterializePlanningInputs { iteration } => {
                self.materialize_planning_inputs(ctx, iteration)
            }

            Effect::CleanupPlanningXml { iteration } => self.cleanup_planning_xml(ctx, iteration),

            Effect::InvokePlanningAgent { iteration } => self.invoke_planning_agent(ctx, iteration),

            Effect::ExtractPlanningXml { iteration } => self.extract_planning_xml(ctx, iteration),

            Effect::ValidatePlanningXml { iteration } => self.validate_planning_xml(ctx, iteration),

            Effect::WritePlanningMarkdown { iteration } => {
                self.write_planning_markdown(ctx, iteration)
            }

            Effect::ArchivePlanningXml { iteration } => self.archive_planning_xml(ctx, iteration),

            Effect::ApplyPlanningOutcome { iteration, valid } => {
                self.apply_planning_outcome(ctx, iteration, valid)
            }

            Effect::PrepareDevelopmentContext { iteration } => {
                self.prepare_development_context(ctx, iteration)
            }

            Effect::MaterializeDevelopmentInputs { iteration } => {
                self.materialize_development_inputs(ctx, iteration)
            }

            Effect::PrepareDevelopmentPrompt {
                iteration,
                prompt_mode,
            } => self.prepare_development_prompt(ctx, iteration, prompt_mode),

            Effect::CleanupDevelopmentXml { iteration } => {
                self.cleanup_development_xml(ctx, iteration)
            }

            Effect::InvokeDevelopmentAgent { iteration } => {
                self.invoke_development_agent(ctx, iteration)
            }

            Effect::ExtractDevelopmentXml { iteration } => {
                self.extract_development_xml(ctx, iteration)
            }

            Effect::ValidateDevelopmentXml { iteration } => {
                self.validate_development_xml(ctx, iteration)
            }

            Effect::ApplyDevelopmentOutcome { iteration } => {
                self.apply_development_outcome(ctx, iteration)
            }

            Effect::ArchiveDevelopmentXml { iteration } => {
                self.archive_development_xml(ctx, iteration)
            }

            Effect::PrepareReviewContext { pass } => self.prepare_review_context(ctx, pass),

            Effect::MaterializeReviewInputs { pass } => self.materialize_review_inputs(ctx, pass),

            Effect::PrepareReviewPrompt { pass, prompt_mode } => {
                self.prepare_review_prompt(ctx, pass, prompt_mode)
            }

            Effect::CleanupReviewIssuesXml { pass } => self.cleanup_review_issues_xml(ctx, pass),

            Effect::InvokeReviewAgent { pass } => self.invoke_review_agent(ctx, pass),

            Effect::ExtractReviewIssuesXml { pass } => self.extract_review_issues_xml(ctx, pass),

            Effect::ValidateReviewIssuesXml { pass } => self.validate_review_issues_xml(ctx, pass),

            Effect::WriteIssuesMarkdown { pass } => self.write_issues_markdown(ctx, pass),

            Effect::ExtractReviewIssueSnippets { pass } => {
                self.extract_review_issue_snippets(ctx, pass)
            }

            Effect::ArchiveReviewIssuesXml { pass } => self.archive_review_issues_xml(ctx, pass),

            Effect::ApplyReviewOutcome {
                pass,
                issues_found,
                clean_no_issues,
            } => self.apply_review_outcome(ctx, pass, issues_found, clean_no_issues),

            Effect::PrepareFixPrompt { pass, prompt_mode } => {
                self.prepare_fix_prompt(ctx, pass, prompt_mode)
            }

            Effect::CleanupFixResultXml { pass } => self.cleanup_fix_result_xml(ctx, pass),

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

            Effect::PrepareCommitPrompt { prompt_mode } => {
                self.prepare_commit_prompt(ctx, prompt_mode)
            }

            Effect::CheckCommitDiff => self.check_commit_diff(ctx),

            Effect::MaterializeCommitInputs { attempt } => {
                self.materialize_commit_inputs(ctx, attempt)
            }

            Effect::InvokeCommitAgent => self.invoke_commit_agent(ctx),

            Effect::CleanupCommitXml => self.cleanup_commit_xml(ctx),

            Effect::ExtractCommitXml => self.extract_commit_xml(ctx),

            Effect::ValidateCommitXml => self.validate_commit_xml(ctx),

            Effect::ApplyCommitMessageOutcome => self.apply_commit_message_outcome(ctx),

            Effect::ArchiveCommitXml => self.archive_commit_xml(ctx),

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

            Effect::ReportAgentChainExhausted { role, phase, cycle } => {
                use crate::reducer::event::ErrorEvent;
                Err(ErrorEvent::AgentChainExhausted { role, phase, cycle }.into())
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
}
