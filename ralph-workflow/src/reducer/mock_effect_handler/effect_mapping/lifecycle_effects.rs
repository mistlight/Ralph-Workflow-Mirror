//! Lifecycle and system effect-to-event mapping.
//!
//! This module handles effect execution for pipeline lifecycle and system effects.
//! These effects manage the overall pipeline state, not specific phase logic.
//!
//! ## Lifecycle Effects
//!
//! ### Agent Management
//! - **InitializeAgentChain** - Set up agent chain for a new phase
//! - **BackoffWait** - Wait before retrying after agent failure
//! - **ReportAgentChainExhausted** - Report when all agents in chain have failed
//!
//! ### Checkpointing
//! - **SaveCheckpoint** - Save pipeline state for resume capability
//!
//! ### Continuation (Development Phase)
//! - **WriteContinuationContext** - Save context for continuing partial work
//! - **CleanupContinuationContext** - Clean continuation context after completion
//!
//! ### Finalization
//! - **ValidateFinalState** - Validate pipeline completed successfully
//! - **CleanupContext** - Clean up temporary files
//! - **LockPromptPermissions** - Lock PROMPT.md with read-only permissions at startup
//! - **RestorePromptPermissions** - Restore file permissions changed during execution
//! - **EnsureGitignoreEntries** - Ensure required gitignore entries exist
//!
//! ### Error Recovery
//! - **TriggerDevFixFlow** - Trigger manual intervention workflow (panics in mock)
//! - **EmitCompletionMarkerAndTerminate** - Emit completion marker for external monitoring
//! - **TriggerLoopRecovery** - Recover from detected infinite loops
//!
//! ## Mock Behavior
//!
//! - **SaveCheckpoint** automatically emits phase completion events when appropriate
//! - **InitializeAgentChain** emits phase transition UI events
//! - **TriggerDevFixFlow** panics (requires real workspace access)
//! - **ReportAgentChainExhausted** panics (should not occur in normal test flow)

use crate::reducer::effect::Effect;
use crate::reducer::event::{AwaitingDevFixEvent, CheckpointTrigger, PipelineEvent, PipelinePhase};
use crate::reducer::ui_event::UIEvent;

use super::super::MockEffectHandler;

impl MockEffectHandler {
    /// Handle lifecycle and system effects.
    ///
    /// Returns appropriate mock events for lifecycle effects without
    /// performing real I/O or system operations.
    pub(super) fn handle_lifecycle_effect(
        &mut self,
        effect: Effect,
    ) -> Option<(PipelineEvent, Vec<UIEvent>, Vec<PipelineEvent>)> {
        match effect {
            Effect::AgentInvocation {
                role,
                agent,
                model: _,
                prompt: _,
            } => {
                let ui = vec![UIEvent::AgentActivity {
                    agent: agent.clone(),
                    message: format!("Completed {} task", role),
                }];
                Some((
                    PipelineEvent::agent_invocation_succeeded(role, agent),
                    ui,
                    vec![],
                ))
            }

            Effect::InitializeAgentChain { role } => {
                // Emit phase transition when initializing agent chain for a new phase
                let ui = match role {
                    crate::agents::AgentRole::Developer
                        if self.state.phase == PipelinePhase::Planning =>
                    {
                        vec![UIEvent::PhaseTransition {
                            from: None,
                            to: PipelinePhase::Planning,
                        }]
                    }
                    crate::agents::AgentRole::Reviewer
                        if self.state.phase == PipelinePhase::Review =>
                    {
                        vec![UIEvent::PhaseTransition {
                            from: Some(self.state.phase),
                            to: PipelinePhase::Review,
                        }]
                    }
                    _ => vec![],
                };
                Some((
                    PipelineEvent::agent_chain_initialized(
                        role,
                        vec!["mock_agent".to_string()],
                        3,
                        1000,
                        2.0,
                        60000,
                    ),
                    ui,
                    vec![],
                ))
            }

            Effect::BackoffWait {
                role,
                cycle,
                duration_ms: _,
            } => Some((
                PipelineEvent::agent_retry_cycle_started(role, cycle),
                vec![],
                vec![],
            )),

            Effect::ReportAgentChainExhausted { role, phase, cycle } => {
                panic!(
                    "MockEffectHandler received ReportAgentChainExhausted effect: role={:?}, phase={:?}, cycle={}",
                    role, phase, cycle
                )
            }

            Effect::ValidateFinalState => {
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::Finalizing,
                }];
                Some((PipelineEvent::finalizing_started(), ui, vec![]))
            }

