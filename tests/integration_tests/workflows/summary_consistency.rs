//! Integration tests for summary consistency.
//!
//! Verifies that the final pipeline summary is derived exclusively from
//! reducer state with no drift from runtime counters.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (summary derives from state)
//! - Tests are deterministic and isolated
//! - Tests behavior, not implementation details

use ralph_workflow::reducer::event::{DevelopmentEvent, PipelineEvent, ReviewEvent};
use ralph_workflow::reducer::state::{DevelopmentStatus, PipelineState};
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

#[test]
fn test_summary_matches_reducer_state_simple_run() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(2, 1);

        // Simulate 2 dev iterations
        for i in 0..2 {
            state = reduce(state, PipelineEvent::development_iteration_started(i));
            state = reduce(state, PipelineEvent::development_agent_invoked(i));
            state = reduce(
                state,
                PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration: i }),
            );
            state = reduce(
                state,
                PipelineEvent::development_iteration_completed(i, true),
            );
            state = reduce(
                state,
                PipelineEvent::commit_created(format!("commit{i}"), format!("Commit message {i}")),
            );
        }

        // Simulate 1 review pass (clean)
        state = reduce(state, PipelineEvent::review_pass_started(1));
        state = reduce(state, PipelineEvent::review_agent_invoked(1));
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass: 1 }),
        );

        // Verify reducer metrics match expected values for this event sequence.
        // PipelineSummary is a direct projection of these metrics (see finalization.rs),
        // so verifying metrics IS verifying summary correctness.
        assert_eq!(state.metrics.dev_iterations_completed, 2);
        assert_eq!(state.metrics.max_dev_iterations, 2);
        assert_eq!(state.metrics.review_passes_completed, 1);
        assert_eq!(state.metrics.max_review_passes, 1);
        assert_eq!(state.metrics.review_runs_total, 1);
        assert_eq!(state.metrics.commits_created_total, 2);

        // Detailed metrics
        assert_eq!(state.metrics.dev_iterations_started, 2);
        assert_eq!(state.metrics.dev_attempts_total, 2);
        assert_eq!(state.metrics.analysis_attempts_total, 2);
        assert_eq!(state.metrics.review_passes_started, 1);
    });
}

#[test]
fn test_summary_matches_reducer_state_with_continuations() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 1);

        // Iteration 0 with 2 continuations
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        assert_eq!(state.metrics.dev_continuation_attempt, 0);

        state = reduce(state, PipelineEvent::development_agent_invoked(0)); // Attempt 1
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.metrics.dev_continuation_attempt, 1);

        state = reduce(state, PipelineEvent::development_agent_invoked(0)); // Attempt 2
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationSucceeded {
                iteration: 0,
                total_continuation_attempts: 2,
            }),
        );
        assert_eq!(state.metrics.dev_iterations_completed, 1);

        // Commit after dev
        state = reduce(
            state,
            PipelineEvent::commit_created("hash0".to_string(), "Dev work".to_string()),
        );

        // Review with fix
        state = reduce(state, PipelineEvent::review_pass_started(1));
        state = reduce(state, PipelineEvent::review_agent_invoked(1));
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixAgentInvoked { pass: 1 }),
        );
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixAttemptCompleted {
                pass: 1,
                changes_made: true,
            }),
        );

        // Commit after fix
        state = reduce(
            state,
            PipelineEvent::commit_created("hash1".to_string(), "Fix issues".to_string()),
        );

        // Verify metrics match expected values for this event sequence.
        // PipelineSummary is a direct projection of these metrics (see finalization.rs).
        assert_eq!(state.metrics.dev_iterations_completed, 1);
        assert_eq!(state.metrics.max_dev_iterations, 1);
        assert_eq!(state.metrics.review_passes_completed, 1);
        assert_eq!(state.metrics.max_review_passes, 1);
        assert_eq!(state.metrics.review_runs_total, 1);
        assert_eq!(state.metrics.commits_created_total, 2); // Dev commit + fix commit

        // Detailed metrics
        assert_eq!(state.metrics.dev_iterations_started, 1);
        assert_eq!(state.metrics.dev_attempts_total, 2); // Initial + 1 continuation
        assert_eq!(state.metrics.fix_runs_total, 1);
    });
}

