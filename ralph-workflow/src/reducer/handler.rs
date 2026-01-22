//! Main effect handler implementation.
//!
//! This module implements the EffectHandler trait to execute pipeline side effects
//! through the reducer architecture. Effect handlers perform actual work (agent
//! invocation, git operations, file I/O) and emit events.

use crate::agents::AgentRole;
use crate::checkpoint::{save_checkpoint, CheckpointBuilder, PipelinePhase as CheckpointPhase};
use crate::phases::{commit, development, review, PhaseContext};
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::prompts::ContextLevel;
use crate::reducer::effect::{Effect, EffectHandler};
use crate::reducer::event::{
    AgentErrorKind, CheckpointTrigger, ConflictStrategy, PipelineEvent, RebasePhase,
};
use crate::reducer::state::PipelineState;
use anyhow::Result;
use std::path::Path;

/// Main effect handler implementation.
///
/// This handler executes effects by calling existing pipeline functions,
/// maintaining compatibility while migrating to reducer architecture.
pub struct MainEffectHandler<'ctx> {
    /// Phase context containing all runtime dependencies
    pub phase_ctx: &'ctx mut PhaseContext<'ctx>,
    /// Current pipeline state
    pub state: PipelineState,
    /// Event log for replay/debugging
    pub event_log: Vec<PipelineEvent>,
}

impl<'ctx> MainEffectHandler<'ctx> {
    /// Create a new effect handler.
    pub fn new(phase_ctx: &'ctx mut PhaseContext<'ctx>, state: PipelineState) -> Self {
        Self {
            phase_ctx,
            state,
            event_log: Vec::new(),
        }
    }
}

impl<'a> EffectHandler for MainEffectHandler<'a> {
    fn execute(&mut self, effect: Effect) -> Result<PipelineEvent> {
        match effect {
            Effect::AgentInvocation {
                role,
                agent,
                model,
                prompt,
            } => self.invoke_agent(role, agent, model, prompt),

            Effect::GeneratePlan { iteration } => self.generate_plan(iteration),

            Effect::RunDevelopmentIteration { iteration } => {
                self.run_development_iteration(iteration)
            }

            Effect::RunReviewPass { pass } => self.run_review_pass(pass),

            Effect::RunFixAttempt { pass } => self.run_fix_attempt(pass),

            Effect::RunRebase {
                phase,
                target_branch,
            } => self.run_rebase(phase, target_branch),

            Effect::ResolveRebaseConflicts { strategy } => self.resolve_rebase_conflicts(strategy),

            Effect::GenerateCommitMessage => self.generate_commit_message(),

            Effect::CreateCommit { message } => self.create_commit(message),

            Effect::SkipCommit { reason } => self.skip_commit(reason),

            Effect::ValidateFinalState => self.validate_final_state(),

            Effect::SaveCheckpoint { trigger } => self.save_checkpoint(trigger),
        }
    }

