// Retry safety tests.
//
// These tests ensure that when an agent invocation fails (timeout, etc.),
// the next retry attempt re-cleans the phase-specific XML output file.

use super::*;

#[test]
fn test_planning_timeout_retry_recleans_plan_xml_before_reinvoke() {
    let pass = 0;
    let mut state = PipelineState {
        phase: PipelinePhase::Planning,
        gitignore_entries_ensured: true,
        context_cleaned: true,
        iteration: pass,
        planning_prompt_prepared_iteration: Some(pass),
        planning_xml_cleaned_iteration: Some(pass),
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    state = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Developer, "claude".to_string()),
    );
    assert!(
        matches!(
            determine_next_effect(&state),
            Effect::PreparePlanningPrompt {
                prompt_mode: PromptMode::SameAgentRetry,
                ..
            }
        ),
        "TimedOut should trigger same-agent retry prompt"
    );

    state = reduce(state, PipelineEvent::planning_prompt_prepared(pass));
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::CleanupPlanningXml { iteration } if iteration == pass),
        "Retry should re-clean plan.xml before reinvoking agent, got {:?}",
        effect
    );
}

#[test]
fn test_development_timeout_retry_recleans_dev_xml_before_reinvoke() {
    let iteration = 0;
    let mut state = PipelineState {
        phase: PipelinePhase::Development,
        iteration,
        total_iterations: 1,
        development_context_prepared_iteration: Some(iteration),
        development_prompt_prepared_iteration: Some(iteration),
        development_xml_cleaned_iteration: Some(iteration),
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    state = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Developer, "claude".to_string()),
    );
    assert!(
        matches!(
            determine_next_effect(&state),
            Effect::PrepareDevelopmentPrompt {
                prompt_mode: PromptMode::SameAgentRetry,
                ..
            }
        ),
        "TimedOut should trigger same-agent retry prompt"
    );

    state = reduce(state, PipelineEvent::development_prompt_prepared(iteration));
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::CleanupDevelopmentXml { iteration: i } if i == iteration),
        "Retry should re-clean development_result.xml before reinvoking agent, got {:?}",
        effect
    );
}

#[test]
fn test_review_timeout_retry_recleans_issues_xml_before_reinvoke() {
    let pass = 0;
    let mut state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: pass,
        total_reviewer_passes: 1,
        review_context_prepared_pass: Some(pass),
        review_prompt_prepared_pass: Some(pass),
        review_issues_xml_cleaned_pass: Some(pass),
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["codex".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..create_test_state()
    };

    state = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Reviewer, "codex".to_string()),
    );
    assert!(
        matches!(
            determine_next_effect(&state),
            Effect::PrepareReviewPrompt {
                prompt_mode: PromptMode::SameAgentRetry,
                ..
            }
        ),
        "TimedOut should trigger same-agent retry prompt"
    );

    state = reduce(state, PipelineEvent::review_prompt_prepared(pass));
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::CleanupReviewIssuesXml { pass: p } if p == pass),
        "Retry should re-clean issues.xml before reinvoking agent, got {:?}",
        effect
    );
}

#[test]
fn test_fix_timeout_retry_recleans_fix_xml_before_reinvoke() {
    let pass = 0;
    let mut state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: pass,
        total_reviewer_passes: 1,
        review_issues_found: true,
        fix_prompt_prepared_pass: Some(pass),
        fix_result_xml_cleaned_pass: Some(pass),
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["codex".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        ),
        ..create_test_state()
    };

    state = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Reviewer, "codex".to_string()),
    );
    assert!(
        matches!(
            determine_next_effect(&state),
            Effect::PrepareFixPrompt {
                prompt_mode: PromptMode::SameAgentRetry,
                ..
            }
        ),
        "TimedOut should trigger same-agent retry prompt"
    );

    state = reduce(state, PipelineEvent::fix_prompt_prepared(pass));
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::CleanupFixResultXml { pass: p } if p == pass),
        "Retry should re-clean fix_result.xml before reinvoking agent, got {:?}",
        effect
    );
}

#[test]
fn test_commit_timeout_retry_recleans_commit_xml_before_reinvoke() {
    let mut state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS,
        },
        commit_diff_prepared: true,
        commit_prompt_prepared: true,
        commit_xml_cleaned: true,
        commit_agent_invoked: false,
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };

    state = reduce(
        state,
        PipelineEvent::agent_timed_out(AgentRole::Commit, "commit-agent".to_string()),
    );
    assert!(
        matches!(
            determine_next_effect(&state),
            Effect::PrepareCommitPrompt {
                prompt_mode: PromptMode::SameAgentRetry
            }
        ),
        "TimedOut should trigger same-agent retry prompt"
    );

    state = reduce(state, PipelineEvent::commit_prompt_prepared(1));
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::CleanupCommitXml),
        "Retry should re-clean commit_message.xml before reinvoking agent, got {:?}",
        effect
    );
}
