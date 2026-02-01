//! Reducer function for state transitions.
//!
//! Implements pure state reduction - no side effects, exhaustive pattern matching.
//!
//! # Architecture
//!
//! The main `reduce` function routes events to category-specific handlers based
//! on event type, providing type-safe dispatch:
//!
//! | Category     | Handler                    | Responsibility                    |
//! |--------------|----------------------------|-----------------------------------|
//! | Lifecycle    | reduce_lifecycle_event     | Pipeline start/stop/abort         |
//! | Planning     | reduce_planning_event      | Plan generation                   |
//! | Development  | reduce_development_event   | Dev iterations, continuation      |
//! | Review       | reduce_review_event        | Review passes, fix attempts       |
//! | Agent        | reduce_agent_event         | Agent chain, fallback, retries    |
//! | Rebase       | reduce_rebase_event        | Rebase state machine              |
//! | Commit       | reduce_commit_event        | Commit message generation         |
//!
//! Each handler is a pure function that takes state and its specific event type,
//! enabling compile-time verification of exhaustive matching within each category.

use super::event::{
    AgentEvent, CommitEvent, DevelopmentEvent, LifecycleEvent, PipelineEvent, PlanningEvent,
    RebaseEvent, ReviewEvent,
};
use super::state::{
    CommitState, ContinuationState, DevelopmentStatus, FixStatus, PipelineState,
    PlanningValidatedOutcome, RebaseState,
};
use crate::agents::AgentRole;

/// Pure reducer - no side effects, exhaustive match.
///
/// Computes new state by applying an event to current state.
/// This function has zero side effects - all state mutations are explicit.
///
/// # Event Routing
///
/// Events are routed to category-specific reducers based on their type:
///
/// | Category     | Handler                    | Responsibility                    |
/// |--------------|----------------------------|-----------------------------------|
/// | Lifecycle    | reduce_lifecycle_event     | Pipeline start/stop/abort         |
/// | Planning     | reduce_planning_event      | Plan generation                   |
/// | Development  | reduce_development_event   | Dev iterations, continuation      |
/// | Review       | reduce_review_event        | Review passes, fix attempts       |
/// | Agent        | reduce_agent_event         | Agent chain, fallback, retries    |
/// | Rebase       | reduce_rebase_event        | Rebase state machine              |
/// | Commit       | reduce_commit_event        | Commit message generation         |
///
/// Miscellaneous events are handled directly in this function.
pub fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        // Route to category-specific reducers
        PipelineEvent::Lifecycle(e) => reduce_lifecycle_event(state, e),
        PipelineEvent::Planning(e) => reduce_planning_event(state, e),
        PipelineEvent::Development(e) => reduce_development_event(state, e),
        PipelineEvent::Review(e) => reduce_review_event(state, e),
        PipelineEvent::Agent(e) => reduce_agent_event(state, e),
        PipelineEvent::Rebase(e) => reduce_rebase_event(state, e),
        PipelineEvent::Commit(e) => reduce_commit_event(state, e),

        // Handle miscellaneous events directly
        PipelineEvent::ContextCleaned => PipelineState {
            context_cleaned: true,
            ..state
        },
        PipelineEvent::CheckpointSaved { .. } => {
            let checkpoint_saved_count = state.checkpoint_saved_count.saturating_add(1);
            PipelineState {
                checkpoint_saved_count,
                ..state
            }
        }
        PipelineEvent::FinalizingStarted => PipelineState {
            phase: super::event::PipelinePhase::Finalizing,
            ..state
        },
        PipelineEvent::PromptPermissionsRestored => PipelineState {
            phase: super::event::PipelinePhase::Complete,
            ..state
        },
    }
}

// ============================================================================
// Category-specific reducers
// ============================================================================

/// Handle pipeline lifecycle events.
///
/// Lifecycle events control the overall pipeline execution state:
/// - Started/Resumed: Initialize or restore pipeline (no state change needed)
/// - Completed: Transition to Complete phase
/// - Aborted: Transition to Interrupted phase
fn reduce_lifecycle_event(state: PipelineState, event: LifecycleEvent) -> PipelineState {
    match event {
        LifecycleEvent::Started => state,
        LifecycleEvent::Resumed { .. } => state,
        LifecycleEvent::Completed => PipelineState {
            phase: super::event::PipelinePhase::Complete,
            ..state
        },
        LifecycleEvent::Aborted { .. } => PipelineState {
            phase: super::event::PipelinePhase::Interrupted,
            ..state
        },
    }
}