            Effect::SaveCheckpoint { trigger } => {
                let checkpoint_saved = PipelineEvent::checkpoint_saved(trigger);
                let mut additional_events = Vec::new();

                if trigger == CheckpointTrigger::PhaseTransition {
                    match self.state.phase {
                        PipelinePhase::Planning => {
                            additional_events.push(PipelineEvent::planning_phase_completed());
                        }
                        PipelinePhase::Development => {
                            // Only emit completion if we've completed all iterations
                            if self.state.iteration >= self.state.total_iterations {
                                additional_events
                                    .push(PipelineEvent::development_phase_completed());
                            }
                        }
                        PipelinePhase::Review
                            if self.state.reviewer_pass >= self.state.total_reviewer_passes =>
                        {
                            additional_events.push(PipelineEvent::review_phase_completed(
                                /* early_exit */ false,
                            ));
                        }
                        PipelinePhase::CommitMessage => {
                            // Commit phase completion is modeled as "commit happened".
                            // The orchestrator uses SaveCheckpoint(PhaseTransition) after commit
                            // reaches a terminal state. Emit a synthetic commit completion that
                            // advances the pipeline to FinalValidation.
                            additional_events.push(PipelineEvent::commit_skipped(
                                "Mock: commit phase transition".to_string(),
                            ));
                        }
                        _ => {}
                    }
                }

                Some((checkpoint_saved, vec![], additional_events))
            }

            Effect::CleanupContext => Some((PipelineEvent::context_cleaned(), vec![], vec![])),

            Effect::RestorePromptPermissions => {
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::Complete,
                }];
                Some((PipelineEvent::prompt_permissions_restored(), ui, vec![]))
            }

            Effect::LockPromptPermissions => {
                // Mock always succeeds with no warning
                Some((
                    PipelineEvent::prompt_permissions_locked(None),
                    vec![],
                    vec![],
                ))
            }

            Effect::WriteContinuationContext(ref data) => Some((
                PipelineEvent::development_continuation_context_written(
                    data.iteration,
                    data.attempt,
                ),
                vec![],
                vec![],
            )),

            Effect::CleanupContinuationContext => Some((
                PipelineEvent::development_continuation_context_cleaned(),
                vec![],
                vec![],
            )),

            Effect::TriggerDevFixFlow { .. } => {
                // Handled in execute() method to access PhaseContext workspace
                panic!(
                    "TriggerDevFixFlow should be handled in execute() method, not execute_mock()"
                )
            }

            Effect::EmitCompletionMarkerAndTerminate {
                is_failure,
                reason: _,
            } => Some((
                PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure,
                }),
                vec![],
                vec![],
            )),

            Effect::TriggerLoopRecovery {
                detected_loop,
                loop_count,
            } => Some((
                PipelineEvent::LoopRecoveryTriggered {
                    detected_loop: detected_loop.clone(),
                    loop_count,
                },
                vec![],
                vec![],
            )),

            Effect::EnsureGitignoreEntries => Some((
                PipelineEvent::gitignore_entries_ensured(
                    vec!["/PROMPT*".to_string(), ".agent/".to_string()],
                    vec![],
                    false,
                ),
                vec![],
                vec![],
            )),

            _ => None,
        }
    }
}
