// Metrics tracking tests.
//
// Verifies that RunMetrics counters increment correctly on reducer events.

use crate::agents::AgentRole;
use crate::reducer::event::{DevelopmentEvent, PipelineEvent, ReviewEvent};
use crate::reducer::state::{ArtifactType, DevelopmentStatus, PipelineState};
use crate::reducer::state_reduction::reduce;

#[test]
fn test_dev_iteration_started_increments_counter() {
    let state = PipelineState::initial(3, 0);
    assert_eq!(state.metrics.dev_iterations_started, 0);

    let event = PipelineEvent::development_iteration_started(0);
    let state = reduce(state, event);

    assert_eq!(state.metrics.dev_iterations_started, 1);
    assert_eq!(state.metrics.analysis_attempts_in_current_iteration, 0);
}

#[test]
fn test_dev_agent_invoked_increments_attempts() {
    let mut state = PipelineState::initial(3, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));

    let event = PipelineEvent::development_agent_invoked(0);
    let state = reduce(state, event);

    assert_eq!(state.metrics.dev_attempts_total, 1);
}

#[test]
fn test_analysis_agent_invoked_increments_both_counters() {
    let mut state = PipelineState::initial(3, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));

    let event = PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration: 0 });
    let state = reduce(state, event);

    assert_eq!(state.metrics.analysis_attempts_total, 1);
    assert_eq!(state.metrics.analysis_attempts_in_current_iteration, 1);

    // Next iteration resets per-iteration counter but not total
    let state = reduce(state, PipelineEvent::development_iteration_started(1));
    assert_eq!(state.metrics.analysis_attempts_total, 1);
    assert_eq!(state.metrics.analysis_attempts_in_current_iteration, 0);
}

#[test]
fn test_iteration_completed_increments_completed_counter() {
    let mut state = PipelineState::initial(3, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));

    let event = PipelineEvent::development_iteration_completed(0, true);
    let state = reduce(state, event);

    assert_eq!(state.metrics.dev_iterations_completed, 1);
}

#[test]
fn test_continuation_does_not_increment_iterations_started() {
    let mut state = PipelineState::initial(3, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));
    assert_eq!(state.metrics.dev_iterations_started, 1);

    // Trigger continuation
    let event = PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
        iteration: 0,
        status: DevelopmentStatus::Partial,
        summary: "some work done".to_string(),
        files_changed: None,
        next_steps: None,
    });
    let state = reduce(state, event);

    // Iterations started should not increment on continuation
    assert_eq!(state.metrics.dev_iterations_started, 1);

    // But dev attempts should increment when continuation runs
    let event = PipelineEvent::development_agent_invoked(0);
    let state = reduce(state, event);
    assert_eq!(state.metrics.dev_attempts_total, 1);
}

#[test]
fn test_review_pass_started_increments_counter() {
    // Start with initial state (reviewer_pass = 0)
    let state = PipelineState::initial(0, 3);
    assert_eq!(state.reviewer_pass, 0);
    assert_eq!(state.metrics.review_passes_started, 0);

    // Starting pass 1 should increment (0 != 1)
    let event = PipelineEvent::review_pass_started(1);
    let state = reduce(state, event);

    assert_eq!(state.metrics.review_passes_started, 1);
    assert_eq!(state.reviewer_pass, 1);
}

#[test]
fn test_review_agent_invoked_increments_runs() {
    let mut state = PipelineState::initial(0, 3);
    state = reduce(state, PipelineEvent::review_pass_started(0));

    let event = PipelineEvent::review_agent_invoked(0);
    let state = reduce(state, event);

    assert_eq!(state.metrics.review_runs_total, 1);
}

#[test]
fn test_fix_agent_invoked_increments_fix_runs() {
    let state = PipelineState::initial(0, 3);
    let event = PipelineEvent::Review(ReviewEvent::FixAgentInvoked { pass: 0 });
    let state = reduce(state, event);

    assert_eq!(state.metrics.fix_runs_total, 1);
}

#[test]
fn test_xsd_retry_increments_total_and_phase_counters() {
    use crate::reducer::event::PipelinePhase;

    let mut state = PipelineState::initial(3, 0);
    state.phase = PipelinePhase::Development;

    let event = PipelineEvent::agent_xsd_validation_failed(
        AgentRole::Developer,
        ArtifactType::DevelopmentResult,
        "validation error".to_string(),
        1,
    );
    let state = reduce(state, event);

    assert_eq!(state.metrics.xsd_retry_attempts_total, 1);
    assert_eq!(state.metrics.xsd_retry_development, 1);
}

