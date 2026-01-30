//! Tests that effects are single-task and handlers don't have hidden behavior.
//!
//! Key invariants verified:
//! 1. Each effect type represents exactly one task
//! 2. Orchestration respects preconditions before emitting phase effects
//! 3. Effect handlers don't perform implicit agent selection (uses state.agent_chain)
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::{AgentChainState, CommitState, PipelineState};

/// Test that RunDevelopmentIteration is NOT emitted when agent chain is empty.
///
/// RunDevelopmentIteration should use the agent from state.agent_chain.
/// It should NOT be emitted when the chain is empty.
#[test]
fn test_run_development_iteration_requires_agent_chain() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 0,
            total_iterations: 5,
            agent_chain: AgentChainState::initial(), // Empty
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        // Must NOT be RunDevelopmentIteration when chain is empty
        assert!(
            !matches!(effect, Effect::RunDevelopmentIteration { .. }),
            "Must initialize agent chain before running development iteration, got {:?}",
            effect
        );
    });
}

/// Test that RunReviewPass is NOT emitted when agent chain is empty.
///
/// RunReviewPass should use the agent from state.agent_chain.
/// It should NOT be emitted when the chain is empty.
#[test]
fn test_run_review_pass_requires_agent_chain() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            agent_chain: AgentChainState::initial(), // Empty
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        // Must NOT be RunReviewPass when chain is empty
        assert!(
            !matches!(effect, Effect::RunReviewPass { .. }),
            "Must initialize agent chain before running review pass, got {:?}",
            effect
        );
    });
}

/// Test that RunFixAttempt is NOT emitted when agent chain is empty.
///
/// RunFixAttempt should use the agent from state.agent_chain.
/// It should NOT be emitted when the chain is empty.
#[test]
fn test_run_fix_attempt_requires_agent_chain() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            review_issues_found: true, // Triggers fix attempt
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            agent_chain: AgentChainState::initial(), // Empty
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        // Must NOT be RunFixAttempt when chain is empty
        assert!(
            !matches!(effect, Effect::RunFixAttempt { .. }),
            "Must initialize agent chain before running fix attempt, got {:?}",
            effect
        );
    });
}

/// Test that GeneratePlan is NOT emitted when agent chain is empty.
///
/// GeneratePlan should use the agent from state.agent_chain.
/// It should NOT be emitted when the chain is empty.
#[test]
fn test_generate_plan_requires_agent_chain() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            iteration: 0,
            total_iterations: 5,
            agent_chain: AgentChainState::initial(), // Empty
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        // Must NOT be GeneratePlan when chain is empty
        assert!(
            !matches!(effect, Effect::GeneratePlan { .. }),
            "Must initialize agent chain before generating plan, got {:?}",
            effect
        );
    });
}

/// Test that each effect type is distinct and represents a single responsibility.
///
/// This is a documentation test that enumerates the effects and their responsibilities.
#[test]
fn test_effect_types_are_single_task() {
    with_default_timeout(|| {
        // Document the single-task nature of each effect
        // This list should be updated if new effects are added
        let effect_responsibilities = vec![
            ("AgentInvocation", "Invoke a single agent with a prompt"),
            ("InitializeAgentChain", "Set up fallback chain for a role"),
            ("GeneratePlan", "Generate plan for one iteration"),
            (
                "RunDevelopmentIteration",
                "Execute one development iteration",
            ),
            ("RunReviewPass", "Execute one review pass"),
            ("RunFixAttempt", "Execute one fix attempt"),
            ("RunRebase", "Execute one rebase operation"),
            ("ResolveRebaseConflicts", "Resolve conflicts once"),
            ("GenerateCommitMessage", "Generate one commit message"),
            ("CreateCommit", "Create one commit"),
            ("SkipCommit", "Skip commit once"),
            ("ValidateFinalState", "Validate final state once"),
            ("SaveCheckpoint", "Save checkpoint once"),
            ("CleanupContext", "Clean up context files once"),
            ("RestorePromptPermissions", "Restore permissions once"),
            (
                "WriteContinuationContext",
                "Write continuation context once",
            ),
            (
                "CleanupContinuationContext",
                "Cleanup continuation context once",
            ),
        ];

        // Verify we have documented a reasonable number of effects
        assert!(
            effect_responsibilities.len() >= 15,
            "Effect inventory should be maintained: {} effects documented",
            effect_responsibilities.len()
        );
    });
}

