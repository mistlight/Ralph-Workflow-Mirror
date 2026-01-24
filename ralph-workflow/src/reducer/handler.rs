//! Main effect handler implementation.
//!
//! This module implements the EffectHandler trait to execute pipeline side effects
//! through the reducer architecture. Effect handlers perform actual work (agent
//! invocation, git operations, file I/O) and emit events.

use crate::agents::AgentRole;
use crate::checkpoint::{save_checkpoint, CheckpointBuilder, PipelinePhase as CheckpointPhase};
use crate::phases::{commit, development, get_primary_commit_agent, review, PhaseContext};
use crate::pipeline::PipelineRuntime;
use crate::prompts::ContextLevel;
use crate::reducer::effect::{Effect, EffectHandler};
use crate::reducer::event::{CheckpointTrigger, ConflictStrategy, PipelineEvent, RebasePhase};
use crate::reducer::fault_tolerant_executor::{
    execute_agent_fault_tolerantly, AgentExecutionConfig,
};
use crate::reducer::state::PipelineState;
use anyhow::Result;
use std::path::Path;

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
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext<'_>) -> Result<PipelineEvent> {
        let event = self.execute_effect(effect, ctx)?;
        self.event_log.push(event.clone());
        Ok(event)
    }
}

impl MainEffectHandler {
    fn execute_effect(
        &mut self,
        effect: Effect,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<PipelineEvent> {
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
        }
    }

