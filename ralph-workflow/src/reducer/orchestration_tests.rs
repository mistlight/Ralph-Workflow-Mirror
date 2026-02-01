//! Comprehensive orchestration tests for pipeline phase transitions.
//!
//! This module contains systematic tests for ALL phase transitions and state management
//! in the reducer-based pipeline architecture. These tests verify that:
//! - Each phase correctly determines the next effect based on state
//! - State transitions happen correctly when events are applied
//! - Iteration/pass counts are respected (no off-by-one errors)
//! - Phase transitions occur at the right time
//! - The complete pipeline flows from Planning → Development → Review → Commit → Complete

use super::orchestration::determine_next_effect;
use super::state_reduction::reduce;
use crate::agents::AgentRole;
use crate::reducer::effect::Effect;
use crate::reducer::event::{PipelineEvent, PipelinePhase};
use crate::reducer::state::PipelineState;

fn create_test_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

// ============================================================================
// Review Phase Pre-Refactor Guard
// ============================================================================

#[test]
fn test_review_phase_emits_initialize_chain_then_prepare_review_context() {
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    // No chain yet => InitializeAgentChain
    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::InitializeAgentChain {
            role: AgentRole::Reviewer
        }
    ));

    // After chain initialized => PrepareReviewContext
    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareReviewContext { pass: 0 }));
}

#[test]
fn test_review_phase_emits_prepare_review_context_after_chain_initialized() {
    // This test is the first step in the single-task-effects refactor.
    // Once the reviewer chain is initialized, the reducer should emit a *single-task*
    // context preparation effect, not a macro "run review" effect.
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareReviewContext { pass: 0 }));
}

#[test]
fn test_review_phase_emits_invoke_review_agent_after_prompt_prepared() {
    // Single-task effect chain: PrepareReviewContext -> PrepareReviewPrompt -> InvokeReviewAgent
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let state = reduce(state, PipelineEvent::review_context_prepared(0));
    let state = reduce(state, PipelineEvent::review_prompt_prepared(0));

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::InvokeReviewAgent { pass: 0 }));
}

#[test]
fn test_review_phase_emits_extract_review_issues_xml_after_agent_invoked() {
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let state = reduce(state, PipelineEvent::review_context_prepared(0));
    let state = reduce(state, PipelineEvent::review_prompt_prepared(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ExtractReviewIssuesXml { pass: 0 }));
}

#[test]
fn test_review_phase_emits_validate_review_issues_xml_after_extracted() {
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let state = reduce(state, PipelineEvent::review_context_prepared(0));
    let state = reduce(state, PipelineEvent::review_prompt_prepared(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));

    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::ValidateReviewIssuesXml { pass: 0 }
    ));
}

#[test]
fn test_review_phase_emits_write_issues_markdown_after_validated() {
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let state = reduce(state, PipelineEvent::review_context_prepared(0));
    let state = reduce(state, PipelineEvent::review_prompt_prepared(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));
    let state = reduce(
        state,
        PipelineEvent::review_issues_xml_validated(0, false, true),
    );

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::WriteIssuesMarkdown { pass: 0 }));
}

#[test]
fn test_review_phase_emits_archive_issues_xml_after_markdown_written() {
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let state = reduce(state, PipelineEvent::review_context_prepared(0));
    let state = reduce(state, PipelineEvent::review_prompt_prepared(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));
    let state = reduce(
        state,
        PipelineEvent::review_issues_xml_validated(0, false, true),
    );
    let state = reduce(state, PipelineEvent::review_issues_markdown_written(0));

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ArchiveReviewIssuesXml { pass: 0 }));
}

#[test]
fn test_review_phase_emits_apply_review_outcome_after_issues_xml_archived() {
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let state = reduce(state, PipelineEvent::review_context_prepared(0));
    let state = reduce(state, PipelineEvent::review_prompt_prepared(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));
    let state = reduce(
        state,
        PipelineEvent::review_issues_xml_validated(0, false, true),
    );
    let state = reduce(state, PipelineEvent::review_issues_markdown_written(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_archived(0));

    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::ApplyReviewOutcome {
            pass: 0,
            issues_found: false,
            clean_no_issues: true
        }
    ));
}

#[test]
fn test_review_with_issues_emits_prepare_fix_prompt() {
    // When review finds issues, the pipeline should enter the fix chain as single-task effects.
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["mock".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(1, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareFixPrompt { pass: 0 }));
}