#[test]
fn test_agent_fallback_increments_counter() {
    let state = PipelineState::initial(3, 0);
    let event = PipelineEvent::agent_fallback_triggered(
        AgentRole::Developer,
        "claude".to_string(),
        "gpt4".to_string(),
    );
    let state = reduce(state, event);

    assert_eq!(state.metrics.agent_fallbacks_total, 1);
}

#[test]
fn test_commit_created_increments_counter() {
    let state = PipelineState::initial(1, 0);
    let event = PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string());
    let state = reduce(state, event);

    assert_eq!(state.metrics.commits_created_total, 1);
}

#[test]
fn test_phase_specific_xsd_retry_increments_planning_metrics() {
    use crate::reducer::event::PlanningEvent;

    let mut state = PipelineState::initial(3, 0);
    state.phase = crate::reducer::event::PipelinePhase::Planning;
    assert_eq!(state.metrics.xsd_retry_planning, 0);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 0);

    let event = PipelineEvent::Planning(PlanningEvent::OutputValidationFailed {
        iteration: 0,
        attempt: 0,
    });
    let state = reduce(state, event);

    assert_eq!(state.metrics.xsd_retry_planning, 1);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 1);
}

#[test]
fn test_phase_specific_xsd_retry_increments_development_metrics() {
    let mut state = PipelineState::initial(3, 0);
    state.phase = crate::reducer::event::PipelinePhase::Development;
    assert_eq!(state.metrics.xsd_retry_development, 0);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 0);

    let event = PipelineEvent::Development(DevelopmentEvent::OutputValidationFailed {
        iteration: 0,
        attempt: 0,
    });
    let state = reduce(state, event);

    assert_eq!(state.metrics.xsd_retry_development, 1);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 1);
}

#[test]
fn test_phase_specific_xsd_retry_increments_review_metrics() {
    let mut state = PipelineState::initial(0, 3);
    state.phase = crate::reducer::event::PipelinePhase::Review;
    assert_eq!(state.metrics.xsd_retry_review, 0);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 0);

    let event = PipelineEvent::Review(ReviewEvent::OutputValidationFailed {
        pass: 0,
        attempt: 0,
        error_detail: None,
    });
    let state = reduce(state, event);

    assert_eq!(state.metrics.xsd_retry_review, 1);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 1);
}

#[test]
fn test_phase_specific_xsd_retry_increments_fix_metrics() {
    let mut state = PipelineState::initial(0, 3);
    state.phase = crate::reducer::event::PipelinePhase::Review;
    assert_eq!(state.metrics.xsd_retry_fix, 0);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 0);

    let event = PipelineEvent::Review(ReviewEvent::FixOutputValidationFailed {
        pass: 0,
        attempt: 0,
        error_detail: None,
    });
    let state = reduce(state, event);

    assert_eq!(state.metrics.xsd_retry_fix, 1);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 1);
}

#[test]
fn test_xsd_retry_does_not_increment_when_exhausted() {
    let mut state = PipelineState::initial(3, 0);
    state.phase = crate::reducer::event::PipelinePhase::Development;
    state.continuation.xsd_retry_count = 98; // One below max (default is 99)
    state.continuation.max_xsd_retry_count = 99;

    // This retry should increment (98 + 1 = 99, which is < max when checked)
    let event = PipelineEvent::Development(DevelopmentEvent::OutputValidationFailed {
        iteration: 0,
        attempt: 0,
    });
    let state = reduce(state, event);

    // After this, xsd_retry_count would be 99, which is >= max, so it switches agents
    assert_eq!(state.metrics.xsd_retry_development, 1);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 1);

    // Try another validation failure - should NOT increment because we're exhausted
    let event = PipelineEvent::Development(DevelopmentEvent::OutputValidationFailed {
        iteration: 0,
        attempt: 0,
    });
    let state = reduce(state, event);

    // Should still be 1 because we switched agents (xsd_retry_count was reset)
    // and the new agent hasn't triggered a retry yet
    assert_eq!(state.metrics.xsd_retry_development, 1);
    assert_eq!(state.metrics.xsd_retry_attempts_total, 1);
}