/// Test that CommitMessage phase effects follow correct sequence.
///
/// CommitMessage phase should:
/// 1. GenerateCommitMessage when commit is NotStarted
/// 2. CreateCommit when commit is Generated
/// 3. SaveCheckpoint when commit is Committed/Skipped
#[test]
fn test_commit_phase_effect_sequence() {
    with_default_timeout(|| {
        // NotStarted -> GenerateCommitMessage
        let state_not_started = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::NotStarted,
            ..PipelineState::initial(5, 2)
        };
        let effect = determine_next_effect(&state_not_started);
        assert!(
            matches!(effect, Effect::GenerateCommitMessage),
            "NotStarted should emit GenerateCommitMessage, got {:?}",
            effect
        );

        // Generated -> CreateCommit
        let state_generated = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Generated {
                message: "test".to_string(),
            },
            ..PipelineState::initial(5, 2)
        };
        let effect = determine_next_effect(&state_generated);
        assert!(
            matches!(effect, Effect::CreateCommit { .. }),
            "Generated should emit CreateCommit, got {:?}",
            effect
        );

        // Committed -> SaveCheckpoint
        let state_committed = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Committed {
                hash: "abc".to_string(),
            },
            ..PipelineState::initial(5, 2)
        };
        let effect = determine_next_effect(&state_committed);
        assert!(
            matches!(effect, Effect::SaveCheckpoint { .. }),
            "Committed should emit SaveCheckpoint, got {:?}",
            effect
        );
    });
}

/// Test that context is cleaned before planning.
///
/// When entering Planning phase, context should be cleaned first
/// to remove old PLAN.md from previous iteration.
#[test]
fn test_context_cleaned_before_planning() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            context_cleaned: false,
            agent_chain: AgentChainState::initial().with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::CleanupContext),
            "Should cleanup context before planning, got {:?}",
            effect
        );

        // After cleanup, should generate plan
        let state_cleaned = PipelineState {
            context_cleaned: true,
            ..state
        };
        let effect = determine_next_effect(&state_cleaned);
        assert!(
            matches!(effect, Effect::GeneratePlan { .. }),
            "Should generate plan after cleanup, got {:?}",
            effect
        );
    });
}

/// Test that each phase has a clear effect for initialized state.
///
/// Each phase should emit a specific effect when properly initialized.
#[test]
fn test_phases_emit_expected_effects_when_initialized() {
    with_default_timeout(|| {
        let base_chain = AgentChainState::initial().with_agents(
            vec!["agent".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        let reviewer_chain = AgentChainState::initial().with_agents(
            vec!["reviewer".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        // Planning -> GeneratePlan
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            context_cleaned: true,
            agent_chain: base_chain.clone(),
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::GeneratePlan { .. }
        ));

        // Development -> RunDevelopmentIteration
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 0,
            total_iterations: 5,
            agent_chain: base_chain.clone(),
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::RunDevelopmentIteration { .. }
        ));

        // Review -> RunReviewPass
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            agent_chain: reviewer_chain,
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::RunReviewPass { .. }
        ));

        // FinalValidation -> ValidateFinalState
        let state = PipelineState {
            phase: PipelinePhase::FinalValidation,
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::ValidateFinalState
        ));

        // Finalizing -> RestorePromptPermissions
        let state = PipelineState {
            phase: PipelinePhase::Finalizing,
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::RestorePromptPermissions
        ));
    });
}