#[test]
fn test_fix_chain_emits_invoke_fix_agent_after_fix_prompt_prepared() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        fix_prompt_prepared_pass: Some(0),
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["mock".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(1, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::InvokeFixAgent { pass: 0 }));
}

#[test]
fn test_fix_chain_emits_extract_fix_result_xml_after_fix_agent_invoked() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        fix_prompt_prepared_pass: Some(0),
        fix_agent_invoked_pass: Some(0),
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["mock".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(1, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ExtractFixResultXml { pass: 0 }));
}

#[test]
fn test_fix_chain_emits_validate_fix_result_xml_after_extracted() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        fix_prompt_prepared_pass: Some(0),
        fix_agent_invoked_pass: Some(0),
        fix_result_xml_extracted_pass: Some(0),
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["mock".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(1, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ValidateFixResultXml { pass: 0 }));
}

#[test]
fn test_fix_chain_applies_all_issues_addressed_to_fix_attempt_completed() {
    // Given: fix outcome is complete
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        fix_prompt_prepared_pass: Some(0),
        fix_agent_invoked_pass: Some(0),
        fix_result_xml_extracted_pass: Some(0),
        fix_validated_outcome: Some(crate::reducer::state::FixValidatedOutcome {
            pass: 0,
            status: crate::reducer::state::FixStatus::AllIssuesAddressed,
            summary: Some("ok".to_string()),
        }),
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["mock".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(1, 1)
    };

    // When: orchestration applies fix outcome
    assert!(matches!(
        determine_next_effect(&state),
        Effect::ApplyFixOutcome { pass: 0 }
    ));

    // Then: handler should emit the existing completion event used by reducer today
    // (until fix outcome is fully refactored).
    let handler_event = crate::reducer::mock_effect_handler::MockEffectHandler::new(state)
        .execute_mock(Effect::ApplyFixOutcome { pass: 0 })
        .event;

    assert!(matches!(
        handler_event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixAttemptCompleted {
            pass: 0,
            ..
        })
    ));
}

#[test]
fn test_fix_chain_emits_archive_fix_result_xml_after_validated() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        fix_prompt_prepared_pass: Some(0),
        fix_agent_invoked_pass: Some(0),
        fix_result_xml_extracted_pass: Some(0),
        fix_validated_outcome: Some(crate::reducer::state::FixValidatedOutcome {
            pass: 0,
            status: crate::reducer::state::FixStatus::AllIssuesAddressed,
            summary: None,
        }),
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["mock".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(1, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ArchiveFixResultXml { pass: 0 }));
}

#[test]
fn test_fix_chain_emits_apply_fix_outcome_after_fix_result_xml_archived() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        fix_prompt_prepared_pass: Some(0),
        fix_agent_invoked_pass: Some(0),
        fix_result_xml_extracted_pass: Some(0),
        fix_validated_outcome: Some(crate::reducer::state::FixValidatedOutcome {
            pass: 0,
            status: crate::reducer::state::FixStatus::AllIssuesAddressed,
            summary: None,
        }),
        fix_result_xml_archived_pass: Some(0),
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["mock".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(1, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ApplyFixOutcome { pass: 0 }));
}

#[test]
fn test_review_phase_emits_prepare_review_prompt_after_context_prepared() {
    let mut state = PipelineState::initial(1, 1);
    state.phase = PipelinePhase::Review;

    let state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec!["mock".to_string()],
            1,
            0,
            1.0,
            0,
        ),
    );
    let state = reduce(state, PipelineEvent::review_context_prepared(0));

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareReviewPrompt { pass: 0 }));
}

// ============================================================================
// Planning Phase Tests
// ============================================================================

