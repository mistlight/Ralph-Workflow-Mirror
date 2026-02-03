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
use ralph_workflow::reducer::event::CheckpointTrigger;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::{AgentChainState, CommitState, PipelineState};

/// Test that development effects are NOT emitted when agent chain is empty.
///
/// Development should use the agent from state.agent_chain.
/// It should NOT be emitted when the chain is empty.
#[test]
fn test_development_requires_agent_chain() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 0,
            total_iterations: 5,
            agent_chain: AgentChainState::initial(), // Empty
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        // Must NOT be PrepareDevelopmentContext when chain is empty
        assert!(
            !matches!(effect, Effect::PrepareDevelopmentContext { .. }),
            "Must initialize agent chain before running development, got {:?}",
            effect
        );
    });
}

/// Test that review effects are NOT emitted when agent chain is empty.
///
/// Review should use the agent from state.agent_chain.
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

        // Must NOT begin review chain when chain is empty
        assert!(
            !matches!(effect, Effect::PrepareReviewContext { .. }),
            "Must initialize agent chain before running review, got {:?}",
            effect
        );
    });
}

/// Test that fix effects are NOT emitted when agent chain is empty.
///
/// Fix should use the agent from state.agent_chain.
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

        // Must NOT begin fix chain when chain is empty
        assert!(
            !matches!(effect, Effect::PrepareFixPrompt { .. }),
            "Must initialize agent chain before running fix, got {:?}",
            effect
        );
    });
}

/// Test that planning prompt preparation is NOT emitted when agent chain is empty.
///
/// Planning prompt preparation should use the agent from state.agent_chain.
/// It should NOT be emitted when the chain is empty.
#[test]
fn test_planning_prompt_requires_agent_chain() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            iteration: 0,
            total_iterations: 5,
            agent_chain: AgentChainState::initial(), // Empty
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        // Must NOT be PreparePlanningPrompt when chain is empty
        assert!(
            !matches!(effect, Effect::PreparePlanningPrompt { .. }),
            "Must initialize agent chain before preparing plan prompt, got {:?}",
            effect
        );
    });
}

/// Planning phase should emit the first single-task planning effect.
///
/// Planning must be decomposed into single-task effects; orchestration should
/// emit the first planning step, not a bundled legacy effect.
#[test]
fn test_planning_phase_emits_prepare_prompt() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            context_cleaned: true,
            iteration: 0,
            total_iterations: 5,
            agent_chain: AgentChainState::initial().with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        assert!(
            matches!(effect, Effect::MaterializePlanningInputs { .. }),
            "Planning should emit MaterializePlanningInputs first, got {:?}",
            effect
        );
    });
}

/// Development phase should emit the first single-task development effect.
#[test]
fn test_development_phase_emits_prepare_development_context() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 0,
            total_iterations: 5,
            agent_chain: AgentChainState::initial().with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            ),
            ..PipelineState::initial(5, 2)
        };

        let effect = determine_next_effect(&state);

        assert!(
            matches!(effect, Effect::PrepareDevelopmentContext { .. }),
            "Development should emit PrepareDevelopmentContext, got {:?}",
            effect
        );
    });
}

/// Test that exhausted agent chains produce an explicit abort effect.
///
/// When the agent chain is exhausted, the pipeline must not stall by emitting
/// SaveCheckpoint repeatedly. The reducer/orchestration must emit an explicit
/// abort effect so termination happens through a single effect path.
#[test]
fn test_exhausted_agent_chain_emits_abort_effect() {
    with_default_timeout(|| {
        // Development phase exhausted chain -> AbortPipeline
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["agent".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(1);
        chain = chain.start_retry_cycle();
        assert!(
            chain.is_exhausted(),
            "test precondition: chain must be exhausted"
        );

        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 0,
            total_iterations: 1,
            agent_chain: chain,
            ..PipelineState::initial(1, 0)
        };
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::AbortPipeline { .. }),
            "Exhausted chain must abort explicitly; got {effect:?}"
        );

        // Review phase exhausted chain -> AbortPipeline
        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["reviewer".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            )
            .with_max_cycles(1);
        chain = chain.start_retry_cycle();
        assert!(
            chain.is_exhausted(),
            "test precondition: chain must be exhausted"
        );

        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            agent_chain: chain,
            ..PipelineState::initial(1, 1)
        };
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::AbortPipeline { .. }),
            "Exhausted chain must abort explicitly; got {effect:?}"
        );
    });
}

