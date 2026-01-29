//! Main effect handler implementation.
//!
//! This module implements the EffectHandler trait to execute pipeline side effects
//! through the reducer architecture. Effect handlers perform actual work (agent
//! invocation, git operations, file I/O) and emit events.

use crate::agents::AgentRole;
use crate::checkpoint::{
    save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase as CheckpointPhase,
};
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::files::llm_output_extraction::validate_issues_xml;
use crate::phases::{commit, development, get_primary_commit_agent, review, PhaseContext};
use crate::pipeline::PipelineRuntime;
use crate::prompts::ContextLevel;
use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
use crate::reducer::event::{
    AgentErrorKind, CheckpointTrigger, ConflictStrategy, PipelineEvent, PipelinePhase, RebasePhase,
};
use crate::reducer::fault_tolerant_executor::{
    execute_agent_fault_tolerantly, AgentExecutionConfig,
};
use crate::reducer::state::PipelineState;
use crate::reducer::ui_event::{UIEvent, XmlCodeSnippet, XmlOutputContext, XmlOutputType};
use crate::workspace::Workspace;
use anyhow::Result;
use regex::Regex;
use std::path::{Path, PathBuf};

/// Main effect handler implementation.
///
/// This handler executes effects by calling existing pipeline functions,
/// maintaining compatibility while migrating to reducer architecture.
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

    fn is_auth_failure(err: &anyhow::Error) -> bool {
        if err.chain().any(|cause| {
            cause
                .downcast_ref::<development::AuthFailureError>()
                .is_some()
        }) {
            return true;
        }

        let msg = err.to_string().to_lowercase();
        msg.contains("authentication error")
            || msg.contains("auth/credential")
            || msg.contains("unauthorized")
            || msg.contains("credential")
            || msg.contains("api key")
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

            Effect::RunReviewPass { pass } => self.run_review_pass(ctx, pass),

            Effect::RunFixAttempt { pass } => self.run_fix_attempt(ctx, pass),

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

            Effect::ValidateFinalState => self.validate_final_state(ctx),

            Effect::SaveCheckpoint { trigger } => self.save_checkpoint(ctx, trigger),

            Effect::CleanupContext => self.cleanup_context(ctx),

            Effect::RestorePromptPermissions => self.restore_prompt_permissions(ctx),

            Effect::WriteContinuationContext(ref data) => {
                write_continuation_context_to_workspace(ctx.workspace, ctx.logger, data)?;
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

    fn invoke_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        role: AgentRole,
        agent: String,
        model: Option<String>,
        prompt: String,
    ) -> Result<EffectResult> {
        // Use agent from state.agent_chain if available
        let effective_agent = self
            .state
            .agent_chain
            .current_agent()
            .unwrap_or(&agent)
            .clone();

        let model_name = self.state.agent_chain.current_model();

        // Use continuation prompt if available (from rate-limited predecessor).
        //
        // Important: only use it when it's the *same* prompt as this invocation.
        // If the pipeline has generated a new prompt (retry/fallback instructions,
        // different phase/role, etc.), do not override it with stale continuation
        // context.
        let effective_prompt = match self
            .state
            .agent_chain
            .rate_limit_continuation_prompt
            .as_ref()
        {
            Some(saved) if saved == &prompt => saved.clone(),
            _ => prompt,
        };

        ctx.logger.info(&format!(
            "Executing with agent: {}, model: {:?}",
            effective_agent, model_name
        ));

        // Get agent configuration from registry
        let agent_config = ctx
            .registry
            .resolve_config(&effective_agent)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", effective_agent))?;

        // Determine log file path
        let safe_agent_name =
            crate::pipeline::logfile::sanitize_agent_name(&effective_agent.to_lowercase());
        let logfile = format!(".agent/logs/{}.log", safe_agent_name);

        // Build command string, honoring reducer-selected model (if any).
        // The reducer's agent chain drives model fallback (advance_to_next_model).
        // When present, the selected model must be threaded into the command.
        let model_override = model_name
            .map(std::string::String::as_str)
            .or(model.as_deref());
        let cmd_str = agent_config.build_cmd_with_model(true, true, true, model_override);

        // Build pipeline runtime
        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            executor: ctx.executor,
            executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
            workspace: ctx.workspace,
        };

        // Execute agent with fault-tolerant wrapper
        let config = AgentExecutionConfig {
            role,
            agent_name: &effective_agent,
            cmd_str: &cmd_str,
            parser_type: agent_config.json_parser,
            env_vars: &agent_config.env_vars,
            prompt: &effective_prompt,
            display_name: &effective_agent,
            logfile: &logfile,
        };

        let event = execute_agent_fault_tolerantly(config, &mut runtime)?;

        // Emit UI event for agent activity
        let ui_event = UIEvent::AgentActivity {
            agent: effective_agent.clone(),
            message: format!("Completed {} task", role),
        };

        Ok(EffectResult::with_ui(event, vec![ui_event]))
    }

    fn generate_plan(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        // Planning must honor the reducer-selected agent chain.
        // We achieve this by running the planning phase with a temporary PhaseContext
        // whose `developer_agent` is set to the current agent in `state.agent_chain`.
        let effective_agent = self
            .state
            .agent_chain
            .current_agent()
            .map(|s| s.as_str())
            .unwrap_or(ctx.developer_agent);

        match with_overridden_developer_agent(ctx, effective_agent, |inner_ctx| {
            development::run_planning_step(inner_ctx, iteration)
        }) {
            Ok(_) => {
                // Validate plan was created
                let plan_path = Path::new(".agent/PLAN.md");
                let plan_exists = ctx.workspace.exists(plan_path);
                let plan_content = if plan_exists {
                    ctx.workspace.read(plan_path).ok().unwrap_or_default()
                } else {
                    String::new()
                };

                let is_valid = plan_exists && !plan_content.trim().is_empty();

                let event = PipelineEvent::plan_generation_completed(iteration, is_valid);

                // Build UI events
                let mut ui_events = vec![];

                // Emit phase transition UI event when plan is valid
                if is_valid {
                    ui_events.push(self.phase_transition_ui(PipelinePhase::Development));

                    // Try to read plan XML for semantic rendering
                    let plan_xml_path = Path::new(".agent/tmp/plan.xml");
                    let processed_path = Path::new(".agent/tmp/plan.xml.processed");
                    if let Some(xml_content) = ctx
                        .workspace
                        .read(plan_xml_path)
                        .ok()
                        .or_else(|| ctx.workspace.read(processed_path).ok())
                    {
                        ui_events.push(UIEvent::XmlOutput {
                            xml_type: XmlOutputType::DevelopmentPlan,
                            content: xml_content,
                            context: Some(XmlOutputContext {
                                iteration: Some(iteration),
                                pass: None,
                                snippets: Vec::new(),
                            }),
                        });
                    }
                }

                Ok(EffectResult::with_ui(event, ui_events))
            }
            Err(err) => {
                if Self::is_auth_failure(&err) {
                    let current_agent = self
                        .state
                        .agent_chain
                        .current_agent()
                        .cloned()
                        .unwrap_or_else(|| ctx.developer_agent.to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Developer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                Ok(EffectResult::event(
                    PipelineEvent::plan_generation_completed(iteration, false),
                ))
            }
        }
    }

    fn run_development_iteration(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<EffectResult> {
        use crate::checkpoint::restore::ResumeContext;
        let developer_context = ContextLevel::from(ctx.config.developer_context);

        // Get current agent from agent chain
        let dev_agent = self.state.agent_chain.current_agent().cloned();

        // Get continuation state from reducer state
        let continuation_state = &self.state.continuation;
        // Config semantics: max_dev_continuations counts *continuation attempts* (fresh sessions)
        // allowed after the initial attempt. Total valid attempts per iteration is
        // `1 + max_dev_continuations`.
        let max_continuations = ctx.config.max_dev_continuations.unwrap_or(2);

        // Defensive guard: if checkpoint state already exceeds the configured limit,
        // abort rather than looping indefinitely.
        if continuation_state.continuation_attempt > max_continuations {
            return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                format!(
                    "Development continuation attempts exhausted (continuation_attempt={}, max_continuations={})",
                    continuation_state.continuation_attempt, max_continuations
                ),
            )));
        }

        // Clean stale continuation context when starting a fresh attempt.
        if continuation_state.continuation_attempt == 0 {
            let _ = cleanup_continuation_context_file(ctx);
        }

        // Run a single development attempt (one session) with XSD retry.
        let attempt = development::run_development_attempt_with_xml_retry(
            ctx,
            iteration,
            developer_context,
            false,
            None::<&ResumeContext>,
            dev_agent.as_deref(),
            continuation_state,
        );

        let attempt = match attempt {
            Ok(a) => a,
            Err(err) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    format!("Development attempt failed: {err}"),
                )));
            }
        };

        // Check for auth failure - trigger agent fallback immediately
        if attempt.auth_failure {
            let current_agent = dev_agent.clone().unwrap_or_else(|| "unknown".to_string());
            return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                current_agent,
                1,
                AgentErrorKind::Authentication,
                false,
            )));
        }

        // Check if output is invalid (XSD/XML parsing failed) - emit event, let reducer decide
        if !attempt.output_valid {
            let mut ui_events = vec![UIEvent::IterationProgress {
                current: iteration,
                total: self.state.total_iterations,
            }];

            // Try to read development result XML for semantic rendering.
            let dev_xml_path = Path::new(".agent/tmp/development_result.xml");
            let processed_path = Path::new(".agent/tmp/development_result.xml.processed");
            if let Some(xml_content) = ctx
                .workspace
                .read(dev_xml_path)
                .ok()
                .or_else(|| ctx.workspace.read(processed_path).ok())
            {
                ui_events.push(UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentResult,
                    content: xml_content,
                    context: Some(XmlOutputContext {
                        iteration: Some(iteration),
                        pass: None,
                        snippets: Vec::new(),
                    }),
                });
            }

            // Emit OutputValidationFailed - reducer decides whether to retry or switch agents
            return Ok(EffectResult::with_ui(
                PipelineEvent::development_output_validation_failed(
                    iteration,
                    continuation_state.invalid_output_attempts,
                ),
                ui_events,
            ));
        }

        // If we reached completed, the iteration can transition to commit.
        if attempt.output_valid
            && matches!(
                attempt.status,
                crate::reducer::state::DevelopmentStatus::Completed
            )
        {
            let _ = cleanup_continuation_context_file(ctx);

            let event = if continuation_state.is_continuation() {
                PipelineEvent::development_iteration_continuation_succeeded(
                    iteration,
                    continuation_state.continuation_attempt,
                )
            } else {
                PipelineEvent::development_iteration_completed(iteration, true)
            };

            let ui_event = UIEvent::IterationProgress {
                current: iteration,
                total: self.state.total_iterations,
            };

            let mut ui_events = vec![ui_event];

            // Try to read development result XML for semantic rendering.
            let dev_xml_path = Path::new(".agent/tmp/development_result.xml");
            let processed_path = Path::new(".agent/tmp/development_result.xml.processed");
            if let Some(xml_content) = ctx
                .workspace
                .read(dev_xml_path)
                .ok()
                .or_else(|| ctx.workspace.read(processed_path).ok())
            {
                ui_events.push(UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentResult,
                    content: xml_content,
                    context: Some(XmlOutputContext {
                        iteration: Some(iteration),
                        pass: None,
                        snippets: Vec::new(),
                    }),
                });
            }

            return Ok(EffectResult::with_ui(event, ui_events));
        }

        // Not completed (valid output): partial/failed status triggers a continuation attempt.
        // Check if continuation budget is exhausted - emit event, let reducer decide
        let next_attempt = continuation_state.continuation_attempt + 1;
        if next_attempt > max_continuations {
            let _ = cleanup_continuation_context_file(ctx);
            // Continuation budget exhausted: abort with a human-readable reason so logs/UI can
            // surface why the pipeline was interrupted.
            let reason = development_continuation_budget_exhausted_abort_reason(
                iteration,
                next_attempt,
                max_continuations,
                attempt.status.clone(),
            );
            return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(reason)));
        }

        ctx.logger.info(&format!(
            "Triggering development continuation attempt {}/{} (previous status={})",
            next_attempt, max_continuations, attempt.status
        ));

        // Write continuation context for the next attempt.
        // NOTE: this uses the same implementation as Effect::WriteContinuationContext.
        let status = attempt.status.clone();
        let summary = attempt.summary.clone();
        let files_changed = attempt.files_changed.clone();
        let next_steps = attempt.next_steps.clone();

        let context_data = crate::reducer::effect::ContinuationContextData {
            iteration,
            attempt: next_attempt,
            status: status.clone(),
            summary: summary.clone(),
            files_changed: files_changed.clone(),
            next_steps: next_steps.clone(),
        };
        write_continuation_context_to_workspace(ctx.workspace, ctx.logger, &context_data)?;

        let event = PipelineEvent::development_iteration_continuation_triggered(
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        );

        let mut ui_events = vec![UIEvent::IterationProgress {
            current: iteration,
            total: self.state.total_iterations,
        }];

        // Try to read development result XML for semantic rendering.
        let dev_xml_path = Path::new(".agent/tmp/development_result.xml");
        let processed_path = Path::new(".agent/tmp/development_result.xml.processed");
        if let Some(xml_content) = ctx
            .workspace
            .read(dev_xml_path)
            .ok()
            .or_else(|| ctx.workspace.read(processed_path).ok())
        {
            ui_events.push(UIEvent::XmlOutput {
                xml_type: XmlOutputType::DevelopmentResult,
                content: xml_content,
                context: Some(XmlOutputContext {
                    iteration: Some(iteration),
                    pass: None,
                    snippets: Vec::new(),
                }),
            });
        }

        Ok(EffectResult::with_ui(event, ui_events))
    }

    fn run_review_pass(&mut self, ctx: &mut PhaseContext<'_>, pass: u32) -> Result<EffectResult> {
        let review_label = format!("review_{}", pass);

        // Get current reviewer agent from agent chain
        let review_agent = self.state.agent_chain.current_agent().cloned();

        // Keep invalid-output attempt tracking deterministic by sourcing it from state.
        let invalid_output_attempt = self.state.continuation.invalid_output_attempts;

        let is_review_output_validation_failure = |workspace: &dyn Workspace| -> bool {
            let marker = "# Review Output XSD Validation Failure";
            workspace
                .read(Path::new(".agent/ISSUES.md"))
                .ok()
                .map(|s| s.starts_with(marker))
                .unwrap_or(false)
        };

        match review::run_review_pass(ctx, pass, &review_label, "", review_agent.as_deref()) {
            Ok(result) => {
                // Check for auth failure - trigger agent fallback
                if result.auth_failure {
                    let current_agent = review_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                let xsd_validation_failed = is_review_output_validation_failure(ctx.workspace);
                let issues_xml_content = ctx
                    .workspace
                    .read(Path::new(".agent/tmp/issues.xml"))
                    .ok()
                    .or_else(|| {
                        ctx.workspace
                            .read(Path::new(".agent/tmp/issues.xml.processed"))
                            .ok()
                    });

                let event = classify_review_pass_event(
                    pass,
                    invalid_output_attempt,
                    result.early_exit,
                    xsd_validation_failed,
                    issues_xml_content.as_deref(),
                );

                // Build UI events
                let mut ui_events = vec![
                    // Emit UI event for review progress
                    UIEvent::ReviewProgress {
                        pass,
                        total: self.state.total_reviewer_passes,
                    },
                ];

                // Try to read issues XML for semantic rendering
                if let Some(xml_content) = issues_xml_content {
                    let snippets = collect_review_issue_snippets(ctx.workspace, &xml_content);
                    ui_events.push(UIEvent::XmlOutput {
                        xml_type: XmlOutputType::ReviewIssues,
                        content: xml_content,
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets,
                        }),
                    });
                }

                Ok(EffectResult::with_ui(event, ui_events))
            }
            Err(err) => {
                if Self::is_auth_failure(&err) {
                    let current_agent = review_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                Ok(EffectResult::event(
                    PipelineEvent::review_output_validation_failed(pass, invalid_output_attempt),
                ))
            }
        }
    }

    fn run_fix_attempt(&mut self, ctx: &mut PhaseContext<'_>, pass: u32) -> Result<EffectResult> {
        use crate::checkpoint::restore::ResumeContext;
        let reviewer_context = ContextLevel::from(ctx.config.reviewer_context);

        // Get current reviewer agent from agent chain
        let fix_agent = self.state.agent_chain.current_agent().cloned();

        match review::run_fix_pass(
            ctx,
            pass,
            reviewer_context,
            None::<&ResumeContext>,
            fix_agent.as_deref(),
        ) {
            Ok(_) => {
                let event = PipelineEvent::fix_attempt_completed(pass, true);

                // Build UI events - try to read fix result XML for semantic rendering
                let mut ui_events = vec![];
                let fix_xml_path = Path::new(".agent/tmp/fix_result.xml");
                let processed_path = Path::new(".agent/tmp/fix_result.xml.processed");
                if let Some(xml_content) = ctx
                    .workspace
                    .read(fix_xml_path)
                    .ok()
                    .or_else(|| ctx.workspace.read(processed_path).ok())
                {
                    ui_events.push(UIEvent::XmlOutput {
                        xml_type: XmlOutputType::FixResult,
                        content: xml_content,
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    });
                }

                Ok(EffectResult::with_ui(event, ui_events))
            }
            Err(err) => {
                if Self::is_auth_failure(&err) {
                    let current_agent = fix_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                Ok(EffectResult::event(PipelineEvent::fix_attempt_completed(
                    pass, false,
                )))
            }
        }
    }

    fn run_rebase(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        phase: RebasePhase,
        target_branch: String,
    ) -> Result<EffectResult> {
        use crate::git_helpers::{get_conflicted_files, rebase_onto};

        match rebase_onto(&target_branch, _ctx.executor) {
            Ok(_) => {
                // Check for conflicts
                let conflicted_files = get_conflicted_files().unwrap_or_default();

                if !conflicted_files.is_empty() {
                    let files = conflicted_files.into_iter().map(|s| s.into()).collect();

                    Ok(EffectResult::event(
                        PipelineEvent::rebase_conflict_detected(files),
                    ))
                } else {
                    // Get current head for success case
                    let new_head = match git2::Repository::open(".") {
                        Ok(repo) => {
                            match repo.head().ok().and_then(|head| head.peel_to_commit().ok()) {
                                Some(commit) => commit.id().to_string(),
                                None => "unknown".to_string(),
                            }
                        }
                        Err(_) => "unknown".to_string(),
                    };

                    Ok(EffectResult::event(PipelineEvent::rebase_succeeded(
                        phase, new_head,
                    )))
                }
            }
            Err(e) => Ok(EffectResult::event(PipelineEvent::rebase_failed(
                phase,
                e.to_string(),
            ))),
        }
    }

    fn resolve_rebase_conflicts(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        strategy: ConflictStrategy,
    ) -> Result<EffectResult> {
        use crate::git_helpers::{abort_rebase, continue_rebase, get_conflicted_files};

        match strategy {
            ConflictStrategy::Continue => match continue_rebase(_ctx.executor) {
                Ok(_) => {
                    let files = get_conflicted_files()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|s| s.into())
                        .collect();

                    Ok(EffectResult::event(
                        PipelineEvent::rebase_conflict_resolved(files),
                    ))
                }
                Err(e) => Ok(EffectResult::event(PipelineEvent::rebase_failed(
                    RebasePhase::PostReview,
                    e.to_string(),
                ))),
            },
            ConflictStrategy::Abort => match abort_rebase(_ctx.executor) {
                Ok(_) => {
                    let restored_to = match git2::Repository::open(".") {
                        Ok(repo) => {
                            match repo.head().ok().and_then(|head| head.peel_to_commit().ok()) {
                                Some(commit) => commit.id().to_string(),
                                None => "HEAD".to_string(),
                            }
                        }
                        Err(_) => "HEAD".to_string(),
                    };

                    Ok(EffectResult::event(PipelineEvent::rebase_aborted(
                        RebasePhase::PostReview,
                        restored_to,
                    )))
                }
                Err(e) => Ok(EffectResult::event(PipelineEvent::rebase_failed(
                    RebasePhase::PostReview,
                    e.to_string(),
                ))),
            },
            ConflictStrategy::Skip => Ok(EffectResult::event(
                PipelineEvent::rebase_conflict_resolved(Vec::new()),
            )),
        }
    }

    fn generate_commit_message(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        let attempt = match &self.state.commit {
            crate::reducer::state::CommitState::Generating { attempt, .. } => *attempt,
            _ => 1,
        };

        // Get git diff for commit message generation
        let diff = crate::git_helpers::git_diff().unwrap_or_default();

        // Check if diff is empty BEFORE attempting to generate commit message
        // This prevents the "Empty diff provided to generate_commit_message" warning
        if diff.trim().is_empty() {
            ctx.logger
                .info("No changes to commit (empty diff), skipping commit");
            return Ok(EffectResult::event(PipelineEvent::commit_skipped(
                "No changes to commit (empty diff)".to_string(),
            )));
        }

        // Get commit agent first to avoid borrow conflicts
        let commit_agent = get_primary_commit_agent(ctx).unwrap_or_else(|| "commit".to_string());

        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            executor: ctx.executor,
            executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
            workspace: ctx.workspace,
        };

        match commit::generate_commit_message(
            &diff,
            ctx.registry,
            &mut runtime,
            &commit_agent,
            ctx.template_context,
            ctx.workspace,
            &ctx.prompt_history,
        ) {
            Ok(result) => {
                let event =
                    PipelineEvent::commit_message_generated(result.message.clone(), attempt);

                // Build UI events
                let mut ui_events = vec![
                    // Emit phase transition UI event
                    self.phase_transition_ui(PipelinePhase::CommitMessage),
                ];

                // Try to read commit message XML for semantic rendering
                if let Some(xml_content) = read_commit_message_xml(ctx.workspace) {
                    ui_events.push(UIEvent::XmlOutput {
                        xml_type: XmlOutputType::CommitMessage,
                        content: xml_content,
                        context: None,
                    });
                }

                Ok(EffectResult::with_ui(event, ui_events))
            }
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::commit_message_generated(
                    "chore: automated commit".to_string(),
                    attempt,
                ),
            )),
        }
    }

    fn create_commit(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        message: String,
    ) -> Result<EffectResult> {
        use crate::git_helpers::{git_add_all, git_commit};

        // Stage all changes
        git_add_all()?;

        // Create commit
        match git_commit(&message, None, None, Some(ctx.executor)) {
            Ok(Some(hash)) => Ok(EffectResult::event(PipelineEvent::commit_created(
                hash.to_string(),
                message,
            ))),
            Ok(None) => {
                // No changes to commit - skip to FinalValidation instead of failing
                // This prevents infinite loop when there are no changes
                Ok(EffectResult::event(PipelineEvent::commit_skipped(
                    "No changes to commit".to_string(),
                )))
            }
            Err(e) => Ok(EffectResult::event(
                PipelineEvent::commit_generation_failed(e.to_string()),
            )),
        }
    }

    fn skip_commit(&mut self, _ctx: &mut PhaseContext<'_>, reason: String) -> Result<EffectResult> {
        Ok(EffectResult::event(PipelineEvent::commit_skipped(reason)))
    }

    fn validate_final_state(&mut self, _ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        // Transition to Finalizing phase to restore PROMPT.md permissions
        // via the effect system before marking the pipeline complete
        let event = PipelineEvent::finalizing_started();

        // Emit phase transition UI event
        let ui_event = self.phase_transition_ui(PipelinePhase::Finalizing);

        Ok(EffectResult::with_ui(event, vec![ui_event]))
    }

    fn save_checkpoint(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        trigger: CheckpointTrigger,
    ) -> Result<EffectResult> {
        if ctx.config.features.checkpoint_enabled {
            let _ = save_checkpoint_from_state(&self.state, ctx);
        }

        Ok(EffectResult::event(PipelineEvent::checkpoint_saved(
            trigger,
        )))
    }

    fn initialize_agent_chain(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        role: AgentRole,
    ) -> Result<EffectResult> {
        let agents = match role {
            AgentRole::Developer => vec![ctx.developer_agent.to_string()],
            AgentRole::Reviewer => vec![ctx.reviewer_agent.to_string()],
            AgentRole::Commit => {
                if let Some(commit_agent) = get_primary_commit_agent(ctx) {
                    vec![commit_agent]
                } else {
                    vec![]
                }
            }
        };

        let _models_per_agent: Vec<Vec<String>> = agents.iter().map(|_| vec![]).collect();

        let max_cycles = self.state.agent_chain.max_cycles;

        ctx.logger.info(&format!(
            "Initializing agent chain with {} cycles",
            max_cycles
        ));

        let event = PipelineEvent::agent_chain_initialized(role, agents);

        // Emit phase transition when entering a new major phase
        let ui_events = match role {
            AgentRole::Developer if self.state.phase == PipelinePhase::Planning => {
                vec![UIEvent::PhaseTransition {
                    from: None,
                    to: PipelinePhase::Planning,
                }]
            }
            AgentRole::Reviewer if self.state.phase == PipelinePhase::Review => {
                vec![self.phase_transition_ui(PipelinePhase::Review)]
            }
            _ => vec![],
        };

        Ok(EffectResult::with_ui(event, ui_events))
    }

    fn cleanup_context(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        use std::path::Path;

        ctx.logger
            .info("Cleaning up context files to prevent pollution...");

        let mut cleaned_count = 0;
        let mut failed_count = 0;

        // Delete PLAN.md via workspace
        let plan_path = Path::new(".agent/PLAN.md");
        if ctx.workspace.exists(plan_path) {
            if let Err(err) = ctx.workspace.remove(plan_path) {
                ctx.logger.warn(&format!("Failed to delete PLAN.md: {err}"));
                failed_count += 1;
            } else {
                cleaned_count += 1;
            }
        }

        // Delete ISSUES.md (may not exist if in isolation mode) via workspace
        let issues_path = Path::new(".agent/ISSUES.md");
        if ctx.workspace.exists(issues_path) {
            if let Err(err) = ctx.workspace.remove(issues_path) {
                ctx.logger
                    .warn(&format!("Failed to delete ISSUES.md: {err}"));
                failed_count += 1;
            } else {
                cleaned_count += 1;
            }
        }

        // Delete ALL .xml files in .agent/tmp/ to prevent context pollution via workspace
        let tmp_dir = Path::new(".agent/tmp");
        if ctx.workspace.exists(tmp_dir) {
            if let Ok(entries) = ctx.workspace.read_dir(tmp_dir) {
                for entry in entries {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("xml") {
                        if let Err(err) = ctx.workspace.remove(path) {
                            ctx.logger.warn(&format!(
                                "Failed to delete {}: {}",
                                path.display(),
                                err
                            ));
                            failed_count += 1;
                        } else {
                            cleaned_count += 1;
                        }
                    }
                }
            }
        }

        // Delete continuation context file (if present) via workspace
        let _ = cleanup_continuation_context_file(ctx);

        if cleaned_count > 0 {
            ctx.logger.success(&format!(
                "Context cleanup complete: {} files deleted{}",
                cleaned_count,
                if failed_count > 0 {
                    format!(", {} failures", failed_count)
                } else {
                    String::new()
                }
            ));
        } else {
            ctx.logger.info("No context files to clean up");
        }

        Ok(EffectResult::event(PipelineEvent::context_cleaned()))
    }

    fn restore_prompt_permissions(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        use crate::files::make_prompt_writable_with_workspace;

        ctx.logger.info("Restoring PROMPT.md write permissions...");

        // Use workspace-based function for testability
        if let Some(warning) = make_prompt_writable_with_workspace(ctx.workspace) {
            ctx.logger.warn(&warning);
        }

        let event = PipelineEvent::prompt_permissions_restored();

        // Emit phase transition UI event to Complete
        let ui_event = self.phase_transition_ui(PipelinePhase::Complete);

        Ok(EffectResult::with_ui(event, vec![ui_event]))
    }

    fn cleanup_continuation_context(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        let path = Path::new(".agent/tmp/continuation_context.md");
        if ctx.workspace.exists(path) {
            ctx.workspace.remove(path)?;
        }
        Ok(EffectResult::event(
            PipelineEvent::development_continuation_context_cleaned(),
        ))
    }
}

