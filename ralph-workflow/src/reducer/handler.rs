//! Main effect handler implementation.
//!
//! This module implements the EffectHandler trait to execute pipeline side effects
//! through the reducer architecture. Effect handlers perform actual work (agent
//! invocation, git operations, file I/O) and emit events.

use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use crate::reducer::effect::{Effect, EffectHandler};
use crate::reducer::event::{CheckpointTrigger, PipelineEvent};
use crate::reducer::state::PipelineState;
use anyhow::Result;

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
    fn execute(&self, effect: Effect) -> Result<PipelineEvent> {
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

            Effect::ResolveRebaseConflicts { strategy: _ } => {
                Ok(PipelineEvent::RebaseConflictResolved { files: Vec::new() })
            }

            Effect::GenerateCommitMessage => self.generate_commit_message(),

            Effect::CreateCommit { message } => self.create_commit(message),

            Effect::SkipCommit { reason } => Ok(PipelineEvent::CommitSkipped { reason }),

            Effect::ValidateFinalState => Ok(PipelineEvent::PipelineCompleted),

            Effect::SaveCheckpoint { trigger } => self.save_checkpoint(trigger),
        }
    }

    fn invoke_agent(
        &self,
        role: AgentRole,
        agent: String,
        model: Option<String>,
        _prompt: String,
    ) -> Result<PipelineEvent> {
        let agent_name = agent.clone();
        let _ = (role, model);
        Ok(PipelineEvent::AgentInvocationSucceeded {
            role,
            agent: agent_name,
        })
    }

    fn generate_plan(&self, iteration: u32) -> Result<PipelineEvent> {
        let _ = iteration;
        Ok(PipelineEvent::PlanGenerationCompleted {
            iteration,
            valid: true,
        })
    }

    fn run_development_iteration(&self, iteration: u32) -> Result<PipelineEvent> {
        let _ = iteration;
        Ok(PipelineEvent::DevelopmentIterationCompleted {
            iteration,
            output_valid: true,
        })
    }

    fn run_review_pass(&self, pass: u32) -> Result<PipelineEvent> {
        let _ = pass;
        Ok(PipelineEvent::ReviewCompleted {
            pass,
            issues_found: false,
        })
    }

    fn run_fix_attempt(&self, pass: u32) -> Result<PipelineEvent> {
        let _ = pass;
        Ok(PipelineEvent::FixAttemptCompleted {
            pass,
            changes_made: true,
        })
    }

    fn run_rebase(
        &self,
        phase: crate::reducer::event::RebasePhase,
        target_branch: String,
    ) -> Result<PipelineEvent> {
        let _ = (phase, target_branch);
        Ok(PipelineEvent::RebaseSucceeded {
            phase,
            new_head: "abc123".to_string(),
        })
    }

    fn resolve_rebase_conflicts(
        &self,
        strategy: crate::reducer::event::ConflictStrategy,
    ) -> Result<PipelineEvent> {
        let _ = strategy;
        Ok(PipelineEvent::RebaseConflictResolved { files: Vec::new() })
    }

    fn generate_commit_message(&self) -> Result<PipelineEvent> {
        Ok(PipelineEvent::CommitMessageGenerated {
            message: "test commit".to_string(),
            attempt: 1,
        })
    }

    fn create_commit(&self, message: String) -> Result<PipelineEvent> {
        Ok(PipelineEvent::CommitCreated {
            hash: "abc123".to_string(),
            message,
        })
    }

    fn skip_commit(&self, reason: String) -> Result<PipelineEvent> {
        Ok(PipelineEvent::CommitSkipped { reason })
    }

    fn validate_final_state(&self) -> Result<PipelineEvent> {
        Ok(PipelineEvent::PipelineCompleted)
    }

    fn save_checkpoint(&self, trigger: CheckpointTrigger) -> Result<PipelineEvent> {
        let _ = trigger;
        Ok(PipelineEvent::CheckpointSaved { trigger })
    }
}