#[test]
fn test_fix_continuation_triggered_increments_counter() {
    use crate::reducer::state::FixStatus;

    let state = PipelineState::initial(0, 3);
    let event = PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
        pass: 0,
        status: FixStatus::IssuesRemain,
        summary: Some("more work needed".to_string()),
    });
    let state = reduce(state, event);

    assert_eq!(state.metrics.fix_continuations_total, 1);
}

#[test]
fn test_review_pass_completed_clean_increments_counter() {
    let mut state = PipelineState::initial(0, 3);
    state = reduce(state, PipelineEvent::review_pass_started(1));
    assert_eq!(state.metrics.review_passes_completed, 0);

    // Simulate clean pass completion
    let event = PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass: 1 });
    let state = reduce(state, event);

    assert_eq!(state.metrics.review_passes_completed, 1);
    assert_eq!(state.reviewer_pass, 2); // Advances to next pass
}

#[test]
fn test_multiple_review_passes_increment_completed() {
    let mut state = PipelineState::initial(0, 3);

    // Pass 1
    state = reduce(state, PipelineEvent::review_pass_started(1));
    state = reduce(
        state,
        PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass: 1 }),
    );
    assert_eq!(state.metrics.review_passes_completed, 1);

    // Pass 2
    state = reduce(state, PipelineEvent::review_pass_started(2));
    state = reduce(
        state,
        PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass: 2 }),
    );
    assert_eq!(state.metrics.review_passes_completed, 2);
}

#[test]
fn test_fix_attempt_completed_increments_review_passes_completed() {
    let mut state = PipelineState::initial(0, 3);
    state = reduce(state, PipelineEvent::review_pass_started(1));
    assert_eq!(state.metrics.review_passes_completed, 0);

    // Simulate fix completing the pass
    let event = PipelineEvent::Review(ReviewEvent::FixAttemptCompleted {
        pass: 1,
        changes_made: true,
    });
    let state = reduce(state, event);

    assert_eq!(state.metrics.review_passes_completed, 1);
}

#[test]
fn test_continuation_succeeded_increments_dev_completed() {
    let mut state = PipelineState::initial(3, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));
    assert_eq!(state.metrics.dev_iterations_completed, 0);

    // Trigger continuation
    state = reduce(
        state,
        PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
            iteration: 0,
            status: DevelopmentStatus::Partial,
            summary: "partial work".to_string(),
            files_changed: None,
            next_steps: None,
        }),
    );

    // Continuation succeeds
    let event = PipelineEvent::Development(DevelopmentEvent::ContinuationSucceeded {
        iteration: 0,
        total_continuation_attempts: 1,
    });
    let state = reduce(state, event);

    assert_eq!(state.metrics.dev_iterations_completed, 1);
}

#[test]
fn test_same_agent_retry_increments_counter() {
    let state = PipelineState::initial(3, 0);
    let event = PipelineEvent::agent_timed_out(AgentRole::Developer, "claude".to_string());
    let state = reduce(state, event);

    assert_eq!(state.metrics.same_agent_retry_attempts_total, 1);
}

#[test]
fn test_model_fallback_increments_counter() {
    let state = PipelineState::initial(3, 0);
    let event = PipelineEvent::agent_model_fallback_triggered(
        AgentRole::Developer,
        "claude".to_string(),
        "claude-sonnet".to_string(),
        "gpt-4".to_string(),
    );
    let state = reduce(state, event);

    assert_eq!(state.metrics.model_fallbacks_total, 1);
}

#[test]
fn test_retry_cycle_started_increments_counter() {
    let state = PipelineState::initial(3, 0);
    let event = PipelineEvent::agent_retry_cycle_started(AgentRole::Developer, 1);
    let state = reduce(state, event);

    assert_eq!(state.metrics.retry_cycles_started_total, 1);
}