/// Write continuation context to workspace.
///
/// This is extracted as a helper to keep the handler method concise.
fn write_continuation_context_to_workspace(
    workspace: &dyn Workspace,
    logger: &crate::logger::Logger,
    data: &crate::reducer::effect::ContinuationContextData,
) -> Result<()> {
    let tmp_dir = Path::new(".agent/tmp");
    if !workspace.exists(tmp_dir) {
        workspace.create_dir_all(tmp_dir)?;
    }

    let mut content = String::new();
    content.push_str("# Development Continuation Context\n\n");
    content.push_str(&format!("- Iteration: {}\n", data.iteration));
    content.push_str(&format!("- Continuation attempt: {}\n", data.attempt));
    content.push_str(&format!("- Previous status: {}\n\n", data.status));

    content.push_str("## Previous summary\n\n");
    content.push_str(&data.summary);
    content.push('\n');

    if let Some(ref files) = data.files_changed {
        content.push_str("\n## Files changed\n\n");
        for file in files {
            content.push_str("- ");
            content.push_str(file);
            content.push('\n');
        }
    }

    if let Some(ref steps) = data.next_steps {
        content.push_str("\n## Recommended next steps\n\n");
        content.push_str(steps);
        content.push('\n');
    }

    content.push_str("\n## Reference files (do not modify)\n\n");
    content.push_str("- PROMPT.md\n");
    content.push_str("- .agent/PLAN.md\n");

    workspace.write(Path::new(".agent/tmp/continuation_context.md"), &content)?;

    logger.info("Continuation context written to .agent/tmp/continuation_context.md");

    Ok(())
}

