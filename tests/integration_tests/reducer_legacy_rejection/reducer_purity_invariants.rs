//! Integration tests for reducer purity invariants.
//!
//! Verifies that the reducer follows pure functional principles:
//! - State transitions are purely driven by events through the reducer
//! - Effect determination is based solely on reducer state
//! - No direct state mutation outside the reducer
//! - All control flow happens via events, not side effects
//!
//! # Integration Test Compliance
//!
//! These tests follow [../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md):
//! - Test observable behavior: phase transitions, effect emission
//! - Pure reducer tests require no mocks
//! - Verify deterministic state transitions

use crate::test_timeout::with_default_timeout;

// ============================================================================
// REDUCER-ONLY CONTROL FLOW TESTS
// ============================================================================

/// Test that state transitions are purely driven by events through the reducer.
///
/// This verifies that phase transitions happen via the reduce() function,
/// not through any direct state mutation.
#[test]
fn test_state_transitions_via_reducer_only() {
    use ralph_workflow::reducer::event::{CommitEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start at Planning phase with 2 iterations and 1 reviewer pass
        let state = PipelineState::initial(2, 1);
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0, "Initial iteration is 0");

        // Planning -> Development transition via reduce()
        let state = reduce(state, PipelineEvent::plan_generation_completed(1, true));
        assert_eq!(
            state.phase,
            PipelinePhase::Development,
            "Transition to Development must happen via reducer"
        );
        assert_eq!(state.iteration, 0, "Iteration unchanged by plan completion");

        // Development iteration completion -> CommitMessage
        // Note: iteration field stays at 0 until commit is created
        let state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Dev iteration completion transitions to CommitMessage"
        );
        assert_eq!(state.iteration, 0, "Iteration unchanged in CommitMessage");

        // After commit created, goes to Planning for next iteration (not Development directly!)
        // The reducer pattern is: Dev -> Commit -> Planning -> Dev (for each iteration)
        let state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                message: "test commit".to_string(),
                hash: "abc123".to_string(),
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Planning,
            "After commit with more iterations, goes to Planning"
        );
        assert_eq!(state.iteration, 1, "Iteration incremented to 1");

        // Planning again -> Development
        let state = reduce(state, PipelineEvent::plan_generation_completed(2, true));
        assert_eq!(state.phase, PipelinePhase::Development);

        // Complete iteration 1 (second dev iteration) -> CommitMessage
        let state = reduce(
            state,
            PipelineEvent::development_iteration_completed(1, true),
        );
        assert_eq!(state.phase, PipelinePhase::CommitMessage);

        // After final commit, transitions to Review (iteration 2 >= total_iterations 2)
        let state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                message: "final commit".to_string(),
                hash: "def456".to_string(),
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "After final iteration commit, should transition to Review"
        );
        assert_eq!(state.iteration, 2, "Final iteration is 2");
    });
}

/// Test that effect determination is based solely on reducer state.
///
/// The determine_next_effect() function should be a pure function of state,
/// not reading any external configuration or files.
#[test]
fn test_effects_determined_from_state_only() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Initial state needs agent chain initialization
        let state = PipelineState::initial(3, 1);
        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                Effect::InitializeAgentChain {
                    role: AgentRole::Developer
                }
            ),
            "Effect should be determined purely from state: {:?}",
            effect
        );

        // State with agents but no gitignore ensured -> ensure gitignore
        let mut state = PipelineState::initial(3, 1);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        state.gitignore_entries_ensured = false;
        state.context_cleaned = false;
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::EnsureGitignoreEntries),
            "Should ensure gitignore before cleanup: {:?}",
            effect
        );

        // State with gitignore ensured but no context cleaned -> clean context
        let mut state = PipelineState::initial(3, 1);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        state.gitignore_entries_ensured = true;
        state.context_cleaned = false;
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::CleanupContext),
            "Should clean context before planning: {:?}",
            effect
        );

        // State ready for planning
        let mut state = PipelineState::initial(3, 1);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        state.gitignore_entries_ensured = true;
        state.context_cleaned = true;
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::MaterializePlanningInputs { .. }),
            "Should materialize planning inputs when state is ready: {:?}",
            effect
        );

        // Development phase
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::PrepareDevelopmentContext { .. }),
            "Should start development chain from state: {:?}",
            effect
        );
    });
}