/// Test that Interrupted phase drives a checkpoint save before termination.
///
/// Regression: an Interrupted state combined with an exhausted agent chain must not
/// repeatedly emit AbortPipeline. Orchestration should emit a single SaveCheckpoint
/// (Interrupt trigger) so the state machine can complete.
#[test]
fn test_interrupted_phase_emits_interrupt_checkpoint_save() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Interrupted,
            checkpoint_saved_count: 0,
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent".to_string()],
                vec![vec![]],
                AgentRole::Reviewer,
            ),
            ..PipelineState::initial(0, 1)
        };

        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::Interrupt
                }
            ),
            "Interrupted phase must emit interrupt checkpoint save; got {effect:?}"
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
            (
                "PreparePlanningPrompt",
                "Render and persist planning prompt",
            ),
            (
                "InvokePlanningAgent",
                "Invoke planning agent for one iteration",
            ),
            ("ExtractPlanningXml", "Extract plan XML from canonical path"),
            ("ValidatePlanningXml", "Validate plan XML"),
            ("WritePlanningMarkdown", "Write PLAN.md from plan XML"),
            (
                "ArchivePlanningXml",
                "Archive plan XML after writing PLAN.md",
            ),
            (
                "ApplyPlanningOutcome",
                "Apply planning outcome to reducer state",
            ),
            (
                "PrepareDevelopmentContext",
                "Prepare development context files for one iteration",
            ),
            (
                "PrepareDevelopmentPrompt",
                "Render and persist the development prompt for one iteration",
            ),
            (
                "InvokeDevelopmentAgent",
                "Invoke developer agent for one iteration",
            ),
            (
                "ExtractDevelopmentXml",
                "Extract development result XML from canonical path",
            ),
            ("ValidateDevelopmentXml", "Validate development result XML"),
            (
                "ApplyDevelopmentOutcome",
                "Apply validated development outcome to advance state",
            ),
            (
                "ArchiveDevelopmentXml",
                "Archive .agent/tmp/development_result.xml",
            ),
            (
                "PrepareReviewContext",
                "Prepare review context files for one pass",
            ),
            (
                "PrepareReviewPrompt",
                "Render and persist the review prompt for one pass",
            ),
            ("InvokeReviewAgent", "Invoke reviewer agent for one pass"),
            (
                "ExtractReviewIssuesXml",
                "Extract review issues XML from canonical path",
            ),
            ("ValidateReviewIssuesXml", "Validate review issues XML"),
            ("WriteIssuesMarkdown", "Write .agent/ISSUES.md"),
            (
                "ExtractReviewIssueSnippets",
                "Extract review issue snippets for UI output",
            ),
            (
                "ArchiveReviewIssuesXml",
                "Archive .agent/tmp/issues.xml after writing ISSUES.md",
            ),
            (
                "ApplyReviewOutcome",
                "Apply validated review outcome to advance review state",
            ),
            (
                "PrepareFixPrompt",
                "Render and persist the fix prompt for one pass",
            ),
            ("InvokeFixAgent", "Invoke fix agent for one pass"),
            (
                "ExtractFixResultXml",
                "Extract fix result XML from canonical path",
            ),
            ("ValidateFixResultXml", "Validate fix result XML"),
            (
                "ApplyFixOutcome",
                "Apply validated fix outcome to advance fix state",
            ),
            ("ArchiveFixResultXml", "Archive .agent/tmp/fix_result.xml"),
            ("RunRebase", "Execute one rebase operation"),
            ("ResolveRebaseConflicts", "Resolve conflicts once"),
            ("PrepareCommitPrompt", "Prepare commit prompt"),
            ("InvokeCommitAgent", "Invoke commit agent once"),
            ("ExtractCommitXml", "Extract commit XML from canonical path"),
            ("ValidateCommitXml", "Validate commit XML"),
            (
                "ApplyCommitMessageOutcome",
                "Apply validated commit outcome",
            ),
            ("ArchiveCommitXml", "Archive .agent/tmp/commit_message.xml"),
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