fn with_overridden_developer_agent<R>(
    ctx: &mut PhaseContext<'_>,
    developer_agent: &str,
    run: impl for<'a> FnOnce(&mut PhaseContext<'a>) -> R,
) -> R {
    // PhaseContext owns some state (run_context/execution_history/prompt_history).
    // To override `developer_agent` without leaking lifetimes, we temporarily move
    // those owned values into a new PhaseContext with a shorter lifetime.
    let run_context = std::mem::take(&mut ctx.run_context);
    let execution_history = std::mem::take(&mut ctx.execution_history);
    let prompt_history = std::mem::take(&mut ctx.prompt_history);

    let (result, run_context, execution_history, prompt_history) = {
        let mut inner_ctx = PhaseContext {
            config: ctx.config,
            registry: ctx.registry,
            logger: ctx.logger,
            colors: ctx.colors,
            timer: &mut *ctx.timer,
            stats: &mut *ctx.stats,
            developer_agent,
            reviewer_agent: ctx.reviewer_agent,
            review_guidelines: ctx.review_guidelines,
            template_context: ctx.template_context,
            run_context,
            execution_history,
            prompt_history,
            executor: ctx.executor,
            executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
            repo_root: ctx.repo_root,
            workspace: ctx.workspace,
        };

        let result = run(&mut inner_ctx);
        (
            result,
            inner_ctx.run_context,
            inner_ctx.execution_history,
            inner_ctx.prompt_history,
        )
    };

    ctx.run_context = run_context;
    ctx.execution_history = execution_history;
    ctx.prompt_history = prompt_history;

    result
}

