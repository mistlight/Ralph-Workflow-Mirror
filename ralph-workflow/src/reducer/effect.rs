//! Effect types and handlers for side effects.
//!
//! Effects represent side-effect operations that the reducer triggers.
//! Effect handlers execute effects and emit events.

use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::event::{CheckpointTrigger, ConflictStrategy, PipelineEvent, RebasePhase};

/// Effects represent side-effect operations.
///
/// The reducer determines which effect to execute next based on state.
/// Effect handlers execute effects and emit events.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Effect {
    AgentInvocation {
        role: AgentRole,
        agent: String,
        model: Option<String>,
        prompt: String,
    },

    InitializeAgentChain {
        role: AgentRole,
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

    CleanupContext,
}

/// Trait for executing effects.
///
/// This trait allows mocking in tests by providing alternative implementations.
pub trait EffectHandler<'ctx> {
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext<'_>) -> Result<PipelineEvent>;
}

#[cfg(test)]
mod tests {
    // #[test]
    // fn test_effect_serialization() {
    //     let effect = Effect::AgentInvocation {
    //         role: AgentRole::Developer,
    //         agent: "claude".to_string(),
    //         model: None,
    //         prompt: "test".to_string(),
    //     };

    //     let json = serde_json::to_string(&effect).unwrap();
    //     let deserialized: Effect = serde_json::from_str(&json).unwrap();

    //     match deserialized {
    //         Effect::AgentInvocation {
    //             role,
    //             agent,
    //             model,
    //             prompt,
    //         } => {
    //             assert_eq!(role, AgentRole::Developer);
    //             assert_eq!(agent, "claude");
    //             assert_eq!(model.is_none());
    //             assert_eq!(prompt, "test");
    //         }
    //         _ => panic!("Expected AgentInvocation effect"),
    //     }
    // }
}
