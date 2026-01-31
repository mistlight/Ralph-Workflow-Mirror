//! Orchestration logic for determining next effect.
//!
//! Contains `determine_next_effect()` which decides which effect to execute
//! based on current pipeline state.

use super::event::{CheckpointTrigger, PipelinePhase, RebasePhase};
use super::state::{CommitState, PipelineState, RebaseState};

use crate::agents::AgentRole;
use crate::reducer::effect::{ContinuationContextData, Effect};

/// Derive the effect for XSD retry based on current phase.
///
/// XSD retry reuses the same agent and session if available.
/// Returns the appropriate phase-specific effect with retry context.
fn derive_xsd_retry_effect(state: &PipelineState) -> Effect {
    match state.phase {
        PipelinePhase::Planning => Effect::GeneratePlan {
            iteration: state.iteration,
        },
        PipelinePhase::Development => Effect::RunDevelopmentIteration {
            iteration: state.iteration,
        },
        PipelinePhase::Review => {
            if state.review_issues_found {
                Effect::RunFixAttempt {
                    pass: state.reviewer_pass,
                }
            } else {
                Effect::RunReviewPass {
                    pass: state.reviewer_pass,
                }
            }
        }
        PipelinePhase::CommitMessage => Effect::GenerateCommitMessage,
        // Other phases don't have XSD retry
        _ => Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::PhaseTransition,
        },
    }
}

/// Derive the effect for continuation based on current phase.
///
/// Continuation starts a new session (agent starts fresh but with context).
/// Only applies to Development and Fix phases where incomplete work can continue.
fn derive_continuation_effect(state: &PipelineState) -> Effect {
    match state.phase {
        PipelinePhase::Development => {
            // Write continuation context first if needed
            if state.continuation.context_write_pending {
                let status = state
                    .continuation
                    .previous_status
                    .clone()
                    .unwrap_or(super::state::DevelopmentStatus::Failed);
                let summary = state
                    .continuation
                    .previous_summary
                    .clone()
                    .unwrap_or_default();
                let files_changed = state.continuation.previous_files_changed.clone();
                let next_steps = state.continuation.previous_next_steps.clone();

                Effect::WriteContinuationContext(ContinuationContextData {
                    iteration: state.iteration,
                    attempt: state.continuation.continuation_attempt,
                    status,
                    summary,
                    files_changed,
                    next_steps,
                })
            } else {
                Effect::RunDevelopmentIteration {
                    iteration: state.iteration,
                }
            }
        }
        // Fix continuation: run fix attempt with continuation context
        PipelinePhase::Review if state.continuation.fix_continue_pending => Effect::RunFixAttempt {
            pass: state.reviewer_pass,
        },
        PipelinePhase::Review if state.review_issues_found => Effect::RunFixAttempt {
            pass: state.reviewer_pass,
        },
        // Other phases don't support continuation
        _ => Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::PhaseTransition,
        },
    }
}