fn collect_review_issue_snippets(
    workspace: &dyn Workspace,
    issues_xml: &str,
) -> Vec<XmlCodeSnippet> {
    let validated = match validate_issues_xml(issues_xml) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut snippets = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for issue in validated.issues {
        if let Some((file, issue_start, issue_end)) = parse_issue_location(&issue) {
            if let Some(snippet) = read_snippet_for_issue(workspace, &file, issue_start, issue_end)
            {
                let key = (
                    snippet.file.clone(),
                    snippet.line_start,
                    snippet.line_end,
                    snippet.content.clone(),
                );
                if seen.insert(key) {
                    snippets.push(snippet);
                }
            }
        }
    }

    snippets
}

fn read_commit_message_xml(workspace: &dyn Workspace) -> Option<String> {
    let primary_path = Path::new(xml_paths::COMMIT_MESSAGE_XML);
    let primary_processed_path =
        PathBuf::from(format!("{}.processed", xml_paths::COMMIT_MESSAGE_XML));
    let legacy_path = Path::new(".agent/tmp/commit.xml");
    let legacy_processed_path = Path::new(".agent/tmp/commit.xml.processed");

    workspace
        .read(primary_path)
        .ok()
        .or_else(|| workspace.read(&primary_processed_path).ok())
        .or_else(|| workspace.read(legacy_path).ok())
        .or_else(|| workspace.read(legacy_processed_path).ok())
}

