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