#[test]
fn test_planning_initializes_agent_chain_when_empty() {
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
fn test_planning_generates_plan_when_agents_ready() {
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
fn test_planning_transitions_to_development_after_completion() {
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

    // Plan generation completes
    state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

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

// ============================================================================
// Development Phase Tests
// ============================================================================

#[test]
fn test_development_runs_exactly_n_iterations() {
    // When total_iterations=5, should run iterations 0,1,2,3,4 (5 total)
    let mut state = PipelineState::initial(5, 0);
    state.agent_chain = state.agent_chain.with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        AgentRole::Developer,
    );

    let mut iterations_run = Vec::new();

    // Simulate the development phase (includes CommitMessage after each iteration)
    while state.phase == PipelinePhase::Planning
        || state.phase == PipelinePhase::Development
        || state.phase == PipelinePhase::CommitMessage
    {
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
            Effect::GenerateCommitMessage => {
                state = reduce(state, PipelineEvent::commit_generation_started());
                state = reduce(
                    state,
                    PipelineEvent::commit_created(
                        format!("abc{}", iterations_run.len()),
                        "test".to_string(),
                    ),
                );
            }
            Effect::SaveCheckpoint { .. } => break,
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
fn test_development_with_agent_chain_exhaustion() {
    let mut chain = PipelineState::initial(5, 2)
        .agent_chain
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

// ============================================================================
// Review Phase Tests
// ============================================================================

#[test]
fn test_review_runs_exactly_n_passes() {
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
            Effect::PrepareReviewContext { pass } => {
                passes_run.push(pass);
                // Simulate a full clean pass through the single-task review chain.
                state = reduce(state, PipelineEvent::review_context_prepared(pass));
                state = reduce(state, PipelineEvent::review_prompt_prepared(pass));
                state = reduce(state, PipelineEvent::review_agent_invoked(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_extracted(pass));
                state = reduce(
                    state,
                    PipelineEvent::review_issues_xml_validated(pass, false, true),
                );
                state = reduce(state, PipelineEvent::review_issues_markdown_written(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_archived(pass));
                state = reduce(state, PipelineEvent::review_pass_completed_clean(pass));
            }
            Effect::SaveCheckpoint { .. } => break,
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
fn test_review_triggers_fix_when_issues_found() {
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

    // Initially should begin review chain
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareReviewContext { pass: 0 }));

    // Review completes with issues found
    state = reduce(state, PipelineEvent::review_completed(0, true));

    assert!(state.review_issues_found);

    // With a populated Reviewer chain, orchestration should begin the fix chain.
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareFixPrompt { pass: 0 }));

    // Fix completes - now transitions to CommitMessage phase
    state = reduce(state, PipelineEvent::fix_attempt_completed(0, true));

    assert!(!state.review_issues_found);
    assert_eq!(state.phase, PipelinePhase::CommitMessage);
    assert_eq!(
        state.previous_phase,
        Some(PipelinePhase::Review),
        "Should remember we came from Review"
    );
    // reviewer_pass stays at 0 until CommitCreated
    assert_eq!(state.reviewer_pass, 0);

    // Generate commit message
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::GenerateCommitMessage));
    state = reduce(
        state,
        PipelineEvent::commit_message_generated("fix: address review issues".to_string(), 1),
    );

    // Create commit
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::CreateCommit { .. }));
    state = reduce(
        state,
        PipelineEvent::commit_created(
            "abc123".to_string(),
            "fix: address review issues".to_string(),
        ),
    );

    // Now we're back in Review with incremented pass
    assert_eq!(state.reviewer_pass, 1);
    assert_eq!(state.phase, PipelinePhase::Review);
}

#[test]
fn test_review_skips_fix_when_no_issues() {
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

    // Begin review chain
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareReviewContext { pass: 0 }));

    // Review completes with NO issues
    state = reduce(state, PipelineEvent::review_completed(0, false));

    assert!(!state.review_issues_found);
    assert_eq!(
        state.reviewer_pass, 1,
        "Should increment to next pass when no issues"
    );

    // Should begin next review chain (pass 1), NOT fix chain
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::PrepareReviewContext { pass: 1 }),
        "Expected PrepareReviewContext pass 1, got {:?}",
        effect
    );
}

// ============================================================================
// Commit Phase Tests
// ============================================================================