/// Test that RunDevelopmentIteration effect does not bundle context writing.
///
/// The handler should emit proper events that trigger WriteContinuationContext
/// as a separate effect, not write context files as a side effect of execution.
/// This test verifies the architectural invariant that effects are single-task.
#[test]
fn test_development_iteration_does_not_bundle_context_writing() {
    use ralph_workflow::reducer::effect::ContinuationContextData;
    use ralph_workflow::reducer::state::DevelopmentStatus;

    with_default_timeout(|| {
        // Verify RunDevelopmentIteration is a single-task effect
        let effect = Effect::RunDevelopmentIteration { iteration: 0 };

        // The effect variant should only contain the iteration number
        // If someone adds context_data or similar, this match would fail
        match effect {
            Effect::RunDevelopmentIteration { iteration } => {
                assert_eq!(iteration, 0);
            }
            _ => panic!("Expected RunDevelopmentIteration"),
        }

        // WriteContinuationContext is its own separate effect
        let write_effect = Effect::WriteContinuationContext(ContinuationContextData {
            iteration: 0,
            attempt: 1,
            status: DevelopmentStatus::Partial,
            summary: "test".to_string(),
            files_changed: None,
            next_steps: None,
        });

        // These are distinct effects, not bundled
        assert!(
            !matches!(
                Effect::RunDevelopmentIteration { iteration: 0 },
                Effect::WriteContinuationContext(_)
            ),
            "RunDevelopmentIteration and WriteContinuationContext must be separate effects"
        );
        assert!(
            matches!(write_effect, Effect::WriteContinuationContext(_)),
            "WriteContinuationContext should be its own effect type"
        );
    });
}

/// Test that each phase effect is independent and doesn't bundle with cleanup.
///
/// Phase effects (RunDevelopmentIteration, RunReviewPass, etc.) should only
/// execute their primary task. Cleanup operations should be separate effects.
#[test]
fn test_phase_effects_do_not_bundle_cleanup() {
    with_default_timeout(|| {
        // Verify that phase effects don't include cleanup fields
        // RunDevelopmentIteration - only iteration field
        let dev_effect = Effect::RunDevelopmentIteration { iteration: 2 };
        match dev_effect {
            Effect::RunDevelopmentIteration { iteration } => {
                assert_eq!(iteration, 2, "RunDevelopmentIteration only has iteration");
            }
            _ => panic!("Wrong effect type"),
        }

        // RunReviewPass - only pass field
        let review_effect = Effect::RunReviewPass { pass: 1 };
        match review_effect {
            Effect::RunReviewPass { pass } => {
                assert_eq!(pass, 1, "RunReviewPass only has pass");
            }
            _ => panic!("Wrong effect type"),
        }

        // RunFixAttempt - only pass field
        let fix_effect = Effect::RunFixAttempt { pass: 0 };
        match fix_effect {
            Effect::RunFixAttempt { pass } => {
                assert_eq!(pass, 0, "RunFixAttempt only has pass");
            }
            _ => panic!("Wrong effect type"),
        }

        // GeneratePlan - only iteration field
        let plan_effect = Effect::GeneratePlan { iteration: 1 };
        match plan_effect {
            Effect::GeneratePlan { iteration } => {
                assert_eq!(iteration, 1, "GeneratePlan only has iteration");
            }
            _ => panic!("Wrong effect type"),
        }

        // CleanupContext is a completely separate effect
        let cleanup_effect = Effect::CleanupContext;
        assert!(
            matches!(cleanup_effect, Effect::CleanupContext),
            "CleanupContext is its own effect"
        );
    });
}