fn parse_issue_location(issue: &str) -> Option<(String, u32, u32)> {
    let location_re = Regex::new(
        r"(?m)(?P<file>[-_./A-Za-z0-9]+\.[A-Za-z0-9]+):(?P<start>\d+)(?:[-–—](?P<end>\d+))?(?::(?P<col>\d+))?",
    )
    .ok()?;
    let gh_location_re = Regex::new(
        r"(?m)(?P<file>[-_./A-Za-z0-9]+\.[A-Za-z0-9]+)#L(?P<start>\d+)(?:-L(?P<end>\d+))?",
    )
    .ok()?;

    if let Some(cap) = location_re.captures(issue) {
        let file = cap.name("file")?.as_str().to_string();
        let start = cap.name("start")?.as_str().parse::<u32>().ok()?;
        let end = cap
            .name("end")
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(start);
        return Some((file, start, end));
    }

    if let Some(cap) = gh_location_re.captures(issue) {
        let file = cap.name("file")?.as_str().to_string();
        let start = cap.name("start")?.as_str().parse::<u32>().ok()?;
        let end = cap
            .name("end")
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(start);
        return Some((file, start, end));
    }

    None
}

fn read_snippet_for_issue(
    workspace: &dyn Workspace,
    file: &str,
    issue_start: u32,
    issue_end: u32,
) -> Option<XmlCodeSnippet> {
    let issue_start = issue_start.max(1);
    let issue_end = issue_end.max(issue_start);

    let context_lines: u32 = 2;
    let start = issue_start.saturating_sub(context_lines).max(1);
    let end = issue_end.saturating_add(context_lines);

    let content = workspace.read(Path::new(file)).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let max_line = u32::try_from(lines.len()).ok()?;
    let end = end.min(max_line);
    if start > end {
        return None;
    }

    let mut snippet = String::new();
    for line_no in start..=end {
        let idx = usize::try_from(line_no.saturating_sub(1)).ok()?;
        let line = lines.get(idx).copied().unwrap_or_default();
        snippet.push_str(&format!("{:>4} | {}\n", line_no, line));
    }

    Some(XmlCodeSnippet {
        file: file.to_string(),
        line_start: start,
        line_end: end,
        content: snippet,
    })
}