#[test]
fn test_commit_empty_chain_initializes_agent_chain() {
    // When agent chain is empty, commit phase should request initialization
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::NotStarted,
        agent_chain: crate::reducer::state::AgentChainState::initial(),
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
fn test_commit_not_started_generates_message() {
    // With initialized agent chain, commit phase should generate message
    use crate::reducer::state::AgentChainState;
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::NotStarted,
        agent_chain: AgentChainState::initial().with_agents(
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
fn test_commit_generated_creates_commit() {
    use crate::reducer::state::AgentChainState;
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::Generated {
            message: "test commit message".to_string(),
        },
        agent_chain: AgentChainState::initial().with_agents(
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
fn test_commit_created_transitions_to_final_validation() {
    let mut state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::Generated {
            message: "test".to_string(),
        },
        ..create_test_state()
    };

    // Create commit
    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test".to_string()),
    );

    assert_eq!(state.phase, PipelinePhase::FinalValidation);
    assert!(matches!(
        state.commit,
        crate::reducer::state::CommitState::Committed { .. }
    ));
}

// ============================================================================
// Complete Pipeline Flow Tests
// ============================================================================

#[test]
fn test_complete_pipeline_flow() {
    // Test Planning → Development → Review → Fix → Commit → FinalValidation → Complete
    let mut state = PipelineState::initial(2, 1); // 2 dev iterations, 1 review pass
    state.agent_chain = state.agent_chain.with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        AgentRole::Developer,
    );

    let mut phase_sequence = Vec::new();
    let mut iterations_run = Vec::new();
    let mut review_passes_run = Vec::new();

    let max_steps = 50;
    for step in 0..max_steps {
        phase_sequence.push(state.phase);
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
            Effect::CleanupContext => {
                state = reduce(state, PipelineEvent::ContextCleaned);
            }
            Effect::CleanupContinuationContext => {
                state = reduce(
                    state,
                    PipelineEvent::development_continuation_context_cleaned(),
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
            Effect::PrepareReviewContext { pass } => {
                review_passes_run.push(pass);
                // Simulate a pass that finds issues.
                state = reduce(state, PipelineEvent::review_context_prepared(pass));
                state = reduce(state, PipelineEvent::review_prompt_prepared(pass));
                state = reduce(state, PipelineEvent::review_agent_invoked(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_extracted(pass));
                state = reduce(
                    state,
                    PipelineEvent::review_issues_xml_validated(pass, true, false),
                );
                state = reduce(state, PipelineEvent::review_issues_markdown_written(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_archived(pass));
                state = reduce(state, PipelineEvent::review_completed(pass, true));
            }
            Effect::PrepareFixPrompt { pass } => {
                state = reduce(state, PipelineEvent::fix_prompt_prepared(pass));
                state = reduce(state, PipelineEvent::fix_agent_invoked(pass));
                state = reduce(state, PipelineEvent::fix_result_xml_extracted(pass));
                state = reduce(
                    state,
                    PipelineEvent::fix_result_xml_validated(
                        pass,
                        crate::reducer::state::FixStatus::AllIssuesAddressed,
                        None,
                    ),
                );
                state = reduce(state, PipelineEvent::fix_result_xml_archived(pass));
                state = reduce(state, PipelineEvent::fix_attempt_completed(pass, true));
            }
            Effect::GenerateCommitMessage => {
                state = reduce(
                    state,
                    PipelineEvent::commit_message_generated("test commit".to_string(), 1),
                );
            }
            Effect::CreateCommit { .. } => {
                state = reduce(
                    state,
                    PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
                );
            }
            Effect::ValidateFinalState => {
                state = reduce(state, PipelineEvent::pipeline_completed());
            }
            Effect::SaveCheckpoint { .. } => {
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

    assert_eq!(iterations_run, vec![0, 1], "Should run 2 dev iterations");
    assert_eq!(review_passes_run, vec![0], "Should run 1 review pass");
    assert_eq!(state.phase, PipelinePhase::Complete);

    // Verify phase progression
    assert!(phase_sequence.contains(&PipelinePhase::Planning));
    assert!(phase_sequence.contains(&PipelinePhase::Development));
    assert!(phase_sequence.contains(&PipelinePhase::Review));
    assert!(phase_sequence.contains(&PipelinePhase::CommitMessage));
    assert!(phase_sequence.contains(&PipelinePhase::FinalValidation));
}

#[test]
fn test_pipeline_skips_planning_dev_when_zero_iterations() {
    let mut state = PipelineState::initial(0, 2); // 0 dev, 2 review
    assert_eq!(state.phase, PipelinePhase::Review);

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
            Effect::PrepareReviewContext { pass } => {
                review_passes.push(pass);
                state = reduce(state, PipelineEvent::review_context_prepared(pass));
                state = reduce(state, PipelineEvent::review_prompt_prepared(pass));
                state = reduce(state, PipelineEvent::review_agent_invoked(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_extracted(pass));
                state = reduce(
                    state,
                    PipelineEvent::review_issues_xml_validated(pass, false, true),
                );
                state = reduce(state, PipelineEvent::review_issues_markdown_written(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_archived(pass));
                state = reduce(state, PipelineEvent::review_pass_completed_clean(pass));
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

    assert_eq!(review_passes, vec![0, 1]);
    assert_eq!(state.phase, PipelinePhase::Complete);
}

#[test]
fn test_pipeline_goes_straight_to_commit_when_zero_work() {
    let state = PipelineState::initial(0, 0); // No dev, no review
    assert_eq!(
        state.phase,
        PipelinePhase::CommitMessage,
        "Should skip straight to commit when no work needed"
    );
}