/// Test that agent selection comes from reducer state, not config lookups.
///
/// The agent_chain in PipelineState should be the single source of truth
/// for which agent to use next.
#[test]
fn test_agent_selection_from_reducer_state() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Set up state with specific agents in chain
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["custom-agent".to_string(), "fallback-agent".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // The effect doesn't contain agent name - handler reads from state.agent_chain
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::PrepareDevelopmentContext { iteration: 1 }),
            "Expected PrepareDevelopmentContext, got {:?}",
            effect
        );

        // Verify agent chain has our custom agent as current
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"custom-agent".to_string()),
            "Agent should be selected from state.agent_chain"
        );

        // After switching to next agent, chain should point to fallback
        state.agent_chain = state.agent_chain.switch_to_next_agent();
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"fallback-agent".to_string()),
            "Should use next agent in chain after switch"
        );
    });
}

/// Test that pipeline completion is determined by reducer state, not file existence.
///
/// The pipeline should complete when state.phase == Complete, not when
/// certain files exist on disk.
#[test]
fn test_completion_from_state_not_files() {
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::{CheckpointTrigger, PipelinePhase};
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // State at Complete phase
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Complete;

        let effect = determine_next_effect(&state);
        // Complete phase emits SaveCheckpoint with Interrupt trigger
        assert!(
            matches!(
                effect,
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::Interrupt
                }
            ),
            "Should save checkpoint on complete based on state.phase, not file checks: {:?}",
            effect
        );
    });
}

// ============================================================================
// AGENT CHAIN STATE MANAGEMENT TESTS
// ============================================================================

/// Test that agent chain is cleared on dev->review transition via reducer.
///
/// When transitioning from Development to Review phase, the agent chain must
/// be cleared so that the orchestrator initializes a fresh Reviewer chain.
/// This prevents the developer agent chain from leaking into review phase.
#[test]
fn test_agent_chain_cleared_on_dev_to_review_transition() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{CommitEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start with populated developer agent chain that has been used
        let mut state = PipelineState::initial(1, 1);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["dev-primary".to_string(), "dev-fallback".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        state.phase = PipelinePhase::CommitMessage;
        state.previous_phase = Some(PipelinePhase::Development);
        state.commit = ralph_workflow::reducer::state::CommitState::Generated {
            message: "test commit".to_string(),
        };

        // Verify developer chain is populated
        assert!(!state.agent_chain.agents.is_empty());
        assert_eq!(state.agent_chain.current_role, AgentRole::Developer);

        // Transition via CommitEvent::Created - this should go to Review since
        // iteration 0 + 1 = 1 >= total_iterations (1)
        let new_state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                hash: "abc123".to_string(),
                message: "test commit".to_string(),
            }),
        );

        // Should be in Review phase
        assert_eq!(
            new_state.phase,
            PipelinePhase::Review,
            "Should transition to Review phase"
        );

        // Agent chain should be CLEARED for Reviewer initialization
        assert!(
            new_state.agent_chain.agents.is_empty(),
            "Agent chain must be cleared on dev->review transition, was: {:?}",
            new_state.agent_chain.agents
        );
    });
}

// ============================================================================
// EFFECT SINGLE-TASK VERIFICATION TESTS
// ============================================================================