    fn invoke_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        role: AgentRole,
        agent: String,
        _model: Option<String>,
        prompt: String,
    ) -> Result<PipelineEvent> {
        // Use agent from state.agent_chain if available
        let effective_agent = self
            .state
            .agent_chain
            .current_agent()
            .unwrap_or(&agent)
            .clone();

        let model_name = self.state.agent_chain.current_model();

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
        let logfile = format!(".agent/logs/{}.log", effective_agent.to_lowercase());

        // Build pipeline runtime
        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            executor: ctx.executor,
            #[cfg(any(test, feature = "test-utils"))]
            agent_executor: None,
        };

        // Execute agent with fault-tolerant wrapper
        let config = AgentExecutionConfig {
            role,
            agent_name: &effective_agent,
            cmd_str: &agent_config.cmd,
            parser_type: agent_config.json_parser,
            env_vars: &agent_config.env_vars,
            prompt: &prompt,
            display_name: &effective_agent,
            logfile: &logfile,
        };

        execute_agent_fault_tolerantly(config, &mut runtime)
    }

    fn generate_plan(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<PipelineEvent> {
        match development::run_planning_step(ctx, iteration) {
            Ok(_) => {
                // Validate plan was created
                let plan_path = Path::new(".agent/PLAN.md");
                let plan_exists = plan_path.exists();
                let plan_content = if plan_exists {
                    std::fs::read_to_string(plan_path).ok().unwrap_or_default()
                } else {
                    String::new()
                };

                let is_valid = plan_exists && !plan_content.trim().is_empty();

                Ok(PipelineEvent::PlanGenerationCompleted {
                    iteration,
                    valid: is_valid,
                })
            }
            Err(_) => Ok(PipelineEvent::PlanGenerationCompleted {
                iteration,
                valid: false,
            }),
        }
    }

    fn run_development_iteration(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        iteration: u32,
    ) -> Result<PipelineEvent> {
        use crate::checkpoint::restore::ResumeContext;
        let developer_context = ContextLevel::from(ctx.config.developer_context);

        // Get current agent from agent chain
        let dev_agent = self.state.agent_chain.current_agent().cloned();

        // Run development iteration
        let result = development::run_development_iteration_with_xml_retry(
            ctx,
            iteration,
            developer_context,
            false,
            None::<&ResumeContext>,
            dev_agent.as_deref(),
        );

        match result {
            Ok(_dev_result) => Ok(PipelineEvent::DevelopmentIterationCompleted {
                iteration,
                output_valid: true,
            }),
            Err(_) => Ok(PipelineEvent::DevelopmentIterationCompleted {
                iteration,
                output_valid: false,
            }),
        }
    }

    fn run_review_pass(&mut self, ctx: &mut PhaseContext<'_>, pass: u32) -> Result<PipelineEvent> {
        let review_label = format!("review_{}", pass);

        // Get current reviewer agent from agent chain
        let review_agent = self.state.agent_chain.current_agent().cloned();

        match review::run_review_pass(ctx, pass, &review_label, "", review_agent.as_deref()) {
            Ok(result) => Ok(PipelineEvent::ReviewCompleted {
                pass,
                issues_found: !result.early_exit,
            }),
            Err(_) => Ok(PipelineEvent::ReviewCompleted {
                pass,
                issues_found: false,
            }),
        }
    }

    fn run_fix_attempt(&mut self, ctx: &mut PhaseContext<'_>, pass: u32) -> Result<PipelineEvent> {
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
            Ok(_) => Ok(PipelineEvent::FixAttemptCompleted {
                pass,
                changes_made: true,
            }),
            Err(_) => Ok(PipelineEvent::FixAttemptCompleted {
                pass,
                changes_made: false,
            }),
        }
    }

    fn run_rebase(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        phase: RebasePhase,
        target_branch: String,
    ) -> Result<PipelineEvent> {
        use crate::git_helpers::{get_conflicted_files, rebase_onto};

        match rebase_onto(&target_branch, _ctx.executor) {
            Ok(_) => {
                // Check for conflicts
                let conflicted_files = get_conflicted_files().unwrap_or_default();

                if !conflicted_files.is_empty() {
                    let files = conflicted_files.into_iter().map(|s| s.into()).collect();

                    Ok(PipelineEvent::RebaseConflictDetected { files })
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

                    Ok(PipelineEvent::RebaseSucceeded { phase, new_head })
                }
            }
            Err(e) => Ok(PipelineEvent::RebaseFailed {
                phase,
                reason: e.to_string(),
            }),
        }
    }

    fn resolve_rebase_conflicts(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        strategy: ConflictStrategy,
    ) -> Result<PipelineEvent> {
        use crate::git_helpers::{abort_rebase, continue_rebase, get_conflicted_files};

        match strategy {
            ConflictStrategy::Continue => match continue_rebase(_ctx.executor) {
                Ok(_) => {
                    let files = get_conflicted_files()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|s| s.into())
                        .collect();

                    Ok(PipelineEvent::RebaseConflictResolved { files })
                }
                Err(e) => Ok(PipelineEvent::RebaseFailed {
                    phase: RebasePhase::PostReview,
                    reason: e.to_string(),
                }),
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

                    Ok(PipelineEvent::RebaseAborted {
                        phase: RebasePhase::PostReview,
                        restored_to,
                    })
                }
                Err(e) => Ok(PipelineEvent::RebaseFailed {
                    phase: RebasePhase::PostReview,
                    reason: e.to_string(),
                }),
            },
            ConflictStrategy::Skip => {
                Ok(PipelineEvent::RebaseConflictResolved { files: Vec::new() })
            }
        }
    }

    fn generate_commit_message(&mut self, ctx: &mut PhaseContext<'_>) -> Result<PipelineEvent> {
        let attempt = match &self.state.commit {
            crate::reducer::state::CommitState::Generating { attempt, .. } => *attempt,
            _ => 1,
        };

        // Get git diff for commit message generation
        let diff = crate::git_helpers::git_diff().unwrap_or_default();

        // Get commit agent first to avoid borrow conflicts
        let commit_agent =
            crate::phases::get_primary_commit_agent(ctx).unwrap_or_else(|| "commit".to_string());

        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            executor: ctx.executor,
            #[cfg(any(test, feature = "test-utils"))]
            agent_executor: None,
        };

        match commit::generate_commit_message(
            &diff,
            ctx.registry,
            &mut runtime,
            &commit_agent,
            ctx.template_context,
            &ctx.prompt_history,
        ) {
            Ok(result) => Ok(PipelineEvent::CommitMessageGenerated {
                message: result.message.clone(),
                attempt,
            }),
            Err(_) => Ok(PipelineEvent::CommitMessageGenerated {
                message: "chore: automated commit".to_string(),
                attempt,
            }),
        }
    }

    fn create_commit(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        message: String,
    ) -> Result<PipelineEvent> {
        use crate::git_helpers::{git_add_all, git_commit};

        // Stage all changes
        git_add_all()?;

        // Create commit
        match git_commit(&message, None, None) {
            Ok(Some(hash)) => Ok(PipelineEvent::CommitCreated {
                hash: hash.to_string(),
                message,
            }),
            Ok(None) => Ok(PipelineEvent::CommitGenerationFailed {
                reason: "No changes to commit".to_string(),
            }),
            Err(e) => Ok(PipelineEvent::CommitGenerationFailed {
                reason: e.to_string(),
            }),
        }
    }

    fn skip_commit(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        reason: String,
    ) -> Result<PipelineEvent> {
        Ok(PipelineEvent::CommitSkipped { reason })
    }

    fn validate_final_state(&mut self, _ctx: &mut PhaseContext<'_>) -> Result<PipelineEvent> {
        Ok(PipelineEvent::PipelineCompleted)
    }

    fn save_checkpoint(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        trigger: CheckpointTrigger,
    ) -> Result<PipelineEvent> {
        if ctx.config.features.checkpoint_enabled {
            let _ = save_checkpoint_from_state(&self.state, ctx);
        }

        Ok(PipelineEvent::CheckpointSaved { trigger })
    }

    fn initialize_agent_chain(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        role: AgentRole,
    ) -> Result<PipelineEvent> {
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

        Ok(PipelineEvent::AgentChainInitialized { role, agents })
    }

    fn cleanup_context(&mut self, ctx: &mut PhaseContext<'_>) -> Result<PipelineEvent> {
        use crate::files::delete_plan_file;
        use std::fs;
        use std::path::Path;

        ctx.logger
            .info("Cleaning up context files to prevent pollution...");

        let mut cleaned_count = 0;
        let mut failed_count = 0;

        // Delete PLAN.md
        if let Err(err) = delete_plan_file() {
            ctx.logger.warn(&format!("Failed to delete PLAN.md: {err}"));
            failed_count += 1;
        } else {
            cleaned_count += 1;
        }

        // Delete ISSUES.md (may not exist if in isolation mode)
        let issues_path = Path::new(".agent/ISSUES.md");
        if issues_path.exists() {
            if let Err(err) = fs::remove_file(issues_path) {
                ctx.logger
                    .warn(&format!("Failed to delete ISSUES.md: {err}"));
                failed_count += 1;
            } else {
                cleaned_count += 1;
            }
        }

        // Delete ALL .xml files in .agent/tmp/ to prevent context pollution
        let tmp_dir = Path::new(".agent/tmp");
        if tmp_dir.exists() {
            if let Ok(entries) = fs::read_dir(tmp_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("xml") {
                        if let Err(err) = fs::remove_file(&path) {
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

        Ok(PipelineEvent::ContextCleaned)
    }
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
        .with_execution_history(ctx.execution_history.clone())
        .with_prompt_history(ctx.clone_prompt_history());

    if let Some(checkpoint) = builder.build() {
        let _ = save_checkpoint(&checkpoint);
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
        crate::reducer::event::PipelinePhase::Complete => CheckpointPhase::Complete,
        crate::reducer::event::PipelinePhase::Interrupted => CheckpointPhase::Complete,
    }
}