fn cleanup_continuation_context_file(ctx: &mut PhaseContext<'_>) -> anyhow::Result<()> {
    let path = Path::new(".agent/tmp/continuation_context.md");
    if ctx.workspace.exists(path) {
        ctx.workspace.remove(path)?;
    }
    Ok(())
}

fn classify_review_pass_event(
    pass: u32,
    invalid_output_attempt: u32,
    early_exit: bool,
    xsd_validation_failed: bool,
    issues_xml: Option<&str>,
) -> PipelineEvent {
    if xsd_validation_failed {
        return PipelineEvent::review_output_validation_failed(pass, invalid_output_attempt);
    }

    if let Some(xml) = issues_xml {
        if let Ok(validated) = validate_issues_xml(xml) {
            let no_issues = validated.issues.is_empty() && validated.no_issues_found.is_some();
            if no_issues {
                if early_exit {
                    return PipelineEvent::review_phase_completed(true);
                }
                return PipelineEvent::review_pass_completed_clean(pass);
            }
        }
    }

    // Default: treat as issues found (review produced a valid pass result and did not signal
    // output validation failure).
    PipelineEvent::review_completed(pass, true)
}

fn development_continuation_budget_exhausted_abort_reason(
    iteration: u32,
    next_attempt: u32,
    max_continuations: u32,
    last_status: crate::reducer::state::DevelopmentStatus,
) -> String {
    format!(
        "Development continuation attempts exhausted (iteration={iteration}, next_attempt={next_attempt}, max_continuations={max_continuations}, last_status={last_status})"
    )
}

/// Save checkpoint from current pipeline state.
fn save_checkpoint_from_state(
    state: &PipelineState,
    ctx: &mut PhaseContext<'_>,
) -> anyhow::Result<()> {
    let builder = CheckpointBuilder::new()
        .phase(
            map_to_checkpoint_phase(state.phase),
            state.iteration,
            state.total_iterations,
        )
        .reviewer_pass(state.reviewer_pass, state.total_reviewer_passes)
        .capture_from_context(
            ctx.config,
            ctx.registry,
            ctx.developer_agent,
            ctx.reviewer_agent,
            ctx.logger,
            &ctx.run_context,
        )
        .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
        .with_execution_history(ctx.execution_history.clone())
        .with_prompt_history(ctx.clone_prompt_history());

    if let Some(checkpoint) = builder.build() {
        let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
    }

    Ok(())
}