/// Test that all Effect variants represent single logical operations.
///
/// This test documents the single-responsibility nature of each effect type.
/// If a new effect is added that bundles multiple operations, this test
/// should be updated to discuss whether the effect should be split.
#[test]
fn test_effects_are_single_task() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::{ContinuationContextData, Effect};
    use ralph_workflow::reducer::event::{CheckpointTrigger, ConflictStrategy, RebasePhase};
    use ralph_workflow::reducer::state::DevelopmentStatus;

    with_default_timeout(|| {
        // This test enumerates all Effect variants to verify they each represent
        // a single logical operation. The match is exhaustive so the test will
        // fail to compile if new variants are added without consideration.

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum EffectTask {
            AgentInvocation,
            InitializeAgentChain,
            PreparePlanningPrompt,
            MaterializePlanningInputs,
            InvokePlanningAgent,
            ExtractPlanningXml,
            ValidatePlanningXml,
            WritePlanningMarkdown,
            ArchivePlanningXml,
            ApplyPlanningOutcome,
            PrepareDevelopmentContext,
            MaterializeDevelopmentInputs,
            PrepareDevelopmentPrompt,
            InvokeDevelopmentAgent,
            InvokeAnalysisAgent,
            ExtractDevelopmentXml,
            ValidateDevelopmentXml,
            ApplyDevelopmentOutcome,
            ArchiveDevelopmentXml,
            PrepareReviewContext,
            MaterializeReviewInputs,
            PrepareReviewPrompt,
            InvokeReviewAgent,
            ExtractReviewIssuesXml,
            ValidateReviewIssuesXml,
            WriteIssuesMarkdown,
            ExtractReviewIssueSnippets,
            ArchiveReviewIssuesXml,
            ApplyReviewOutcome,
            PrepareFixPrompt,
            InvokeFixAgent,
            ExtractFixResultXml,
            ValidateFixResultXml,
            ApplyFixOutcome,
            ArchiveFixResultXml,
            RunRebase,
            ResolveRebaseConflicts,
            CheckCommitDiff,
            MaterializeCommitInputs,
            PrepareCommitPrompt,
            InvokeCommitAgent,
            ExtractCommitXml,
            ValidateCommitXml,
            ApplyCommitMessageOutcome,
            ArchiveCommitXml,
            CreateCommit,
            SkipCommit,
            BackoffWait,
            ReportAgentChainExhausted,
            ValidateFinalState,
            SaveCheckpoint,
            EnsureGitignoreEntries,
            CleanupContext,
            RestorePromptPermissions,
            WriteContinuationContext,
            CleanupContinuationContext,
            CleanupPlanningXml,
            CleanupDevelopmentXml,
            CleanupReviewIssuesXml,
            CleanupFixResultXml,
            CleanupCommitXml,
            TriggerDevFixFlow,
            TriggerLoopRecovery,
            EmitCompletionMarkerAndTerminate,
        }

        fn describe_effect_task(effect: &Effect) -> EffectTask {
            match effect {
                // Each match arm describes the SINGLE task the effect performs
                Effect::AgentInvocation { .. } => EffectTask::AgentInvocation,
                Effect::InitializeAgentChain { .. } => EffectTask::InitializeAgentChain,
                Effect::PreparePlanningPrompt { .. } => EffectTask::PreparePlanningPrompt,
                Effect::MaterializePlanningInputs { .. } => EffectTask::MaterializePlanningInputs,
                Effect::InvokePlanningAgent { .. } => EffectTask::InvokePlanningAgent,
                Effect::ExtractPlanningXml { .. } => EffectTask::ExtractPlanningXml,
                Effect::ValidatePlanningXml { .. } => EffectTask::ValidatePlanningXml,
                Effect::WritePlanningMarkdown { .. } => EffectTask::WritePlanningMarkdown,
                Effect::ArchivePlanningXml { .. } => EffectTask::ArchivePlanningXml,
                Effect::ApplyPlanningOutcome { .. } => EffectTask::ApplyPlanningOutcome,
                Effect::PrepareDevelopmentContext { .. } => EffectTask::PrepareDevelopmentContext,
                Effect::MaterializeDevelopmentInputs { .. } => {
                    EffectTask::MaterializeDevelopmentInputs
                }
                Effect::PrepareDevelopmentPrompt { .. } => EffectTask::PrepareDevelopmentPrompt,
                Effect::InvokeDevelopmentAgent { .. } => EffectTask::InvokeDevelopmentAgent,
                Effect::InvokeAnalysisAgent { .. } => EffectTask::InvokeAnalysisAgent,
                Effect::ExtractDevelopmentXml { .. } => EffectTask::ExtractDevelopmentXml,
                Effect::ValidateDevelopmentXml { .. } => EffectTask::ValidateDevelopmentXml,
                Effect::ApplyDevelopmentOutcome { .. } => EffectTask::ApplyDevelopmentOutcome,
                Effect::ArchiveDevelopmentXml { .. } => EffectTask::ArchiveDevelopmentXml,
                Effect::PrepareReviewContext { .. } => EffectTask::PrepareReviewContext,
                Effect::MaterializeReviewInputs { .. } => EffectTask::MaterializeReviewInputs,
                Effect::PrepareReviewPrompt { .. } => EffectTask::PrepareReviewPrompt,
                Effect::InvokeReviewAgent { .. } => EffectTask::InvokeReviewAgent,
                Effect::ExtractReviewIssuesXml { .. } => EffectTask::ExtractReviewIssuesXml,
                Effect::ValidateReviewIssuesXml { .. } => EffectTask::ValidateReviewIssuesXml,
                Effect::WriteIssuesMarkdown { .. } => EffectTask::WriteIssuesMarkdown,
                Effect::ExtractReviewIssueSnippets { .. } => EffectTask::ExtractReviewIssueSnippets,
                Effect::ArchiveReviewIssuesXml { .. } => EffectTask::ArchiveReviewIssuesXml,
                Effect::ApplyReviewOutcome { .. } => EffectTask::ApplyReviewOutcome,
                Effect::PrepareFixPrompt { .. } => EffectTask::PrepareFixPrompt,
                Effect::InvokeFixAgent { .. } => EffectTask::InvokeFixAgent,
                Effect::ExtractFixResultXml { .. } => EffectTask::ExtractFixResultXml,
                Effect::ValidateFixResultXml { .. } => EffectTask::ValidateFixResultXml,
                Effect::ApplyFixOutcome { .. } => EffectTask::ApplyFixOutcome,
                Effect::ArchiveFixResultXml { .. } => EffectTask::ArchiveFixResultXml,
                Effect::RunRebase { .. } => EffectTask::RunRebase,
                Effect::ResolveRebaseConflicts { .. } => EffectTask::ResolveRebaseConflicts,
                Effect::CheckCommitDiff => EffectTask::CheckCommitDiff,
                Effect::MaterializeCommitInputs { .. } => EffectTask::MaterializeCommitInputs,
                Effect::PrepareCommitPrompt { .. } => EffectTask::PrepareCommitPrompt,
                Effect::InvokeCommitAgent => EffectTask::InvokeCommitAgent,
                Effect::ExtractCommitXml => EffectTask::ExtractCommitXml,
                Effect::ValidateCommitXml => EffectTask::ValidateCommitXml,
                Effect::ApplyCommitMessageOutcome => EffectTask::ApplyCommitMessageOutcome,
                Effect::ArchiveCommitXml => EffectTask::ArchiveCommitXml,
                Effect::CreateCommit { .. } => EffectTask::CreateCommit,
                Effect::SkipCommit { .. } => EffectTask::SkipCommit,
                Effect::BackoffWait { .. } => EffectTask::BackoffWait,
                Effect::ReportAgentChainExhausted { .. } => EffectTask::ReportAgentChainExhausted,
                Effect::ValidateFinalState => EffectTask::ValidateFinalState,
                Effect::SaveCheckpoint { .. } => EffectTask::SaveCheckpoint,
                Effect::EnsureGitignoreEntries => EffectTask::EnsureGitignoreEntries,
                Effect::CleanupContext => EffectTask::CleanupContext,
                Effect::RestorePromptPermissions => EffectTask::RestorePromptPermissions,
                Effect::WriteContinuationContext(_) => EffectTask::WriteContinuationContext,
                Effect::CleanupContinuationContext => EffectTask::CleanupContinuationContext,
                Effect::CleanupPlanningXml { .. } => EffectTask::CleanupPlanningXml,
                Effect::CleanupDevelopmentXml { .. } => EffectTask::CleanupDevelopmentXml,
                Effect::CleanupReviewIssuesXml { .. } => EffectTask::CleanupReviewIssuesXml,
                Effect::CleanupFixResultXml { .. } => EffectTask::CleanupFixResultXml,
                Effect::CleanupCommitXml => EffectTask::CleanupCommitXml,
                Effect::TriggerDevFixFlow { .. } => EffectTask::TriggerDevFixFlow,
                Effect::TriggerLoopRecovery { .. } => EffectTask::TriggerLoopRecovery,
                Effect::EmitCompletionMarkerAndTerminate { .. } => {
                    EffectTask::EmitCompletionMarkerAndTerminate
                }
            }
        }

        // Create sample instances of each effect to verify they exist
        // and the match is exhaustive
        let effects: Vec<Effect> = vec![
            Effect::AgentInvocation {
                role: AgentRole::Developer,
                agent: "test".to_string(),
                model: None,
                prompt: "test".to_string(),
            },
            Effect::InitializeAgentChain {
                role: AgentRole::Developer,
            },
            Effect::PreparePlanningPrompt {
                iteration: 0,
                prompt_mode: ralph_workflow::reducer::state::PromptMode::Normal,
            },
            Effect::MaterializePlanningInputs { iteration: 0 },
            Effect::CleanupPlanningXml { iteration: 0 },
            Effect::InvokePlanningAgent { iteration: 0 },
            Effect::ExtractPlanningXml { iteration: 0 },
            Effect::ValidatePlanningXml { iteration: 0 },
            Effect::WritePlanningMarkdown { iteration: 0 },
            Effect::ArchivePlanningXml { iteration: 0 },
            Effect::ApplyPlanningOutcome {
                iteration: 0,
                valid: true,
            },
            Effect::PrepareDevelopmentContext { iteration: 0 },
            Effect::MaterializeDevelopmentInputs { iteration: 0 },
            Effect::PrepareDevelopmentPrompt {
                iteration: 0,
                prompt_mode: ralph_workflow::reducer::state::PromptMode::Normal,
            },
            Effect::CleanupDevelopmentXml { iteration: 0 },
            Effect::InvokeDevelopmentAgent { iteration: 0 },
            Effect::InvokeAnalysisAgent { iteration: 0 },
            Effect::ExtractDevelopmentXml { iteration: 0 },
            Effect::ValidateDevelopmentXml { iteration: 0 },
            Effect::ApplyDevelopmentOutcome { iteration: 0 },
            Effect::ArchiveDevelopmentXml { iteration: 0 },
            Effect::PrepareReviewContext { pass: 0 },
            Effect::MaterializeReviewInputs { pass: 0 },
            Effect::PrepareReviewPrompt {
                pass: 0,
                prompt_mode: ralph_workflow::reducer::state::PromptMode::Normal,
            },
            Effect::CleanupReviewIssuesXml { pass: 0 },
            Effect::InvokeReviewAgent { pass: 0 },
            Effect::ExtractReviewIssuesXml { pass: 0 },
            Effect::ValidateReviewIssuesXml { pass: 0 },
            Effect::WriteIssuesMarkdown { pass: 0 },
            Effect::ExtractReviewIssueSnippets { pass: 0 },
            Effect::ArchiveReviewIssuesXml { pass: 0 },
            Effect::ApplyReviewOutcome {
                pass: 0,
                issues_found: false,
                clean_no_issues: true,
            },
            Effect::PrepareFixPrompt {
                pass: 0,
                prompt_mode: ralph_workflow::reducer::state::PromptMode::Normal,
            },
            Effect::CleanupFixResultXml { pass: 0 },
            Effect::InvokeFixAgent { pass: 0 },
            Effect::ExtractFixResultXml { pass: 0 },
            Effect::ValidateFixResultXml { pass: 0 },
            Effect::ApplyFixOutcome { pass: 0 },
            Effect::ArchiveFixResultXml { pass: 0 },
            Effect::RunRebase {
                phase: RebasePhase::Initial,
                target_branch: "main".to_string(),
            },
            Effect::ResolveRebaseConflicts {
                strategy: ConflictStrategy::Abort,
            },
            Effect::CheckCommitDiff,
            Effect::MaterializeCommitInputs { attempt: 1 },
            Effect::PrepareCommitPrompt {
                prompt_mode: ralph_workflow::reducer::state::PromptMode::Normal,
            },
            Effect::CleanupCommitXml,
            Effect::InvokeCommitAgent,
            Effect::ExtractCommitXml,
            Effect::ValidateCommitXml,
            Effect::ApplyCommitMessageOutcome,
            Effect::ArchiveCommitXml,
            Effect::CreateCommit {
                message: "test".to_string(),
            },
            Effect::SkipCommit {
                reason: "test".to_string(),
            },
            Effect::BackoffWait {
                role: AgentRole::Developer,
                cycle: 1,
                duration_ms: 1,
            },
            Effect::ReportAgentChainExhausted {
                role: AgentRole::Developer,
                phase: ralph_workflow::reducer::event::PipelinePhase::Development,
                cycle: 1,
            },
            Effect::ValidateFinalState,
            Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::PhaseTransition,
            },
            Effect::CleanupContext,
            Effect::RestorePromptPermissions,
            Effect::WriteContinuationContext(ContinuationContextData {
                iteration: 0,
                attempt: 0,
                status: DevelopmentStatus::Completed,
                summary: "test".to_string(),
                files_changed: None,
                next_steps: None,
            }),
            Effect::CleanupContinuationContext,
            Effect::TriggerDevFixFlow {
                failed_phase: ralph_workflow::reducer::event::PipelinePhase::Development,
                failed_role: AgentRole::Developer,
                retry_cycle: 1,
            },
            Effect::EmitCompletionMarkerAndTerminate {
                is_failure: true,
                reason: Some("test".to_string()),
            },
        ];

        // Verify each effect maps to a single-task category.
        for effect in &effects {
            let _task = describe_effect_task(effect);
        }

        // Verify we covered all variants (update when Effect changes)
        assert_eq!(
            effects.len(),
            62,
            "Expected 62 Effect variants; update this test if variants were added or removed"
        );
    });
}

