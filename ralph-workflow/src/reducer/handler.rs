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

impl crate::app::event_loop::StatefulHandler for MainEffectHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
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

            Effect::RestorePromptPermissions => self.restore_prompt_permissions(ctx),
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
            executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
            workspace: ctx.workspace,
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
                let plan_exists = ctx.workspace.exists(plan_path);
                let plan_content = if plan_exists {
                    ctx.workspace.read(plan_path).ok().unwrap_or_default()
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
        ctx: &mut PhaseContext<'_>,
        message: String,
    ) -> Result<PipelineEvent> {
        use crate::git_helpers::{git_add_all, git_commit};

        // Stage all changes
        git_add_all()?;

        // Create commit
        match git_commit(&message, None, None, Some(ctx.executor)) {
            Ok(Some(hash)) => Ok(PipelineEvent::CommitCreated {
                hash: hash.to_string(),
                message,
            }),
            Ok(None) => {
                // No changes to commit - skip to FinalValidation instead of failing
                // This prevents infinite loop when there are no changes
                Ok(PipelineEvent::CommitSkipped {
                    reason: "No changes to commit".to_string(),
                })
            }
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
        // Transition to Finalizing phase to restore PROMPT.md permissions
        // via the effect system before marking the pipeline complete
        Ok(PipelineEvent::FinalizingStarted)
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

    fn restore_prompt_permissions(&mut self, ctx: &mut PhaseContext<'_>) -> Result<PipelineEvent> {
        use crate::files::make_prompt_writable_with_workspace;

        ctx.logger.info("Restoring PROMPT.md write permissions...");

        // Use workspace-based function for testability
        if let Some(warning) = make_prompt_writable_with_workspace(ctx.workspace) {
            ctx.logger.warn(&warning);
        }

        Ok(PipelineEvent::PromptPermissionsRestored)
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
        .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
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
        crate::reducer::event::PipelinePhase::Finalizing => CheckpointPhase::FinalValidation,
        crate::reducer::event::PipelinePhase::Complete => CheckpointPhase::Complete,
        crate::reducer::event::PipelinePhase::Interrupted => CheckpointPhase::Complete,
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

        let event = handler.execute_mock(Effect::RestorePromptPermissions);

        assert!(
            matches!(event, PipelineEvent::PromptPermissionsRestored),
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

        let event = handler.execute_mock(Effect::ValidateFinalState);

        assert!(
            matches!(event, PipelineEvent::FinalizingStarted),
            "ValidateFinalState should return FinalizingStarted to trigger finalization phase, got: {:?}",
            event
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
}