#[test]
fn test_metrics_survive_checkpoint_serialization() {
    let mut state = PipelineState::initial(5, 3);

    // Simulate some progress
    state.metrics.dev_iterations_started = 2;
    state.metrics.dev_iterations_completed = 1;
    state.metrics.dev_attempts_total = 3;
    state.metrics.analysis_attempts_total = 2;
    state.metrics.review_passes_started = 1;
    state.metrics.review_runs_total = 1;
    state.metrics.xsd_retry_attempts_total = 5;
    state.metrics.xsd_retry_development = 3;
    state.metrics.same_agent_retry_attempts_total = 2;
    state.metrics.commits_created_total = 1;

    // Serialize and deserialize
    let json = serde_json::to_string(&state).expect("serialization failed");
    let restored: PipelineState = serde_json::from_str(&json).expect("deserialization failed");

    // Verify all metrics are preserved
    assert_eq!(restored.metrics.dev_iterations_started, 2);
    assert_eq!(restored.metrics.dev_iterations_completed, 1);
    assert_eq!(restored.metrics.dev_attempts_total, 3);
    assert_eq!(restored.metrics.analysis_attempts_total, 2);
    assert_eq!(restored.metrics.review_passes_started, 1);
    assert_eq!(restored.metrics.review_runs_total, 1);
    assert_eq!(restored.metrics.xsd_retry_attempts_total, 5);
    assert_eq!(restored.metrics.xsd_retry_development, 3);
    assert_eq!(restored.metrics.same_agent_retry_attempts_total, 2);
    assert_eq!(restored.metrics.commits_created_total, 1);
}

#[test]
fn test_metrics_default_for_old_checkpoints() {
    // Simulate an old checkpoint without metrics field
    let json = r#"{
        "phase": "Development",
        "iteration": 1,
        "total_iterations": 5,
        "reviewer_pass": 0,
        "total_reviewer_passes": 2,
        "agent_chain": {
            "agents": [],
            "models_per_agent": [],
            "current_agent_index": 0,
            "current_model_index": 0,
            "retry_cycle": 0,
            "max_cycles": 1,
            "session_id": null,
            "continuation_prompt": null,
            "retry_delay_ms": 1000,
            "backoff_multiplier": 2.0,
            "max_backoff_ms": 60000
        },
        "continuation": {
            "invalid_output_attempts": 0,
            "continuation_attempt": 0,
            "max_continue_count": 3,
            "continue_pending": false,
            "context_write_pending": false,
            "context_cleanup_pending": false,
            "xsd_retry_count": 0,
            "xsd_retry_pending": false,
            "xsd_retry_session_reuse_pending": false,
            "max_xsd_retry_count": 99,
            "same_agent_retry_count": 0,
            "same_agent_retry_pending": false,
            "same_agent_retry_reason": null,
            "max_same_agent_retry_count": 3,
            "fix_continuation_attempt": 0,
            "max_fix_continue_count": 3,
            "fix_continue_pending": false,
            "last_xsd_error": null,
            "last_review_xsd_error": null,
            "last_fix_xsd_error": null,
            "dev_continuation_context": null
        },
        "commit": "NotStarted",
        "prompt_inputs": {
            "development": null,
            "review": null,
            "commit": null
        },
        "review_issues_found": false,
        "commit_prompt_prepared": false,
        "commit_diff_prepared": false,
        "commit_diff_empty": false,
        "commit_diff_content_id_sha256": null,
        "commit_agent_invoked": false,
        "commit_xml_cleaned": false,
        "commit_xml_extracted": false,
        "commit_validated_outcome": null,
        "commit_xml_archived": false,
        "context_cleaned": false,
        "dev_fix_triggered": false,
        "previous_phase": null,
        "planning_prompt_prepared_iteration": null,
        "planning_xml_cleaned_iteration": null,
        "planning_agent_invoked_iteration": null,
        "planning_xml_extracted_iteration": null,
        "planning_validated_outcome": null,
        "planning_markdown_written_iteration": null,
        "planning_xml_archived_iteration": null,
        "development_context_prepared_iteration": null,
        "development_prompt_prepared_iteration": null,
        "development_xml_cleaned_iteration": null,
        "development_agent_invoked_iteration": null,
        "analysis_agent_invoked_iteration": null,
        "development_xml_extracted_iteration": null,
        "development_validated_outcome": null,
        "development_xml_archived_iteration": null,
        "review_context_prepared_pass": null,
        "review_prompt_prepared_pass": null,
        "review_issues_xml_cleaned_pass": null,
        "review_agent_invoked_pass": null,
        "review_issues_xml_extracted_pass": null,
        "review_validated_outcome": null,
        "review_issues_markdown_written_pass": null,
        "review_issue_snippets_extracted_pass": null,
        "review_issues_xml_archived_pass": null,
        "fix_prompt_prepared_pass": null,
        "fix_result_xml_cleaned_pass": null,
        "fix_agent_invoked_pass": null,
        "fix_result_xml_extracted_pass": null,
        "fix_validated_outcome": null,
        "fix_result_xml_archived_pass": null,
        "checkpoint_saved_count": 0
    }"#;

    let restored: PipelineState = serde_json::from_str(json).expect("deserialization failed");

    // Verify metrics field is present with defaults
    assert_eq!(restored.metrics.dev_iterations_started, 0);
    assert_eq!(restored.metrics.max_dev_iterations, 0); // Not initialized from config in this test
}