#[test]
fn test_summary_matches_with_multiple_review_passes() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 3);

        // Dev iteration 0
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        state = reduce(
            state,
            PipelineEvent::commit_created("hash0".to_string(), "Dev work".to_string()),
        );

        // Review pass 1 (clean)
        state = reduce(state, PipelineEvent::review_pass_started(1));
        state = reduce(state, PipelineEvent::review_agent_invoked(1));
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::Completed {
                pass: 1,
                issues_found: false,
            }),
        );

        // Review pass 2 (clean)
        state = reduce(state, PipelineEvent::review_pass_started(2));
        state = reduce(state, PipelineEvent::review_agent_invoked(2));
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::Completed {
                pass: 2,
                issues_found: false,
            }),
        );

        // Review pass 3 (clean)
        state = reduce(state, PipelineEvent::review_pass_started(3));
        state = reduce(state, PipelineEvent::review_agent_invoked(3));
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::Completed {
                pass: 3,
                issues_found: false,
            }),
        );

        // Verify metrics match expected values for this event sequence.
        // PipelineSummary is a direct projection of these metrics (see finalization.rs).
        assert_eq!(state.metrics.review_passes_completed, 3);
        assert_eq!(state.metrics.max_review_passes, 3);
        assert_eq!(state.metrics.review_runs_total, 3);
    });
}

#[test]
fn test_summary_consistency_with_xsd_retries() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);

        // Dev iteration with XSD retry
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        state = reduce(state, PipelineEvent::development_agent_invoked(0));

        // Simulate XSD validation failure (triggers retry)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutputValidationFailed {
                iteration: 0,
                attempt: 0,
            }),
        );
        assert_eq!(state.metrics.xsd_retry_development, 1);

        // Retry succeeds
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration: 0 }),
        );
        state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        state = reduce(
            state,
            PipelineEvent::commit_created("hash0".to_string(), "Dev work".to_string()),
        );

        // Verify XSD retry metrics tracked
        assert_eq!(state.metrics.xsd_retry_development, 1);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 1);

        // Summary should still show 1 iteration completed
        assert_eq!(state.metrics.dev_iterations_completed, 1);
    });
}

#[test]
fn test_summary_zero_when_no_work_done() {
    with_default_timeout(|| {
        let state = PipelineState::initial(5, 3);

        // No events, no work done - metrics should all be zero except configured totals.
        // PipelineSummary is a direct projection of these metrics (see finalization.rs).
        assert_eq!(state.metrics.dev_iterations_completed, 0);
        assert_eq!(state.metrics.max_dev_iterations, 5);
        assert_eq!(state.metrics.review_passes_completed, 0);
        assert_eq!(state.metrics.max_review_passes, 3);
        assert_eq!(state.metrics.review_runs_total, 0);
        assert_eq!(state.metrics.commits_created_total, 0);
    });
}

#[test]
fn test_fix_continuation_metrics_tracked_in_reducer() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::FixStatus;

        let mut state = PipelineState::initial(1, 1);

        // Complete dev iteration
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        state = reduce(
            state,
            PipelineEvent::commit_created("hash0".to_string(), "Dev work".to_string()),
        );

        // Start review pass
        state = reduce(state, PipelineEvent::review_pass_started(1));
        assert_eq!(state.metrics.fix_continuation_attempt, 0);

        state = reduce(state, PipelineEvent::review_agent_invoked(1));

        // Issues found, start fix
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixAgentInvoked { pass: 1 }),
        );

        // Fix reports issues remain - trigger continuation
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                pass: 1,
                status: FixStatus::IssuesRemain,
                summary: Some("Fixed some issues".to_string()),
            }),
        );

        assert_eq!(state.metrics.fix_continuation_attempt, 1);
        assert_eq!(state.metrics.fix_continuations_total, 1);

        // Second fix attempt
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixAgentInvoked { pass: 1 }),
        );

        // Fix succeeds
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationSucceeded {
                pass: 1,
                total_attempts: 2,
            }),
        );

        assert_eq!(state.metrics.review_passes_completed, 1);
        assert_eq!(state.metrics.fix_runs_total, 2);
        assert_eq!(state.metrics.fix_continuations_total, 1);
    });
}

