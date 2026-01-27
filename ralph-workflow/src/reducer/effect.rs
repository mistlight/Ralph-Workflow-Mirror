//! Effect types and handlers for side effects.
//!
//! Effects represent side-effect operations that the reducer triggers.
//! Effect handlers execute effects and emit events.

use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::event::{CheckpointTrigger, ConflictStrategy, PipelineEvent, RebasePhase};
use super::ui_event::UIEvent;

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
        let event = PipelineEvent::PipelineStarted;
        let result = EffectResult::event(event.clone());

        assert!(matches!(result.event, PipelineEvent::PipelineStarted));
        assert!(result.ui_events.is_empty());
    }

    #[test]
    fn test_effect_result_with_ui() {
        let event = PipelineEvent::DevelopmentIterationCompleted {
            iteration: 1,
            output_valid: true,
        };
        let ui_events = vec![UIEvent::IterationProgress {
            current: 1,
            total: 3,
        }];

        let result = EffectResult::with_ui(event, ui_events);

        assert!(matches!(
            result.event,
            PipelineEvent::DevelopmentIterationCompleted { .. }
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
        let event = PipelineEvent::PlanGenerationCompleted {
            iteration: 1,
            valid: true,
        };

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
        let event = PipelineEvent::DevelopmentIterationCompleted {
            iteration: 2,
            output_valid: true,
        };

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