/// Handle planning phase events.
///
/// Planning events manage plan generation and validation:
/// - PhaseStarted: Set phase to Planning
/// - GenerationCompleted(valid=true): Transition to Development
/// - GenerationCompleted(valid=false): Stay in Planning for retry
/// - PhaseCompleted: Transition to Development
fn reduce_planning_event(state: PipelineState, event: PlanningEvent) -> PipelineState {
    match event {
        PlanningEvent::PhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Planning,
            planning_prompt_prepared_iteration: None,
            planning_xml_cleaned_iteration: None,
            planning_agent_invoked_iteration: None,
            planning_xml_extracted_iteration: None,
            planning_validated_outcome: None,
            planning_markdown_written_iteration: None,
            planning_xml_archived_iteration: None,
            continuation: ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Development,
            planning_prompt_prepared_iteration: None,
            planning_xml_cleaned_iteration: None,
            planning_agent_invoked_iteration: None,
            planning_xml_extracted_iteration: None,
            planning_validated_outcome: None,
            planning_markdown_written_iteration: None,
            planning_xml_archived_iteration: None,
            continuation: ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PromptPrepared { iteration } => PipelineState {
            planning_prompt_prepared_iteration: Some(iteration),
            continuation: ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PlanXmlCleaned { iteration } => PipelineState {
            planning_xml_cleaned_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::AgentInvoked { iteration } => PipelineState {
            planning_agent_invoked_iteration: Some(iteration),
            continuation: ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PlanXmlExtracted { iteration } => PipelineState {
            planning_xml_extracted_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::PlanXmlValidated {
            iteration,
            valid,
            markdown,
        } => PipelineState {
            planning_validated_outcome: Some(PlanningValidatedOutcome {
                iteration,
                valid,
                markdown,
            }),
            ..state
        },
        PlanningEvent::PlanMarkdownWritten { iteration } => PipelineState {
            planning_markdown_written_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::PlanXmlArchived { iteration } => PipelineState {
            planning_xml_archived_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::GenerationCompleted { valid, .. } => {
            if valid {
                PipelineState {
                    phase: super::event::PipelinePhase::Development,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    continuation: ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                // Do not proceed to Development without a valid plan.
                PipelineState {
                    phase: super::event::PipelinePhase::Planning,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    ..state
                }
            }
        }

        PlanningEvent::OutputValidationFailed { iteration, attempt }
        | PlanningEvent::PlanXmlMissing { iteration, attempt } => {
            let new_xsd_count = state.continuation.xsd_retry_count + 1;
            if new_xsd_count >= state.continuation.max_xsd_retry_count {
                // XSD retries exhausted - switch to next agent
                let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
                PipelineState {
                    phase: super::event::PipelinePhase::Planning,
                    iteration,
                    agent_chain: new_agent_chain,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    continuation: ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                // Stay in Planning, increment attempt counters, set retry pending
                PipelineState {
                    phase: super::event::PipelinePhase::Planning,
                    iteration,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    continuation: ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        xsd_retry_count: new_xsd_count,
                        xsd_retry_pending: true,
                        ..state.continuation
                    },
                    ..state
                }
            }
        }
    }
}

/// Handle development phase events.
///
/// Development events manage iteration execution and continuation:
/// - IterationStarted: Reset agent chain, clear continuation state
/// - IterationCompleted(valid=true): Transition to CommitMessage
/// - IterationCompleted(valid=false): Stay in Development for retry
/// - ContinuationTriggered: Save context for retry
/// - ContinuationSucceeded: Clear continuation, proceed to CommitMessage
/// - PhaseCompleted: Transition to Review
fn reduce_development_event(state: PipelineState, event: DevelopmentEvent) -> PipelineState {
    match event {
        DevelopmentEvent::PhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Development,
            continuation: super::state::ContinuationState {
                context_write_pending: false,
                context_cleanup_pending: false,
                ..state.continuation
            },
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            ..state
        },
        DevelopmentEvent::IterationStarted { iteration } => PipelineState {
            iteration,
            agent_chain: state.agent_chain.reset(),
            // Reset continuation state when starting a new iteration
            continuation: super::state::ContinuationState {
                context_cleanup_pending: true,
                ..state.continuation.reset()
            },
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            ..state
        },
        DevelopmentEvent::ContextPrepared { iteration } => PipelineState {
            development_context_prepared_iteration: Some(iteration),
            ..state
        },
        DevelopmentEvent::PromptPrepared { iteration } => PipelineState {
            development_prompt_prepared_iteration: Some(iteration),
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },
        DevelopmentEvent::XmlCleaned { iteration } => PipelineState {
            development_xml_cleaned_iteration: Some(iteration),
            ..state
        },
        DevelopmentEvent::AgentInvoked { iteration } => PipelineState {
            development_agent_invoked_iteration: Some(iteration),
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },
        DevelopmentEvent::XmlExtracted { iteration } => PipelineState {
            development_xml_extracted_iteration: Some(iteration),
            ..state
        },
        DevelopmentEvent::XmlValidated {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        } => PipelineState {
            development_validated_outcome: Some(super::state::DevelopmentValidatedOutcome {
                iteration,
                status,
                summary,
                files_changed,
                next_steps,
            }),
            ..state
        },
        DevelopmentEvent::XmlArchived { iteration } => PipelineState {
            development_xml_archived_iteration: Some(iteration),
            ..state
        },
        DevelopmentEvent::OutcomeApplied { iteration } => {
            let Some(outcome) = state
                .development_validated_outcome
                .as_ref()
                .filter(|o| o.iteration == iteration)
            else {
                return state;
            };

            let continuation_state = &state.continuation;
            let max_continuations = continuation_state.max_continue_count.saturating_sub(1);

            let next_event = if matches!(outcome.status, DevelopmentStatus::Completed) {
                if continuation_state.is_continuation() {
                    DevelopmentEvent::ContinuationSucceeded {
                        iteration,
                        total_continuation_attempts: continuation_state.continuation_attempt,
                    }
                } else {
                    DevelopmentEvent::IterationCompleted {
                        iteration,
                        output_valid: true,
                    }
                }
            } else if continuation_state.continuation_attempt > max_continuations
                || continuation_state.continuation_attempt + 1 > max_continuations
            {
                DevelopmentEvent::ContinuationBudgetExhausted {
                    iteration,
                    total_attempts: continuation_state.continuation_attempt,
                    last_status: outcome.status.clone(),
                }
            } else {
                DevelopmentEvent::ContinuationTriggered {
                    iteration,
                    status: outcome.status.clone(),
                    summary: outcome.summary.clone(),
                    files_changed: outcome.files_changed.clone(),
                    next_steps: outcome.next_steps.clone(),
                }
            };

            reduce_development_event(state, next_event)
        }
        DevelopmentEvent::IterationCompleted {
            iteration,
            output_valid,
        } => {
            if output_valid {
                // After a successful dev iteration, go to CommitMessage phase to create a commit.
                PipelineState {
                    phase: super::event::PipelinePhase::CommitMessage,
                    previous_phase: Some(super::event::PipelinePhase::Development),
                    iteration,
                    commit: super::state::CommitState::NotStarted,
                    commit_prompt_prepared: false,
                    commit_diff_prepared: false,
                    commit_diff_empty: false,
                    commit_agent_invoked: false,
                    commit_xml_cleaned: false,
                    commit_xml_extracted: false,
                    commit_validated_outcome: None,
                    commit_xml_archived: false,
                    context_cleaned: false,
                    // Reset continuation state on successful completion
                    // Use reset() to preserve configured limits (max_xsd_retry_count, etc.)
                    continuation: ContinuationState {
                        context_cleanup_pending: true,
                        ..state.continuation.reset()
                    },
                    development_context_prepared_iteration: None,
                    development_prompt_prepared_iteration: None,
                    development_xml_cleaned_iteration: None,
                    development_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    ..state
                }
            } else {
                // Output was not valid enough to proceed to commit; retry in Development.
                let invalid_output_attempts = state.continuation.invalid_output_attempts + 1;
                if invalid_output_attempts > super::state::MAX_DEV_INVALID_OUTPUT_RERUNS {
                    let new_agent_chain = state.agent_chain.switch_to_next_agent();
                    let continuation = ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    };

                    PipelineState {
                        phase: super::event::PipelinePhase::Development,
                        iteration,
                        continuation,
                        agent_chain: new_agent_chain,
                        development_context_prepared_iteration: None,
                        development_prompt_prepared_iteration: None,
                        development_xml_cleaned_iteration: None,
                        development_agent_invoked_iteration: None,
                        development_xml_extracted_iteration: None,
                        development_validated_outcome: None,
                        development_xml_archived_iteration: None,
                        ..state
                    }
                } else {
                    let continuation = ContinuationState {
                        invalid_output_attempts,
                        ..state.continuation
                    };

                    PipelineState {
                        phase: super::event::PipelinePhase::Development,
                        iteration,
                        continuation,
                        development_context_prepared_iteration: None,
                        development_prompt_prepared_iteration: None,
                        development_xml_cleaned_iteration: None,
                        development_agent_invoked_iteration: None,
                        development_xml_extracted_iteration: None,
                        development_validated_outcome: None,
                        development_xml_archived_iteration: None,
                        ..state
                    }
                }
            }
        }
        DevelopmentEvent::PhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Review,
            // Reset continuation state when phase completes
            continuation: ContinuationState::new(),
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            ..state
        },
        DevelopmentEvent::ContinuationTriggered {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        } => {
            // Trigger continuation with context from the previous attempt
            PipelineState {
                iteration,
                continuation: state.continuation.trigger_continuation(
                    status,
                    summary,
                    files_changed,
                    next_steps,
                ),
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                ..state
            }
        }
        DevelopmentEvent::ContinuationSucceeded {
            iteration,
            total_continuation_attempts: _,
        } => {
            // Continuation succeeded; proceed to CommitMessage and reset continuation state.
            PipelineState {
                phase: super::event::PipelinePhase::CommitMessage,
                previous_phase: Some(super::event::PipelinePhase::Development),
                iteration,
                commit: super::state::CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_diff_prepared: false,
                commit_diff_empty: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                context_cleaned: false,
                continuation: ContinuationState {
                    context_cleanup_pending: true,
                    ..ContinuationState::new()
                },
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                ..state
            }
        }
        DevelopmentEvent::OutputValidationFailed { iteration, attempt }
        | DevelopmentEvent::XmlMissing { iteration, attempt } => {
            // Policy: After configured XSD retries are exhausted, switch to next agent.
            // This keeps invalid output retry logic in the reducer, not the handler.
            let new_xsd_count = state.continuation.xsd_retry_count + 1;
            if new_xsd_count >= state.continuation.max_xsd_retry_count {
                // XSD retries exhausted - switch to next agent
                let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
                PipelineState {
                    phase: super::event::PipelinePhase::Development,
                    iteration,
                    agent_chain: new_agent_chain,
                    continuation: ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    development_context_prepared_iteration: None,
                    development_prompt_prepared_iteration: None,
                    development_xml_cleaned_iteration: None,
                    development_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    ..state
                }
            } else {
                // Stay in Development, increment attempt counters, set retry pending
                PipelineState {
                    phase: super::event::PipelinePhase::Development,
                    iteration,
                    continuation: ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        xsd_retry_count: new_xsd_count,
                        xsd_retry_pending: true,
                        ..state.continuation
                    },
                    development_context_prepared_iteration: None,
                    development_prompt_prepared_iteration: None,
                    development_xml_cleaned_iteration: None,
                    development_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    ..state
                }
            }
        }
        DevelopmentEvent::ContinuationBudgetExhausted {
            iteration,
            total_attempts: _,
            last_status: _,
        } => {
            // Policy: Abort pipeline when continuations exhausted.
            // Future enhancement: Could try fallback agent instead.
            PipelineState {
                phase: super::event::PipelinePhase::Interrupted,
                iteration,
                continuation: ContinuationState {
                    context_cleanup_pending: true,
                    ..ContinuationState::new()
                },
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                ..state
            }
        }
        DevelopmentEvent::ContinuationContextWritten {
            iteration,
            attempt: _,
        } => {
            // Context file was written, state remains unchanged.
            // The continuation state is already set by ContinuationTriggered.
            PipelineState {
                iteration,
                continuation: super::state::ContinuationState {
                    context_write_pending: false,
                    ..state.continuation
                },
                ..state
            }
        }
        DevelopmentEvent::ContinuationContextCleaned => {
            // Context file was cleaned up, no state change needed.
            PipelineState {
                continuation: super::state::ContinuationState {
                    context_cleanup_pending: false,
                    ..state.continuation
                },
                ..state
            }
        }
    }
}

/// Handle review phase events.
///
/// Review events manage review passes and fix attempts:
/// - PhaseStarted: Set phase to Review, reset pass counter
/// - PassStarted: Reset agent chain, prepare for review
/// - Completed(issues_found=true): Stay in Review, issues need fixing
/// - Completed(issues_found=false): Advance to next pass or CommitMessage
/// - FixAttemptCompleted: Transition to CommitMessage
/// - PhaseCompleted: Transition to CommitMessage
fn reduce_review_event(state: PipelineState, event: ReviewEvent) -> PipelineState {
    match event {
        ReviewEvent::PhaseStarted => PipelineState {
            phase: super::event::PipelinePhase::Review,
            reviewer_pass: 0,
            review_issues_found: false,
            // IMPORTANT: entering Review must not reuse a populated developer chain.
            // Clearing the chain ensures orchestration deterministically emits
            // InitializeAgentChain for AgentRole::Reviewer.
            agent_chain: {
                // Entering Review must clear any populated developer chain, but must preserve
                // the configured retry/backoff policy so behavior stays consistent across phases.
                super::state::AgentChainState::initial()
                    .with_max_cycles(state.agent_chain.max_cycles)
                    .with_backoff_policy(
                        state.agent_chain.retry_delay_ms,
                        state.agent_chain.backoff_multiplier,
                        state.agent_chain.max_backoff_ms,
                    )
                    .reset_for_role(AgentRole::Reviewer)
            },
            // Entering Review must reset continuation state to avoid leaking
            // development continuation context into review/fix/rebase logic.
            continuation: super::state::ContinuationState::new(),
            review_issues_xml_cleaned_pass: None,
            review_issue_snippets_extracted_pass: None,
            fix_result_xml_cleaned_pass: None,
            ..state
        },
        ReviewEvent::PassStarted { pass } => PipelineState {
            reviewer_pass: pass,
            review_issues_found: false,
            review_context_prepared_pass: None,
            review_prompt_prepared_pass: None,
            review_issues_xml_cleaned_pass: None,
            review_agent_invoked_pass: None,
            review_issues_xml_extracted_pass: None,
            review_validated_outcome: None,
            review_issues_markdown_written_pass: None,
            review_issue_snippets_extracted_pass: None,
            review_issues_xml_archived_pass: None,
            agent_chain: if pass == state.reviewer_pass {
                // If orchestration re-emits PassStarted for the same pass (e.g., retry after
                // OutputValidationFailed), preserve the agent selection so fallback is effective.
                state.agent_chain.clone()
            } else {
                state.agent_chain.reset()
            },
            continuation: if pass == state.reviewer_pass {
                // If orchestration re-emits PassStarted for the same pass (e.g., retry after
                // OutputValidationFailed), clear xsd_retry_pending to prevent infinite loops.
                // The reducer owns retry accounting for determinism.
                super::state::ContinuationState {
                    xsd_retry_pending: false,
                    ..state.continuation
                }
            } else {
                // New pass: reset retry state but preserve configured limits
                super::state::ContinuationState {
                    invalid_output_attempts: 0,
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    ..state.continuation
                }
            },
            ..state
        },

        ReviewEvent::ContextPrepared { pass } => PipelineState {
            review_context_prepared_pass: Some(pass),
            ..state
        },

        ReviewEvent::PromptPrepared { pass } => PipelineState {
            review_prompt_prepared_pass: Some(pass),
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::IssuesXmlCleaned { pass } => PipelineState {
            review_issues_xml_cleaned_pass: Some(pass),
            ..state
        },

        ReviewEvent::AgentInvoked { pass } => PipelineState {
            review_agent_invoked_pass: Some(pass),
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::IssuesXmlExtracted { pass } => PipelineState {
            review_issues_xml_extracted_pass: Some(pass),
            ..state
        },

        ReviewEvent::IssuesXmlValidated {
            pass,
            issues_found,
            clean_no_issues,
            issues,
            no_issues_found,
        } => PipelineState {
            review_validated_outcome: Some(super::state::ReviewValidatedOutcome {
                pass,
                issues_found,
                clean_no_issues,
                issues,
                no_issues_found,
            }),
            ..state
        },

        ReviewEvent::IssuesMarkdownWritten { pass } => PipelineState {
            review_issues_markdown_written_pass: Some(pass),
            ..state
        },

        ReviewEvent::IssueSnippetsExtracted { pass } => PipelineState {
            review_issue_snippets_extracted_pass: Some(pass),
            ..state
        },

        ReviewEvent::IssuesXmlArchived { pass } => PipelineState {
            review_issues_xml_archived_pass: Some(pass),
            ..state
        },
        ReviewEvent::Completed { pass, issues_found } => {
            let next_pass = if issues_found { pass } else { pass + 1 };
            let next_phase = if !issues_found && next_pass >= state.total_reviewer_passes {
                super::event::PipelinePhase::CommitMessage
            } else {
                state.phase
            };

            if next_phase == super::event::PipelinePhase::CommitMessage {
                PipelineState {
                    phase: next_phase,
                    previous_phase: None,
                    reviewer_pass: next_pass,
                    review_issues_found: issues_found,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    commit: super::state::CommitState::NotStarted,
                    commit_prompt_prepared: false,
                    commit_diff_prepared: false,
                    commit_diff_empty: false,
                    commit_agent_invoked: false,
                    commit_xml_cleaned: false,
                    commit_xml_extracted: false,
                    commit_validated_outcome: None,
                    commit_xml_archived: false,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    ..state
                }
            } else {
                PipelineState {
                    phase: next_phase,
                    reviewer_pass: next_pass,
                    review_issues_found: issues_found,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    ..state
                }
            }
        }
        // Fix attempts use the Reviewer agent chain by design. The pipeline has three
        // agent roles: Developer, Reviewer, and Commit. Fixes are performed by the same
        // agent chain configured for review (there is no separate "Fixer" role), since
        // the fix phase is part of the review workflow.
        ReviewEvent::FixAttemptStarted { .. } => PipelineState {
            agent_chain: super::state::AgentChainState::initial()
                .with_max_cycles(state.agent_chain.max_cycles)
                .with_backoff_policy(
                    state.agent_chain.retry_delay_ms,
                    state.agent_chain.backoff_multiplier,
                    state.agent_chain.max_backoff_ms,
                )
                .reset_for_role(AgentRole::Reviewer),
            // Clear pending flags when fix attempt starts to prevent infinite loops.
            // xsd_retry_pending is cleared to ensure the XSD retry effect doesn't re-trigger
            // after the fix attempt starts a fresh agent invocation.
            continuation: super::state::ContinuationState {
                invalid_output_attempts: 0,
                fix_continue_pending: false,
                xsd_retry_pending: false,
                ..state.continuation
            },
            fix_prompt_prepared_pass: None,
            fix_result_xml_cleaned_pass: None,
            fix_agent_invoked_pass: None,
            fix_result_xml_extracted_pass: None,
            fix_validated_outcome: None,
            fix_result_xml_archived_pass: None,
            ..state
        },

        ReviewEvent::FixPromptPrepared { pass } => PipelineState {
            fix_prompt_prepared_pass: Some(pass),
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::FixResultXmlCleaned { pass } => PipelineState {
            fix_result_xml_cleaned_pass: Some(pass),
            ..state
        },

        ReviewEvent::FixAgentInvoked { pass } => PipelineState {
            fix_agent_invoked_pass: Some(pass),
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::FixResultXmlExtracted { pass } => PipelineState {
            fix_result_xml_extracted_pass: Some(pass),
            ..state
        },

        ReviewEvent::FixResultXmlValidated {
            pass,
            status,
            summary,
        } => PipelineState {
            fix_validated_outcome: Some(super::state::FixValidatedOutcome {
                pass,
                status,
                summary,
            }),
            ..state
        },

        ReviewEvent::FixResultXmlArchived { pass } => PipelineState {
            fix_result_xml_archived_pass: Some(pass),
            ..state
        },
        ReviewEvent::FixOutcomeApplied { pass } => {
            let Some(outcome) = state
                .fix_validated_outcome
                .as_ref()
                .filter(|o| o.pass == pass)
            else {
                return state;
            };

            let next_event = if outcome.status.needs_continuation() {
                let next_attempt = state.continuation.fix_continuation_attempt + 1;
                if next_attempt >= state.continuation.max_fix_continue_count {
                    ReviewEvent::FixContinuationBudgetExhausted {
                        pass,
                        total_attempts: next_attempt,
                        last_status: outcome.status.clone(),
                    }
                } else {
                    ReviewEvent::FixContinuationTriggered {
                        pass,
                        status: outcome.status.clone(),
                        summary: outcome.summary.clone(),
                    }
                }
            } else {
                let changes_made = matches!(outcome.status, FixStatus::AllIssuesAddressed);
                ReviewEvent::FixAttemptCompleted { pass, changes_made }
            };

            reduce_review_event(state, next_event)
        }
        ReviewEvent::FixAttemptCompleted { pass, .. } => PipelineState {
            phase: super::event::PipelinePhase::CommitMessage,
            previous_phase: Some(super::event::PipelinePhase::Review),
            reviewer_pass: pass,
            review_issues_found: false,
            fix_prompt_prepared_pass: None,
            fix_result_xml_cleaned_pass: None,
            fix_agent_invoked_pass: None,
            fix_result_xml_extracted_pass: None,
            fix_validated_outcome: None,
            fix_result_xml_archived_pass: None,
            commit: super::state::CommitState::NotStarted,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            continuation: super::state::ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            ..state
        },
        ReviewEvent::PhaseCompleted { .. } => PipelineState {
            phase: super::event::PipelinePhase::CommitMessage,
            previous_phase: None,
            commit: super::state::CommitState::NotStarted,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            continuation: super::state::ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            review_issues_xml_cleaned_pass: None,
            fix_result_xml_cleaned_pass: None,
            ..state
        },
        ReviewEvent::PassCompletedClean { pass } => {
            // Clean pass means no issues found in this pass.
            // Advance to the next pass when more passes remain.
            let next_pass = pass + 1;
            let next_phase = if next_pass >= state.total_reviewer_passes {
                super::event::PipelinePhase::CommitMessage
            } else {
                super::event::PipelinePhase::Review
            };

            if next_phase == super::event::PipelinePhase::CommitMessage {
                PipelineState {
                    phase: next_phase,
                    previous_phase: None,
                    reviewer_pass: next_pass,
                    review_issues_found: false,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    commit: super::state::CommitState::NotStarted,
                    commit_prompt_prepared: false,
                    commit_diff_prepared: false,
                    commit_diff_empty: false,
                    commit_agent_invoked: false,
                    commit_xml_cleaned: false,
                    commit_xml_extracted: false,
                    commit_validated_outcome: None,
                    commit_xml_archived: false,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    ..state
                }
            } else {
                PipelineState {
                    phase: next_phase,
                    reviewer_pass: next_pass,
                    review_issues_found: false,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    ..state
                }
            }
        }
        ReviewEvent::OutputValidationFailed { pass, attempt }
        | ReviewEvent::IssuesXmlMissing { pass, attempt } => {
            // Policy: The reducer maintains retry state for determinism.
            // Handlers should emit `attempt` from state (checkpoint-resume safe).
            let new_xsd_count = state.continuation.xsd_retry_count + 1;

            if new_xsd_count >= state.continuation.max_xsd_retry_count {
                // XSD retries exhausted - switch to next agent
                let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
                PipelineState {
                    phase: super::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    agent_chain: new_agent_chain,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    review_issues_xml_cleaned_pass: None,
                    ..state
                }
            } else {
                // Stay in Review, increment attempt counters, set retry pending
                PipelineState {
                    phase: super::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        xsd_retry_count: new_xsd_count,
                        xsd_retry_pending: true,
                        ..state.continuation
                    },
                    review_issues_xml_cleaned_pass: None,
                    ..state
                }
            }
        }

        // Fix continuation events
        ReviewEvent::FixContinuationTriggered {
            pass,
            status,
            summary,
        } => {
            // Fix output is valid but indicates work is incomplete (issues_remain)
            PipelineState {
                reviewer_pass: pass,
                fix_prompt_prepared_pass: None,
                fix_result_xml_cleaned_pass: None,
                fix_agent_invoked_pass: None,
                fix_result_xml_extracted_pass: None,
                fix_validated_outcome: None,
                fix_result_xml_archived_pass: None,
                continuation: state.continuation.trigger_fix_continuation(status, summary),
                ..state
            }
        }

        ReviewEvent::FixContinuationSucceeded {
            pass,
            total_attempts: _,
        } => {
            // Fix continuation succeeded - transition to CommitMessage
            // Use reset() instead of new() to preserve configured limits
            PipelineState {
                phase: super::event::PipelinePhase::CommitMessage,
                previous_phase: Some(super::event::PipelinePhase::Review),
                reviewer_pass: pass,
                review_issues_found: false,
                commit: super::state::CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: state.continuation.reset(),
                fix_result_xml_cleaned_pass: None,
                ..state
            }
        }

        ReviewEvent::FixContinuationBudgetExhausted {
            pass,
            total_attempts: _,
            last_status: _,
        } => {
            // Fix continuation budget exhausted - proceed to commit with current state
            // Policy: We accept partial fixes rather than blocking the pipeline
            // Use reset() instead of new() to preserve configured limits
            PipelineState {
                phase: super::event::PipelinePhase::CommitMessage,
                previous_phase: Some(super::event::PipelinePhase::Review),
                reviewer_pass: pass,
                commit: super::state::CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: state.continuation.reset(),
                fix_result_xml_cleaned_pass: None,
                ..state
            }
        }

        ReviewEvent::FixOutputValidationFailed { pass, attempt }
        | ReviewEvent::FixResultXmlMissing { pass, attempt } => {
            // Same policy as review output validation failure
            let new_xsd_count = state.continuation.xsd_retry_count + 1;

            if new_xsd_count >= state.continuation.max_xsd_retry_count {
                // XSD retries exhausted - switch to next agent
                let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
                PipelineState {
                    phase: super::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    agent_chain: new_agent_chain,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    ..state
                }
            } else {
                // Stay in Review, increment attempt counters, set retry pending
                PipelineState {
                    phase: super::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        xsd_retry_count: new_xsd_count,
                        xsd_retry_pending: true,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    ..state
                }
            }
        }
    }
}

/// Handle agent-related events.
///
/// Agent events manage the fallback chain and retry logic:
/// - InvocationSucceeded: Clear continuation prompt
/// - InvocationFailed(retriable=true): Try next model
/// - InvocationFailed(retriable=false): Switch to next agent
/// - RateLimitFallback: Immediate agent switch with prompt preservation
/// - AuthFallback: Immediate agent switch, clear session (no prompt preservation)
/// - ChainExhausted: Start new retry cycle
/// - ChainInitialized: Set up agent chain for a role
fn reduce_agent_event(state: PipelineState, event: AgentEvent) -> PipelineState {
    match event {
        AgentEvent::InvocationStarted { .. } => state,
        // Clear continuation prompt on success
        AgentEvent::InvocationSucceeded { .. } => PipelineState {
            agent_chain: state.agent_chain.clear_continuation_prompt(),
            ..state
        },
        // Rate limit (429): immediate agent fallback, preserve prompt context
        // Unlike other retriable errors, rate limits indicate the provider is
        // temporarily exhausted, so we switch to the next agent immediately
        // to continue work without delay.
        AgentEvent::RateLimitFallback { prompt_context, .. } => PipelineState {
            agent_chain: state
                .agent_chain
                .switch_to_next_agent_with_prompt(prompt_context),
            ..state
        },
        // Auth failure (401/403): immediate agent fallback, clear session
        // Unlike rate limits, auth failures indicate credential issues with
        // the current agent, so we don't preserve prompt context - the next
        // agent may have different (valid) credentials.
        AgentEvent::AuthFallback { .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            ..state
        },
        // Other retriable errors (Network, Timeout): try next model
        AgentEvent::InvocationFailed {
            retriable: true, ..
        } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        // Non-retriable errors: switch agent and clear session
        AgentEvent::InvocationFailed {
            retriable: false, ..
        } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            ..state
        },
        AgentEvent::FallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            ..state
        },
        AgentEvent::ChainExhausted { .. } => PipelineState {
            agent_chain: state.agent_chain.start_retry_cycle(),
            ..state
        },
        AgentEvent::ModelFallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        AgentEvent::RetryCycleStarted { .. } => PipelineState {
            agent_chain: state.agent_chain.clear_backoff_pending(),
            ..state
        },
        AgentEvent::ChainInitialized {
            role,
            agents,
            max_cycles,
            retry_delay_ms,
            backoff_multiplier,
            max_backoff_ms,
        } => {
            let models_per_agent = agents.iter().map(|_| vec![]).collect();
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .with_agents(agents, models_per_agent, role)
                    .with_max_cycles(max_cycles)
                    .with_backoff_policy(retry_delay_ms, backoff_multiplier, max_backoff_ms)
                    .reset_for_role(role),
                ..state
            }
        }
        // Session established: store session ID for potential XSD retry
        AgentEvent::SessionEstablished { session_id, .. } => PipelineState {
            agent_chain: state.agent_chain.with_session_id(Some(session_id)),
            ..state
        },
        // XSD validation failed: trigger XSD retry via continuation state
        AgentEvent::XsdValidationFailed { .. } => PipelineState {
            continuation: state.continuation.trigger_xsd_retry(),
            ..state
        },

        // Template variables invalid: switch to next agent (different agent may have different templates)
        // This is treated as a non-retriable error since the template system itself failed.
        AgentEvent::TemplateVariablesInvalid { .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            ..state
        },
    }
}

/// Handle rebase-related events.
///
/// Rebase events manage the rebase state machine:
/// - Started: Transition to InProgress
/// - ConflictDetected: Transition to Conflicted
/// - ConflictResolved: Return to InProgress
/// - Succeeded: Transition to Completed
/// - Failed: Reset to NotStarted
/// - Skipped: Transition to Skipped
/// - Aborted: Keep current state (caller handles restoration)
fn reduce_rebase_event(state: PipelineState, event: RebaseEvent) -> PipelineState {
    match event {
        RebaseEvent::Started {
            target_branch,
            phase: _,
        } => PipelineState {
            rebase: RebaseState::InProgress {
                original_head: state.current_head(),
                target_branch,
            },
            ..state
        },
        RebaseEvent::ConflictDetected { files } => PipelineState {
            rebase: match &state.rebase {
                RebaseState::InProgress {
                    original_head,
                    target_branch,
                } => RebaseState::Conflicted {
                    original_head: original_head.clone(),
                    target_branch: target_branch.clone(),
                    files,
                    resolution_attempts: 0,
                },
                _ => state.rebase.clone(),
            },
            ..state
        },
        RebaseEvent::ConflictResolved { .. } => PipelineState {
            rebase: match &state.rebase {
                RebaseState::Conflicted {
                    original_head,
                    target_branch,
                    ..
                } => RebaseState::InProgress {
                    original_head: original_head.clone(),
                    target_branch: target_branch.clone(),
                },
                _ => state.rebase.clone(),
            },
            ..state
        },
        RebaseEvent::Succeeded { new_head, .. } => PipelineState {
            rebase: RebaseState::Completed { new_head },
            ..state
        },
        RebaseEvent::Failed { .. } => PipelineState {
            rebase: RebaseState::NotStarted,
            ..state
        },
        RebaseEvent::Skipped { .. } => PipelineState {
            rebase: RebaseState::Skipped,
            ..state
        },
        RebaseEvent::Aborted { .. } => state,
    }
}

/// Handle commit-related events.
///
/// Commit events manage commit message generation and creation:
/// - GenerationStarted: Transition to Generating
/// - MessageGenerated: Transition to Generated
/// - Created: Transition to Committed, advance phase
/// - Skipped: Transition to Skipped, advance phase
/// - GenerationFailed: Reset to NotStarted
/// - MessageValidationFailed: Retry or advance agent
fn reduce_commit_event(state: PipelineState, event: CommitEvent) -> PipelineState {
    match event {
        CommitEvent::GenerationStarted => PipelineState {
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: super::state::MAX_VALIDATION_RETRY_ATTEMPTS,
            },
            commit_prompt_prepared: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            ..state
        },
        CommitEvent::DiffPrepared { empty } => PipelineState {
            commit_diff_prepared: true,
            commit_diff_empty: empty,
            ..state
        },
        CommitEvent::DiffFailed { .. } => PipelineState {
            phase: super::event::PipelinePhase::Interrupted,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            ..state
        },
        CommitEvent::PromptPrepared { .. } => PipelineState {
            commit: match state.commit {
                CommitState::NotStarted => CommitState::Generating {
                    attempt: 1,
                    max_attempts: super::state::MAX_VALIDATION_RETRY_ATTEMPTS,
                },
                _ => state.commit.clone(),
            },
            commit_prompt_prepared: true,
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },
        CommitEvent::AgentInvoked { .. } => PipelineState {
            commit_agent_invoked: true,
            continuation: super::state::ContinuationState {
                xsd_retry_pending: false,
                ..state.continuation
            },
            ..state
        },
        CommitEvent::CommitXmlCleaned { .. } => PipelineState {
            commit_xml_cleaned: true,
            ..state
        },
        CommitEvent::CommitXmlExtracted { .. } => PipelineState {
            commit_xml_extracted: true,
            ..state
        },
        CommitEvent::CommitXmlMissing { attempt } => PipelineState {
            commit_xml_extracted: true,
            commit_validated_outcome: Some(super::state::CommitValidatedOutcome {
                attempt,
                message: None,
                reason: Some("Commit XML missing".to_string()),
            }),
            ..state
        },
        CommitEvent::CommitXmlValidated { message, attempt } => PipelineState {
            commit_validated_outcome: Some(super::state::CommitValidatedOutcome {
                attempt,
                message: Some(message),
                reason: None,
            }),
            ..state
        },
        CommitEvent::CommitXmlValidationFailed { reason, attempt } => PipelineState {
            commit_validated_outcome: Some(super::state::CommitValidatedOutcome {
                attempt,
                message: None,
                reason: Some(reason),
            }),
            ..state
        },
        CommitEvent::CommitXmlArchived { .. } => PipelineState {
            commit_xml_archived: true,
            ..state
        },
        CommitEvent::MessageGenerated { message, .. } => PipelineState {
            commit: CommitState::Generated { message },
            ..state
        },
        CommitEvent::Created { hash, .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            // When transitioning from Development to Review, clear the agent chain
            // so orchestration will emit InitializeAgentChain for Reviewer role.
            // This ensures the reviewer fallback chain is used, not the developer chain.
            let agent_chain = if next_phase == super::event::PipelinePhase::Review
                && state.previous_phase == Some(super::event::PipelinePhase::Development)
            {
                super::state::AgentChainState::initial()
                    .with_max_cycles(state.agent_chain.max_cycles)
                    .with_backoff_policy(
                        state.agent_chain.retry_delay_ms,
                        state.agent_chain.backoff_multiplier,
                        state.agent_chain.max_backoff_ms,
                    )
                    .reset_for_role(crate::agents::AgentRole::Reviewer)
            } else {
                state.agent_chain.clone()
            };

            let continuation = if next_phase == super::event::PipelinePhase::Planning {
                ContinuationState {
                    invalid_output_attempts: 0,
                    ..state.continuation
                }
            } else {
                state.continuation.clone()
            };
            PipelineState {
                commit: CommitState::Committed { hash },
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                context_cleaned: false,
                commit_xml_cleaned: false,
                agent_chain,
                continuation,
                ..state
            }
        }
        CommitEvent::GenerationFailed { .. } => PipelineState {
            commit: CommitState::NotStarted,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            ..state
        },
        CommitEvent::Skipped { .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            // When transitioning from Development to Review, clear the agent chain
            // so orchestration will emit InitializeAgentChain for Reviewer role.
            // This ensures the reviewer fallback chain is used, not the developer chain.
            let agent_chain = if next_phase == super::event::PipelinePhase::Review
                && state.previous_phase == Some(super::event::PipelinePhase::Development)
            {
                super::state::AgentChainState::initial()
                    .with_max_cycles(state.agent_chain.max_cycles)
                    .with_backoff_policy(
                        state.agent_chain.retry_delay_ms,
                        state.agent_chain.backoff_multiplier,
                        state.agent_chain.max_backoff_ms,
                    )
                    .reset_for_role(crate::agents::AgentRole::Reviewer)
            } else {
                state.agent_chain.clone()
            };

            let continuation = if next_phase == super::event::PipelinePhase::Planning {
                ContinuationState {
                    invalid_output_attempts: 0,
                    ..state.continuation
                }
            } else {
                state.continuation.clone()
            };
            PipelineState {
                commit: CommitState::Skipped,
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                context_cleaned: false,
                agent_chain,
                continuation,
                ..state
            }
        }
        CommitEvent::MessageValidationFailed { attempt, .. } => {
            reduce_commit_validation_failed(state, attempt)
        }
    }
}

/// Compute phase transition after a commit (used by CommitCreated and CommitSkipped).
fn compute_post_commit_transition(
    state: &PipelineState,
) -> (super::event::PipelinePhase, u32, u32) {
    match state.previous_phase {
        Some(super::event::PipelinePhase::Development) => {
            let next_iter = state.iteration + 1;
            if next_iter >= state.total_iterations {
                if state.total_reviewer_passes == 0 {
                    (
                        super::event::PipelinePhase::FinalValidation,
                        next_iter,
                        state.reviewer_pass,
                    )
                } else {
                    (
                        super::event::PipelinePhase::Review,
                        next_iter,
                        state.reviewer_pass,
                    )
                }
            } else {
                (
                    super::event::PipelinePhase::Planning,
                    next_iter,
                    state.reviewer_pass,
                )
            }
        }
        Some(super::event::PipelinePhase::Review) => {
            let next_pass = state.reviewer_pass + 1;
            if next_pass >= state.total_reviewer_passes {
                (
                    super::event::PipelinePhase::FinalValidation,
                    state.iteration,
                    next_pass,
                )
            } else {
                (
                    super::event::PipelinePhase::Review,
                    state.iteration,
                    next_pass,
                )
            }
        }
        _ => (
            super::event::PipelinePhase::FinalValidation,
            state.iteration,
            state.reviewer_pass,
        ),
    }
}

/// Handle commit message validation failure with XSD retry logic.
///
/// This now integrates with the XSD retry tracking in ContinuationState
/// for uniformity with other phases.
fn reduce_commit_validation_failed(state: PipelineState, attempt: u32) -> PipelineState {
    let new_xsd_count = state.continuation.xsd_retry_count + 1;
    let max_attempts = super::state::MAX_VALIDATION_RETRY_ATTEMPTS;

    // Check if XSD retries are exhausted (global limit) or local attempts exhausted
    if new_xsd_count >= state.continuation.max_xsd_retry_count || attempt >= max_attempts {
        // XSD retries exhausted - switch to next agent
        let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();

        // Check if we successfully advanced to next agent
        let advanced = new_agent_chain.current_agent_index != state.agent_chain.current_agent_index
            && new_agent_chain.retry_cycle == state.agent_chain.retry_cycle;

        if advanced {
            // Reset for new agent
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::Generating {
                    attempt: 1,
                    max_attempts,
                },
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: super::state::ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    ..state.continuation
                },
                ..state
            }
        } else {
            // All agents exhausted - reset so orchestration can handle
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: super::state::ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    ..state.continuation
                },
                ..state
            }
        }
    } else {
        // Set XSD retry pending - orchestration will trigger retry with same agent/session
        PipelineState {
            commit: CommitState::Generating {
                attempt: attempt + 1,
                max_attempts,
            },
            commit_prompt_prepared: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            continuation: super::state::ContinuationState {
                xsd_retry_count: new_xsd_count,
                xsd_retry_pending: true,
                ..state.continuation
            },
            ..state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRole;
    use crate::reducer::event::AgentErrorKind;
    use crate::reducer::event::PipelinePhase;
    use crate::reducer::event::RebasePhase;
    use crate::reducer::state::AgentChainState;

    fn create_test_state() -> PipelineState {
        PipelineState {
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec!["model1".to_string(), "model2".to_string()]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(5, 2)
        }
    }

    #[test]
    fn test_review_phase_started_clears_agent_chain_for_reviewer_role() {
        use crate::reducer::orchestration::determine_next_effect;

        // Simulate typical state after Development where the agent chain is populated
        // for developer runs.
        let state = create_test_state();

        // Enter Review phase.
        let review_state = reduce(state, PipelineEvent::review_phase_started());

        // The reviewer phase must not reuse the developer chain.
        assert!(
            review_state.agent_chain.agents.is_empty(),
            "Review phase should clear populated agent_chain to force reviewer initialization"
        );
        assert_eq!(
            review_state.agent_chain.current_role,
            AgentRole::Reviewer,
            "Review phase should set agent_chain role to Reviewer"
        );

        // Orchestration should deterministically emit InitializeAgentChain for reviewers.
        let effect = determine_next_effect(&review_state);
        assert!(matches!(
            effect,
            crate::reducer::effect::Effect::InitializeAgentChain {
                role: AgentRole::Reviewer
            }
        ));
    }

    #[test]
    fn test_review_phase_started_resets_continuation_state() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let state = PipelineState {
            continuation: ContinuationState {
                previous_status: Some(DevelopmentStatus::Partial),
                previous_summary: Some("prev summary".to_string()),
                previous_files_changed: Some(vec!["src/lib.rs".to_string()]),
                previous_next_steps: Some("next steps".to_string()),
                continuation_attempt: 2,
                invalid_output_attempts: 3,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let review_state = reduce(state, PipelineEvent::review_phase_started());

        assert_eq!(
            review_state.continuation,
            ContinuationState::new(),
            "Entering Review should reset continuation state to avoid cross-phase leakage"
        );
    }

    #[test]
    fn test_review_phase_started_preserves_agent_chain_backoff_policy() {
        // Review phase resets the chain, but must preserve the configured
        // retry/backoff policy so behavior is consistent across phases.
        let mut state = create_test_state();
        state.agent_chain = state
            .agent_chain
            .with_max_cycles(7)
            .with_backoff_policy(1234, 3.5, 98765);

        let review_state = reduce(state.clone(), PipelineEvent::review_phase_started());

        assert_eq!(
            review_state.agent_chain.max_cycles,
            state.agent_chain.max_cycles
        );
        assert_eq!(
            review_state.agent_chain.retry_delay_ms,
            state.agent_chain.retry_delay_ms
        );
        assert_eq!(
            review_state.agent_chain.backoff_multiplier,
            state.agent_chain.backoff_multiplier
        );
        assert_eq!(
            review_state.agent_chain.max_backoff_ms,
            state.agent_chain.max_backoff_ms
        );
    }

    #[test]
    fn test_reduce_pipeline_started() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::pipeline_started());
        assert_eq!(new_state.phase, PipelinePhase::Planning);
    }

    #[test]
    fn test_reduce_pipeline_completed() {
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::pipeline_completed());
        assert_eq!(new_state.phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_reduce_development_iteration_completed() {
        // DevelopmentIterationCompleted transitions to CommitMessage phase
        // The iteration counter stays the same; it gets incremented by CommitCreated
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_completed(2, true),
        );
        // Iteration stays at 2 (incremented by CommitCreated later)
        assert_eq!(new_state.iteration, 2);
        // Goes to CommitMessage phase to create a commit
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        // Previous phase stored for return after commit
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Development));
    }

    #[test]
    fn test_reduce_development_iteration_complete_goes_to_commit() {
        // Even on last iteration, DevelopmentIterationCompleted goes to CommitMessage
        // The transition to Review happens after CommitCreated
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 5,
            total_iterations: 5,
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_completed(5, true),
        );
        // Iteration stays at 5 (incremented by CommitCreated later)
        assert_eq!(new_state.iteration, 5);
        // Goes to CommitMessage phase first
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_plan_generation_completed_invalid_does_not_transition_to_development() {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::plan_generation_completed(1, false));

        assert_eq!(
            new_state.phase,
            PipelinePhase::Planning,
            "Invalid plan should keep pipeline in Planning phase"
        );
    }

    #[test]
    fn test_reduce_agent_fallback_to_next_model() {
        let state = create_test_state();
        let initial_agent = state.agent_chain.current_agent().unwrap().clone();
        let initial_model_index = state.agent_chain.current_model_index;

        let new_state = reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                initial_agent.clone(),
                1,
                AgentErrorKind::Network,
                true,
            ),
        );

        assert_ne!(
            new_state.agent_chain.current_model_index,
            initial_model_index
        );
    }

    #[test]
    fn test_reduce_rebase_started() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::rebase_started(RebasePhase::Initial, "main".to_string()),
        );

        assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
    }

    #[test]
    fn test_reduce_rebase_succeeded() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::rebase_succeeded(RebasePhase::Initial, "abc123".to_string()),
        );

        assert!(matches!(new_state.rebase, RebaseState::Completed { .. }));
    }

    #[test]
    fn test_reduce_commit_generation_started() {
        let state = PipelineState {
            commit_diff_prepared: true,
            commit_diff_empty: false,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::commit_generation_started());

        assert!(matches!(new_state.commit, CommitState::Generating { .. }));
        assert!(new_state.commit_diff_prepared);
        assert!(!new_state.commit_diff_empty);
    }

    #[test]
    fn test_reduce_commit_diff_failed_interrupts_pipeline() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::commit_diff_failed("diff failed".to_string()),
        );

        assert_eq!(new_state.phase, PipelinePhase::Interrupted);
        assert!(!new_state.commit_diff_prepared);
        assert!(!new_state.commit_diff_empty);
    }

    #[test]
    fn test_reduce_commit_created() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
        );

        assert!(matches!(new_state.commit, CommitState::Committed { .. }));
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    }

    #[test]
    fn test_reduce_all_agent_failure_scenarios() {
        let state = create_test_state();
        let initial_agent_index = state.agent_chain.current_agent_index;
        let initial_model_index = state.agent_chain.current_model_index;
        let agent_name = state.agent_chain.current_agent().unwrap().clone();

        let network_error_state = reduce(
            state.clone(),
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                agent_name.clone(),
                1,
                AgentErrorKind::Network,
                true,
            ),
        );
        assert_eq!(
            network_error_state.agent_chain.current_agent_index,
            initial_agent_index
        );
        assert!(network_error_state.agent_chain.current_model_index > initial_model_index);

        let auth_error_state = reduce(
            state.clone(),
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                agent_name.clone(),
                1,
                AgentErrorKind::Authentication,
                false,
            ),
        );
        assert!(auth_error_state.agent_chain.current_agent_index > initial_agent_index);
        assert_eq!(
            auth_error_state.agent_chain.current_model_index,
            initial_model_index
        );

        let internal_error_state = reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                agent_name,
                139,
                AgentErrorKind::InternalError,
                false,
            ),
        );
        assert!(internal_error_state.agent_chain.current_agent_index > initial_agent_index);
    }

    #[test]
    fn test_reduce_rebase_full_state_machine() {
        let mut state = create_test_state();

        state = reduce(
            state,
            PipelineEvent::rebase_started(RebasePhase::Initial, "main".to_string()),
        );
        assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

        state = reduce(
            state,
            PipelineEvent::rebase_conflict_detected(vec![std::path::PathBuf::from("file1.txt")]),
        );
        assert!(matches!(state.rebase, RebaseState::Conflicted { .. }));

        state = reduce(
            state,
            PipelineEvent::rebase_conflict_resolved(vec![std::path::PathBuf::from("file1.txt")]),
        );
        assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

        state = reduce(
            state,
            PipelineEvent::rebase_succeeded(RebasePhase::Initial, "def456".to_string()),
        );
        assert!(matches!(state.rebase, RebaseState::Completed { .. }));
    }

    #[test]
    fn test_reduce_commit_full_state_machine() {
        let mut state = create_test_state();

        state = reduce(state, PipelineEvent::commit_generation_started());
        assert!(matches!(state.commit, CommitState::Generating { .. }));

        state = reduce(
            state,
            PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
        );
        assert!(matches!(state.commit, CommitState::Committed { .. }));
    }

    #[test]
    fn test_reduce_phase_transitions() {
        let mut state = create_test_state();

        state = reduce(state, PipelineEvent::planning_phase_completed());
        assert_eq!(state.phase, PipelinePhase::Development);

        state = reduce(state, PipelineEvent::development_phase_started());
        assert_eq!(state.phase, PipelinePhase::Development);

        state = reduce(state, PipelineEvent::development_phase_completed());
        assert_eq!(state.phase, PipelinePhase::Review);

        state = reduce(state, PipelineEvent::review_phase_started());
        assert_eq!(state.phase, PipelinePhase::Review);

        state = reduce(state, PipelineEvent::review_phase_completed(false));
        assert_eq!(state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_reduce_agent_chain_exhaustion() {
        let state = PipelineState {
            agent_chain: AgentChainState::initial()
                .with_agents(
                    vec!["agent1".to_string()],
                    vec![vec!["model1".to_string()]],
                    AgentRole::Developer,
                )
                .with_max_cycles(3),
            ..create_test_state()
        };

        let exhausted_state = reduce(
            state,
            PipelineEvent::agent_chain_exhausted(AgentRole::Developer),
        );

        assert_eq!(exhausted_state.agent_chain.current_agent_index, 0);
        assert_eq!(exhausted_state.agent_chain.current_model_index, 0);
        assert_eq!(exhausted_state.agent_chain.retry_cycle, 1);
    }

    #[test]
    fn test_reduce_agent_fallback_triggers_fallback_event() {
        let state = create_test_state();
        let agent = state.agent_chain.current_agent().unwrap().clone();

        let new_state = reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                agent.clone(),
                1,
                AgentErrorKind::Authentication,
                false,
            ),
        );

        assert!(new_state.agent_chain.current_agent_index > 0);
    }

    #[test]
    fn test_reduce_model_fallback_triggers_for_network_error() {
        let state = create_test_state();
        let initial_model_index = state.agent_chain.current_model_index;
        let agent_name = state.agent_chain.current_agent().unwrap().clone();

        let new_state = reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                agent_name,
                1,
                AgentErrorKind::Network,
                true,
            ),
        );

        assert!(new_state.agent_chain.current_model_index > initial_model_index);
    }

    #[test]
    fn test_rate_limit_fallback_switches_agent() {
        let state = create_test_state();
        let initial_agent_index = state.agent_chain.current_agent_index;

        let new_state = reduce(
            state,
            PipelineEvent::agent_rate_limit_fallback(
                AgentRole::Developer,
                "agent1".to_string(),
                Some("test prompt".to_string()),
            ),
        );

        // Should switch to next agent
        assert!(
            new_state.agent_chain.current_agent_index > initial_agent_index,
            "Rate limit should trigger agent fallback, not model fallback"
        );
        // Should preserve prompt
        assert_eq!(
            new_state.agent_chain.rate_limit_continuation_prompt,
            Some("test prompt".to_string())
        );
    }

    #[test]
    fn test_rate_limit_fallback_with_no_prompt_context() {
        let state = create_test_state();
        let initial_agent_index = state.agent_chain.current_agent_index;

        let new_state = reduce(
            state,
            PipelineEvent::agent_rate_limit_fallback(
                AgentRole::Developer,
                "agent1".to_string(),
                None,
            ),
        );

        // Should still switch to next agent
        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
        // Prompt context should be None
        assert!(new_state
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none());
    }

    #[test]
    fn test_success_clears_rate_limit_continuation_prompt() {
        let mut state = create_test_state();
        state.agent_chain.rate_limit_continuation_prompt = Some("old prompt".to_string());

        let new_state = reduce(
            state,
            PipelineEvent::agent_invocation_succeeded(AgentRole::Developer, "agent1".to_string()),
        );

        assert!(
            new_state
                .agent_chain
                .rate_limit_continuation_prompt
                .is_none(),
            "Success should clear rate limit continuation prompt"
        );
    }

    #[test]
    fn test_auth_fallback_clears_session_and_advances_agent() {
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("session-123".to_string()));
        chain.rate_limit_continuation_prompt = Some("some saved prompt".to_string());

        let state = PipelineState {
            phase: PipelinePhase::Development,
            agent_chain: chain,
            ..PipelineState::initial(5, 2)
        };

        let new_state = reduce(
            state,
            PipelineEvent::agent_auth_fallback(AgentRole::Developer, "agent1".to_string()),
        );

        // Should advance to next agent
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent2");

        // Session should be cleared
        assert!(new_state.agent_chain.last_session_id.is_none());

        // Existing continuation prompt should remain (auth fallback doesn't touch it,
        // but also doesn't SET a new prompt like rate limit does)
        // The key semantic: auth fallback does NOT set rate_limit_continuation_prompt
    }

    #[test]
    fn test_reduce_finalizing_started() {
        let state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::finalizing_started());
        assert_eq!(new_state.phase, PipelinePhase::Finalizing);
    }

    #[test]
    fn test_reduce_prompt_permissions_restored() {
        let state = PipelineState {
            phase: PipelinePhase::Finalizing,
            ..create_test_state()
        };
        let new_state = reduce(state, PipelineEvent::prompt_permissions_restored());
        assert_eq!(new_state.phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_reduce_finalization_full_flow() {
        let mut state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..create_test_state()
        };

        // FinalValidation -> Finalizing
        state = reduce(state, PipelineEvent::finalizing_started());
        assert_eq!(state.phase, PipelinePhase::Finalizing);

        // Finalizing -> Complete
        state = reduce(state, PipelineEvent::prompt_permissions_restored());
        assert_eq!(state.phase, PipelinePhase::Complete);
    }

    /// Test the complete finalization flow from FinalValidation through effects.
    ///
    /// This tests the orchestration + reduction path:
    /// 1. FinalValidation phase -> ValidateFinalState effect
    /// 2. ValidateFinalState effect -> FinalizingStarted event
    /// 3. FinalizingStarted event -> Finalizing phase
    /// 4. Finalizing phase -> RestorePromptPermissions effect
    /// 5. RestorePromptPermissions effect -> PromptPermissionsRestored event
    /// 6. PromptPermissionsRestored event -> Complete phase
    #[test]
    fn test_finalization_orchestration_integration() {
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        use crate::reducer::orchestration::determine_next_effect;

        // Start in FinalValidation
        let initial_state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..PipelineState::initial(5, 2)
        };

        let mut handler = MockEffectHandler::new(initial_state.clone());

        // Step 1: Determine effect for FinalValidation
        let effect1 = determine_next_effect(&initial_state);
        assert!(
            matches!(effect1, crate::reducer::effect::Effect::ValidateFinalState),
            "FinalValidation should emit ValidateFinalState effect"
        );

        // Step 2: Execute effect, get event
        let result1 = handler.execute_mock(effect1);
        assert!(
            matches!(result1.event, PipelineEvent::FinalizingStarted),
            "ValidateFinalState should return FinalizingStarted"
        );

        // Step 3: Reduce state with event
        let state2 = reduce(initial_state, result1.event);
        assert_eq!(state2.phase, PipelinePhase::Finalizing);
        assert!(!state2.is_complete(), "Finalizing should not be complete");

        // Step 4: Determine effect for Finalizing
        let effect2 = determine_next_effect(&state2);
        assert!(
            matches!(
                effect2,
                crate::reducer::effect::Effect::RestorePromptPermissions
            ),
            "Finalizing should emit RestorePromptPermissions effect"
        );

        // Step 5: Execute effect, get event
        let result2 = handler.execute_mock(effect2);
        assert!(
            matches!(result2.event, PipelineEvent::PromptPermissionsRestored),
            "RestorePromptPermissions should return PromptPermissionsRestored"
        );

        // Step 6: Reduce state with event
        let final_state = reduce(state2, result2.event);
        assert_eq!(final_state.phase, PipelinePhase::Complete);
        assert!(final_state.is_complete(), "Complete should be complete");

        // Verify effects were captured
        let effects = handler.captured_effects();
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            effects[0],
            crate::reducer::effect::Effect::ValidateFinalState
        ));
        assert!(matches!(
            effects[1],
            crate::reducer::effect::Effect::RestorePromptPermissions
        ));
    }

    // =========================================================================
    // Continuation event handling tests
    // =========================================================================

    #[test]
    fn test_continuation_triggered_updates_state() {
        use crate::reducer::state::DevelopmentStatus;

        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_continuation_triggered(
                1,
                DevelopmentStatus::Partial,
                "Did work".to_string(),
                Some(vec!["src/main.rs".to_string()]),
                Some("Continue".to_string()),
            ),
        );

        assert!(new_state.continuation.is_continuation());
        assert_eq!(
            new_state.continuation.previous_status,
            Some(DevelopmentStatus::Partial)
        );
        assert_eq!(
            new_state.continuation.previous_summary,
            Some("Did work".to_string())
        );
        assert_eq!(
            new_state.continuation.previous_files_changed,
            Some(vec!["src/main.rs".to_string()])
        );
        assert_eq!(
            new_state.continuation.previous_next_steps,
            Some("Continue".to_string())
        );
        assert_eq!(new_state.continuation.continuation_attempt, 1);
    }

    #[test]
    fn test_continuation_triggered_sets_iteration_from_event() {
        use crate::reducer::state::DevelopmentStatus;

        let state = PipelineState {
            iteration: 99,
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_continuation_triggered(
                2,
                DevelopmentStatus::Partial,
                "Did work".to_string(),
                None,
                None,
            ),
        );

        assert_eq!(new_state.iteration, 2);
    }

    #[test]
    fn test_continuation_triggered_with_failed_status() {
        use crate::reducer::state::DevelopmentStatus;

        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_continuation_triggered(
                1,
                DevelopmentStatus::Failed,
                "Build failed".to_string(),
                None,
                Some("Fix errors".to_string()),
            ),
        );

        assert!(new_state.continuation.is_continuation());
        assert_eq!(
            new_state.continuation.previous_status,
            Some(DevelopmentStatus::Failed)
        );
        assert_eq!(
            new_state.continuation.previous_summary,
            Some("Build failed".to_string())
        );
        assert!(new_state.continuation.previous_files_changed.is_none());
    }

    #[test]
    fn test_continuation_succeeded_resets_state() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );
        assert!(state.continuation.is_continuation());

        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_continuation_succeeded(1, 2),
        );

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.continuation.continuation_attempt, 0);
        assert!(new_state.continuation.previous_status.is_none());
    }

    #[test]
    fn test_continuation_succeeded_sets_iteration_from_event() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 99,
            ..create_test_state()
        };
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );

        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_continuation_succeeded(1, 1),
        );

        assert_eq!(new_state.iteration, 1);
    }

    #[test]
    fn test_iteration_started_resets_continuation() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );
        assert!(state.continuation.is_continuation());

        let new_state = reduce(state, PipelineEvent::development_iteration_started(2));

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.iteration, 2);
    }

    #[test]
    fn test_iteration_completed_resets_continuation() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );

        let new_state = reduce(
            state,
            PipelineEvent::development_iteration_completed(1, true),
        );

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_development_phase_completed_resets_continuation() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );

        let new_state = reduce(state, PipelineEvent::development_phase_completed());

        assert!(!new_state.continuation.is_continuation());
        assert_eq!(new_state.phase, PipelinePhase::Review);
    }

    #[test]
    fn test_multiple_continuation_triggers_accumulate() {
        use crate::reducer::state::DevelopmentStatus;

        let state = create_test_state();

        // First continuation
        let state = reduce(
            state,
            PipelineEvent::development_iteration_continuation_triggered(
                1,
                DevelopmentStatus::Partial,
                "First attempt".to_string(),
                None,
                None,
            ),
        );
        assert_eq!(state.continuation.continuation_attempt, 1);

        // Second continuation
        let state = reduce(
            state,
            PipelineEvent::development_iteration_continuation_triggered(
                1,
                DevelopmentStatus::Partial,
                "Second attempt".to_string(),
                None,
                None,
            ),
        );
        assert_eq!(state.continuation.continuation_attempt, 2);
        assert_eq!(
            state.continuation.previous_summary,
            Some("Second attempt".to_string())
        );
    }

    // =========================================================================
    // OutputValidationFailed event tests
    // =========================================================================

    #[test]
    fn test_output_validation_failed_retries_within_limit() {
        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );
        assert_eq!(new_state.phase, PipelinePhase::Development);
        assert_eq!(new_state.continuation.invalid_output_attempts, 1);
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
    }

    #[test]
    fn test_output_validation_failed_increments_attempt_counter() {
        let mut state = create_test_state();
        state.continuation.invalid_output_attempts = 1;

        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 1),
        );
        assert_eq!(new_state.phase, PipelinePhase::Development);
        assert_eq!(new_state.continuation.invalid_output_attempts, 2);
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
    }

    #[test]
    fn test_output_validation_failed_switches_agent_at_limit() {
        use crate::reducer::state::ContinuationState;

        let state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_count: 1,
                max_xsd_retry_count: 2,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };
        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );
        assert_eq!(new_state.continuation.invalid_output_attempts, 0);
        assert!(
            new_state.agent_chain.current_agent_index > 0,
            "Should switch to next agent after max invalid output attempts"
        );
    }

    #[test]
    fn test_output_validation_failed_resets_counter_on_agent_switch() {
        use crate::reducer::state::ContinuationState;

        let mut state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_count: 1,
                max_xsd_retry_count: 2,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };
        state.continuation.invalid_output_attempts = 2;

        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );
        assert_eq!(
            new_state.continuation.invalid_output_attempts, 0,
            "Counter should reset when switching agents"
        );
    }

    #[test]
    fn test_output_validation_failed_stays_in_development_phase() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Development;

        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );
        assert_eq!(
            new_state.phase,
            PipelinePhase::Development,
            "Should stay in Development phase for retry"
        );
    }

    #[test]
    fn test_output_validation_failed_respects_configured_xsd_retry_limit() {
        use crate::reducer::state::ContinuationState;

        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 1,
            continuation: ContinuationState {
                xsd_retry_count: 1,
                max_xsd_retry_count: 5,
                ..ContinuationState::new()
            },
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(1, 0),
        );

        assert_eq!(
            new_state.agent_chain.current_agent_index, 0,
            "Configured XSD retry limit should allow retries before agent fallback"
        );
        assert!(
            new_state.continuation.xsd_retry_pending,
            "Should request XSD retry while under configured limit"
        );
    }

    // =========================================================================
    // Review output validation / clean pass tests
    // =========================================================================

    #[test]
    fn test_review_output_validation_failed_increments_state_counter() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 2;

        let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));

        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.reviewer_pass, 0);
        assert_eq!(new_state.continuation.invalid_output_attempts, 1);
    }

    #[test]
    fn test_review_output_validation_failed_switches_agent_after_limit() {
        use crate::reducer::state::ContinuationState;

        let mut state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_count: 1,
                max_xsd_retry_count: 2,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 2;
        state.continuation.invalid_output_attempts = 2;

        let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));

        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.reviewer_pass, 0);
        assert_eq!(
            new_state.continuation.invalid_output_attempts, 0,
            "Counter should reset when switching agents"
        );
        assert!(
            new_state.agent_chain.current_agent_index > 0,
            "Should switch to next agent after max invalid output attempts"
        );
    }

    #[test]
    fn test_review_pass_completed_clean_exits_review_phase() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 2;

        let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

        assert_eq!(
            new_state.phase,
            PipelinePhase::Review,
            "Clean pass should not exit review when passes remain"
        );
        assert_eq!(new_state.reviewer_pass, 1);
        assert_eq!(new_state.review_issues_found, false);
    }

    #[test]
    fn test_review_pass_completed_clean_on_last_pass_clears_previous_phase() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 1;
        state.previous_phase = Some(PipelinePhase::Development);

        let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert_eq!(new_state.previous_phase, None);
        assert!(matches!(new_state.commit, CommitState::NotStarted));
    }

    #[test]
    fn test_review_pass_started_does_not_reset_invalid_output_attempts_on_retry() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.continuation.invalid_output_attempts = 1;

        let new_state = reduce(state, PipelineEvent::review_pass_started(0));

        assert_eq!(new_state.reviewer_pass, 0);
        assert_eq!(
            new_state.continuation.invalid_output_attempts, 1,
            "Retrying the same pass should not clear invalid output attempt counter"
        );
    }

    #[test]
    fn test_review_pass_started_preserves_agent_chain_on_retry() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 2;

        // Simulate switching agents due to repeated output validation failures.
        state.continuation.invalid_output_attempts = 2;
        let state = reduce(state, PipelineEvent::review_output_validation_failed(0, 2));
        assert!(
            state.agent_chain.current_agent_index > 0,
            "Precondition: review_output_validation_failed should have switched agents"
        );

        // Orchestration can re-emit PassStarted for the same pass during retries.
        let new_state = reduce(state.clone(), PipelineEvent::review_pass_started(0));

        assert_eq!(
            new_state.agent_chain.current_agent_index, state.agent_chain.current_agent_index,
            "Retrying the same pass should preserve the current agent selection"
        );
    }

    #[test]
    fn test_review_pass_started_resets_invalid_output_attempts_for_new_pass() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.continuation.invalid_output_attempts = 2;

        let new_state = reduce(state, PipelineEvent::review_pass_started(1));

        assert_eq!(new_state.reviewer_pass, 1);
        assert_eq!(new_state.continuation.invalid_output_attempts, 0);
    }

    #[test]
    fn test_review_phase_completed_resets_commit_state() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.commit = CommitState::Committed {
            hash: "abc123".to_string(),
        };

        let new_state = reduce(state, PipelineEvent::review_phase_completed(true));

        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert!(matches!(new_state.commit, CommitState::NotStarted));
        assert_eq!(new_state.previous_phase, None);
    }

    #[test]
    fn test_review_completed_no_issues_on_last_pass_resets_commit_state() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 1;
        state.commit = CommitState::Committed {
            hash: "abc123".to_string(),
        };

        let new_state = reduce(state, PipelineEvent::review_completed(0, false));

        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert!(matches!(new_state.commit, CommitState::NotStarted));
    }

    // =========================================================================
    // ContinuationBudgetExhausted event tests
    // =========================================================================

    #[test]
    fn test_continuation_budget_exhausted_transitions_to_interrupted() {
        use crate::reducer::state::DevelopmentStatus;

        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::development_continuation_budget_exhausted(
                0,
                3,
                DevelopmentStatus::Partial,
            ),
        );
        assert_eq!(
            new_state.phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted when continuation budget exhausted"
        );
    }

    #[test]
    fn test_continuation_budget_exhausted_resets_continuation_state() {
        use crate::reducer::state::{ContinuationState, DevelopmentStatus};

        let mut state = create_test_state();
        state.continuation = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );
        assert!(state.continuation.is_continuation());

        let new_state = reduce(
            state,
            PipelineEvent::development_continuation_budget_exhausted(
                0,
                3,
                DevelopmentStatus::Partial,
            ),
        );
        assert!(
            !new_state.continuation.is_continuation(),
            "Continuation state should be reset"
        );
    }

    #[test]
    fn test_continuation_budget_exhausted_preserves_iteration() {
        use crate::reducer::state::DevelopmentStatus;

        let mut state = create_test_state();
        state.iteration = 5;

        let new_state = reduce(
            state,
            PipelineEvent::development_continuation_budget_exhausted(
                5,
                3,
                DevelopmentStatus::Failed,
            ),
        );
        assert_eq!(
            new_state.iteration, 5,
            "Should preserve the iteration number"
        );
    }

    // =========================================================================
    // Event sequence tests for determinism
    // =========================================================================

    #[test]
    fn test_event_sequence_output_validation_retry_then_success() {
        use crate::reducer::state::ContinuationState;

        let mut state = PipelineState {
            continuation: ContinuationState {
                max_xsd_retry_count: 3,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };
        state.phase = PipelinePhase::Development;

        // Simulate: validation fail -> validation fail -> success
        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );
        assert_eq!(state.continuation.invalid_output_attempts, 1);
        assert_eq!(state.phase, PipelinePhase::Development);

        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 1),
        );
        assert_eq!(state.continuation.invalid_output_attempts, 2);

        assert_eq!(
            state.agent_chain.current_agent_index, 0,
            "Should not switch agents yet"
        );

        // Now succeed
        state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        assert_eq!(state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_event_sequence_validation_failures_trigger_agent_switch() {
        use crate::reducer::state::ContinuationState;

        let mut state = PipelineState {
            continuation: ContinuationState {
                max_xsd_retry_count: 3,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };
        state.phase = PipelinePhase::Development;

        // First validation failure
        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );

        // Second validation failure
        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 1),
        );

        // Third validation failure - should trigger agent switch
        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 2),
        );

        // After max failures, should switch agents and reset counter
        assert_eq!(
            state.continuation.invalid_output_attempts, 0,
            "Counter should reset"
        );
        assert!(
            state.agent_chain.current_agent_index > 0 || state.agent_chain.retry_cycle > 0,
            "Should have advanced to next agent or started retry cycle"
        );
    }

    #[test]
    fn test_determinism_same_events_same_state() {
        use crate::reducer::state::DevelopmentStatus;

        // Create two identical initial states
        let state1 = create_test_state();
        let state2 = create_test_state();

        // Apply the same sequence of events
        let events = vec![
            PipelineEvent::development_iteration_started(0),
            PipelineEvent::development_output_validation_failed(0, 0),
            PipelineEvent::development_iteration_continuation_triggered(
                0,
                DevelopmentStatus::Partial,
                "Work".to_string(),
                None,
                None,
            ),
        ];

        let mut final1 = state1;
        let mut final2 = state2;

        for event in events {
            final1 = reduce(final1, event.clone());
            final2 = reduce(final2, event);
        }

        // States should be identical
        assert_eq!(final1.iteration, final2.iteration);
        assert_eq!(final1.phase, final2.phase);
        assert_eq!(
            final1.continuation.continuation_attempt,
            final2.continuation.continuation_attempt
        );
        assert_eq!(
            final1.continuation.invalid_output_attempts,
            final2.continuation.invalid_output_attempts
        );
    }

    // =========================================================================
    // Dev->Review transition agent chain tests
    // =========================================================================

    /// When transitioning from Development to Review (via CommitCreated or CommitSkipped),
    /// the agent chain must be cleared so that orchestration will emit InitializeAgentChain
    /// for the Reviewer role. This ensures the reviewer fallback chain is used.
    #[test]
    fn test_commit_created_clears_agent_chain_when_dev_to_review() {
        let mut state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            previous_phase: Some(PipelinePhase::Development),
            iteration: 4, // Last iteration (will trigger transition to Review)
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..create_test_state()
        };

        // Populate the agent chain as if it was used for development
        state.agent_chain = AgentChainState::initial()
            .with_agents(
                vec!["dev-agent-1".to_string(), "dev-agent-2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);
        state.agent_chain.current_agent_index = 1; // Simulate having advanced to second agent

        let new_state = reduce(
            state,
            PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
        );

        // Should transition to Review
        assert_eq!(new_state.phase, PipelinePhase::Review);
        // Agent chain should be cleared (empty agents list) so orchestration
        // will initialize it for Reviewer role
        assert!(
            new_state.agent_chain.agents.is_empty(),
            "Agent chain should be cleared when transitioning from Dev to Review, got agents: {:?}",
            new_state.agent_chain.agents
        );
        assert_eq!(
            new_state.agent_chain.current_role,
            AgentRole::Reviewer,
            "Agent chain role should be set to Reviewer"
        );
        assert_eq!(
            new_state.agent_chain.current_agent_index, 0,
            "Agent chain index should be reset to 0"
        );
    }

    /// Same test for CommitSkipped - should also clear agent chain for dev->review transition
    #[test]
    fn test_commit_skipped_clears_agent_chain_when_dev_to_review() {
        let mut state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            previous_phase: Some(PipelinePhase::Development),
            iteration: 4, // Last iteration
            total_iterations: 5,
            total_reviewer_passes: 2,
            ..create_test_state()
        };

        state.agent_chain = AgentChainState::initial()
            .with_agents(
                vec!["dev-agent-1".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        let new_state = reduce(
            state,
            PipelineEvent::commit_skipped("no changes".to_string()),
        );

        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert!(
            new_state.agent_chain.agents.is_empty(),
            "Agent chain should be cleared when transitioning from Dev to Review via skip"
        );
        assert_eq!(new_state.agent_chain.current_role, AgentRole::Reviewer);
    }

    /// Verify that after ChainInitialized for Reviewer, the reducer correctly populates
    /// state.agent_chain with the fallback agents in order.
    #[test]
    fn test_chain_initialized_populates_reviewer_chain() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.agent_chain = AgentChainState::initial().reset_for_role(AgentRole::Reviewer);

        let new_state = reduce(
            state,
            PipelineEvent::agent_chain_initialized(
                AgentRole::Reviewer,
                vec![
                    "codex".to_string(),
                    "opencode".to_string(),
                    "claude".to_string(),
                ],
                3,
                1000,
                2.0,
                60000,
            ),
        );

        assert_eq!(
            new_state.agent_chain.agents,
            vec![
                "codex".to_string(),
                "opencode".to_string(),
                "claude".to_string()
            ],
            "Reducer should store the exact fallback chain from ChainInitialized event"
        );
        assert_eq!(
            new_state.agent_chain.current_agent(),
            Some(&"codex".to_string()),
            "First agent in chain should be 'codex' (first fallback)"
        );
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.current_role, AgentRole::Reviewer);
    }

    /// Auth failure during review should advance the reducer's agent chain,
    /// not just a local variable in review.rs
    #[test]
    fn test_auth_failure_during_review_advances_reducer_chain() {
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.agent_chain = AgentChainState::initial()
            .with_agents(
                vec![
                    "codex".to_string(),
                    "opencode".to_string(),
                    "claude".to_string(),
                ],
                vec![vec![], vec![], vec![]],
                AgentRole::Reviewer,
            )
            .reset_for_role(AgentRole::Reviewer);

        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"codex".to_string()),
            "Precondition: current agent should be codex"
        );

        // Simulate auth failure - this should advance to next agent
        let new_state = reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Reviewer,
                "codex".to_string(),
                1,
                AgentErrorKind::Authentication,
                false, // not retriable
            ),
        );

        assert_eq!(
            new_state.agent_chain.current_agent(),
            Some(&"opencode".to_string()),
            "Auth failure should advance reducer's agent chain to opencode"
        );
        assert_eq!(new_state.agent_chain.current_agent_index, 1);
    }

    /// Orchestration should emit InitializeAgentChain when entering Review phase
    /// with an empty agent chain.
    #[test]
    fn test_orchestration_emits_init_chain_for_reviewer_after_dev_review_transition() {
        use crate::reducer::orchestration::determine_next_effect;

        let mut state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            ..create_test_state()
        };

        // Clear the agent chain as would happen after dev->review transition
        state.agent_chain = AgentChainState::initial().reset_for_role(AgentRole::Reviewer);

        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                crate::reducer::effect::Effect::InitializeAgentChain {
                    role: AgentRole::Reviewer
                }
            ),
            "Orchestration should emit InitializeAgentChain for Reviewer when chain is empty, got {:?}",
            effect
        );
    }

    /// Verify that the agent chain used in review comes from reducer state,
    /// not from local construction.
    #[test]
    fn test_review_phase_agent_selection_uses_reducer_state() {
        use crate::reducer::orchestration::determine_next_effect;

        let mut state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            ..create_test_state()
        };

        // Initialize the agent chain with specific agents
        state.agent_chain = AgentChainState::initial()
            .with_agents(
                vec![
                    "codex".to_string(),
                    "opencode".to_string(),
                    "claude".to_string(),
                ],
                vec![vec![], vec![], vec![]],
                AgentRole::Reviewer,
            )
            .reset_for_role(AgentRole::Reviewer);

        // Verify the current agent is codex (first in chain)
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"codex".to_string()),
            "Current agent should be 'codex' from reducer state"
        );

        // Advance to next agent (simulating auth failure)
        state.agent_chain = state.agent_chain.switch_to_next_agent();

        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"opencode".to_string()),
            "After advance, current agent should be 'opencode'"
        );

        // Orchestration should still work - it reads from state.agent_chain
        let effect = determine_next_effect(&state);

        // Should emit PrepareReviewContext, not InitializeAgentChain (chain is already populated)
        assert!(
            matches!(
                effect,
                crate::reducer::effect::Effect::PrepareReviewContext { .. }
            ),
            "Should emit PrepareReviewContext when chain is already initialized, got {:?}",
            effect
        );
    }

    // =========================================================================
    // XSD retry state transitions
    // =========================================================================

    #[test]
    fn test_development_output_validation_failed_sets_xsd_retry_pending() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 1,
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(1, 0),
        );

        assert!(
            new_state.continuation.xsd_retry_pending,
            "XSD retry should be pending after validation failure"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 1,
            "XSD retry count should be incremented"
        );
        assert_eq!(
            new_state.continuation.invalid_output_attempts, 1,
            "Invalid output attempts should be incremented"
        );
    }

    #[test]
    fn test_development_output_validation_failed_exhausts_xsd_retries() {
        use crate::reducer::state::ContinuationState;

        // Create state with custom max_xsd_retry_count = 2
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 1,
            continuation: ContinuationState {
                xsd_retry_count: 1,
                max_xsd_retry_count: 2,
                ..ContinuationState::new()
            },
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };

        let new_state = reduce(
            state.clone(),
            PipelineEvent::development_output_validation_failed(1, 0),
        );

        // XSD retries exhausted, should switch agent
        assert!(
            !new_state.continuation.xsd_retry_pending,
            "XSD retry should not be pending after exhaustion"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 0,
            "XSD retry count should be reset after agent switch"
        );
        assert_eq!(
            new_state.agent_chain.current_agent_index, 1,
            "Should have switched to next agent"
        );
    }

    #[test]
    fn test_planning_output_validation_failed_sets_xsd_retry_pending() {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            iteration: 0,
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::planning_output_validation_failed(0, 0),
        );

        assert!(
            new_state.continuation.xsd_retry_pending,
            "XSD retry should be pending after validation failure"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 1,
            "XSD retry count should be incremented"
        );
    }

    #[test]
    fn test_review_output_validation_failed_sets_xsd_retry_pending() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));

        assert!(
            new_state.continuation.xsd_retry_pending,
            "XSD retry should be pending after validation failure"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 1,
            "XSD retry count should be incremented"
        );
    }

    #[test]
    fn test_plan_generation_completed_clears_xsd_retry_state() {
        use crate::reducer::state::ContinuationState;

        let state = PipelineState {
            phase: PipelinePhase::Planning,
            continuation: ContinuationState {
                xsd_retry_count: 3,
                xsd_retry_pending: true,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

        assert!(
            !new_state.continuation.xsd_retry_pending,
            "XSD retry pending should be cleared on success"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 0,
            "XSD retry count should be reset on success"
        );
    }

    #[test]
    fn test_session_established_stores_session_id() {
        let state = create_test_state();

        let new_state = reduce(
            state,
            PipelineEvent::agent_session_established(
                AgentRole::Developer,
                "claude".to_string(),
                "ses_abc123".to_string(),
            ),
        );

        assert_eq!(
            new_state.agent_chain.last_session_id,
            Some("ses_abc123".to_string()),
            "Session ID should be stored"
        );
    }

    #[test]
    fn test_agent_switch_clears_session_id() {
        let state = PipelineState {
            agent_chain: AgentChainState::initial()
                .with_agents(
                    vec!["agent1".to_string(), "agent2".to_string()],
                    vec![vec![], vec![]],
                    AgentRole::Developer,
                )
                .with_session_id(Some("ses_abc123".to_string())),
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent1".to_string(),
                1,
                crate::reducer::event::AgentErrorKind::InternalError,
                false,
            ),
        );

        assert!(
            new_state.agent_chain.last_session_id.is_none(),
            "Session ID should be cleared when switching agents"
        );
    }

    // =========================================================================
    // Tests for commit XSD retry
    // =========================================================================

    #[test]
    fn test_commit_message_validation_failed_sets_xsd_retry_pending() {
        use crate::reducer::state::ContinuationState;

        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: super::CommitState::Generating {
                attempt: 1,
                max_attempts: 3,
            },
            continuation: ContinuationState::new(),
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
        );

        assert!(
            new_state.continuation.xsd_retry_pending,
            "XSD retry pending should be set"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 1,
            "XSD retry count should be incremented"
        );
    }

    #[test]
    fn test_commit_xsd_retry_exhausted_switches_agent() {
        use crate::reducer::state::ContinuationState;

        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: super::CommitState::Generating {
                attempt: 1,
                max_attempts: 3,
            },
            continuation: ContinuationState {
                xsd_retry_count: 9,
                max_xsd_retry_count: 10,
                ..ContinuationState::new()
            },
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Commit,
            ),
            ..PipelineState::initial(5, 2)
        };

        let new_state = reduce(
            state,
            PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
        );

        // Should have switched to next agent
        assert_eq!(
            new_state.agent_chain.current_agent_index, 1,
            "Should switch to next agent"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 0,
            "XSD retry count should be reset"
        );
        assert!(
            !new_state.continuation.xsd_retry_pending,
            "XSD retry pending should be cleared"
        );
    }

    #[test]
    fn test_planning_prompt_prepared_clears_xsd_retry_pending() {
        let state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_pending: true,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::planning_prompt_prepared(0));

        assert!(
            !new_state.continuation.xsd_retry_pending,
            "Prompt preparation should clear xsd retry pending"
        );
    }

    #[test]
    fn test_planning_agent_invoked_clears_xsd_retry_pending() {
        let state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_pending: true,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::planning_agent_invoked(0));

        assert!(
            !new_state.continuation.xsd_retry_pending,
            "Agent invocation should clear xsd retry pending"
        );
    }

    #[test]
    fn test_review_prompt_prepared_clears_xsd_retry_pending() {
        let state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_pending: true,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::review_prompt_prepared(0));

        assert!(
            !new_state.continuation.xsd_retry_pending,
            "Prompt preparation should clear xsd retry pending"
        );
    }

    #[test]
    fn test_review_agent_invoked_clears_xsd_retry_pending() {
        let state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_pending: true,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::review_agent_invoked(0));

        assert!(
            !new_state.continuation.xsd_retry_pending,
            "Agent invocation should clear xsd retry pending"
        );
    }

    #[test]
    fn test_commit_prompt_prepared_clears_xsd_retry_pending() {
        let state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_pending: true,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::commit_prompt_prepared(1));

        assert!(
            !new_state.continuation.xsd_retry_pending,
            "Prompt preparation should clear xsd retry pending"
        );
    }

    #[test]
    fn test_commit_agent_invoked_clears_xsd_retry_pending() {
        let state = PipelineState {
            continuation: ContinuationState {
                xsd_retry_pending: true,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::commit_agent_invoked(1));

        assert!(
            !new_state.continuation.xsd_retry_pending,
            "Agent invocation should clear xsd retry pending"
        );
    }

    #[test]
    fn test_review_pass_completed_clean_resets_commit_diff_flags() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            commit_diff_prepared: true,
            commit_diff_empty: true,
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

        assert!(!new_state.commit_diff_prepared);
        assert!(!new_state.commit_diff_empty);
    }

    // =========================================================================
    // Tests for fix continuation
    // =========================================================================

    #[test]
    fn test_fix_continuation_triggered_sets_pending() {
        use crate::reducer::state::{ContinuationState, FixStatus};

        let state = PipelineState {
            phase: PipelinePhase::Review,
            review_issues_found: true,
            reviewer_pass: 0,
            continuation: ContinuationState {
                invalid_output_attempts: 3, // Set non-zero to verify reset
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::fix_continuation_triggered(
                0,
                FixStatus::IssuesRemain,
                Some("Fixed 2 of 5 issues".to_string()),
            ),
        );

        assert!(
            new_state.continuation.fix_continue_pending,
            "Fix continue pending should be set"
        );
        assert_eq!(
            new_state.continuation.fix_continuation_attempt, 1,
            "Fix continuation attempt should be incremented"
        );
        assert_eq!(
            new_state.continuation.fix_status,
            Some(FixStatus::IssuesRemain),
            "Fix status should be stored"
        );
        assert_eq!(
            new_state.continuation.invalid_output_attempts, 0,
            "Invalid output attempts should be reset for new continuation"
        );
    }

    #[test]
    fn test_fix_continuation_succeeded_transitions_to_commit() {
        use crate::reducer::state::{ContinuationState, FixStatus};

        let state = PipelineState {
            phase: PipelinePhase::Review,
            review_issues_found: true,
            reviewer_pass: 0,
            continuation: ContinuationState {
                fix_continue_pending: true,
                fix_continuation_attempt: 2,
                fix_status: Some(FixStatus::IssuesRemain),
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::fix_continuation_succeeded(0, 2));

        assert_eq!(
            new_state.phase,
            PipelinePhase::CommitMessage,
            "Should transition to CommitMessage phase"
        );
        assert!(
            !new_state.continuation.fix_continue_pending,
            "Fix continue pending should be cleared"
        );
    }

    #[test]
    fn test_fix_continuation_budget_exhausted_transitions_to_commit() {
        use crate::reducer::state::{ContinuationState, FixStatus};

        let state = PipelineState {
            phase: PipelinePhase::Review,
            review_issues_found: true,
            reviewer_pass: 0,
            continuation: ContinuationState {
                fix_continue_pending: true,
                fix_continuation_attempt: 3,
                fix_status: Some(FixStatus::IssuesRemain),
                max_fix_continue_count: 3,
                ..ContinuationState::new()
            },
            ..create_test_state()
        };

        let new_state = reduce(
            state,
            PipelineEvent::fix_continuation_budget_exhausted(0, 3, FixStatus::IssuesRemain),
        );

        assert_eq!(
            new_state.phase,
            PipelinePhase::CommitMessage,
            "Should transition to CommitMessage even when budget exhausted"
        );
    }

    // =========================================================================
    // Tests for TEMPLATE_VARIABLES_INVALID
    // =========================================================================

    #[test]
    fn test_template_variables_invalid_switches_agent() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            agent_chain: AgentChainState::initial()
                .with_agents(
                    vec!["agent1".to_string(), "agent2".to_string()],
                    vec![vec![], vec![]],
                    AgentRole::Developer,
                )
                .with_session_id(Some("ses_abc123".to_string())),
            ..PipelineState::initial(5, 2)
        };

        let new_state = reduce(
            state,
            PipelineEvent::agent_template_variables_invalid(
                AgentRole::Developer,
                "dev_iteration".to_string(),
                vec!["PLAN".to_string()],
                vec!["{{XSD_ERROR}}".to_string()],
            ),
        );

        // Should switch to next agent
        assert_eq!(
            new_state.agent_chain.current_agent_index, 1,
            "Should switch to next agent on template failure"
        );
        // Session ID should be cleared
        assert!(
            new_state.agent_chain.last_session_id.is_none(),
            "Session ID should be cleared when switching agents"
        );
    }

    // =========================================================================
    // Tests for fix output validation failed
    // =========================================================================

    #[test]
    fn test_fix_output_validation_failed_sets_xsd_retry_pending() {
        use crate::reducer::state::ContinuationState;

        let state = PipelineState {
            phase: PipelinePhase::Review,
            review_issues_found: true,
            reviewer_pass: 0,
            continuation: ContinuationState::new(),
            ..create_test_state()
        };

        let new_state = reduce(state, PipelineEvent::fix_output_validation_failed(0, 0));

        assert!(
            new_state.continuation.xsd_retry_pending,
            "XSD retry pending should be set"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 1,
            "XSD retry count should be incremented"
        );
    }

    #[test]
    fn test_fix_output_validation_exhausted_switches_agent() {
        use crate::reducer::state::ContinuationState;

        let state = PipelineState {
            phase: PipelinePhase::Review,
            review_issues_found: true,
            reviewer_pass: 0,
            continuation: ContinuationState {
                xsd_retry_count: 9,
                max_xsd_retry_count: 10,
                ..ContinuationState::new()
            },
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Reviewer,
            ),
            ..PipelineState::initial(5, 2)
        };

        let new_state = reduce(state, PipelineEvent::fix_output_validation_failed(0, 2));

        // Should have switched to next agent
        assert_eq!(
            new_state.agent_chain.current_agent_index, 1,
            "Should switch to next agent when XSD retries exhausted"
        );
        assert_eq!(
            new_state.continuation.xsd_retry_count, 0,
            "XSD retry count should be reset"
        );
    }
}