#[test]
fn test_new_metrics_backward_compatible() {
    // Simulate checkpoint from before review_passes_completed was added
    let mut state = PipelineState::initial(5, 3);
    state.metrics.review_passes_started = 2;
    state.metrics.review_runs_total = 2;
    // review_passes_completed field didn't exist in old checkpoint

    let json = serde_json::to_string(&state).unwrap();
    let mut json_obj: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Remove the new field to simulate old checkpoint
    if let Some(metrics) = json_obj.get_mut("metrics") {
        if let Some(metrics_obj) = metrics.as_object_mut() {
            metrics_obj.remove("review_passes_completed");
        }
    }

    let restored: PipelineState =
        serde_json::from_value(json_obj).expect("should deserialize with defaults");

    // New field should default to 0
    assert_eq!(restored.metrics.review_passes_completed, 0);
    // Existing fields should be preserved
    assert_eq!(restored.metrics.review_passes_started, 2);
    assert_eq!(restored.metrics.review_runs_total, 2);
}

#[test]
fn test_same_agent_retry_exhausted_does_not_increment() {
    let mut state = PipelineState::initial(3, 0);
    state.continuation.max_same_agent_retry_count = 3;
    state.continuation.same_agent_retry_count = 2; // One below max

    // First retry (count becomes 3, which is >= max) should NOT increment because will_retry = false
    let event = PipelineEvent::agent_timed_out(AgentRole::Developer, "claude".to_string());
    let state = reduce(state, event);

    // Should be 0 because new_retry_count (3) >= max (3), so we fall back without incrementing
    assert_eq!(state.metrics.same_agent_retry_attempts_total, 0);
    // Verify agent chain switched
    assert!(state.agent_chain.current_agent_index > 0 || state.agent_chain.retry_cycle > 0);
}

#[test]
fn test_same_agent_retry_within_budget_does_increment() {
    let mut state = PipelineState::initial(3, 0);
    state.continuation.max_same_agent_retry_count = 3;
    state.continuation.same_agent_retry_count = 0;

    // First retry (count becomes 1, which is < max) should increment
    let event = PipelineEvent::agent_timed_out(AgentRole::Developer, "claude".to_string());
    let state = reduce(state, event);

    assert_eq!(state.metrics.same_agent_retry_attempts_total, 1);
    assert_eq!(state.continuation.same_agent_retry_count, 1);
    assert!(state.continuation.same_agent_retry_pending);
}

// ==============================================================================
// New tests for per-iteration/pass continuation attempt tracking
// ==============================================================================

#[test]
fn test_dev_continuation_attempt_increments_on_continuation_triggered() {
    let mut state = PipelineState::initial(1, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));

    assert_eq!(state.metrics.dev_continuation_attempt, 0);

    // Trigger continuation
    let event = PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
        iteration: 0,
        status: DevelopmentStatus::Partial,
        summary: "Partial work".to_string(),
        files_changed: None,
        next_steps: None,
    });
    state = reduce(state, event);

    assert_eq!(state.metrics.dev_continuation_attempt, 1);
    assert_eq!(state.metrics.dev_iterations_completed, 0); // Not completed yet
}

#[test]
fn test_dev_continuation_attempt_resets_on_new_iteration() {
    let mut state = PipelineState::initial(2, 0);

    // Iteration 0 with continuation
    state = reduce(state, PipelineEvent::development_iteration_started(0));
    state = reduce(
        state,
        PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
            iteration: 0,
            status: DevelopmentStatus::Partial,
            summary: "Partial".to_string(),
            files_changed: None,
            next_steps: None,
        }),
    );
    assert_eq!(state.metrics.dev_continuation_attempt, 1);

    // New iteration should reset
    state = reduce(state, PipelineEvent::development_iteration_started(1));
    assert_eq!(state.metrics.dev_continuation_attempt, 0);
}

