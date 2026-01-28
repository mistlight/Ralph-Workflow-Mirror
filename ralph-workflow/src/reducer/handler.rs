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
    CheckpointTrigger, ConflictStrategy, PipelineEvent, PipelinePhase, RebasePhase,
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
        match development::run_planning_step(ctx, iteration) {
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

                let event = PipelineEvent::PlanGenerationCompleted {
                    iteration,
                    valid: is_valid,
                };

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
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::PlanGenerationCompleted {
                    iteration,
                    valid: false,
                },
            )),
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
            return Ok(EffectResult::event(PipelineEvent::PipelineAborted {
                reason: format!(
                    "Development continuation attempts exhausted (continuation_attempt={}, max_continuations={})",
                    continuation_state.continuation_attempt, max_continuations
                ),
            }));
        }

        // Clean stale continuation context when starting a fresh attempt.
        if continuation_state.continuation_attempt == 0 {
            let _ = cleanup_continuation_context_file(ctx);
        }

        // If the agent repeatedly fails to produce valid XML even after in-session
        // XSD retries, rerun the attempt a small number of times without consuming
        // the continuation budget (which is reserved for valid partial/failed work).
        const MAX_INVALID_OUTPUT_RERUNS: u32 = 2;

        let mut invalid_reruns: u32 = 0;
        let attempt = loop {
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
                    return Ok(EffectResult::event(PipelineEvent::PipelineAborted {
                        reason: format!("Development attempt failed: {err}"),
                    }));
                }
            };

            match decide_dev_iteration_next_step(
                continuation_state.continuation_attempt,
                max_continuations,
                &attempt,
            ) {
                DevIterationNextStep::RetryInvalidOutput
                    if invalid_reruns < MAX_INVALID_OUTPUT_RERUNS =>
                {
                    invalid_reruns += 1;
                    ctx.logger.info(&format!(
                        "Development output invalid after XSD retries; rerunning attempt without consuming continuation budget ({}/{})",
                        invalid_reruns, MAX_INVALID_OUTPUT_RERUNS
                    ));
                    continue;
                }
                DevIterationNextStep::RetryInvalidOutput => {
                    return Ok(EffectResult::event(PipelineEvent::PipelineAborted {
                        reason: format!(
                            "Development output remained invalid after XSD retries and {} reruns. Last summary={}",
                            MAX_INVALID_OUTPUT_RERUNS,
                            attempt.summary
                        ),
                    }));
                }
                _ => break attempt,
            }
        };

        // If we reached completed, the iteration can transition to commit.
        if matches!(
            decide_dev_iteration_next_step(
                continuation_state.continuation_attempt,
                max_continuations,
                &attempt
            ),
            DevIterationNextStep::Completed
        ) {
            let _ = cleanup_continuation_context_file(ctx);

            let event = if continuation_state.is_continuation() {
                PipelineEvent::DevelopmentIterationContinuationSucceeded {
                    iteration,
                    total_continuation_attempts: continuation_state.continuation_attempt,
                }
            } else {
                PipelineEvent::DevelopmentIterationCompleted {
                    iteration,
                    output_valid: true,
                }
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
        let next_attempt = match decide_dev_iteration_next_step(
            continuation_state.continuation_attempt,
            max_continuations,
            &attempt,
        ) {
            DevIterationNextStep::Continue {
                next_continuation_attempt,
            } => next_continuation_attempt,
            DevIterationNextStep::Abort { .. } => {
                let _ = cleanup_continuation_context_file(ctx);
                let total_valid_attempts = 1 + max_continuations;
                return Ok(EffectResult::event(PipelineEvent::PipelineAborted {
                    reason: format!(
                        "Development did not reach status='completed' after {} total valid attempts. Last status={:?}. Last summary={}",
                        total_valid_attempts,
                        attempt.status,
                        attempt.summary
                    ),
                }));
            }
            DevIterationNextStep::RetryInvalidOutput | DevIterationNextStep::Completed => {
                // Completed is handled above. Invalid output is handled by the rerun loop above.
                unreachable!("Unexpected dev iteration next step after invalid-output handling")
            }
        };

        ctx.logger.info(&format!(
            "Triggering development continuation attempt {}/{} (previous status={})",
            next_attempt, max_continuations, attempt.status
        ));

        // Write continuation context for the next attempt.
        write_continuation_context_file(ctx, iteration, next_attempt, &attempt)?;
        ctx.logger
            .info("Continuation context written to .agent/tmp/continuation_context.md");

        let event = PipelineEvent::DevelopmentIterationContinuationTriggered {
            iteration,
            status: attempt.status,
            summary: attempt.summary,
            files_changed: attempt.files_changed,
            next_steps: attempt.next_steps,
        };

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

        match review::run_review_pass(ctx, pass, &review_label, "", review_agent.as_deref()) {
            Ok(result) => {
                let event = PipelineEvent::ReviewCompleted {
                    pass,
                    issues_found: !result.early_exit,
                };

                // Build UI events
                let mut ui_events = vec![
                    // Emit UI event for review progress
                    UIEvent::ReviewProgress {
                        pass,
                        total: self.state.total_reviewer_passes,
                    },
                ];

                // Try to read issues XML for semantic rendering
                let issues_xml_path = Path::new(".agent/tmp/issues.xml");
                let processed_path = Path::new(".agent/tmp/issues.xml.processed");
                if let Some(xml_content) = ctx
                    .workspace
                    .read(issues_xml_path)
                    .ok()
                    .or_else(|| ctx.workspace.read(processed_path).ok())
                {
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
            Err(_) => Ok(EffectResult::event(PipelineEvent::ReviewCompleted {
                pass,
                issues_found: false,
            })),
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
                let event = PipelineEvent::FixAttemptCompleted {
                    pass,
                    changes_made: true,
                };

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
            Err(_) => Ok(EffectResult::event(PipelineEvent::FixAttemptCompleted {
                pass,
                changes_made: false,
            })),
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

                    Ok(EffectResult::event(PipelineEvent::RebaseConflictDetected {
                        files,
                    }))
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

                    Ok(EffectResult::event(PipelineEvent::RebaseSucceeded {
                        phase,
                        new_head,
                    }))
                }
            }
            Err(e) => Ok(EffectResult::event(PipelineEvent::RebaseFailed {
                phase,
                reason: e.to_string(),
            })),
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

                    Ok(EffectResult::event(PipelineEvent::RebaseConflictResolved {
                        files,
                    }))
                }
                Err(e) => Ok(EffectResult::event(PipelineEvent::RebaseFailed {
                    phase: RebasePhase::PostReview,
                    reason: e.to_string(),
                })),
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

                    Ok(EffectResult::event(PipelineEvent::RebaseAborted {
                        phase: RebasePhase::PostReview,
                        restored_to,
                    }))
                }
                Err(e) => Ok(EffectResult::event(PipelineEvent::RebaseFailed {
                    phase: RebasePhase::PostReview,
                    reason: e.to_string(),
                })),
            },
            ConflictStrategy::Skip => {
                Ok(EffectResult::event(PipelineEvent::RebaseConflictResolved {
                    files: Vec::new(),
                }))
            }
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
            return Ok(EffectResult::event(PipelineEvent::CommitSkipped {
                reason: "No changes to commit (empty diff)".to_string(),
            }));
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
                let event = PipelineEvent::CommitMessageGenerated {
                    message: result.message.clone(),
                    attempt,
                };

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
            Err(_) => Ok(EffectResult::event(PipelineEvent::CommitMessageGenerated {
                message: "chore: automated commit".to_string(),
                attempt,
            })),
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
            Ok(Some(hash)) => Ok(EffectResult::event(PipelineEvent::CommitCreated {
                hash: hash.to_string(),
                message,
            })),
            Ok(None) => {
                // No changes to commit - skip to FinalValidation instead of failing
                // This prevents infinite loop when there are no changes
                Ok(EffectResult::event(PipelineEvent::CommitSkipped {
                    reason: "No changes to commit".to_string(),
                }))
            }
            Err(e) => Ok(EffectResult::event(PipelineEvent::CommitGenerationFailed {
                reason: e.to_string(),
            })),
        }
    }

    fn skip_commit(&mut self, _ctx: &mut PhaseContext<'_>, reason: String) -> Result<EffectResult> {
        Ok(EffectResult::event(PipelineEvent::CommitSkipped { reason }))
    }

    fn validate_final_state(&mut self, _ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        // Transition to Finalizing phase to restore PROMPT.md permissions
        // via the effect system before marking the pipeline complete
        let event = PipelineEvent::FinalizingStarted;

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

        Ok(EffectResult::event(PipelineEvent::CheckpointSaved {
            trigger,
        }))
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

        let event = PipelineEvent::AgentChainInitialized { role, agents };

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

        Ok(EffectResult::event(PipelineEvent::ContextCleaned))
    }

    fn restore_prompt_permissions(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        use crate::files::make_prompt_writable_with_workspace;

        ctx.logger.info("Restoring PROMPT.md write permissions...");

        // Use workspace-based function for testability
        if let Some(warning) = make_prompt_writable_with_workspace(ctx.workspace) {
            ctx.logger.warn(&warning);
        }

        let event = PipelineEvent::PromptPermissionsRestored;

        // Emit phase transition UI event to Complete
        let ui_event = self.phase_transition_ui(PipelinePhase::Complete);

        Ok(EffectResult::with_ui(event, vec![ui_event]))
    }
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

