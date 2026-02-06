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
mod analysis;
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

    fn write_completion_marker(ctx: &PhaseContext<'_>, content: &str, is_failure: bool) -> bool {
        let marker_dir = std::path::Path::new(".agent/tmp");
        if let Err(err) = ctx.workspace.create_dir_all(marker_dir) {
            ctx.logger.warn(&format!(
                "Failed to create completion marker directory: {}",
                err
            ));
        }

        let marker_path = std::path::Path::new(".agent/tmp/completion_marker");
        match ctx.workspace.write(marker_path, content) {
            Ok(()) => {
                ctx.logger.info(&format!(
                    "Completion marker written: {}",
                    if is_failure { "failure" } else { "success" }
                ));
                true
            }
            Err(err) => {
                ctx.logger
                    .warn(&format!("Failed to write completion marker: {}", err));
                false
            }
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

            Effect::InvokeAnalysisAgent { iteration } => self.invoke_analysis_agent(ctx, iteration),

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

            Effect::TriggerLoopRecovery {
                detected_loop,
                loop_count,
            } => self.trigger_loop_recovery(ctx, detected_loop, loop_count),

            Effect::TriggerDevFixFlow {
                failed_phase,
                failed_role,
                retry_cycle,
            } => {
                ctx.logger.error("⚠️  PIPELINE FAILURE DETECTED ⚠️");
                ctx.logger.warn(&format!(
                    "Pipeline failure detected (phase: {}, role: {:?}, cycle: {})",
                    failed_phase, failed_role, retry_cycle
                ));
                ctx.logger.info("Entering AwaitingDevFix flow...");
                ctx.logger
                    .info("Dispatching dev-fix agent for remediation...");

                let read_or_fallback = |path: &str, label: &str| -> String {
                    match ctx.workspace.read(std::path::Path::new(path)) {
                        Ok(content) => content,
                        Err(err) => {
                            ctx.logger.warn(&format!(
                                "Dev-fix prompt fallback: failed to read {}: {}",
                                label, err
                            ));
                            format!("(Missing {}: {})", label, err)
                        }
                    }
                };

                let prompt_content = read_or_fallback("PROMPT.md", "PROMPT.md");
                let plan_content = read_or_fallback(".agent/PLAN.md", ".agent/PLAN.md");
                let issues_content = format!(
                    "# Issues\n\n- [High] Pipeline failure (phase: {}, role: {:?}, cycle: {}).\n  Diagnose the root cause and fix the failure.\n",
                    failed_phase, failed_role, retry_cycle
                );
                let dev_fix_prompt = crate::prompts::prompt_fix_with_context(
                    ctx.template_context,
                    &prompt_content,
                    &plan_content,
                    &issues_content,
                    ctx.workspace,
                );

                if let Err(err) = ctx.workspace.write(
                    std::path::Path::new(".agent/tmp/dev_fix_prompt.txt"),
                    &dev_fix_prompt,
                ) {
                    ctx.logger.warn(&format!(
                        "Failed to write dev-fix prompt to workspace: {}",
                        err
                    ));
                }

                let agent = self
                    .state
                    .agent_chain
                    .current_agent()
                    .cloned()
                    .unwrap_or_else(|| ctx.developer_agent.to_string());

                let completion_marker_content = format!(
                    "failure\nPipeline failure: phase={}, role={:?}, cycle={}",
                    failed_phase, failed_role, retry_cycle
                );
                Self::write_completion_marker(ctx, &completion_marker_content, true);

                let agent_result = match self.invoke_agent(
                    ctx,
                    crate::agents::AgentRole::Developer,
                    agent,
                    None,
                    dev_fix_prompt,
                ) {
                    Ok(result) => Ok(result),
                    Err(err) => {
                        // Check if error is due to agent unavailability (quota/usage limit)
                        let err_msg = err.to_string().to_lowercase();
                        let is_agent_unavailable = err_msg.contains("usage limit")
                            || err_msg.contains("quota exceeded")
                            || err_msg.contains("rate limit");

                        if is_agent_unavailable {
                            ctx.logger.warn(&format!(
                                "Dev-fix agent unavailable: {}. Pipeline will terminate with failure marker.",
                                err
                            ));
                        } else {
                            ctx.logger
                                .warn(&format!("Dev-fix agent invocation failed: {}", err));
                        }
                        Err(err)
                    }
                };

                let is_agent_unavailable = agent_result
                    .as_ref()
                    .err()
                    .map(|err| {
                        let err_msg = err.to_string().to_lowercase();
                        err_msg.contains("usage limit")
                            || err_msg.contains("quota exceeded")
                            || err_msg.contains("rate limit")
                    })
                    .unwrap_or(false);

                let dev_fix_success = agent_result
                    .as_ref()
                    .map(|result| {
                        result.additional_events.iter().any(|event| {
                            matches!(
                                event,
                                PipelineEvent::Agent(
                                    crate::reducer::event::AgentEvent::InvocationSucceeded { .. }
                                )
                            )
                        })
                    })
                    .unwrap_or(false);

                let dev_fix_summary = agent_result
                    .as_ref()
                    .err()
                    .map(|err| format!("Dev-fix agent invocation failed: {}", err));

                // Extract error reason before consuming agent_result
                let error_reason = agent_result.as_ref().err().map(|e| e.to_string());

                let mut result = match agent_result.as_ref() {
                    Ok(result) => EffectResult::with_ui(
                        PipelineEvent::AwaitingDevFix(
                            crate::reducer::event::AwaitingDevFixEvent::DevFixTriggered {
                                failed_phase,
                                failed_role,
                            },
                        ),
                        result.ui_events.clone(),
                    ),
                    Err(_) => EffectResult::event(PipelineEvent::AwaitingDevFix(
                        crate::reducer::event::AwaitingDevFixEvent::DevFixTriggered {
                            failed_phase,
                            failed_role,
                        },
                    )),
                };

                if let Ok(result_events) = agent_result {
                    result = result.with_additional_event(result_events.event);
                    for event in result_events.additional_events {
                        result = result.with_additional_event(event);
                    }
                }

                // Emit appropriate event based on agent availability
                if is_agent_unavailable {
                    result = result.with_additional_event(PipelineEvent::AwaitingDevFix(
                        crate::reducer::event::AwaitingDevFixEvent::DevFixAgentUnavailable {
                            failed_phase,
                            reason: error_reason.unwrap_or_else(|| "unknown".to_string()),
                        },
                    ));
                } else {
                    result = result.with_additional_event(PipelineEvent::AwaitingDevFix(
                        crate::reducer::event::AwaitingDevFixEvent::DevFixCompleted {
                            success: dev_fix_success,
                            summary: dev_fix_summary,
                        },
                    ));
                }
                result = result.with_additional_event(PipelineEvent::AwaitingDevFix(
                    crate::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                        is_failure: true,
                    },
                ));

                Ok(result)
            }

            Effect::EmitCompletionMarkerAndTerminate { is_failure, reason } => {
                // Write completion marker to .agent/tmp/completion_marker
                let content = if is_failure {
                    format!(
                        "failure\n{}",
                        reason.unwrap_or_else(|| "unknown".to_string())
                    )
                } else {
                    "success\n".to_string()
                };

                Self::write_completion_marker(ctx, &content, is_failure);

                // Emit event to transition to Interrupted
                Ok(EffectResult::event(PipelineEvent::AwaitingDevFix(
                    crate::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                        is_failure,
                    },
                )))
            }
        }
    }
}