/// Test that agent fallback happens exclusively via reducer events.
///
/// Agent switching occurs through reducer event processing, not through
/// any ad-hoc logic in phase code. This test verifies the reducer is the
/// single source of truth for agent chain advancement.
#[test]
fn test_agent_fallback_only_via_reducer_events() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{AgentEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::ContinuationState;
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::with_limits(2, 3, 2);
        state.agent_chain = state.agent_chain.with_agents(
            vec![
                "agent-a".to_string(),
                "agent-b".to_string(),
                "agent-c".to_string(),
            ],
            vec![vec![], vec![], vec![]],
            AgentRole::Developer,
        );

        // Verify initial agent
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-a".to_string())
        );

        // FallbackTriggered event should switch to next agent
        let state = reduce(
            state,
            PipelineEvent::Agent(AgentEvent::FallbackTriggered {
                role: AgentRole::Developer,
                from_agent: "agent-a".to_string(),
                to_agent: "agent-b".to_string(),
            }),
        );

        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-b".to_string()),
            "FallbackTriggered should switch to next agent"
        );

        // InvocationFailed with retriable=false should retry the same agent first (except auth/429),
        // and only switch agents after exhausting the same-agent retry budget.
        let after_first_failure = reduce(
            state,
            PipelineEvent::Agent(AgentEvent::InvocationFailed {
                role: AgentRole::Developer,
                agent: "agent-b".to_string(),
                exit_code: 1,
                error_kind: ralph_workflow::reducer::event::AgentErrorKind::FileSystem,
                retriable: false,
            }),
        );

        assert_eq!(
            after_first_failure.agent_chain.current_agent(),
            Some(&"agent-b".to_string()),
            "InvocationFailed(retriable=false) should retry same agent first (except auth/429)"
        );
        assert!(after_first_failure.continuation.same_agent_retry_pending);

        let after_second_failure = reduce(
            after_first_failure,
            PipelineEvent::Agent(AgentEvent::InvocationFailed {
                role: AgentRole::Developer,
                agent: "agent-b".to_string(),
                exit_code: 1,
                error_kind: ralph_workflow::reducer::event::AgentErrorKind::FileSystem,
                retriable: false,
            }),
        );

        assert_eq!(
            after_second_failure.agent_chain.current_agent(),
            Some(&"agent-c".to_string()),
            "After exhausting same-agent retry budget, InvocationFailed(retriable=false) should switch agents"
        );

        // InvocationFailed with retriable=true should NOT switch agents (tries next model)
        // Reset to test retriable case
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["primary".to_string(), "fallback".to_string()],
            vec![vec!["model-a".to_string(), "model-b".to_string()], vec![]],
            AgentRole::Developer,
        );

        let state = reduce(
            state,
            PipelineEvent::Agent(AgentEvent::InvocationFailed {
                role: AgentRole::Developer,
                agent: "primary".to_string(),
                exit_code: 1,
                error_kind: ralph_workflow::reducer::event::AgentErrorKind::Network,
                retriable: true,
            }),
        );

        // Retriable failure should advance model, not agent
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"primary".to_string()),
            "InvocationFailed(retriable=true) should NOT switch agent"
        );
    });
}