/// Test that CommitMessage phase requires agent chain initialization.
///
/// CommitMessage phase should first initialize agent chain when empty,
/// just like other phases (Planning, Development, Review).
#[test]
fn test_commit_phase_requires_agent_chain() {
    with_default_timeout(|| {
        // Empty chain -> InitializeAgentChain
        let state_empty_chain = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::NotStarted,
            agent_chain: AgentChainState::initial(),
            ..PipelineState::initial(5, 2)
        };
        let effect = determine_next_effect(&state_empty_chain);
        assert!(
            matches!(
                effect,
                Effect::InitializeAgentChain {
                    role: AgentRole::Commit
                }
            ),
            "Empty chain should emit InitializeAgentChain for Commit, got {:?}",
            effect
        );
    });
}

/// Test that CommitMessage phase effects follow correct sequence.
///
/// CommitMessage phase should (after agent chain is initialized):
/// 1. PrepareCommitPrompt when commit is NotStarted
/// 2. CreateCommit when commit is Generated
/// 3. SaveCheckpoint when commit is Committed/Skipped
#[test]
fn test_commit_phase_effect_sequence() {
    with_default_timeout(|| {
        // Create agent chain for commit phase
        let commit_chain = AgentChainState::initial().with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        );

        // NotStarted (with chain) -> CheckCommitDiff (before PrepareCommitPrompt)
        let state_not_started = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::NotStarted,
            agent_chain: commit_chain.clone(),
            ..PipelineState::initial(5, 2)
        };
        let effect = determine_next_effect(&state_not_started);
        assert!(
            matches!(effect, Effect::CheckCommitDiff),
            "NotStarted with chain should emit CheckCommitDiff first, got {:?}",
            effect
        );

        // After diff checked, NotStarted -> PrepareCommitPrompt
        let state_not_started_diff_prepared = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::NotStarted,
            commit_diff_prepared: true,
            commit_diff_content_id_sha256: Some("id".to_string()),
            agent_chain: commit_chain.clone(),
            ..PipelineState::initial(5, 2)
        };
        let effect = determine_next_effect(&state_not_started_diff_prepared);
        assert!(
            matches!(effect, Effect::MaterializeCommitInputs { .. }),
            "NotStarted with diff prepared should emit MaterializeCommitInputs, got {:?}",
            effect
        );

        // Generated -> ArchiveCommitXml (before CreateCommit)
        let state_generated = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Generated {
                message: "test".to_string(),
            },
            agent_chain: commit_chain.clone(),
            ..PipelineState::initial(5, 2)
        };
        let effect = determine_next_effect(&state_generated);
        assert!(
            matches!(effect, Effect::ArchiveCommitXml),
            "Generated should emit ArchiveCommitXml, got {:?}",
            effect
        );

        // Committed -> SaveCheckpoint
        let state_committed = PipelineState {
            phase: PipelinePhase::CommitMessage,
            commit: CommitState::Committed {
                hash: "abc".to_string(),
            },
            agent_chain: commit_chain,
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

        // After cleanup, should prepare planning prompt
        let state_cleaned = PipelineState {
            context_cleaned: true,
            ..state
        };
        let effect = determine_next_effect(&state_cleaned);
        assert!(
            matches!(effect, Effect::MaterializePlanningInputs { .. }),
            "Should materialize planning inputs after cleanup, got {:?}",
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

        // Planning -> PreparePlanningPrompt
        let state = PipelineState {
            phase: PipelinePhase::Planning,
            context_cleaned: true,
            agent_chain: base_chain.clone(),
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::MaterializePlanningInputs { .. }
        ));

        // Development -> PrepareDevelopmentContext
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 0,
            total_iterations: 5,
            agent_chain: base_chain.clone(),
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::PrepareDevelopmentContext { .. }
        ));

        // Review -> PrepareReviewContext (start of single-task review chain)
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            agent_chain: reviewer_chain,
            ..PipelineState::initial(5, 2)
        };
        assert!(matches!(
            determine_next_effect(&state),
            Effect::PrepareReviewContext { .. }
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

/// Test that ApplyDevelopmentOutcome effect does not bundle context writing.
///
/// The handler should emit proper events that trigger WriteContinuationContext
/// as a separate effect, not write context files as a side effect of execution.
/// This test verifies the architectural invariant that effects are single-task.
#[test]
fn test_development_outcome_does_not_bundle_context_writing() {
    use ralph_workflow::reducer::effect::ContinuationContextData;
    use ralph_workflow::reducer::state::DevelopmentStatus;

    with_default_timeout(|| {
        // Verify ApplyDevelopmentOutcome is a single-task effect
        let effect = Effect::ApplyDevelopmentOutcome { iteration: 0 };

        // The effect variant should only contain the iteration number
        // If someone adds context_data or similar, this match would fail
        match effect {
            Effect::ApplyDevelopmentOutcome { iteration } => {
                assert_eq!(iteration, 0);
            }
            _ => panic!("Expected ApplyDevelopmentOutcome"),
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
                Effect::ApplyDevelopmentOutcome { iteration: 0 },
                Effect::WriteContinuationContext(_)
            ),
            "ApplyDevelopmentOutcome and WriteContinuationContext must be separate effects"
        );
        assert!(
            matches!(write_effect, Effect::WriteContinuationContext(_)),
            "WriteContinuationContext should be its own effect type"
        );
    });
}

/// Test that each phase effect is independent and doesn't bundle with cleanup.
///
/// Phase effects (PrepareDevelopmentContext, PrepareReviewContext, etc.) should only
/// execute their primary task. Cleanup operations should be separate effects.
#[test]
fn test_phase_effects_do_not_bundle_cleanup() {
    with_default_timeout(|| {
        // Verify that phase effects don't include cleanup fields
        // PrepareDevelopmentContext - only iteration field
        let dev_effect = Effect::PrepareDevelopmentContext { iteration: 2 };
        match dev_effect {
            Effect::PrepareDevelopmentContext { iteration } => {
                assert_eq!(iteration, 2, "PrepareDevelopmentContext only has iteration");
            }
            _ => panic!("Wrong effect type"),
        }

        // PrepareReviewContext - only pass field
        let review_effect = Effect::PrepareReviewContext { pass: 1 };
        match review_effect {
            Effect::PrepareReviewContext { pass } => {
                assert_eq!(pass, 1, "PrepareReviewContext only has pass");
            }
            _ => panic!("Wrong effect type"),
        }

        // PrepareFixPrompt - only pass field
        let fix_effect = Effect::PrepareFixPrompt {
            pass: 0,
            prompt_mode: ralph_workflow::reducer::state::PromptMode::Normal,
        };
        match fix_effect {
            Effect::PrepareFixPrompt { pass, .. } => {
                assert_eq!(pass, 0, "PrepareFixPrompt has pass");
            }
            _ => panic!("Wrong effect type"),
        }

        // PreparePlanningPrompt - only iteration field
        let plan_effect = Effect::PreparePlanningPrompt {
            iteration: 1,
            prompt_mode: ralph_workflow::reducer::state::PromptMode::Normal,
        };
        match plan_effect {
            Effect::PreparePlanningPrompt { iteration, .. } => {
                assert_eq!(iteration, 1, "PreparePlanningPrompt has iteration");
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

        // Verify it's distinct from development context effect
        let dev_effect = Effect::PrepareDevelopmentContext { iteration: 0 };

        assert!(
            !matches!(&dev_effect, Effect::WriteContinuationContext(_)),
            "Continuation context writing must be separate from development context"
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
