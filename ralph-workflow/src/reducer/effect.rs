//! Effect types and handlers for side effects.
//!
//! Effects represent impure operations (git, filesystem, agent execution) that
//! handlers execute on behalf of the reducer. The reducer is pure and determines
//! which effect to execute next; handlers execute effects and produce events.
//!
//! # Key Types
//!
//! - [`Effect`] - Enum of all possible side-effect operations
//! - [`EffectHandler`] - Trait for executing effects (impure code lives here)
//! - [`EffectResult`] - Contains both pipeline event and optional UI events
//!
//! # Design
//!
//! This separation keeps business logic pure (in reducers) while isolating
//! side effects (in handlers). See [`CODE_STYLE.md`](https://codeberg.org/mistlight/RalphWithReviewer/src/branch/main/CODE_STYLE.md)
//! for the full architecture overview.

use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::event::{CheckpointTrigger, ConflictStrategy, PipelineEvent, RebasePhase};
use super::ui_event::UIEvent;

/// Data for continuation context writing.
///
/// Groups parameters for [`Effect::WriteContinuationContext`] to avoid
/// exceeding the function argument limit.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ContinuationContextData {
    pub iteration: u32,
    pub attempt: u32,
    pub status: super::state::DevelopmentStatus,
    pub summary: String,
    pub files_changed: Option<Vec<String>>,
    pub next_steps: Option<String>,
}

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

    /// Restore PROMPT.md write permissions after pipeline completion.
    ///
    /// This effect is emitted during the Finalizing phase to restore
    /// write permissions on PROMPT.md so users can edit it normally
    /// after Ralph exits.
    RestorePromptPermissions,

    /// Write continuation context file for next development attempt.
    ///
    /// This effect is emitted when a development iteration returns
    /// partial/failed status and needs to continue. The context file
    /// provides the next attempt with information about what was done.
    ///
    /// Note: The current reducer orchestration does not always schedule this
    /// effect; the main handler may write the continuation context directly
    /// during `Effect::RunDevelopmentIteration` for compatibility.
    WriteContinuationContext(ContinuationContextData),

    /// Clean up continuation context file.
    ///
    /// Emitted when an iteration completes successfully or when
    /// starting a fresh iteration (to remove stale context).
    ///
    /// Note: The current reducer orchestration does not always schedule this
    /// effect; the main handler may clean the continuation context directly
    /// during `Effect::RunDevelopmentIteration` for compatibility.
    CleanupContinuationContext,
}

/// Result of executing an effect.
///
/// Contains both the PipelineEvent (for reducer) and optional UIEvents (for display).
/// This separation keeps UI concerns out of the reducer while allowing handlers
/// to emit rich feedback during execution.
#[derive(Clone, Debug)]
pub struct EffectResult {
    /// Event for reducer (affects state).
    pub event: PipelineEvent,
    /// UI events for display (do not affect state).
    pub ui_events: Vec<UIEvent>,
}

impl EffectResult {
    /// Create result with just a pipeline event (no UI events).
    pub fn event(event: PipelineEvent) -> Self {
        Self {
            event,
            ui_events: Vec::new(),
        }
    }

    /// Create result with pipeline event and UI events.
    pub fn with_ui(event: PipelineEvent, ui_events: Vec<UIEvent>) -> Self {
        Self { event, ui_events }
    }

    /// Add a UI event to the result.
    pub fn with_ui_event(mut self, ui_event: UIEvent) -> Self {
        self.ui_events.push(ui_event);
        self
    }
}

/// Trait for executing effects.
///
/// Returns EffectResult containing both PipelineEvent (for state) and
/// UIEvents (for display). This allows mocking in tests.
pub trait EffectHandler<'ctx> {
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext<'_>) -> Result<EffectResult>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::event::PipelinePhase;

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

    #[test]
    fn test_effect_result_event_only() {
        let event = PipelineEvent::pipeline_started();
        let result = EffectResult::event(event.clone());

        assert!(matches!(
            result.event,
            PipelineEvent::Lifecycle(crate::reducer::event::LifecycleEvent::Started)
        ));
        assert!(result.ui_events.is_empty());
    }

    #[test]
    fn test_effect_result_with_ui() {
        let event = PipelineEvent::development_iteration_completed(1, true);
        let ui_events = vec![UIEvent::IterationProgress {
            current: 1,
            total: 3,
        }];

        let result = EffectResult::with_ui(event, ui_events);

        assert!(matches!(
            result.event,
            PipelineEvent::Development(
                crate::reducer::event::DevelopmentEvent::IterationCompleted { .. }
            )
        ));
        assert_eq!(result.ui_events.len(), 1);
        assert!(matches!(
            result.ui_events[0],
            UIEvent::IterationProgress {
                current: 1,
                total: 3
            }
        ));
    }

    #[test]
    fn test_effect_result_with_ui_event_builder() {
        let event = PipelineEvent::plan_generation_completed(1, true);

        let result = EffectResult::event(event).with_ui_event(UIEvent::PhaseTransition {
            from: Some(PipelinePhase::Planning),
            to: PipelinePhase::Development,
        });

        assert_eq!(result.ui_events.len(), 1);
        assert!(matches!(
            result.ui_events[0],
            UIEvent::PhaseTransition { .. }
        ));
    }

    #[test]
    fn test_effect_result_multiple_ui_events() {
        let event = PipelineEvent::development_iteration_completed(2, true);

        let result = EffectResult::event(event)
            .with_ui_event(UIEvent::IterationProgress {
                current: 2,
                total: 5,
            })
            .with_ui_event(UIEvent::AgentActivity {
                agent: "claude".to_string(),
                message: "Completed iteration".to_string(),
            });

        assert_eq!(result.ui_events.len(), 2);
    }
}