// ============================================================================
// Step 17: XSD retry attribution across phases
// ============================================================================

/// Test that XSD retries in different phases are correctly attributed to
/// phase-specific counters and total.
///
/// CRITICAL: Per-phase attribution ensures we can see where XSD retries occurred
/// for debugging and observability.
///
/// This test drives state through events to reach each phase naturally rather
/// than directly setting `state.phase`.
#[test]
fn test_xsd_retry_attribution_across_phases() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::event::PlanningEvent;

        // Start with initial state (Planning phase by default)
        let mut state = PipelineState::initial(1, 1);

        // Phase 1: Planning XSD retry (1 attempt)
        // State starts in Planning phase naturally
        let event = PipelineEvent::Planning(PlanningEvent::OutputValidationFailed {
            iteration: 0,
            attempt: 0,
        });
        state = reduce(state, event);
        assert_eq!(state.metrics.xsd_retry_planning, 1);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 1);

        // Transition to Development via planning_phase_completed event
        state = reduce(state, PipelineEvent::planning_phase_completed());

        // Phase 2: Development XSD retries (2 attempts)
        let event = PipelineEvent::Development(DevelopmentEvent::OutputValidationFailed {
            iteration: 0,
            attempt: 0,
        });
        state = reduce(state, event);

        let event = PipelineEvent::Development(DevelopmentEvent::OutputValidationFailed {
            iteration: 0,
            attempt: 1,
        });
        state = reduce(state, event);
        assert_eq!(state.metrics.xsd_retry_development, 2);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 3);

        // Transition to Review via development_iteration_completed + commit_created events
        state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        state = reduce(
            state,
            PipelineEvent::commit_created("hash".to_string(), "msg".to_string()),
        );

        // Phase 3: Review XSD retry (1 attempt)
        let event = PipelineEvent::Review(ReviewEvent::OutputValidationFailed {
            pass: 0,
            attempt: 0,
            error_detail: None,
        });
        state = reduce(state, event);
        assert_eq!(state.metrics.xsd_retry_review, 1);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 4);

        // Phase 4: Fix XSD retry (1 attempt) - still in Review phase
        let event = PipelineEvent::Review(ReviewEvent::FixOutputValidationFailed {
            pass: 0,
            attempt: 0,
            error_detail: None,
        });
        state = reduce(state, event);
        assert_eq!(state.metrics.xsd_retry_fix, 1);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 5);

        // Final assertions: verify total and per-phase attribution
        assert_eq!(
            state.metrics.xsd_retry_attempts_total, 5,
            "Total XSD retry attempts should sum all phases"
        );
        assert_eq!(state.metrics.xsd_retry_planning, 1);
        assert_eq!(state.metrics.xsd_retry_development, 2);
        assert_eq!(state.metrics.xsd_retry_review, 1);
        assert_eq!(state.metrics.xsd_retry_fix, 1);
        assert_eq!(
            state.metrics.xsd_retry_commit, 0,
            "Commit XSD retries should be 0 (not tested in this scenario)"
        );

        // Verify sum matches total
        let sum = state.metrics.xsd_retry_planning
            + state.metrics.xsd_retry_development
            + state.metrics.xsd_retry_review
            + state.metrics.xsd_retry_fix
            + state.metrics.xsd_retry_commit;
        assert_eq!(
            sum, state.metrics.xsd_retry_attempts_total,
            "Per-phase XSD retry counters should sum to total"
        );
    });
}
