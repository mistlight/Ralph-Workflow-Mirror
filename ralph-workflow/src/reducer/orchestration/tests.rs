use super::*;
use crate::reducer::state::AgentChainState;
use crate::reducer::{reduce, PipelineEvent};

fn create_test_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

#[test]
fn test_interrupted_phase_saves_checkpoint_before_abort_loop() {
    // Regression: if agent chain exhaustion triggers AbortPipeline and the reducer
    // transitions to Interrupted, orchestration must not keep returning AbortPipeline.
    // It should drive a checkpoint save so the event loop can mark completion.
    let state = PipelineState {
        phase: PipelinePhase::Interrupted,
        checkpoint_saved_count: 0,
        agent_chain: AgentChainState::initial()
            .with_agents(vec!["a".to_string()], vec![vec![]], AgentRole::Reviewer)
            .with_max_cycles(0),
        ..PipelineState::initial(0, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt
        }
    ));
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
    assert!(matches!(effect, Effect::PreparePlanningPrompt { .. }));
}

#[test]
fn test_planning_phase_emits_single_task_effect() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        context_cleaned: true,
        iteration: 0,
        total_iterations: 3,
        agent_chain: PipelineState::initial(3, 0).agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);

    assert!(
        matches!(effect, Effect::PreparePlanningPrompt { .. }),
        "Planning should emit PreparePlanningPrompt, got {:?}",
        effect
    );
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

    // Orchestration should now return PrepareDevelopmentContext
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::PrepareDevelopmentContext { .. }),
        "Expected PrepareDevelopmentContext, got {:?}",
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
    assert!(matches!(effect, Effect::PrepareDevelopmentContext { .. }));
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
            Effect::PreparePlanningPrompt { iteration, .. } => {
                state = reduce(state, PipelineEvent::planning_prompt_prepared(iteration));
            }
            Effect::CleanupPlanningXml { iteration } => {
                state = reduce(state, PipelineEvent::planning_xml_cleaned(iteration));
            }
            Effect::InvokePlanningAgent { iteration } => {
                state = reduce(state, PipelineEvent::planning_agent_invoked(iteration));
            }
            Effect::ExtractPlanningXml { iteration } => {
                state = reduce(state, PipelineEvent::planning_xml_extracted(iteration));
            }
            Effect::ValidatePlanningXml { iteration } => {
                state = reduce(
                    state,
                    PipelineEvent::planning_xml_validated(
                        iteration,
                        true,
                        Some("# Plan\n\n- step\n".to_string()),
                    ),
                );
            }
            Effect::WritePlanningMarkdown { iteration } => {
                state = reduce(state, PipelineEvent::planning_markdown_written(iteration));
            }
            Effect::ArchivePlanningXml { iteration } => {
                state = reduce(state, PipelineEvent::planning_xml_archived(iteration));
            }
            Effect::ApplyPlanningOutcome { iteration, valid } => {
                state = reduce(
                    state,
                    PipelineEvent::plan_generation_completed(iteration, valid),
                );
            }
            Effect::PrepareDevelopmentContext { iteration } => {
                state = reduce(
                    state,
                    PipelineEvent::development_context_prepared(iteration),
                );
            }
            Effect::PrepareDevelopmentPrompt { iteration, .. } => {
                state = reduce(state, PipelineEvent::development_prompt_prepared(iteration));
            }
            Effect::CleanupDevelopmentXml { iteration } => {
                state = reduce(state, PipelineEvent::development_xml_cleaned(iteration));
            }
            Effect::InvokeDevelopmentAgent { iteration } => {
                state = reduce(state, PipelineEvent::development_agent_invoked(iteration));
            }
            Effect::ExtractDevelopmentXml { iteration } => {
                state = reduce(state, PipelineEvent::development_xml_extracted(iteration));
            }
            Effect::ValidateDevelopmentXml { iteration } => {
                state = reduce(
                    state,
                    PipelineEvent::development_xml_validated(
                        iteration,
                        crate::reducer::state::DevelopmentStatus::Completed,
                        "done".to_string(),
                        None,
                        None,
                    ),
                );
            }
            Effect::ArchiveDevelopmentXml { iteration } => {
                state = reduce(state, PipelineEvent::development_xml_archived(iteration));
            }
            Effect::ApplyDevelopmentOutcome { iteration } => {
                iterations_run.push(iteration);
                state = reduce(
                    state,
                    PipelineEvent::development_iteration_completed(iteration, true),
                );
            }
            Effect::CheckCommitDiff => {
                state = reduce(state, PipelineEvent::commit_diff_prepared(false));
            }
            Effect::PrepareCommitPrompt { .. } => {
                state = reduce(state, PipelineEvent::commit_generation_started());
                state = reduce(state, PipelineEvent::commit_prompt_prepared(1));
            }
            Effect::CleanupCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_cleaned(1));
            }
            Effect::InvokeCommitAgent => {
                state = reduce(state, PipelineEvent::commit_agent_invoked(1));
            }
            Effect::ExtractCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_extracted(1));
            }
            Effect::ValidateCommitXml => {
                state = reduce(
                    state,
                    PipelineEvent::commit_xml_validated("test".to_string(), 1),
                );
            }
            Effect::ApplyCommitMessageOutcome => {
                state = reduce(
                    state,
                    PipelineEvent::commit_message_generated("test".to_string(), 1),
                );
            }
            Effect::ArchiveCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_archived(1));
            }
            Effect::CreateCommit { .. } => {
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
    assert!(matches!(effect, Effect::PrepareReviewContext { pass: 1 }));
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

    // Initially should begin review chain
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::PrepareReviewContext { pass: 0 }),
        "Expected PrepareReviewContext, got {:?}",
        effect
    );

    // Simulate review completing with issues found
    state = reduce(state, PipelineEvent::review_completed(0, true));

    // State should now have issues_found flag set
    assert!(
        state.review_issues_found,
        "review_issues_found should be true"
    );

    // With a populated Reviewer chain, orchestration should begin the fix chain.
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::PrepareFixPrompt { pass: 0, .. }),
        "Expected PrepareFixPrompt after issues found, got {:?}",
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
    // Test the COMPLETE flow: Planning -> Development -> Review -> Fix -> Commit -> FinalValidation
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
            Effect::CleanupPlanningXml { iteration } => {
                state = reduce(state, PipelineEvent::planning_xml_cleaned(iteration));
            }
            Effect::PreparePlanningPrompt { iteration, .. } => {
                state = reduce(state, PipelineEvent::planning_prompt_prepared(iteration));
            }
            Effect::InvokePlanningAgent { iteration } => {
                state = reduce(state, PipelineEvent::planning_agent_invoked(iteration));
            }
            Effect::ExtractPlanningXml { iteration } => {
                state = reduce(state, PipelineEvent::planning_xml_extracted(iteration));
            }
            Effect::ValidatePlanningXml { iteration } => {
                state = reduce(
                    state,
                    PipelineEvent::planning_xml_validated(
                        iteration,
                        true,
                        Some("# Plan\n\n- step\n".to_string()),
                    ),
                );
            }
            Effect::WritePlanningMarkdown { iteration } => {
                state = reduce(state, PipelineEvent::planning_markdown_written(iteration));
            }
            Effect::ArchivePlanningXml { iteration } => {
                state = reduce(state, PipelineEvent::planning_xml_archived(iteration));
            }
            Effect::ApplyPlanningOutcome { iteration, valid } => {
                state = reduce(
                    state,
                    PipelineEvent::plan_generation_completed(iteration, valid),
                );
            }
            Effect::PrepareDevelopmentContext { iteration } => {
                state = reduce(
                    state,
                    PipelineEvent::development_context_prepared(iteration),
                );
            }
            Effect::CleanupDevelopmentXml { iteration } => {
                state = reduce(state, PipelineEvent::development_xml_cleaned(iteration));
            }
            Effect::PrepareDevelopmentPrompt { iteration, .. } => {
                state = reduce(state, PipelineEvent::development_prompt_prepared(iteration));
            }
            Effect::InvokeDevelopmentAgent { iteration } => {
                state = reduce(state, PipelineEvent::development_agent_invoked(iteration));
            }
            Effect::ExtractDevelopmentXml { iteration } => {
                state = reduce(state, PipelineEvent::development_xml_extracted(iteration));
            }
            Effect::ValidateDevelopmentXml { iteration } => {
                state = reduce(
                    state,
                    PipelineEvent::development_xml_validated(
                        iteration,
                        crate::reducer::state::DevelopmentStatus::Completed,
                        "done".to_string(),
                        None,
                        None,
                    ),
                );
            }
            Effect::ArchiveDevelopmentXml { iteration } => {
                state = reduce(state, PipelineEvent::development_xml_archived(iteration));
            }
            Effect::ApplyDevelopmentOutcome { iteration } => {
                iterations_run.push(iteration);
                state = reduce(
                    state,
                    PipelineEvent::development_iteration_completed(iteration, true),
                );
            }
            Effect::PrepareReviewContext { pass } => {
                review_passes_run.push(pass);
                state = reduce(state, PipelineEvent::review_context_prepared(pass));
            }
            Effect::PrepareReviewPrompt { pass, .. } => {
                state = reduce(state, PipelineEvent::review_prompt_prepared(pass));
            }
            Effect::CleanupReviewIssuesXml { pass } => {
                state = reduce(state, PipelineEvent::review_issues_xml_cleaned(pass));
            }
            Effect::InvokeReviewAgent { pass } => {
                state = reduce(state, PipelineEvent::review_agent_invoked(pass));
            }
            Effect::ExtractReviewIssuesXml { pass } => {
                state = reduce(state, PipelineEvent::review_issues_xml_extracted(pass));
            }
            Effect::ValidateReviewIssuesXml { pass } => {
                // Simulate finding issues
                state = reduce(
                    state,
                    PipelineEvent::review_issues_xml_validated(
                        pass,
                        true,
                        false,
                        vec!["issue".to_string()],
                        None,
                    ),
                );
            }
            Effect::WriteIssuesMarkdown { pass } => {
                state = reduce(state, PipelineEvent::review_issues_markdown_written(pass));
            }
            Effect::ExtractReviewIssueSnippets { pass } => {
                state = reduce(state, PipelineEvent::review_issue_snippets_extracted(pass));
            }
            Effect::ArchiveReviewIssuesXml { pass } => {
                state = reduce(state, PipelineEvent::review_issues_xml_archived(pass));
            }
            Effect::ApplyReviewOutcome {
                pass,
                issues_found,
                clean_no_issues,
            } => {
                state = reduce(
                    state,
                    if clean_no_issues {
                        PipelineEvent::review_pass_completed_clean(pass)
                    } else {
                        PipelineEvent::review_completed(pass, issues_found)
                    },
                );
            }

            Effect::CleanupFixResultXml { pass } => {
                state = reduce(state, PipelineEvent::fix_result_xml_cleaned(pass));
            }
            Effect::PrepareFixPrompt { pass, .. } => {
                state = reduce(state, PipelineEvent::fix_prompt_prepared(pass));
            }
            Effect::InvokeFixAgent { pass } => {
                state = reduce(state, PipelineEvent::fix_agent_invoked(pass));
            }
            Effect::ExtractFixResultXml { pass } => {
                state = reduce(state, PipelineEvent::fix_result_xml_extracted(pass));
            }
            Effect::ValidateFixResultXml { pass } => {
                state = reduce(
                    state,
                    PipelineEvent::fix_result_xml_validated(
                        pass,
                        crate::reducer::state::FixStatus::AllIssuesAddressed,
                        None,
                    ),
                );
            }
            Effect::ArchiveFixResultXml { pass } => {
                state = reduce(state, PipelineEvent::fix_result_xml_archived(pass));
            }
            Effect::ApplyFixOutcome { pass } => {
                state = reduce(state, PipelineEvent::fix_attempt_completed(pass, true));
            }
            Effect::CheckCommitDiff => {
                state = reduce(state, PipelineEvent::commit_diff_prepared(false));
            }
            Effect::PrepareCommitPrompt { .. } => {
                state = reduce(state, PipelineEvent::commit_generation_started());
                state = reduce(state, PipelineEvent::commit_prompt_prepared(1));
            }
            Effect::CleanupCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_cleaned(1));
            }
            Effect::InvokeCommitAgent => {
                state = reduce(state, PipelineEvent::commit_agent_invoked(1));
            }
            Effect::ExtractCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_extracted(1));
            }
            Effect::ValidateCommitXml => {
                state = reduce(
                    state,
                    PipelineEvent::commit_xml_validated("test commit".to_string(), 1),
                );
            }
            Effect::ApplyCommitMessageOutcome => {
                state = reduce(
                    state,
                    PipelineEvent::commit_message_generated("test commit".to_string(), 1),
                );
            }
            Effect::ArchiveCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_archived(1));
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
            Effect::PrepareReviewContext { pass } => {
                review_passes.push(pass);
                // Simulate full clean pass
                state = reduce(state, PipelineEvent::review_context_prepared(pass));
                state = reduce(state, PipelineEvent::review_prompt_prepared(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_cleaned(pass));
                state = reduce(state, PipelineEvent::review_agent_invoked(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_extracted(pass));
                state = reduce(
                    state,
                    PipelineEvent::review_issues_xml_validated(
                        pass,
                        false,
                        true,
                        Vec::new(),
                        Some("ok".to_string()),
                    ),
                );
                state = reduce(state, PipelineEvent::review_issues_markdown_written(pass));
                state = reduce(state, PipelineEvent::review_issue_snippets_extracted(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_archived(pass));
                state = reduce(state, PipelineEvent::review_pass_completed_clean(pass));
            }
            Effect::CheckCommitDiff => {
                state = reduce(state, PipelineEvent::commit_diff_prepared(false));
            }
            Effect::PrepareCommitPrompt { .. } => {
                state = reduce(state, PipelineEvent::commit_generation_started());
                state = reduce(state, PipelineEvent::commit_prompt_prepared(1));
            }
            Effect::CleanupCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_cleaned(1));
            }
            Effect::InvokeCommitAgent => {
                state = reduce(state, PipelineEvent::commit_agent_invoked(1));
            }
            Effect::ExtractCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_extracted(1));
            }
            Effect::ValidateCommitXml => {
                state = reduce(
                    state,
                    PipelineEvent::commit_xml_validated("test".to_string(), 1),
                );
            }
            Effect::ApplyCommitMessageOutcome => {
                state = reduce(
                    state,
                    PipelineEvent::commit_message_generated("test".to_string(), 1),
                );
            }
            Effect::ArchiveCommitXml => {
                state = reduce(state, PipelineEvent::commit_xml_archived(1));
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
            Effect::PrepareReviewContext { pass } => {
                passes_run.push(pass);
                state = reduce(state, PipelineEvent::review_context_prepared(pass));
                state = reduce(state, PipelineEvent::review_prompt_prepared(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_cleaned(pass));
                state = reduce(state, PipelineEvent::review_agent_invoked(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_extracted(pass));
                state = reduce(
                    state,
                    PipelineEvent::review_issues_xml_validated(
                        pass,
                        false,
                        true,
                        Vec::new(),
                        Some("ok".to_string()),
                    ),
                );
                state = reduce(state, PipelineEvent::review_issues_markdown_written(pass));
                state = reduce(state, PipelineEvent::review_issue_snippets_extracted(pass));
                state = reduce(state, PipelineEvent::review_issues_xml_archived(pass));
                state = reduce(state, PipelineEvent::review_pass_completed_clean(pass));
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

    // Begin review chain
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::PrepareReviewContext { pass: 0 }));

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

    // Should begin next review chain (pass 1), NOT fix chain
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::PrepareReviewContext { pass: 1 }),
        "Expected PrepareReviewContext pass 1 when no issues, got {:?}",
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
    // With initialized agent chain and diff prepared, commit phase should prepare prompt
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::NotStarted,
        commit_diff_prepared: true, // Diff already done
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    // Since diff is prepared but prompt is not, next step is to prepare prompt
    assert!(matches!(
        effect,
        Effect::PrepareCommitPrompt {
            prompt_mode: PromptMode::Normal
        }
    ));
}

#[test]
fn test_determine_effect_commit_message_ignores_stale_validated_outcome() {
    // Stale outcome (attempt 1) should be ignored when current attempt is 2
    // Should proceed to prepare prompt instead of applying stale outcome
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 2,
            max_attempts: 5,
        },
        commit_diff_prepared: true, // Diff already done
        commit_prompt_prepared: false,
        commit_agent_invoked: false,
        commit_xml_extracted: false,
        commit_validated_outcome: Some(crate::reducer::state::CommitValidatedOutcome {
            attempt: 1, // Stale: from attempt 1, not current attempt 2
            message: Some("stale message".to_string()),
            reason: None,
        }),
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);
    // Stale outcome is ignored, so proceeds to prepare prompt
    assert!(matches!(
        effect,
        Effect::PrepareCommitPrompt {
            prompt_mode: PromptMode::Normal
        }
    ));
}

#[test]
fn test_determine_effect_commit_message_generated() {
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generated {
            message: "test commit message".to_string(),
        },
        commit_xml_archived: true,
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
