//! Effect handler implementation for pipeline side effects.
//!
//! This module implements the [`EffectHandler`] trait to execute pipeline effects
//! through the reducer architecture. Effect handlers perform actual work (agent
//! invocation, git operations, file I/O) and emit events that drive state transitions.
//!
//! # Architecture Contract
//!
//! ```text
//! State → Orchestrator → Effect → Handler → Event → Reducer → State
//!                                  ^^^^^^^
//!                                  Impure execution (this module)
//! ```
//!
//! ## Handler Responsibilities
//!
//! - **Execute effects**: Perform the I/O operation specified by the effect
//! - **Report outcomes**: Emit events describing what happened (success/failure)
//! - **Use workspace abstraction**: All filesystem access via `ctx.workspace`
//! - **Single-task execution**: Execute exactly one effect, no hidden retry logic
//!
//! ## Reducer Responsibilities (NOT handler)
//!
//! - **Pure state transitions**: Process events to update state
//! - **Policy decisions**: Retry, fallback, phase progression
//! - **Control flow**: Determine what happens next based on events
//!
//! # Key Principle: Handlers Report, Reducers Decide
//!
//! Handlers must NOT contain decision logic. Examples:
//!
//! ```ignore
//! // WRONG - Handler decides to retry
//! fn handle_invoke_agent() -> Result<EffectResult> {
//!     for attempt in 0..3 {  // NO! Reducer controls retry
//!         if let Ok(output) = invoke_agent() {
//!             return Ok(output);
//!         }
//!     }
//! }
//!
//! // CORRECT - Handler reports outcome, reducer decides
//! fn handle_invoke_agent() -> Result<EffectResult> {
//!     match invoke_agent() {
//!         Ok(output) => Ok(EffectResult::event(
//!             AgentEvent::InvocationSucceeded { output }
//!         )),
//!         Err(e) => Ok(EffectResult::event(
//!             AgentEvent::InvocationFailed { error: e, retriable: true }
//!         )),
//!     }
//! }
//! ```
//!
//! The reducer processes `InvocationFailed` and decides whether to retry
//! (increment retry count, emit retry effect) or fallback (advance chain).
//!
//! # Workspace Abstraction
//!
//! All filesystem operations MUST use `ctx.workspace`:
//!
//! ```ignore
//! // CORRECT
//! ctx.workspace.write(path, content)?;
//! let content = ctx.workspace.read(path)?;
//!
//! // WRONG - Never use std::fs in handlers
//! std::fs::write(path, content)?;
//! ```
//!
//! This abstraction enables:
//! - In-memory testing with `MemoryWorkspace`
//! - Proper error handling and path resolution
//! - Consistent file operations across the pipeline
//!
//! See [`docs/agents/workspace-trait.md`] for details.
//!
//! # Testing Handlers
//!
//! Handlers require mocks for I/O (workspace) but NOT for reducer/orchestration:
//!
//! ```ignore
//! #[test]
//! fn test_invoke_agent_emits_success_event() {
//!     let workspace = MemoryWorkspace::new_test();
//!     let mut ctx = create_test_context(&workspace);
//!
//!     let result = handler.execute(
//!         Effect::InvokeAgent { role, agent, prompt },
//!         &mut ctx
//!     )?;
//!
//!     assert!(matches!(
//!         result.event,
//!         PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
//!     ));
//! }
//! ```
//!
//! # Module Organization
//!
//! - [`agent`] - Agent invocation and chain management
//! - [`planning`] - Planning phase effects (prompt, XML, validation)
//! - [`development`] - Development phase effects (iteration, continuation)
//! - [`review`] - Review phase effects (issue detection, fix application)
//! - [`commit`] - Commit phase effects (message generation, commit creation)
//! - [`rebase`] - Rebase effects (conflict resolution, validation)
//! - [`checkpoint`] - Checkpoint save/restore
//! - [`context`] - Context preparation and cleanup
//!
//! [`docs/agents/workspace-trait.md`]: https://codeberg.org/mistlight/RalphWithReviewer/src/branch/main/docs/agents/workspace-trait.md

mod agent;
mod analysis;
mod chain;
mod checkpoint;
mod cloud;
mod commit;
mod context;
mod development;
mod lifecycle;
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

    fn write_completion_marker(
        ctx: &PhaseContext<'_>,
        content: &str,
        is_failure: bool,
    ) -> std::result::Result<(), String> {
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
                Ok(())
            }
            Err(err) => {
                ctx.logger
                    .warn(&format!("Failed to write completion marker: {}", err));
                Err(err.to_string())
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

            Effect::CheckUncommittedChangesBeforeTermination => {
                self.check_uncommitted_changes_before_termination(ctx)
            }

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

            Effect::EnsureGitignoreEntries => self.ensure_gitignore_entries(ctx),

            Effect::CleanupContext => self.cleanup_context(ctx),

            Effect::LockPromptPermissions => self.lock_prompt_permissions(ctx),

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

            Effect::EmitRecoveryReset {
                reset_type,
                target_phase,
            } => self.emit_recovery_reset(ctx, reset_type, target_phase),

            Effect::AttemptRecovery {
                level,
                attempt_count,
            } => self.attempt_recovery(ctx, level, attempt_count),

            Effect::EmitRecoverySuccess {
                level,
                total_attempts,
            } => self.emit_recovery_success(ctx, level, total_attempts),

            Effect::TriggerDevFixFlow {
                failed_phase,
                failed_role,
                retry_cycle,
            } => self.trigger_dev_fix_flow(ctx, failed_phase, failed_role, retry_cycle),

            Effect::EmitCompletionMarkerAndTerminate { is_failure, reason } => {
                Self::emit_completion_marker_and_terminate(ctx, is_failure, reason)
            }

            // Cloud mode effects - only executed when cloud mode is enabled
            Effect::ConfigureGitAuth { auth_method } => {
                self.handle_configure_git_auth(ctx, auth_method)
            }

            Effect::PushToRemote {
                remote,
                branch,
                force,
                commit_sha,
            } => self.handle_push_to_remote(ctx, remote, branch, force, commit_sha),

            Effect::CreatePullRequest {
                base_branch,
                head_branch,
                title,
                body,
            } => self.handle_create_pull_request(ctx, base_branch, head_branch, title, body),
        }
    }
}