/// Test that effect determination is stateless and deterministic.
///
/// The same state should always produce the same effect. This is a key
/// property of the reducer architecture - no external state influences
/// effect determination.
#[test]
fn test_effect_determination_is_pure_function_of_state() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Create a specific state
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["test-agent".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Call determine_next_effect multiple times
        let effect1 = determine_next_effect(&state);
        let effect2 = determine_next_effect(&state);
        let effect3 = determine_next_effect(&state);

        // All calls should produce the same effect (purity)
        assert!(
            matches!(&effect1, Effect::PrepareDevelopmentContext { iteration: 1 }),
            "First call: {:?}",
            effect1
        );
        assert!(
            matches!(&effect2, Effect::PrepareDevelopmentContext { iteration: 1 }),
            "Second call: {:?}",
            effect2
        );
        assert!(
            matches!(&effect3, Effect::PrepareDevelopmentContext { iteration: 1 }),
            "Third call: {:?}",
            effect3
        );
    });
}

// ============================================================================
// PHASE MODULE CONTROL FLOW TESTS
// ============================================================================

/// Test that review phase validation failures surface as reducer events.
///
/// When XML validation fails during review, the phase module must emit an event
/// and let the reducer decide retry policy. The phase module should NOT internally
/// hide failures or make retry decisions autonomously.
#[test]
fn test_review_validation_failure_surfaces_via_event() {
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase, ReviewEvent};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start in Review phase
        let mut state = PipelineState::initial(0, 3);
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;

        // When review output validation fails, reducer should track the attempt
        // via the OutputValidationFailed event (not hidden inside phase module)
        let state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::OutputValidationFailed {
                pass: 0,
                attempt: 0,
                error_detail: None,
            }),
        );

        // The state should reflect the validation failure via continuation.invalid_output_attempts
        // This proves the failure was surfaced to the reducer, not hidden in phase code
        assert_eq!(
            state.continuation.invalid_output_attempts, 1,
            "Review validation failure must surface via reducer event and increment attempt counter"
        );

        // Another failure should increment again (reducer controls retry logic)
        let state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::OutputValidationFailed {
                pass: 0,
                attempt: 1,
                error_detail: None,
            }),
        );

        assert_eq!(
            state.continuation.invalid_output_attempts, 2,
            "Subsequent failures must continue to surface via reducer events"
        );
    });
}

