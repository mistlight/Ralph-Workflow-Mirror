//! Effect types and handlers for side effects.
//!
//! Effects represent side-effect operations that the reducer triggers.
//! The pure reducer computes effects, and effect handlers execute them.

use crate::agents::AgentRole;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::event::{CheckpointTrigger, ConflictStrategy, PipelineEvent, RebasePhase};

/// Effects represent side-effect operations.
///
/// The reducer determines which effect to execute next based on state.
/// Effect handlers execute effects and emit events.
#[derive(Clone, Serialize, Deserialize)]
pub enum Effect {
    AgentInvocation {
        role: AgentRole,
        agent: String,
        model: Option<String>,
        prompt: String,
    },

    GeneratePlan {
        iteration: u32,
    },

    RunDevelopmentIteration {
        iteration: u32,
    },

    RunReviewPass {
        pass: u32,
    },

    RunFixAttempt {
        pass: u32,
    },

    RunRebase {
        phase: RebasePhase,
        target_branch: String,
    },

    ResolveRebaseConflicts {
        strategy: ConflictStrategy,
    },

    GenerateCommitMessage,

    CreateCommit {
        message: String,
    },

    SkipCommit {
        reason: String,
    },

    ValidateFinalState,

    SaveCheckpoint {
        trigger: CheckpointTrigger,
    },
}

/// Trait for executing effects.
///
/// This trait allows mocking in tests by providing alternative implementations.
pub trait EffectHandler {
    fn execute(&self, effect: Effect) -> Result<PipelineEvent>;

    fn invoke_agent(
        &self,
        role: AgentRole,
        agent: String,
        model: Option<String>,
        prompt: String,
    ) -> Result<PipelineEvent>;

    fn generate_plan(&self, iteration: u32) -> Result<PipelineEvent>;

    fn run_development_iteration(&self, iteration: u32) -> Result<PipelineEvent>;

    fn run_review_pass(&self, pass: u32) -> Result<PipelineEvent>;

    fn run_fix_attempt(&self, pass: u32) -> Result<PipelineEvent>;

    fn run_rebase(&self, phase: RebasePhase, target_branch: String) -> Result<PipelineEvent>;

    fn resolve_rebase_conflicts(&self, strategy: ConflictStrategy) -> Result<PipelineEvent>;

    fn generate_commit_message(&self) -> Result<PipelineEvent>;

    fn create_commit(&self, message: String) -> Result<PipelineEvent>;

    fn skip_commit(&self, reason: String) -> Result<PipelineEvent>;

    fn validate_final_state(&self) -> Result<PipelineEvent>;

    fn save_checkpoint(&self, trigger: CheckpointTrigger) -> Result<PipelineEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_serialization() {
        let effect = Effect::AgentInvocation {
            role: AgentRole::Developer,
            agent: "claude".to_string(),
            model: None,
            prompt: "test".to_string(),
        };

        let json = serde_json::to_string(&effect).unwrap();
        let deserialized: Effect = serde_json::from_str(&json).unwrap();

        match deserialized {
            Effect::AgentInvocation {
                role,
                agent,
                model,
                prompt,
            } => {
                assert_eq!(role, AgentRole::Developer);
                assert_eq!(agent, "claude");
                assert!(model.is_none());
                assert_eq!(prompt, "test");
            }
            _ => panic!("Expected AgentInvocation effect"),
        }
    }
}