fn write_continuation_context_file(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    continuation_attempt: u32,
    attempt: &development::DevAttemptResult,
) -> anyhow::Result<()> {
    let tmp_dir = Path::new(".agent/tmp");
    if !ctx.workspace.exists(tmp_dir) {
        ctx.workspace.create_dir_all(tmp_dir)?;
    }

    let mut content = String::new();
    content.push_str("# Development Continuation Context\n\n");
    content.push_str(&format!("- Iteration: {iteration}\n"));
    content.push_str(&format!("- Continuation attempt: {continuation_attempt}\n"));
    content.push_str(&format!("- Previous status: {}\n\n", attempt.status));

    content.push_str("## Previous summary\n\n");
    content.push_str(&attempt.summary);
    content.push('\n');

    if let Some(ref files) = attempt.files_changed {
        content.push_str("\n## Files changed\n\n");
        for file in files {
            content.push_str("- ");
            content.push_str(file);
            content.push('\n');
        }
    }

    if let Some(ref next_steps) = attempt.next_steps {
        content.push_str("\n## Recommended next steps\n\n");
        content.push_str(next_steps);
        content.push('\n');
    }

    content.push_str("\n## Reference files (do not modify)\n\n");
    content.push_str("- PROMPT.md\n");
    content.push_str("- .agent/PLAN.md\n");

    ctx.workspace
        .write(Path::new(".agent/tmp/continuation_context.md"), &content)?;

    Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DevIterationNextStep {
    Completed,
    RetryInvalidOutput,
    Continue { next_continuation_attempt: u32 },
    Abort { next_continuation_attempt: u32 },
}

fn decide_dev_iteration_next_step(
    continuation_attempt: u32,
    max_continuations: u32,
    attempt: &crate::phases::development::DevAttemptResult,
) -> DevIterationNextStep {
    if !attempt.output_valid {
        return DevIterationNextStep::RetryInvalidOutput;
    }

    if attempt.output_valid
        && matches!(
            attempt.status,
            crate::reducer::state::DevelopmentStatus::Completed
        )
    {
        return DevIterationNextStep::Completed;
    }

    let next_attempt = continuation_attempt + 1;
    // Config semantics: max_continuations counts *continuation attempts* beyond the initial
    // attempt (where continuation_attempt == 0). So next_attempt is allowed as long as it does
    // not exceed max_continuations.
    if next_attempt > max_continuations {
        DevIterationNextStep::Abort {
            next_continuation_attempt: next_attempt,
        }
    } else {
        DevIterationNextStep::Continue {
            next_continuation_attempt: next_attempt,
        }
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
    fn test_decide_dev_iteration_next_step_invalid_output_does_not_consume_continuation_budget() {
        use crate::phases::development::DevAttemptResult;
        use crate::reducer::state::DevelopmentStatus;

        let attempt = DevAttemptResult {
            had_error: true,
            output_valid: false,
            status: DevelopmentStatus::Failed,
            summary: "invalid xml".to_string(),
            files_changed: None,
            next_steps: None,
        };

        let next = decide_dev_iteration_next_step(0, 2, &attempt);

        assert_eq!(next, DevIterationNextStep::RetryInvalidOutput);
    }

    #[test]
    fn test_decide_dev_iteration_next_step_partial_consumes_continuation_budget() {
        use crate::phases::development::DevAttemptResult;
        use crate::reducer::state::DevelopmentStatus;

        let attempt = DevAttemptResult {
            had_error: false,
            output_valid: true,
            status: DevelopmentStatus::Partial,
            summary: "partial".to_string(),
            files_changed: None,
            next_steps: None,
        };

        let next = decide_dev_iteration_next_step(0, 2, &attempt);

        assert_eq!(
            next,
            DevIterationNextStep::Continue {
                next_continuation_attempt: 1
            }
        );
    }

    #[test]
    fn test_decide_dev_iteration_next_step_partial_allows_max_continuations() {
        use crate::phases::development::DevAttemptResult;
        use crate::reducer::state::DevelopmentStatus;

        let attempt = DevAttemptResult {
            had_error: false,
            output_valid: true,
            status: DevelopmentStatus::Partial,
            summary: "partial".to_string(),
            files_changed: None,
            next_steps: None,
        };

        let next = decide_dev_iteration_next_step(1, 2, &attempt);

        assert_eq!(
            next,
            DevIterationNextStep::Continue {
                next_continuation_attempt: 2
            }
        );
    }

    #[test]
    fn test_decide_dev_iteration_next_step_partial_aborts_when_next_exceeds_max_continuations() {
        use crate::phases::development::DevAttemptResult;
        use crate::reducer::state::DevelopmentStatus;

        let attempt = DevAttemptResult {
            had_error: false,
            output_valid: true,
            status: DevelopmentStatus::Partial,
            summary: "partial".to_string(),
            files_changed: None,
            next_steps: None,
        };

        let next = decide_dev_iteration_next_step(2, 2, &attempt);

        assert_eq!(
            next,
            DevIterationNextStep::Abort {
                next_continuation_attempt: 3
            }
        );
    }
}