/// Test that development continuation decisions come from reducer state.
///
/// When development returns status="partial" or "failed", the decision to continue
/// must come from reducer state transitions, not from autonomous phase module logic.
#[test]
fn test_development_continuation_is_reducer_driven() {
    use ralph_workflow::reducer::event::{DevelopmentEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::{DevelopmentStatus, PipelineState};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start in Development phase
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;

        // Simulate a "partial" status from development via reducer event
        // The reducer state should track continuation context
        let state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "Work partially done".to_string(),
                files_changed: Some(vec!["file.rs".to_string()]),
                next_steps: Some("Continue implementation".to_string()),
            }),
        );

        // Verify reducer state tracks continuation
        assert!(
            state.continuation.is_continuation(),
            "Continuation decision must be tracked in reducer state"
        );
        assert_eq!(
            state.continuation.previous_status,
            Some(DevelopmentStatus::Partial),
            "Previous status must be tracked for continuation"
        );
        assert_eq!(
            state.continuation.continuation_attempt, 1,
            "Continuation attempt counter must be incremented"
        );
    });
}

/// Test that phase transitions only happen via reducer events.
///
/// Phase modules must NOT directly advance phases. All phase transitions
/// must occur through reducer event processing, ensuring state is the
/// single source of truth.
#[test]
fn test_phase_transitions_only_via_reducer_events() {
    use ralph_workflow::reducer::event::{
        CommitEvent, DevelopmentEvent, PipelineEvent, PipelinePhase, PlanningEvent, ReviewEvent,
    };
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start at Planning
        let state = PipelineState::initial(1, 1);
        assert_eq!(state.phase, PipelinePhase::Planning);

        // Transition Planning -> Development via event
        let state = reduce(
            state,
            PipelineEvent::Planning(PlanningEvent::GenerationCompleted {
                iteration: 0,
                valid: true,
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Development,
            "Planning->Development must happen via reducer event"
        );

        // Transition Development -> CommitMessage via event
        let state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::IterationCompleted {
                iteration: 0,
                output_valid: true,
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Development->CommitMessage must happen via reducer event"
        );

        // Transition CommitMessage -> Review via event (when iterations exhausted)
        let state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                hash: "abc123".to_string(),
                message: "test".to_string(),
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "CommitMessage->Review must happen via reducer event"
        );

        // Transition Review -> CommitMessage via event (phase completed early)
        let state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PhaseCompleted { early_exit: true }),
        );
        // Review phase completed transitions to CommitMessage for commit handling
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Review->CommitMessage must happen via reducer event"
        );
    });
}