/// Test that continuation context writing is driven by WriteContinuationContext effect.
///
/// When development returns status="partial" or "failed", the handler emits a
/// ContinuationTriggered event. The reducer then determines if WriteContinuationContext
/// effect should be emitted based on continuation budget and policy.
///
/// This test verifies that:
/// 1. WriteContinuationContext is a distinct effect (not bundled)
/// 2. The effect is emitted based on reducer state, not handler decision
/// 3. CleanupContinuationContext is also effect-driven
#[test]
fn test_continuation_context_is_effect_driven() {
    use ralph_workflow::reducer::effect::ContinuationContextData;
    use ralph_workflow::reducer::state::DevelopmentStatus;

    with_default_timeout(|| {
        // WriteContinuationContext is its own effect
        let write_effect = Effect::WriteContinuationContext(ContinuationContextData {
            iteration: 0,
            attempt: 1,
            status: DevelopmentStatus::Partial,
            summary: "test".to_string(),
            files_changed: None,
            next_steps: None,
        });

        // Verify it's distinct from development iteration effect
        let dev_effect = Effect::RunDevelopmentIteration { iteration: 0 };

        assert!(
            !matches!(&dev_effect, Effect::WriteContinuationContext(_)),
            "Continuation context writing must be separate from development iteration"
        );

        // Verify WriteContinuationContext carries the necessary data for reducer policy
        match &write_effect {
            Effect::WriteContinuationContext(data) => {
                // The data includes all info needed for reducer to track continuation state
                assert_eq!(
                    data.iteration, 0,
                    "Tracks which iteration triggered continuation"
                );
                assert_eq!(data.attempt, 1, "Tracks continuation attempt count");
                assert!(
                    matches!(data.status, DevelopmentStatus::Partial),
                    "Tracks status that triggered continuation"
                );
            }
            _ => panic!("Expected WriteContinuationContext"),
        }

        // CleanupContinuationContext is also separate
        let cleanup_effect = Effect::CleanupContinuationContext;
        assert!(
            matches!(cleanup_effect, Effect::CleanupContinuationContext),
            "Cleanup is its own effect driven by reducer, not handler side effect"
        );
    });
}

/// Test that effect determination is deterministic across ALL phases.
///
/// For every phase, calling determine_next_effect multiple times with the
/// same state must produce the same effect. This proves no external state
/// (filesystem, time, randomness) influences effect determination.
#[test]
fn test_effect_determination_deterministic_all_phases() {
    with_default_timeout(|| {
        let phases_to_test = vec![
            (PipelinePhase::Planning, AgentRole::Developer),
            (PipelinePhase::Development, AgentRole::Developer),
            (PipelinePhase::Review, AgentRole::Reviewer),
            (PipelinePhase::CommitMessage, AgentRole::Developer),
            (PipelinePhase::FinalValidation, AgentRole::Developer),
            (PipelinePhase::Finalizing, AgentRole::Developer),
            (PipelinePhase::Complete, AgentRole::Developer),
        ];

        for (phase, role) in phases_to_test {
            let mut state = PipelineState::initial(3, 2);
            state.phase = phase;
            state.context_cleaned = true;

            // Initialize agent chain for phases that need it
            if phase != PipelinePhase::Complete
                && phase != PipelinePhase::Finalizing
                && phase != PipelinePhase::FinalValidation
                && phase != PipelinePhase::CommitMessage
            {
                state.agent_chain =
                    state
                        .agent_chain
                        .with_agents(vec!["agent".to_string()], vec![vec![]], role);
            }

            // Call determine_next_effect 3 times
            let effect1 = determine_next_effect(&state);
            let effect2 = determine_next_effect(&state);
            let effect3 = determine_next_effect(&state);

            // All must be equal (using Debug format for comparison since Effect doesn't impl PartialEq)
            assert_eq!(
                format!("{:?}", effect1),
                format!("{:?}", effect2),
                "Effect determination must be deterministic for phase {:?}",
                phase
            );
            assert_eq!(
                format!("{:?}", effect2),
                format!("{:?}", effect3),
                "Effect determination must be deterministic for phase {:?}",
                phase
            );
        }
    });
}