#[test]
fn test_dev_iterations_completed_increments_on_continuation_succeeded() {
    let mut state = PipelineState::initial(1, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));

    assert_eq!(state.metrics.dev_iterations_completed, 0);

    // Continuation succeeded
    let event = PipelineEvent::Development(DevelopmentEvent::ContinuationSucceeded {
        iteration: 0,
        total_continuation_attempts: 2,
    });
    state = reduce(state, event);

    assert_eq!(state.metrics.dev_iterations_completed, 1);
    assert_eq!(
        state.phase,
        crate::reducer::event::PipelinePhase::CommitMessage
    );
}

#[test]
fn test_dev_iterations_completed_not_incremented_on_continuation_triggered() {
    let mut state = PipelineState::initial(1, 0);
    state = reduce(state, PipelineEvent::development_iteration_started(0));

    // Trigger continuation - should NOT increment completed counter
    let event = PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
        iteration: 0,
        status: DevelopmentStatus::Partial,
        summary: "Partial".to_string(),
        files_changed: None,
        next_steps: None,
    });
    state = reduce(state, event);

    assert_eq!(state.metrics.dev_iterations_completed, 0);
    assert_eq!(state.metrics.dev_continuation_attempt, 1);
}

#[test]
fn test_review_pass_increments_current_pass_on_pass_started() {
    let state = PipelineState::initial(0, 3);

    let event = PipelineEvent::review_pass_started(1);
    let state = reduce(state, event);

    assert_eq!(state.metrics.current_review_pass, 1);
    assert_eq!(state.metrics.review_passes_started, 1);
}

#[test]
fn test_fix_continuation_attempt_increments_on_continuation_triggered() {
    use crate::reducer::state::FixStatus;

    let mut state = PipelineState::initial(0, 1);
    state = reduce(state, PipelineEvent::review_pass_started(1));

    assert_eq!(state.metrics.fix_continuation_attempt, 0);

    // Trigger fix continuation
    let event = PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
        pass: 1,
        status: FixStatus::IssuesRemain,
        summary: Some("Fixed some issues".to_string()),
    });
    state = reduce(state, event);

    assert_eq!(state.metrics.fix_continuation_attempt, 1);
    assert_eq!(state.metrics.fix_continuations_total, 1);
}

#[test]
fn test_fix_continuation_attempt_resets_on_new_pass() {
    use crate::reducer::state::FixStatus;

    let mut state = PipelineState::initial(0, 2);

    // Pass 1 with fix continuation
    state = reduce(state, PipelineEvent::review_pass_started(1));
    state = reduce(
        state,
        PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
            pass: 1,
            status: FixStatus::IssuesRemain,
            summary: None,
        }),
    );
    assert_eq!(state.metrics.fix_continuation_attempt, 1);

    // New pass should reset
    state = reduce(state, PipelineEvent::review_pass_started(2));
    assert_eq!(state.metrics.fix_continuation_attempt, 0);
}

#[test]
fn test_review_passes_completed_increments_on_clean_pass() {
    let mut state = PipelineState::initial(0, 1);
    state = reduce(state, PipelineEvent::review_pass_started(1));

    assert_eq!(state.metrics.review_passes_completed, 0);

    // Clean pass
    let event = PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass: 1 });
    state = reduce(state, event);

    assert_eq!(state.metrics.review_passes_completed, 1);
}

#[test]
fn test_review_passes_completed_increments_on_fix_attempt_completed() {
    let mut state = PipelineState::initial(0, 1);
    state = reduce(state, PipelineEvent::review_pass_started(1));

    assert_eq!(state.metrics.review_passes_completed, 0);

    // Fix attempt completed
    let event = PipelineEvent::Review(ReviewEvent::FixAttemptCompleted {
        pass: 1,
        changes_made: true,
    });
    state = reduce(state, event);

    assert_eq!(state.metrics.review_passes_completed, 1);
}

#[test]
fn test_review_passes_completed_not_incremented_when_issues_found() {
    let mut state = PipelineState::initial(0, 1);
    state = reduce(state, PipelineEvent::review_pass_started(1));

    assert_eq!(state.metrics.review_passes_completed, 0);

    // Pass completed with issues found - should NOT increment
    let event = PipelineEvent::Review(ReviewEvent::Completed {
        pass: 1,
        issues_found: true,
    });
    state = reduce(state, event);

    assert_eq!(state.metrics.review_passes_completed, 0);
}