    fn invoke_agent(
        &mut self,
        role: AgentRole,
        agent: String,
        _model: Option<String>,
        prompt: String,
    ) -> Result<PipelineEvent> {
        let ctx = unsafe { &mut *(self.phase_ctx as *const PhaseContext as *mut PhaseContext) };

        // Get agent configuration from registry
        let agent_config = ctx
            .registry
            .resolve_config(&agent)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent))?;

        // Determine log file path
        let logfile = format!(".agent/logs/{}.log", agent.to_lowercase());

        // Build prompt command
        let prompt_cmd = PromptCommand {
            label: &agent,
            display_name: &agent,
            cmd_str: &agent_config.cmd,
            prompt: &prompt,
            logfile: &logfile,
            parser_type: agent_config.json_parser,
            env_vars: &agent_config.env_vars,
        };

        // Build pipeline runtime
        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            #[cfg(any(test, feature = "test-utils"))]
            agent_executor: None,
        };

        // Execute agent with fallback chain
        match run_with_prompt(&prompt_cmd, &mut runtime) {
            Ok(result) if result.exit_code == 0 => Ok(PipelineEvent::AgentInvocationSucceeded {
                role,
                agent: agent.clone(),
            }),
            Ok(result) => {
                let exit_code = result.exit_code;
                let error_kind = classify_agent_error(exit_code, &result.stderr);
                let retriable = is_retriable_agent_error(&error_kind);

                Ok(PipelineEvent::AgentInvocationFailed {
                    role,
                    agent,
                    exit_code,
                    error_kind,
                    retriable,
                })
            }
            Err(_) => Ok(PipelineEvent::AgentInvocationFailed {
                role,
                agent,
                exit_code: 1,
                error_kind: AgentErrorKind::InternalError,
                retriable: true,
            }),
        }
    }

    fn generate_plan(&mut self, iteration: u32) -> Result<PipelineEvent> {
        let ctx = unsafe { &mut *(self.phase_ctx as *const PhaseContext as *mut PhaseContext) };

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

    fn run_development_iteration(&mut self, iteration: u32) -> Result<PipelineEvent> {
        use crate::checkpoint::restore::ResumeContext;

        let ctx = unsafe { &mut *(self.phase_ctx as *const PhaseContext as *mut PhaseContext) };
        let developer_context = ContextLevel::from(ctx.config.developer_context);

        // Run development iteration
        let result = development::run_development_iteration_with_xml_retry(
            ctx,
            iteration,
            developer_context,
            false,
            None::<&ResumeContext>,
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

    fn run_review_pass(&mut self, pass: u32) -> Result<PipelineEvent> {
        let ctx = unsafe { &mut *(self.phase_ctx as *const PhaseContext as *mut PhaseContext) };
        let review_label = format!("review_{}", pass);

        match review::run_review_pass(ctx, pass, &review_label, "") {
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

    fn run_fix_attempt(&mut self, pass: u32) -> Result<PipelineEvent> {
        use crate::checkpoint::restore::ResumeContext;

        let ctx = unsafe { &mut *(self.phase_ctx as *const PhaseContext as *mut PhaseContext) };
        let reviewer_context = ContextLevel::from(ctx.config.reviewer_context);

        match review::run_fix_pass(ctx, pass, reviewer_context, None::<&ResumeContext>) {
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

    fn run_rebase(&mut self, phase: RebasePhase, target_branch: String) -> Result<PipelineEvent> {
        use crate::git_helpers::{get_conflicted_files, rebase_onto};

        match rebase_onto(&target_branch) {
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

    fn resolve_rebase_conflicts(&mut self, strategy: ConflictStrategy) -> Result<PipelineEvent> {
        use crate::git_helpers::{abort_rebase, continue_rebase, get_conflicted_files};

        match strategy {
            ConflictStrategy::Continue => match continue_rebase() {
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
            ConflictStrategy::Abort => match abort_rebase() {
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

    fn generate_commit_message(&mut self) -> Result<PipelineEvent> {
        let ctx = unsafe { &mut *(self.phase_ctx as *const PhaseContext as *mut PhaseContext) };

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

    fn create_commit(&mut self, message: String) -> Result<PipelineEvent> {
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

    fn skip_commit(&mut self, reason: String) -> Result<PipelineEvent> {
        Ok(PipelineEvent::CommitSkipped { reason })
    }

    fn validate_final_state(&mut self) -> Result<PipelineEvent> {
        Ok(PipelineEvent::PipelineCompleted)
    }

    fn save_checkpoint(&mut self, trigger: CheckpointTrigger) -> Result<PipelineEvent> {
        let ctx = unsafe { &mut *(self.phase_ctx as *const PhaseContext as *mut PhaseContext) };

        if ctx.config.features.checkpoint_enabled {
            let _ = save_checkpoint_from_state(&self.state, ctx);
        }

        Ok(PipelineEvent::CheckpointSaved { trigger })
    }
}

/// Classify agent error from exit code and stderr.
fn classify_agent_error(_exit_code: i32, stderr: &str) -> AgentErrorKind {
    let stderr_lower = stderr.to_lowercase();

    if stderr_lower.contains("network")
        || stderr_lower.contains("connection")
        || stderr_lower.contains("timeout")
    {
        AgentErrorKind::Network
    } else if stderr_lower.contains("auth")
        || stderr_lower.contains("api key")
        || stderr_lower.contains("unauthorized")
    {
        AgentErrorKind::Authentication
    } else if stderr_lower.contains("rate limit")
        || stderr_lower.contains("quota")
        || stderr_lower.contains("too many requests")
    {
        AgentErrorKind::RateLimit
    } else if stderr_lower.contains("model")
        && (stderr_lower.contains("not found") || stderr_lower.contains("unavailable"))
    {
        AgentErrorKind::ModelUnavailable
    } else if stderr_lower.contains("parse")
        || stderr_lower.contains("invalid")
        || stderr_lower.contains("malformed")
    {
        AgentErrorKind::ParsingError
    } else if stderr_lower.contains("permission")
        || stderr_lower.contains("access denied")
        || stderr_lower.contains("file")
    {
        AgentErrorKind::FileSystem
    } else {
        AgentErrorKind::InternalError
    }
}

/// Determine if agent error is retriable.
fn is_retriable_agent_error(error_kind: &AgentErrorKind) -> bool {
    matches!(
        error_kind,
        AgentErrorKind::Network
            | AgentErrorKind::RateLimit
            | AgentErrorKind::Timeout
            | AgentErrorKind::ModelUnavailable
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
