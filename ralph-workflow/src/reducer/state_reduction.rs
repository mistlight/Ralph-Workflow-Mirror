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
use super::state::{CommitState, ContinuationState, PipelineState, RebaseState};

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
        PipelineEvent::CheckpointSaved { .. } => state,
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
            ..state
        },
        PlanningEvent::PhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Development,
            ..state
        },
        PlanningEvent::GenerationStarted { .. } => state,
        PlanningEvent::GenerationCompleted { valid, .. } => {
            if valid {
                PipelineState {
                    phase: super::event::PipelinePhase::Development,
                    ..state
                }
            } else {
                // Do not proceed to Development without a valid plan.
                PipelineState {
                    phase: super::event::PipelinePhase::Planning,
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
            ..state
        },
        DevelopmentEvent::IterationStarted { iteration } => PipelineState {
            iteration,
            agent_chain: state.agent_chain.reset(),
            // Reset continuation state when starting a new iteration
            continuation: state.continuation.reset(),
            ..state
        },
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
                    context_cleaned: false,
                    // Reset continuation state on successful completion
                    continuation: ContinuationState::new(),
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
                        ..state
                    }
                }
            }
        }
        DevelopmentEvent::PhaseCompleted => PipelineState {
            phase: super::event::PipelinePhase::Review,
            // Reset continuation state when phase completes
            continuation: ContinuationState::new(),
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
                context_cleaned: false,
                continuation: ContinuationState::new(),
                ..state
            }
        }
        DevelopmentEvent::OutputValidationFailed { iteration, attempt } => {
            // Policy: After MAX_DEV_INVALID_OUTPUT_RERUNS, switch to next agent.
            // This keeps invalid output retry logic in the reducer, not the handler.
            if attempt >= super::state::MAX_DEV_INVALID_OUTPUT_RERUNS {
                let new_agent_chain = state.agent_chain.switch_to_next_agent();
                PipelineState {
                    phase: super::event::PipelinePhase::Development,
                    iteration,
                    agent_chain: new_agent_chain,
                    continuation: ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                // Stay in Development, increment attempt counter
                PipelineState {
                    phase: super::event::PipelinePhase::Development,
                    iteration,
                    continuation: ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        ..state.continuation
                    },
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
                continuation: ContinuationState::new(),
                ..state
            }
        }
        DevelopmentEvent::ContinuationContextWritten {
            iteration,
            attempt: _,
        } => {
            // Context file was written, state remains unchanged.
            // The continuation state is already set by ContinuationTriggered.
            PipelineState { iteration, ..state }
        }
        DevelopmentEvent::ContinuationContextCleaned => {
            // Context file was cleaned up, no state change needed.
            state
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
            continuation: super::state::ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            ..state
        },
        ReviewEvent::PassStarted { pass } => PipelineState {
            reviewer_pass: pass,
            review_issues_found: false,
            agent_chain: state.agent_chain.reset(),
            continuation: if pass == state.reviewer_pass {
                // If orchestration re-emits PassStarted for the same pass (e.g., retry after
                // OutputValidationFailed), do not reset the invalid-output attempt counter.
                // The reducer owns retry accounting for determinism.
                state.continuation
            } else {
                super::state::ContinuationState {
                    invalid_output_attempts: 0,
                    ..state.continuation
                }
            },
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
                    commit: super::state::CommitState::NotStarted,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                PipelineState {
                    phase: next_phase,
                    reviewer_pass: next_pass,
                    review_issues_found: issues_found,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    },
                    ..state
                }
            }
        }
        ReviewEvent::FixAttemptStarted { .. } => PipelineState {
            agent_chain: state.agent_chain.reset(),
            continuation: super::state::ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            ..state
        },
        ReviewEvent::FixAttemptCompleted { pass, .. } => PipelineState {
            phase: super::event::PipelinePhase::CommitMessage,
            previous_phase: Some(super::event::PipelinePhase::Review),
            reviewer_pass: pass,
            review_issues_found: false,
            commit: super::state::CommitState::NotStarted,
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
            continuation: super::state::ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
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
                    reviewer_pass: next_pass,
                    review_issues_found: false,
                    commit: super::state::CommitState::NotStarted,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                PipelineState {
                    phase: next_phase,
                    reviewer_pass: next_pass,
                    review_issues_found: false,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    },
                    ..state
                }
            }
        }
        ReviewEvent::OutputValidationFailed { pass, attempt } => {
            // Policy: The reducer maintains retry state for determinism.
            // Handlers should emit `attempt` from state (checkpoint-resume safe).
            const MAX_REVIEW_INVALID_OUTPUT_RERUNS: u32 = 2;

            if attempt >= MAX_REVIEW_INVALID_OUTPUT_RERUNS {
                let new_agent_chain = state.agent_chain.switch_to_next_agent();
                PipelineState {
                    phase: super::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    agent_chain: new_agent_chain,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                PipelineState {
                    phase: super::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    continuation: super::state::ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        ..state.continuation
                    },
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
        // Other retriable errors (Network, Timeout): try next model
        AgentEvent::InvocationFailed {
            retriable: true, ..
        } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        // Non-retriable errors: switch agent
        AgentEvent::InvocationFailed {
            retriable: false, ..
        } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent(),
            ..state
        },
        AgentEvent::FallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent(),
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
        AgentEvent::RetryCycleStarted { .. } => state,
        AgentEvent::ChainInitialized { role, agents } => {
            let models_per_agent = agents.iter().map(|_| vec![]).collect();
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .with_agents(agents, models_per_agent, role)
                    .reset_for_role(role),
                ..state
            }
        }
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
            ..state
        },
        CommitEvent::MessageGenerated { message, .. } => PipelineState {
            commit: CommitState::Generated { message },
            ..state
        },
        CommitEvent::Created { hash, .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            PipelineState {
                commit: CommitState::Committed { hash },
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                context_cleaned: false,
                ..state
            }
        }
        CommitEvent::GenerationFailed { .. } => PipelineState {
            commit: CommitState::NotStarted,
            ..state
        },
        CommitEvent::Skipped { .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            PipelineState {
                commit: CommitState::Skipped,
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                context_cleaned: false,
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
                (
                    super::event::PipelinePhase::Review,
                    next_iter,
                    state.reviewer_pass,
                )
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

/// Handle commit message validation failure with retry logic.
fn reduce_commit_validation_failed(state: PipelineState, attempt: u32) -> PipelineState {
    let next_attempt = attempt + 1;
    let max_attempts = super::state::MAX_VALIDATION_RETRY_ATTEMPTS;

    if next_attempt <= max_attempts {
        PipelineState {
            commit: CommitState::Generating {
                attempt: next_attempt,
                max_attempts,
            },
            ..state
        }
    } else {
        // Exceeded max attempts with current agent - try next agent
        let old_agent_index = state.agent_chain.current_agent_index;
        let old_retry_cycle = state.agent_chain.retry_cycle;
        let new_agent_chain = state.agent_chain.switch_to_next_agent();

        let wrapped_around = new_agent_chain.retry_cycle > old_retry_cycle;
        let advanced_to_next =
            new_agent_chain.current_agent_index != old_agent_index && !wrapped_around;

        if advanced_to_next {
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::Generating {
                    attempt: 1,
                    max_attempts,
                },
                ..state
            }
        } else {
            // All agents exhausted - reset so orchestration can handle
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::NotStarted,
                ..state
            }
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
        let state = create_test_state();
        let new_state = reduce(state, PipelineEvent::commit_generation_started());

        assert!(matches!(new_state.commit, CommitState::Generating { .. }));
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
        use crate::reducer::state::MAX_DEV_INVALID_OUTPUT_RERUNS;

        let state = create_test_state();
        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, MAX_DEV_INVALID_OUTPUT_RERUNS),
        );
        assert_eq!(new_state.continuation.invalid_output_attempts, 0);
        assert!(
            new_state.agent_chain.current_agent_index > 0,
            "Should switch to next agent after max invalid output attempts"
        );
    }

    #[test]
    fn test_output_validation_failed_resets_counter_on_agent_switch() {
        use crate::reducer::state::MAX_DEV_INVALID_OUTPUT_RERUNS;

        let mut state = create_test_state();
        state.continuation.invalid_output_attempts = MAX_DEV_INVALID_OUTPUT_RERUNS;

        let new_state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, MAX_DEV_INVALID_OUTPUT_RERUNS),
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
        let mut state = create_test_state();
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 2;
        // Simulate reaching the retry limit before this failure.
        state.continuation.invalid_output_attempts = 2;

        let new_state = reduce(
            state,
            // `attempt` should be sourced from state for determinism.
            PipelineEvent::review_output_validation_failed(0, 2),
        );

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
        use crate::reducer::state::MAX_DEV_INVALID_OUTPUT_RERUNS;

        let mut state = create_test_state();
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

        // Still within limit (MAX_DEV_INVALID_OUTPUT_RERUNS is 2)
        if 2 < MAX_DEV_INVALID_OUTPUT_RERUNS {
            assert_eq!(
                state.agent_chain.current_agent_index, 0,
                "Should not switch agents yet"
            );
        }

        // Now succeed
        state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        assert_eq!(state.phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_event_sequence_validation_failures_trigger_agent_switch() {
        use crate::reducer::state::MAX_DEV_INVALID_OUTPUT_RERUNS;

        let mut state = create_test_state();
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
            PipelineEvent::development_output_validation_failed(0, MAX_DEV_INVALID_OUTPUT_RERUNS),
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
}