#[test]
fn test_review_passes_completed_increments_on_fix_continuation_succeeded() {
    let mut state = PipelineState::initial(0, 1);
    state = reduce(state, PipelineEvent::review_pass_started(1));

    assert_eq!(state.metrics.review_passes_completed, 0);

    // Fix continuation succeeded
    let event = PipelineEvent::Review(ReviewEvent::FixContinuationSucceeded {
        pass: 1,
        total_attempts: 2,
    });
    state = reduce(state, event);

    assert_eq!(state.metrics.review_passes_completed, 1);
    assert_eq!(
        state.phase,
        crate::reducer::event::PipelinePhase::CommitMessage
    );
}

// ==============================================================================
// Checkpoint compatibility tests
// ==============================================================================

#[test]
fn test_new_metrics_fields_checkpoint_compatible() {
    let mut state = PipelineState::initial(2, 2);

    // Set new metrics fields
    state.metrics.dev_continuation_attempt = 2;
    state.metrics.fix_continuation_attempt = 1;
    state.metrics.current_review_pass = 1;

    // Serialize
    let serialized = serde_json::to_string(&state).expect("serialize failed");

    // Deserialize
    let restored: PipelineState = serde_json::from_str(&serialized).expect("deserialize failed");

    // Verify new fields are preserved
    assert_eq!(restored.metrics.dev_continuation_attempt, 2);
    assert_eq!(restored.metrics.fix_continuation_attempt, 1);
    assert_eq!(restored.metrics.current_review_pass, 1);
}

#[test]
fn test_old_checkpoint_loads_with_new_metrics_fields_defaulted() {
    // Simulate old checkpoint JSON without new fields
    let old_checkpoint_json = r#"{
        "phase": "Development",
        "previous_phase": null,
        "iteration": 1,
        "total_iterations": 2,
        "reviewer_pass": 0,
        "total_reviewer_passes": 2,
        "review_issues_found": false,
        "context_cleaned": false,
        "agent_chain": {
            "agents": [],
            "current_agent_index": 0,
            "session_id": null,
            "continuation_prompt": null,
            "cycle_count": 0,
            "same_agent_retry_count": 0,
            "same_agent_retry_pending": false,
            "same_agent_retry_reason": null,
            "retry_delay_ms": 0,
            "backoff_multiplier": 1.0,
            "max_backoff_ms": 0,
            "max_cycles": 1
        },
        "rebase": "NotStarted",
        "commit": "NotStarted",
        "execution_history": [],
        "checkpoint_saved_count": 0,
        "continuation": {
            "previous_status": null,
            "previous_summary": null,
            "previous_files_changed": null,
            "previous_next_steps": null,
            "continuation_attempt": 0,
            "invalid_output_attempts": 0,
            "context_write_pending": false,
            "context_cleanup_pending": false,
            "xsd_retry_count": 0,
            "xsd_retry_pending": false,
            "xsd_retry_session_reuse_pending": false,
            "max_xsd_retry_count": 10,
            "max_same_agent_retry_count": 2,
            "max_continue_count": 3
        },
        "dev_fix_triggered": false,
        "prompt_inputs": {},
        "metrics": {
            "dev_iterations_started": 1,
            "dev_iterations_completed": 0,
            "dev_attempts_total": 3,
            "analysis_attempts_total": 1,
            "analysis_attempts_in_current_iteration": 1,
            "review_passes_started": 0,
            "review_passes_completed": 0,
            "review_runs_total": 0,
            "fix_runs_total": 0,
            "fix_continuations_total": 0,
            "xsd_retry_attempts_total": 2,
            "xsd_retry_planning": 0,
            "xsd_retry_development": 2,
            "xsd_retry_review": 0,
            "xsd_retry_fix": 0,
            "xsd_retry_commit": 0,
            "same_agent_retry_attempts_total": 0,
            "agent_fallbacks_total": 0,
            "model_fallbacks_total": 0,
            "retry_cycles_started_total": 0,
            "commits_created_total": 0,
            "max_dev_iterations": 2,
            "max_review_passes": 2
        }
    }"#;

    let restored: PipelineState = serde_json::from_str(old_checkpoint_json)
        .expect("old checkpoint should deserialize with defaults");

    // New fields should default to 0
    assert_eq!(restored.metrics.dev_continuation_attempt, 0);
    assert_eq!(restored.metrics.fix_continuation_attempt, 0);
    assert_eq!(restored.metrics.current_review_pass, 0);

    // Existing fields should be preserved
    assert_eq!(restored.metrics.dev_iterations_started, 1);
    assert_eq!(restored.metrics.xsd_retry_attempts_total, 2);
}