/// Determine the next effect to execute based on current state.
///
/// This function is pure - it only reads state and returns an effect.
/// The actual execution happens in the effect handler.
///
/// # Priority Order for Effects
///
/// 1. Continuation context cleanup (highest priority)
/// 2. XSD retry pending (validation failed, retry with same agent/session)
/// 3. Continue pending (output valid but incomplete, new session)
/// 4. Rebase in progress
/// 5. Agent chain exhausted
/// 6. Backoff wait
/// 7. Phase-specific effects
pub fn determine_next_effect(state: &PipelineState) -> Effect {
    if state.continuation.context_cleanup_pending {
        return Effect::CleanupContinuationContext;
    }

    // XSD retry: validation failed, retry with same agent/session if not exhausted.
    // Note: The reducer should clear xsd_retry_pending when retries are exhausted, so
    // normally we wouldn't see xsd_retry_pending=true AND xsd_retries_exhausted()=true.
    // However, to be robust against edge cases, we still derive the retry effect even
    // when exhausted - the handler's retry attempt will cause the reducer to process
    // another validation event, which will detect exhaustion and switch agents.
    if state.continuation.xsd_retry_pending && !state.continuation.xsd_retries_exhausted() {
        return derive_xsd_retry_effect(state);
    }

    // Development continuation pending: output valid but work incomplete, start new session
    // Only check continue_pending in Development phase to avoid confusion with fix_continue_pending
    if state.phase == PipelinePhase::Development && state.continuation.continue_pending {
        if state.continuation.continuations_exhausted() {
            // Exhausted continuation budget - accept current state as complete
            // The budget exhaustion is handled by state reduction, so we proceed
            // to normal phase-specific effects
        } else {
            // Trigger continuation with new session
            return derive_continuation_effect(state);
        }
    }

    // Fix continuation pending: fix output valid but issues remain, start new session
    // Only check fix_continue_pending in Review phase to be explicit about phase context
    if state.phase == PipelinePhase::Review && state.continuation.fix_continue_pending {
        if state.continuation.fix_continuations_exhausted() {
            // Exhausted fix continuation budget - proceed to commit
            // The budget exhaustion is handled by state reduction
        } else {
            // Trigger fix continuation with new session
            return derive_continuation_effect(state);
        }
    }

    if matches!(
        state.rebase,
        RebaseState::InProgress { .. } | RebaseState::Conflicted { .. }
    ) {
        let phase = match state.phase {
            PipelinePhase::Planning => RebasePhase::Initial,
            _ => RebasePhase::PostReview,
        };

        return match &state.rebase {
            RebaseState::InProgress { target_branch, .. } => Effect::RunRebase {
                phase,
                target_branch: target_branch.clone(),
            },
            RebaseState::Conflicted { .. } => Effect::ResolveRebaseConflicts {
                strategy: super::event::ConflictStrategy::Continue,
            },
            _ => unreachable!("checked rebase state before matching"),
        };
    }

    if !state.agent_chain.agents.is_empty() && state.agent_chain.is_exhausted() {
        let progressed = match state.phase {
            PipelinePhase::Planning => state.iteration > 0,
            PipelinePhase::Development => state.iteration > 0,
            PipelinePhase::Review => state.reviewer_pass > 0,
            PipelinePhase::CommitMessage => matches!(
                state.commit,
                CommitState::Generated { .. }
                    | CommitState::Committed { .. }
                    | CommitState::Skipped
            ),
            PipelinePhase::FinalValidation
            | PipelinePhase::Finalizing
            | PipelinePhase::Complete
            | PipelinePhase::Interrupted => false,
        };

        if progressed
            && state.checkpoint_saved_count == 0
            && !matches!(
                state.phase,
                PipelinePhase::Complete | PipelinePhase::Interrupted
            )
        {
            return Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::Interrupt,
            };
        }

        return Effect::AbortPipeline {
            reason: format!(
                "Agent chain exhausted for role {:?} in phase {:?} (cycle {})",
                state.agent_chain.current_role, state.phase, state.agent_chain.retry_cycle
            ),
        };
    }

    if let Some(duration_ms) = state.agent_chain.backoff_pending_ms {
        return Effect::BackoffWait {
            role: state.agent_chain.current_role,
            cycle: state.agent_chain.retry_cycle,
            duration_ms,
        };
    }

    match state.phase {
        PipelinePhase::Planning => {
            if state.iteration == 0
                && state.checkpoint_saved_count == 0
                && matches!(
                    state.rebase,
                    RebaseState::Skipped | RebaseState::Completed { .. }
                )
            {
                return Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                };
            }

            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Developer,
                };
            }

            // Clean up BEFORE planning to remove old PLAN.md from previous iteration
            if !state.context_cleaned {
                return Effect::CleanupContext;
            }

            Effect::GeneratePlan {
                iteration: state.iteration,
            }
        }

        PipelinePhase::Development => {
            if state.continuation.context_write_pending {
                let status = state
                    .continuation
                    .previous_status
                    .clone()
                    .unwrap_or(super::state::DevelopmentStatus::Failed);
                let summary = state
                    .continuation
                    .previous_summary
                    .clone()
                    .unwrap_or_default();
                let files_changed = state.continuation.previous_files_changed.clone();
                let next_steps = state.continuation.previous_next_steps.clone();

                return Effect::WriteContinuationContext(ContinuationContextData {
                    iteration: state.iteration,
                    attempt: state.continuation.continuation_attempt,
                    status,
                    summary,
                    files_changed,
                    next_steps,
                });
            }

            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Developer,
                };
            }

            if state.iteration < state.total_iterations {
                Effect::RunDevelopmentIteration {
                    iteration: state.iteration,
                }
            } else {
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                }
            }
        }

        PipelinePhase::Review => {
            // If review found issues, run fix attempt
            if state.review_issues_found {
                if state.agent_chain.agents.is_empty()
                    || state.agent_chain.current_role != AgentRole::Reviewer
                {
                    return Effect::InitializeAgentChain {
                        role: AgentRole::Reviewer,
                    };
                }

                return Effect::RunFixAttempt {
                    pass: state.reviewer_pass,
                };
            }

            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Reviewer,
                };
            }

            // Otherwise, run next review pass or complete phase
            if state.reviewer_pass < state.total_reviewer_passes {
                Effect::RunReviewPass {
                    pass: state.reviewer_pass,
                }
            } else {
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                }
            }
        }

        PipelinePhase::CommitMessage => {
            // Commit phase requires explicit agent chain initialization like other phases
            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Commit,
                };
            }
            match state.commit {
                CommitState::NotStarted => Effect::GenerateCommitMessage,
                CommitState::Generated { ref message } => Effect::CreateCommit {
                    message: message.clone(),
                },
                CommitState::Committed { .. } | CommitState::Skipped => Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                },
                CommitState::Generating { .. } => Effect::GenerateCommitMessage,
            }
        }

        PipelinePhase::FinalValidation => Effect::ValidateFinalState,

        PipelinePhase::Finalizing => Effect::RestorePromptPermissions,

        PipelinePhase::Complete | PipelinePhase::Interrupted => Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::state::AgentChainState;
    use crate::reducer::{reduce, PipelineEvent};

    fn create_test_state() -> PipelineState {
        PipelineState::initial(5, 2)
    }

    #[test]
    fn test_determine_effect_planning_phase() {
        let state = create_test_state();
        let effect = determine_next_effect(&state);
        assert!(matches!(
            effect,
            Effect::InitializeAgentChain {
                role: AgentRole::Developer
            }
        ));
    }

    #[test]
    fn test_determine_effect_planning_with_agents() {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            context_cleaned: true, // Context must be cleaned before planning
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::GeneratePlan { .. }));
    }

    #[test]
    fn test_planning_phase_transitions_to_development_after_completion() {
        // Create state in Planning phase with agents initialized
        let mut state = PipelineState {
            phase: PipelinePhase::Planning,
            iteration: 1,
            total_iterations: 5,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };

        // Simulate plan generation completing
        state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

        // After plan generation completes, phase should transition to Development
        assert_eq!(
            state.phase,
            PipelinePhase::Development,
            "Phase should transition to Development after PlanGenerationCompleted"
        );

        // Orchestration should now return RunDevelopmentIteration, not GeneratePlan
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunDevelopmentIteration { .. }),
            "Expected RunDevelopmentIteration, got {:?}",
            effect
        );
    }

    #[test]
    fn test_initial_state_skips_planning_when_zero_developer_iters() {
        // When developer_iters=0, the initial state should skip Planning phase entirely
        let state = PipelineState::initial(0, 2);
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Initial phase should be Review when developer_iters=0 and reviewer_reviews>0"
        );
    }

    #[test]
    fn test_initial_state_skips_to_commit_when_zero_iters_and_reviews() {
        // When both developer_iters=0 and reviewer_reviews=0, skip to CommitMessage
        let state = PipelineState::initial(0, 0);
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Initial phase should be CommitMessage when developer_iters=0 and reviewer_reviews=0"
        );
    }

    #[test]
    fn test_initial_state_starts_planning_when_developer_iters_nonzero() {
        // When developer_iters>0, start in Planning phase as normal
        let state = PipelineState::initial(1, 0);
        assert_eq!(
            state.phase,
            PipelinePhase::Planning,
            "Initial phase should be Planning when developer_iters>0"
        );
    }

    #[test]
    fn test_determine_effect_development_phase_empty_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            agent_chain: AgentChainState::initial(),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(
            effect,
            Effect::InitializeAgentChain {
                role: AgentRole::Developer
            }
        ));
    }

    #[test]
    fn test_determine_effect_development_phase_exhausted_chain() {
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            agent_chain: chain,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_exhausted_chain_after_checkpoint_aborts() {
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            checkpoint_saved_count: 1,
            agent_chain: chain,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::AbortPipeline { .. }));
    }

    #[test]
    fn test_determine_effect_development_phase_with_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::RunDevelopmentIteration { .. }));
    }

    #[test]
    fn test_determine_effect_development_complete() {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 6,
            total_iterations: 5,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_development_runs_exactly_n_iterations() {
        // When total_iterations=5, should run iterations 0,1,2,3,4 (5 total)
        let mut state = PipelineState::initial(5, 0);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Track which iterations actually run
        let mut iterations_run = Vec::new();

        // Simulate the development phase
        while state.phase == PipelinePhase::Planning
            || state.phase == PipelinePhase::Development
            || state.phase == PipelinePhase::CommitMessage
        {
            let effect = determine_next_effect(&state);

            match effect {
                Effect::CleanupContext => {
                    // Context cleanup before planning
                    state = reduce(state, PipelineEvent::ContextCleaned);
                }
                Effect::CleanupContinuationContext => {
                    state = reduce(
                        state,
                        PipelineEvent::development_continuation_context_cleaned(),
                    );
                }
                Effect::GeneratePlan { iteration } => {
                    // Complete planning
                    state = reduce(
                        state,
                        PipelineEvent::plan_generation_completed(iteration, true),
                    );
                }
                Effect::RunDevelopmentIteration { iteration } => {
                    iterations_run.push(iteration);
                    // Complete the iteration (goes to CommitMessage phase)
                    state = reduce(
                        state,
                        PipelineEvent::development_iteration_completed(iteration, true),
                    );
                }
                Effect::GenerateCommitMessage => {
                    // Generate and commit
                    state = reduce(state, PipelineEvent::commit_generation_started());
                    state = reduce(
                        state,
                        PipelineEvent::commit_created(
                            format!("abc{}", iterations_run.len()),
                            "test".to_string(),
                        ),
                    );
                }
                Effect::SaveCheckpoint { .. } => {
                    // Phase complete
                    break;
                }
                Effect::InitializeAgentChain { .. } => {
                    state = reduce(
                        state,
                        PipelineEvent::agent_chain_initialized(
                            AgentRole::Developer,
                            vec!["claude".to_string()],
                            3,
                            1000,
                            2.0,
                            60000,
                        ),
                    );
                }
                _ => panic!("Unexpected effect: {:?}", effect),
            }
        }

        // Should run exactly 5 iterations (0,1,2,3,4), not 6 (0,1,2,3,4,5)
        assert_eq!(
            iterations_run.len(),
            5,
            "Should run exactly 5 iterations, ran: {:?}",
            iterations_run
        );
        assert_eq!(
            iterations_run,
            vec![0, 1, 2, 3, 4],
            "Should run iterations 0-4"
        );
        // With total_reviewer_passes=0, we go to FinalValidation, not Review
        assert_eq!(
            state.phase,
            PipelinePhase::FinalValidation,
            "Should transition to FinalValidation after 5 iterations when reviewer_passes=0"
        );
    }

    #[test]
    fn test_determine_effect_review_phase_empty_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            agent_chain: AgentChainState::initial(),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(
            effect,
            Effect::InitializeAgentChain {
                role: AgentRole::Reviewer
            }
        ));
    }

    #[test]
    fn test_determine_effect_review_phase_exhausted_chain() {
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            )
            .with_max_cycles(3);
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            agent_chain: chain,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_determine_effect_review_exhausted_chain_after_checkpoint_aborts() {
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            )
            .with_max_cycles(3);
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            checkpoint_saved_count: 1,
            agent_chain: chain,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::AbortPipeline { .. }));
    }

    #[test]
    fn test_determine_effect_review_phase_with_chain() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::RunReviewPass { .. }));
    }

    #[test]
    fn test_determine_effect_review_complete() {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 2,
            total_reviewer_passes: 2,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }

    #[test]
    fn test_review_triggers_fix_when_issues_found() {
        // Create state in Review phase with issues found
        let mut state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: false,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };

        // Initially should run review pass
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunReviewPass { pass: 0 }),
            "Expected RunReviewPass, got {:?}",
            effect
        );

        // Simulate review completing with issues found
        state = reduce(state, PipelineEvent::review_completed(0, true));

        // State should now have issues_found flag set
        assert!(
            state.review_issues_found,
            "review_issues_found should be true"
        );

        // With a populated Reviewer chain, orchestration should run the fix attempt directly.
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunFixAttempt { pass: 0 }),
            "Expected RunFixAttempt after issues found, got {:?}",
            effect
        );

        // After fix completes, goes to CommitMessage phase
        state = reduce(state, PipelineEvent::fix_attempt_completed(0, true));

        assert!(
            !state.review_issues_found,
            "review_issues_found should be reset after fix"
        );
        // After fix, goes to CommitMessage phase (pass increment happens after commit)
        assert_eq!(
            state.reviewer_pass, 0,
            "Pass stays at 0 until CommitCreated"
        );
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Should go to CommitMessage phase after fix"
        );

        // After commit is created, pass is incremented
        state = reduce(state, PipelineEvent::commit_generation_started());
        state = reduce(
            state,
            PipelineEvent::commit_created("abc123".to_string(), "fix commit".to_string()),
        );

        assert_eq!(
            state.reviewer_pass, 1,
            "Should increment to next pass after commit"
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Should return to Review phase after commit"
        );
    }

    #[test]
    fn test_complete_pipeline_flow_with_planning_dev_review_commit() {
        // Test the COMPLETE flow: Planning → Development → Review → Fix → Commit → FinalValidation
        let mut state = PipelineState::initial(2, 1); // 2 dev iterations, 1 review pass
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let mut phase_sequence = Vec::new();
        let mut iterations_run = Vec::new();
        let mut review_passes_run = Vec::new();

        // Simulate complete pipeline execution
        let max_steps = 100; // Safety limit to prevent infinite loops (increased for commit flow)
        for step in 0..max_steps {
            phase_sequence.push(state.phase);
            let effect = determine_next_effect(&state);

            match effect {
                Effect::CleanupContext => {
                    state = reduce(state, PipelineEvent::ContextCleaned);
                }
                Effect::CleanupContinuationContext => {
                    state = reduce(
                        state,
                        PipelineEvent::development_continuation_context_cleaned(),
                    );
                }
                Effect::InitializeAgentChain { role } => {
                    state = reduce(
                        state,
                        PipelineEvent::agent_chain_initialized(
                            role,
                            vec!["claude".to_string()],
                            3,
                            1000,
                            2.0,
                            60000,
                        ),
                    );
                }
                Effect::GeneratePlan { iteration } => {
                    state = reduce(
                        state,
                        PipelineEvent::plan_generation_completed(iteration, true),
                    );
                }
                Effect::RunDevelopmentIteration { iteration } => {
                    iterations_run.push(iteration);
                    state = reduce(
                        state,
                        PipelineEvent::development_iteration_completed(iteration, true),
                    );
                }
                Effect::RunReviewPass { pass } => {
                    review_passes_run.push(pass);
                    state = reduce(
                        state,
                        PipelineEvent::review_completed(pass, true), // Simulate finding issues
                    );
                }
                Effect::RunFixAttempt { pass } => {
                    state = reduce(state, PipelineEvent::fix_attempt_completed(pass, true));
                }
                Effect::GenerateCommitMessage => {
                    state = reduce(state, PipelineEvent::commit_generation_started());
                    state = reduce(
                        state,
                        PipelineEvent::commit_message_generated("test commit".to_string(), 1),
                    );
                }
                Effect::CreateCommit { .. } => {
                    state = reduce(
                        state,
                        PipelineEvent::commit_created(
                            "abc123".to_string(),
                            "test commit".to_string(),
                        ),
                    );
                }
                Effect::ValidateFinalState => {
                    state = reduce(state, PipelineEvent::pipeline_completed());
                }
                Effect::SaveCheckpoint { .. } => {
                    // Phase transition checkpoint - continue
                    if state.phase == PipelinePhase::Complete {
                        break;
                    }
                }
                _ => panic!("Unexpected effect at step {}: {:?}", step, effect),
            }

            if state.phase == PipelinePhase::Complete {
                break;
            }
        }

        // Verify the complete flow
        assert_eq!(
            iterations_run,
            vec![0, 1],
            "Should run exactly 2 development iterations"
        );
        assert_eq!(
            review_passes_run,
            vec![0],
            "Should run exactly 1 review pass"
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Complete,
            "Pipeline should complete"
        );

        // Verify phase progression
        assert!(
            phase_sequence.contains(&PipelinePhase::Planning),
            "Should go through Planning"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::Development),
            "Should go through Development"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::Review),
            "Should go through Review"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::CommitMessage),
            "Should go through CommitMessage"
        );
        assert!(
            phase_sequence.contains(&PipelinePhase::FinalValidation),
            "Should go through FinalValidation"
        );
    }

    #[test]
    fn test_pipeline_flow_skip_planning_when_zero_iterations() {
        // When developer_iters=0, should skip Planning and Development entirely
        let mut state = PipelineState::initial(0, 2); // 0 dev iterations, 2 review passes
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Should start in Review when developer_iters=0"
        );

        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        let mut review_passes = Vec::new();
        let max_steps = 30;

        for _ in 0..max_steps {
            let effect = determine_next_effect(&state);

            match effect {
                Effect::InitializeAgentChain { role } => {
                    state = reduce(
                        state,
                        PipelineEvent::agent_chain_initialized(
                            role,
                            vec!["claude".to_string()],
                            3,
                            1000,
                            2.0,
                            60000,
                        ),
                    );
                }
                Effect::RunReviewPass { pass } => {
                    review_passes.push(pass);
                    state = reduce(
                        state,
                        PipelineEvent::review_completed(pass, false), // No issues
                    );
                }
                Effect::GenerateCommitMessage => {
                    state = reduce(
                        state,
                        PipelineEvent::commit_message_generated("test".to_string(), 1),
                    );
                }
                Effect::CreateCommit { .. } => {
                    state = reduce(
                        state,
                        PipelineEvent::commit_created("abc".to_string(), "test".to_string()),
                    );
                }
                Effect::ValidateFinalState => {
                    state = reduce(state, PipelineEvent::pipeline_completed());
                    break;
                }
                Effect::SaveCheckpoint { .. } => {
                    if state.phase == PipelinePhase::Complete {
                        break;
                    }
                }
                _ => panic!("Unexpected effect: {:?}", effect),
            }
        }

        assert_eq!(review_passes, vec![0, 1], "Should run 2 review passes");
        assert_eq!(state.phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_review_runs_exactly_n_passes() {
        // Similar to development iteration test, verify review passes count
        let mut state = PipelineState::initial(0, 3); // 0 dev, 3 review passes
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        let mut passes_run = Vec::new();
        let max_steps = 30;

        for _ in 0..max_steps {
            let effect = determine_next_effect(&state);

            match effect {
                Effect::InitializeAgentChain { role } => {
                    state = reduce(
                        state,
                        PipelineEvent::agent_chain_initialized(
                            role,
                            vec!["claude".to_string()],
                            3,
                            1000,
                            2.0,
                            60000,
                        ),
                    );
                }
                Effect::RunReviewPass { pass } => {
                    passes_run.push(pass);
                    state = reduce(
                        state,
                        PipelineEvent::review_completed(pass, false), // No issues, no fix needed
                    );
                }
                Effect::SaveCheckpoint { .. } => {
                    // Review complete
                    break;
                }
                _ => break,
            }
        }

        assert_eq!(
            passes_run.len(),
            3,
            "Should run exactly 3 review passes, ran: {:?}",
            passes_run
        );
        assert_eq!(passes_run, vec![0, 1, 2], "Should run passes 0-2");
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Should transition to CommitMessage after reviews"
        );
    }

    #[test]
    fn test_review_skips_fix_when_no_issues() {
        // Create state in Review phase
        let mut state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: false,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..create_test_state()
        };

        // Run review pass
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::RunReviewPass { pass: 0 }));

        // Review completes with NO issues
        state = reduce(state, PipelineEvent::review_completed(0, false));

        assert!(
            !state.review_issues_found,
            "review_issues_found should be false"
        );

        assert_eq!(
            state.reviewer_pass, 1,
            "Should increment to next pass when no issues"
        );

        // Should run next review pass (pass 1), NOT fix attempt
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunReviewPass { pass: 1 }),
            "Expected RunReviewPass pass 1 when no issues, got {:?}",
            effect
        );
    }

    #[test]
    fn test_determine_effect_commit_message_empty_chain() {
        // When agent chain is empty, commit phase should request initialization
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::NotStarted,
            agent_chain: AgentChainState::initial(),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(
            effect,
            Effect::InitializeAgentChain {
                role: AgentRole::Commit
            }
        ));
    }

    #[test]
    fn test_determine_effect_commit_message_not_started() {
        // With initialized agent chain, commit phase should generate message
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::NotStarted,
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["commit-agent".to_string()],
                vec![vec![]],
                AgentRole::Commit,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::GenerateCommitMessage));
    }

    #[test]
    fn test_determine_effect_commit_message_generated() {
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Generated {
                message: "test commit message".to_string(),
            },
            agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
                vec!["commit-agent".to_string()],
                vec![vec![]],
                AgentRole::Commit,
            ),
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        match effect {
            Effect::CreateCommit { message } => {
                assert_eq!(message, "test commit message");
            }
            _ => panic!("Expected CreateCommit effect, got {:?}", effect),
        }
    }

    #[test]
    fn test_determine_effect_final_validation() {
        let state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::ValidateFinalState));
    }

    #[test]
    fn test_determine_effect_complete() {
        let state = PipelineState {
            phase: PipelinePhase::Complete,
            ..create_test_state()
        };
        let effect = determine_next_effect(&state);
        assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
    }
}