/// Map reducer phase to checkpoint phase.
fn map_to_checkpoint_phase(phase: crate::reducer::event::PipelinePhase) -> CheckpointPhase {
    match phase {
        crate::reducer::event::PipelinePhase::Planning => CheckpointPhase::Planning,
        crate::reducer::event::PipelinePhase::Development => CheckpointPhase::Development,
        crate::reducer::event::PipelinePhase::Review => CheckpointPhase::Review,
        crate::reducer::event::PipelinePhase::CommitMessage => CheckpointPhase::CommitMessage,
        crate::reducer::event::PipelinePhase::FinalValidation => CheckpointPhase::FinalValidation,
        crate::reducer::event::PipelinePhase::Finalizing => CheckpointPhase::FinalValidation,
        crate::reducer::event::PipelinePhase::Complete => CheckpointPhase::Complete,
        crate::reducer::event::PipelinePhase::Interrupted => CheckpointPhase::Interrupted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that RestorePromptPermissions effect returns the correct event.
    ///
    /// The actual workspace interaction is tested via integration tests.
    /// This unit test verifies the mock handler returns the expected event.
    #[test]
    fn test_mock_handler_restore_prompt_permissions() {
        use crate::reducer::mock_effect_handler::MockEffectHandler;

        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let result = handler.execute_mock(Effect::RestorePromptPermissions);

        assert!(
            matches!(result.event, PipelineEvent::PromptPermissionsRestored),
            "RestorePromptPermissions effect should return PromptPermissionsRestored event"
        );

        assert!(
            handler.was_effect_executed(|e| matches!(e, Effect::RestorePromptPermissions)),
            "Effect should be captured"
        );
    }

    /// Test that ValidateFinalState transitions to Finalizing phase, not Complete.
    ///
    /// This ensures that the reducer goes through Finalizing phase to restore
    /// PROMPT.md permissions before marking the pipeline complete.
    #[test]
    fn test_mock_handler_validate_final_state_goes_to_finalizing() {
        use crate::reducer::mock_effect_handler::MockEffectHandler;

        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let result = handler.execute_mock(Effect::ValidateFinalState);

        assert!(
            matches!(result.event, PipelineEvent::FinalizingStarted),
            "ValidateFinalState should return FinalizingStarted to trigger finalization phase, got: {:?}",
            result.event
        );
    }

    #[test]
    fn test_map_to_checkpoint_phase_interrupted_maps_to_interrupted() {
        use crate::reducer::event::PipelinePhase;

        assert_eq!(
            map_to_checkpoint_phase(PipelinePhase::Interrupted),
            CheckpointPhase::Interrupted
        );
    }

    /// Test that cleanup_context uses workspace for file operations.
    ///
    /// This verifies that cleanup_context:
    /// 1. Deletes PLAN.md via workspace
    /// 2. Deletes ISSUES.md via workspace  
    /// 3. Deletes .xml files in .agent/tmp/ via workspace
    #[test]
    fn test_cleanup_context_uses_workspace() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::context::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::workspace::{MemoryWorkspace, Workspace};
        use std::path::{Path, PathBuf};

        // Create workspace with files that should be cleaned
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/PLAN.md", "# Plan")
            .with_file(".agent/ISSUES.md", "# Issues")
            .with_dir(".agent/tmp")
            .with_file(".agent/tmp/issues.xml", "<issues/>")
            .with_file(".agent/tmp/development_result.xml", "<result/>")
            .with_file(".agent/tmp/keep.txt", "not xml");

        // Set up all the context fields
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();
        let executor_arc = std::sync::Arc::new(MockProcessExecutor::new())
            as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
        let repo_root = PathBuf::from("/test/repo");

        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "test-dev",
            reviewer_agent: "test-reviewer",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*executor_arc,
            executor_arc: std::sync::Arc::clone(&executor_arc),
            repo_root: &repo_root,
            workspace: &workspace,
        };

        // Create a real handler and call cleanup_context
        let state = PipelineState::initial(1, 0);
        let mut handler = super::MainEffectHandler::new(state);
        let result = handler.cleanup_context(&mut ctx);

        assert!(result.is_ok(), "cleanup_context should succeed");

        // Verify files were deleted via workspace
        assert!(
            !workspace.exists(Path::new(".agent/PLAN.md")),
            "PLAN.md should be deleted via workspace"
        );
        assert!(
            !workspace.exists(Path::new(".agent/ISSUES.md")),
            "ISSUES.md should be deleted via workspace"
        );
        assert!(
            !workspace.exists(Path::new(".agent/tmp/issues.xml")),
            "issues.xml should be deleted via workspace"
        );
        assert!(
            !workspace.exists(Path::new(".agent/tmp/development_result.xml")),
            "development_result.xml should be deleted via workspace"
        );
        // Non-xml files should remain
        assert!(
            workspace.exists(Path::new(".agent/tmp/keep.txt")),
            "non-xml file should not be deleted"
        );
    }

    /// Test that save_checkpoint uses workspace for file operations.
    ///
    /// This verifies that save_checkpoint writes to .agent/checkpoint.json
    /// via the workspace abstraction, not std::fs.
    #[test]
    fn test_save_checkpoint_uses_workspace() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::context::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::workspace::{MemoryWorkspace, Workspace};
        use std::path::{Path, PathBuf};

        // Create an empty workspace - no checkpoint should exist initially
        let workspace = MemoryWorkspace::new_test();

        // Verify no checkpoint exists before
        assert!(
            !workspace.exists(Path::new(".agent/checkpoint.json")),
            "checkpoint should not exist initially"
        );

        // Set up all the context fields - use real agent names from the registry
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();
        let executor_arc = std::sync::Arc::new(MockProcessExecutor::new())
            as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
        let repo_root = PathBuf::from("/test/repo");

        // Use "claude" which should exist in the default registry
        let developer_agent = "claude";
        let reviewer_agent = "claude";

        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent,
            reviewer_agent,
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*executor_arc,
            executor_arc: std::sync::Arc::clone(&executor_arc),
            repo_root: &repo_root,
            workspace: &workspace,
        };

        // Create state and handler
        let state = PipelineState::initial(1, 0);
        let mut handler = super::MainEffectHandler::new(state);

        // Execute save checkpoint effect
        let result = handler.save_checkpoint(&mut ctx, CheckpointTrigger::PhaseTransition);

        assert!(result.is_ok(), "save_checkpoint should succeed");

        // Verify checkpoint was written via workspace
        assert!(
            workspace.exists(Path::new(".agent/checkpoint.json")),
            "checkpoint should be written via workspace"
        );

        // Verify the content is valid JSON
        let content = workspace.read(Path::new(".agent/checkpoint.json")).unwrap();
        assert!(
            content.contains("\"phase\""),
            "checkpoint should contain phase field"
        );
        assert!(
            content.contains("\"version\""),
            "checkpoint should contain version field"
        );
    }

    #[test]
    fn test_read_commit_message_xml_falls_back_to_legacy_commit_xml() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/tmp")
            .with_file(".agent/tmp/commit.xml", "<legacy/>");

        let xml = read_commit_message_xml(&workspace).expect("expected xml");
        assert_eq!(xml, "<legacy/>");
    }

    #[test]
    fn test_read_commit_message_xml_prefers_commit_message_xml() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/tmp")
            .with_file(".agent/tmp/commit.xml", "<legacy/>")
            .with_file(".agent/tmp/commit_message.xml", "<preferred/>");

        let xml = read_commit_message_xml(&workspace).expect("expected xml");
        assert_eq!(xml, "<preferred/>");
    }

    #[test]
    fn test_invoke_agent_sanitizes_logfile_name() {
        use crate::agents::{AgentConfig, AgentRegistry, JsonParserType};
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::context::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::state::AgentChainState;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let mut registry = AgentRegistry::new().unwrap();
        registry.register(
            "ccs/glm",
            AgentConfig {
                cmd: "mock-glm-agent".to_string(),
                output_flag: String::new(),
                yolo_flag: String::new(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
                model_flag: None,
                print_flag: String::new(),
                streaming_flag: String::new(),
                session_flag: String::new(),
                env_vars: std::collections::HashMap::new(),
                display_name: Some("mock".to_string()),
            },
        );

        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();

        let mock_executor = std::sync::Arc::new(MockProcessExecutor::new().with_agent_result(
            "mock-glm-agent",
            Ok(crate::executor::AgentCommandResult::success()),
        ));
        let executor_arc =
            mock_executor.clone() as std::sync::Arc<dyn crate::executor::ProcessExecutor>;

        let workspace = MemoryWorkspace::new_test();
        let repo_root = PathBuf::from("/test/repo");
        let config = Config::default();

        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "ccs/glm",
            reviewer_agent: "ccs/glm",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: executor_arc.as_ref(),
            executor_arc: executor_arc.clone(),
            repo_root: &repo_root,
            workspace: &workspace,
        };

        let state = PipelineState {
            agent_chain: AgentChainState::initial().with_agents(
                vec!["ccs/glm".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(1, 0)
        };

        let mut handler = super::MainEffectHandler::new(state);
        let _ = handler
            .invoke_agent(
                &mut ctx,
                AgentRole::Developer,
                "ccs/glm".to_string(),
                None,
                "prompt".to_string(),
            )
            .unwrap();

        let calls = mock_executor.agent_calls_for("mock-glm-agent");
        assert_eq!(calls.len(), 1, "expected one agent spawn call");
        let logfile = &calls[0].logfile;
        assert!(
            !logfile.contains("ccs/glm"),
            "logfile should not contain raw agent name with slashes: {logfile}"
        );
        assert!(
            logfile.contains("ccs-glm"),
            "logfile should use sanitized agent name: {logfile}"
        );
    }

    #[test]
    fn test_invoke_agent_applies_selected_model_to_command() {
        use crate::agents::{AgentConfig, AgentRegistry, JsonParserType};
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::context::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::state::AgentChainState;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let mut registry = AgentRegistry::new().unwrap();
        registry.register(
            "mock-agent",
            AgentConfig {
                cmd: "mock-agent-bin".to_string(),
                output_flag: String::new(),
                yolo_flag: String::new(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
                model_flag: None,
                print_flag: String::new(),
                streaming_flag: String::new(),
                session_flag: String::new(),
                env_vars: std::collections::HashMap::new(),
                display_name: Some("mock".to_string()),
            },
        );

        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();

        let mock_executor = std::sync::Arc::new(MockProcessExecutor::new().with_agent_result(
            "mock-agent-bin",
            Ok(crate::executor::AgentCommandResult::success()),
        ));
        let executor_arc =
            mock_executor.clone() as std::sync::Arc<dyn crate::executor::ProcessExecutor>;

        let workspace = MemoryWorkspace::new_test();
        let repo_root = PathBuf::from("/test/repo");
        let config = Config::default();

        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "mock-agent",
            reviewer_agent: "mock-agent",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: executor_arc.as_ref(),
            executor_arc: executor_arc.clone(),
            repo_root: &repo_root,
            workspace: &workspace,
        };

        let selected_model = "-m openai/gpt-5.2".to_string();
        let state = PipelineState {
            agent_chain: AgentChainState::initial().with_agents(
                vec!["mock-agent".to_string()],
                vec![vec![selected_model.clone()]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(1, 0)
        };

        let mut handler = super::MainEffectHandler::new(state);
        let _ = handler
            .invoke_agent(
                &mut ctx,
                AgentRole::Developer,
                "mock-agent".to_string(),
                None,
                "prompt".to_string(),
            )
            .unwrap();

        let calls = mock_executor.agent_calls_for("mock-agent-bin");
        assert_eq!(calls.len(), 1, "expected one agent spawn call");
        assert!(
            calls[0].args.iter().any(|a| a == "-m"
                || a == "-m=openai/gpt-5.2"
                || a.contains("openai/gpt-5.2")
                || a == &selected_model),
            "expected selected model to be threaded into agent command args; args={:?}",
            calls[0].args
        );
    }

    #[test]
    fn test_invoke_agent_does_not_override_new_prompt_with_stale_rate_limit_prompt() {
        use crate::agents::{AgentConfig, AgentRegistry, JsonParserType};
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::context::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::reducer::state::AgentChainState;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let mut registry = AgentRegistry::new().unwrap();
        registry.register(
            "mock-agent",
            AgentConfig {
                cmd: "mock-agent-bin".to_string(),
                output_flag: String::new(),
                yolo_flag: String::new(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
                model_flag: None,
                print_flag: String::new(),
                streaming_flag: String::new(),
                session_flag: String::new(),
                env_vars: std::collections::HashMap::new(),
                display_name: Some("mock".to_string()),
            },
        );

        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();

        let mock_executor = std::sync::Arc::new(MockProcessExecutor::new().with_agent_result(
            "mock-agent-bin",
            Ok(crate::executor::AgentCommandResult::success()),
        ));
        let executor_arc =
            mock_executor.clone() as std::sync::Arc<dyn crate::executor::ProcessExecutor>;

        let workspace = MemoryWorkspace::new_test();
        let repo_root = PathBuf::from("/test/repo");
        let config = Config::default();

        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "mock-agent",
            reviewer_agent: "mock-agent",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: executor_arc.as_ref(),
            executor_arc: executor_arc.clone(),
            repo_root: &repo_root,
            workspace: &workspace,
        };

        let mut state = PipelineState {
            agent_chain: AgentChainState::initial().with_agents(
                vec!["mock-agent".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(1, 0)
        };
        state.agent_chain.rate_limit_continuation_prompt = Some("stale".to_string());

        let mut handler = super::MainEffectHandler::new(state);
        let _ = handler
            .invoke_agent(
                &mut ctx,
                AgentRole::Developer,
                "mock-agent".to_string(),
                None,
                "fresh".to_string(),
            )
            .unwrap();

        let calls = mock_executor.agent_calls_for("mock-agent-bin");
        assert_eq!(calls.len(), 1, "expected one agent spawn call");
        assert_eq!(
            calls[0].prompt,
            "fresh",
            "invoke_agent should not override a new prompt with a stale rate_limit_continuation_prompt"
        );
    }

    #[test]
    fn test_with_overridden_developer_agent_overrides_only_inner_ctx() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::workspace::MemoryWorkspace;
        use std::collections::HashMap;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new_test();
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();
        let executor_arc = std::sync::Arc::new(MockProcessExecutor::new())
            as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
        let repo_root = PathBuf::from("/test/repo");

        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "primary-agent",
            reviewer_agent: "reviewer-agent",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: HashMap::new(),
            executor: &*executor_arc,
            executor_arc: std::sync::Arc::clone(&executor_arc),
            repo_root: &repo_root,
            workspace: &workspace,
        };

        let orig_dev = ctx.developer_agent;
        ctx.prompt_history
            .insert("existing".to_string(), "value".to_string());

        let res = with_overridden_developer_agent(&mut ctx, "fallback-agent", |inner| {
            assert_eq!(inner.developer_agent, "fallback-agent");
            inner
                .prompt_history
                .insert("new".to_string(), "prompt".to_string());
            inner.record_developer_iteration();
            7
        });

        assert_eq!(res, 7);
        assert_eq!(ctx.developer_agent, orig_dev);
        assert!(ctx.prompt_history.contains_key("existing"));
        assert!(ctx.prompt_history.contains_key("new"));
    }

    #[test]
    fn test_classify_review_pass_event_no_issues_without_early_exit_emits_clean_pass() {
        let xml = r#"<ralph-issues>
 <ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
 </ralph-issues>"#;

        let event = classify_review_pass_event(0, 0, false, false, Some(xml));

        assert!(matches!(
            event,
            PipelineEvent::Review(crate::reducer::event::ReviewEvent::PassCompletedClean {
                pass: 0
            })
        ));
    }

    #[test]
    fn test_classify_review_pass_event_no_issues_with_early_exit_emits_phase_completed() {
        let xml = r#"<ralph-issues>
 <ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
 </ralph-issues>"#;

        let event = classify_review_pass_event(0, 0, true, false, Some(xml));

        assert!(matches!(
            event,
            PipelineEvent::Review(crate::reducer::event::ReviewEvent::PhaseCompleted {
                early_exit: true
            })
        ));
    }

    #[test]
    fn test_classify_review_pass_event_issues_found_emits_completed_with_issues_found_true() {
        let xml = r#"<ralph-issues>
 <ralph-issue>Something is wrong</ralph-issue>
 </ralph-issues>"#;

        let event = classify_review_pass_event(3, 0, false, false, Some(xml));

        assert!(matches!(
            event,
            PipelineEvent::Review(crate::reducer::event::ReviewEvent::Completed {
                pass: 3,
                issues_found: true
            })
        ));
    }

    #[test]
    fn test_development_continuation_budget_exhausted_abort_reason_is_informative() {
        use crate::reducer::state::DevelopmentStatus;

        let reason = development_continuation_budget_exhausted_abort_reason(
            7,
            3,
            2,
            DevelopmentStatus::Partial,
        );

        assert!(reason.contains("continuation"));
        assert!(reason.contains("iteration=7"));
        assert!(reason.contains("next_attempt=3"));
        assert!(reason.contains("max_continuations=2"));
        assert!(reason.contains("last_status=partial"));
    }
}
