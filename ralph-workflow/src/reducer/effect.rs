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
//! # Single-Task Effect Principle
//!
//! Each Effect variant represents exactly **one** logical operation. Effects must NOT:
//! - Perform multiple unrelated file operations
//! - Combine "decide" and "do" in one effect
//! - Bundle agent execution with parsing, retry, or output writing
//!
//! If an effect needs multiple responsibilities, split it into separate effects.
//! The effect handler executes effects atomically; all coordination happens via
//! reducer state and events.
//!
//! This principle is tested in `reducer_legacy_rejection.rs::test_effects_are_single_task`.
//!
//! **This rule is absolute.**
//!
//! If you touch this codebase for any reason and you notice an effect that bundles
//! hidden logic (multiple responsibilities, policy decisions, retries, phase transitions,
//! parsing + writing + archiving, etc.), you must refactor it into a sequence of
//! single-task effects and explicit events. Do not leave effect "shortcuts" behind.
//!
//! # Redux-Style Event Modeling
//!
//! This project intentionally follows the Redux style-guide guidance:
//! - Think of events/actions as "something that happened" (not "setters")
//! - Keep reducer logic pure and deterministic
//! - Keep state serializable
//! - Put side effects in handlers/middleware (effects), not in reducers
//!
//! References (official Redux docs):
//! - Actions are events: https://redux.js.org/tutorials/fundamentals/part-2-concepts-data-flow
//! - Event-based actions vs setters: https://redux.js.org/style-guide/
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

    /// Prepare review context files (single-task).
    ///
    /// This effect must only write the review inputs (prompt backups, diffs, etc.)
    /// needed for a subsequent `AgentInvocation` and must not invoke agents.
    PrepareReviewContext {
        pass: u32,
    },

    /// Prepare the review prompt for a pass (single-task).
    ///
    /// This effect must only render/write the prompt that will be used for the
    /// subsequent reviewer agent invocation.
    PrepareReviewPrompt {
        pass: u32,
    },

    /// Invoke the reviewer agent for a review pass (single-task).
    ///
    /// This effect must only perform agent execution using the prepared review prompt
    /// (written by `PrepareReviewPrompt`) and must not parse/validate outputs.
    InvokeReviewAgent {
        pass: u32,
    },

    /// Extract the review issues XML from the canonical workspace path (single-task).
    ///
    /// This effect must only verify that `.agent/tmp/issues.xml` exists and is readable.
    /// It must not validate XML, write ISSUES.md, or change phase.
    ExtractReviewIssuesXml {
        pass: u32,
    },

    /// Validate the extracted review issues XML (single-task).
    ///
    /// This effect must only validate/parses the XML at `.agent/tmp/issues.xml` and
    /// emit a review validation event. It must not write ISSUES.md, archive files,
    /// or transition phases.
    ValidateReviewIssuesXml {
        pass: u32,
    },

    /// Write `.agent/ISSUES.md` from the validated issues XML (single-task).
    ///
    /// This effect must only write markdown. It must not archive XML or transition phases.
    WriteIssuesMarkdown {
        pass: u32,
    },

    /// Archive `.agent/tmp/issues.xml` after ISSUES.md is written (single-task).
    ///
    /// This effect must only archive the canonical issues XML (move to `.processed`).
    ArchiveReviewIssuesXml {
        pass: u32,
    },

    /// Apply the already-validated review outcome to advance the reducer state (single-task).
    ///
    /// This effect must only emit the appropriate review outcome event.
    ApplyReviewOutcome {
        pass: u32,
        issues_found: bool,
        clean_no_issues: bool,
    },

    /// Prepare the fix prompt for a review pass (single-task).
    ///
    /// This effect must only render/write the prompt that will be used for the
    /// subsequent fix agent invocation.
    PrepareFixPrompt {
        pass: u32,
    },

    /// Invoke the fix agent for a review pass (single-task).
    ///
    /// This effect must only perform agent execution using the prepared fix prompt
    /// (written by `PrepareFixPrompt`) and must not parse/validate outputs.
    InvokeFixAgent {
        pass: u32,
    },

    /// Extract the fix result XML from the canonical workspace path (single-task).
    ///
    /// This effect must only verify that `.agent/tmp/fix_result.xml` exists and is readable.
    /// It must not validate XML, apply outcomes, or archive files.
    ExtractFixResultXml {
        pass: u32,
    },

    /// Validate the extracted fix result XML (single-task).
    ///
    /// This effect must only validate/parses the XML at `.agent/tmp/fix_result.xml` and
    /// emit a fix validation event. It must not apply outcomes or archive files.
    ValidateFixResultXml {
        pass: u32,
    },

    /// Apply the already-validated fix outcome to advance the reducer state (single-task).
    ///
    /// This effect must only emit the appropriate fix outcome event.
    ApplyFixOutcome {
        pass: u32,
    },

    /// Archive `.agent/tmp/fix_result.xml` after validation (single-task).
    ///
    /// This is intentionally sequenced before `ApplyFixOutcome` so the reducer can
    /// archive artifacts while still in the fix chain (before state transitions
    /// reset per-pass tracking).
    ArchiveFixResultXml {
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

    /// Wait for a retry-cycle backoff delay.
    ///
    /// This effect is emitted when the reducer determines the agent chain has
    /// entered a new retry cycle and a backoff delay must be applied before
    /// attempting more work.
    BackoffWait {
        role: AgentRole,
        cycle: u32,
        duration_ms: u64,
    },

    /// Abort the pipeline with a reason.
    ///
    /// This provides an explicit terminal effect for unrecoverable situations
    /// (e.g., exhausted agent chain) so the pipeline never stalls on checkpoints.
    AbortPipeline {
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
    /// The effect handler executes this as part of the development iteration
    /// flow when the reducer determines continuation is needed.
    WriteContinuationContext(ContinuationContextData),

    /// Clean up continuation context file.
    ///
    /// Emitted when an iteration completes successfully or when
    /// starting a fresh iteration (to remove stale context).
    ///
    /// The effect handler executes this as part of the development iteration
    /// flow when the reducer determines cleanup is needed.
    CleanupContinuationContext,
}

/// Result of executing an effect.
///
/// Contains both the PipelineEvent (for reducer) and optional UIEvents (for display).
/// This separation keeps UI concerns out of the reducer while allowing handlers
/// to emit rich feedback during execution.
///
/// # Multiple Events
///
/// Some effects produce multiple reducer events. For example, agent invocation
/// may produce:
/// 1. `InvocationSucceeded` - the primary event
/// 2. `SessionEstablished` - additional event when session ID is extracted
///
/// The `additional_events` field holds events that should be processed after
/// the primary event. The reducer loop processes all events in order.
#[derive(Clone, Debug)]
pub struct EffectResult {
    /// Primary event for reducer (affects state).
    pub event: PipelineEvent,
    /// Additional events to process after the primary event.
    ///
    /// Used for cases where an effect produces multiple events, such as
    /// agent invocation followed by session establishment. Each event is
    /// processed by the reducer in order.
    pub additional_events: Vec<PipelineEvent>,
    /// UI events for display (do not affect state).
    pub ui_events: Vec<UIEvent>,
}

impl EffectResult {
    /// Create result with just a pipeline event (no UI events).
    pub fn event(event: PipelineEvent) -> Self {
        Self {
            event,
            additional_events: Vec::new(),
            ui_events: Vec::new(),
        }
    }

    /// Create result with pipeline event and UI events.
    pub fn with_ui(event: PipelineEvent, ui_events: Vec<UIEvent>) -> Self {
        Self {
            event,
            additional_events: Vec::new(),
            ui_events,
        }
    }

    /// Add an additional event to process after the primary event.
    ///
    /// Used for emitting separate events like SessionEstablished after
    /// agent invocation completes. Each additional event is processed
    /// by the reducer in order.
    pub fn with_additional_event(mut self, event: PipelineEvent) -> Self {
        self.additional_events.push(event);
        self
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
