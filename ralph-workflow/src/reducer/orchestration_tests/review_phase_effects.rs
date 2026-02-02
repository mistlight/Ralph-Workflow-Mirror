// Review phase single-task effect chain tests.
//
// Tests for review phase effect emission: initialize chain, prepare context,
// prepare prompt, invoke agent, extract/validate XML, write markdown, etc.

use super::*;

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
fn test_review_phase_emits_cleanup_review_issues_xml_after_prompt_prepared() {
    // Single-task effect chain: PrepareReviewContext -> PrepareReviewPrompt -> CleanupReviewIssuesXml
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
    assert!(matches!(effect, Effect::CleanupReviewIssuesXml { pass: 0 }));
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
    let state = reduce(state, PipelineEvent::review_issues_xml_cleaned(0));
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
    let state = reduce(state, PipelineEvent::review_issues_xml_cleaned(0));
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
    let state = reduce(state, PipelineEvent::review_issues_xml_cleaned(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));
    let state = reduce(
        state,
        PipelineEvent::review_issues_xml_validated(
            0,
            false,
            true,
            Vec::new(),
            Some("ok".to_string()),
        ),
    );

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::WriteIssuesMarkdown { pass: 0 }));
}

#[test]
fn test_review_phase_emits_extract_issue_snippets_after_markdown_written() {
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
    let state = reduce(state, PipelineEvent::review_issues_xml_cleaned(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));
    let state = reduce(
        state,
        PipelineEvent::review_issues_xml_validated(
            0,
            false,
            true,
            Vec::new(),
            Some("ok".to_string()),
        ),
    );
    let state = reduce(state, PipelineEvent::review_issues_markdown_written(0));

    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::ExtractReviewIssueSnippets { pass: 0 }
    ));
}

#[test]
fn test_review_phase_emits_archive_issues_xml_after_snippets_extracted() {
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
    let state = reduce(state, PipelineEvent::review_issues_xml_cleaned(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));
    let state = reduce(
        state,
        PipelineEvent::review_issues_xml_validated(
            0,
            false,
            true,
            Vec::new(),
            Some("ok".to_string()),
        ),
    );
    let state = reduce(state, PipelineEvent::review_issues_markdown_written(0));
    let state = reduce(state, PipelineEvent::review_issue_snippets_extracted(0));

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
    let state = reduce(state, PipelineEvent::review_issues_xml_cleaned(0));
    let state = reduce(state, PipelineEvent::review_agent_invoked(0));
    let state = reduce(state, PipelineEvent::review_issues_xml_extracted(0));
    let state = reduce(
        state,
        PipelineEvent::review_issues_xml_validated(
            0,
            false,
            true,
            Vec::new(),
            Some("ok".to_string()),
        ),
    );
    let state = reduce(state, PipelineEvent::review_issues_markdown_written(0));
    let state = reduce(state, PipelineEvent::review_issue_snippets_extracted(0));
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
    assert!(matches!(
        effect,
        Effect::PrepareReviewPrompt { pass: 0, .. }
    ));
}
