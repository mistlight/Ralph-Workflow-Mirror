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

use ralph_workflow::banner::PipelineSummary;
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
                PipelineEvent::commit_created(
                    format!("commit{}", i),
                    format!("Commit message {}", i),
                ),
            );
        }

        // Simulate 1 review pass (clean)
        state = reduce(state, PipelineEvent::review_pass_started(1));
        state = reduce(state, PipelineEvent::review_agent_invoked(1));
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass: 1 }),
        );

        // Construct summary as finalize_pipeline does
        let summary = PipelineSummary {
            total_time: "1m 23s".to_string(), // Mock
            dev_runs_completed: state.metrics.dev_iterations_completed as usize,
            dev_runs_total: state.metrics.max_dev_iterations as usize,
            review_passes_completed: state.metrics.review_passes_completed as usize,
            review_passes_total: state.metrics.max_review_passes as usize,
            review_runs: state.metrics.review_runs_total as usize,
            changes_detected: state.metrics.commits_created_total as usize,
            isolation_mode: false,
            verbose: false,
            review_summary: None,
        };

        // Assert all summary fields match reducer state
        assert_eq!(summary.dev_runs_completed, 2);
        assert_eq!(summary.dev_runs_total, 2);
        assert_eq!(summary.review_passes_completed, 1);
        assert_eq!(summary.review_passes_total, 1);
        assert_eq!(summary.review_runs, 1);
        assert_eq!(summary.changes_detected, 2);

        // Verify reducer metrics directly
        assert_eq!(state.metrics.dev_iterations_started, 2);
        assert_eq!(state.metrics.dev_iterations_completed, 2);
        assert_eq!(state.metrics.dev_attempts_total, 2);
        assert_eq!(state.metrics.analysis_attempts_total, 2);
        assert_eq!(state.metrics.review_passes_started, 1);
        assert_eq!(state.metrics.review_passes_completed, 1);
        assert_eq!(state.metrics.commits_created_total, 2);
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

        // Construct summary
        let summary = PipelineSummary {
            total_time: "2m 15s".to_string(),
            dev_runs_completed: state.metrics.dev_iterations_completed as usize,
            dev_runs_total: state.metrics.max_dev_iterations as usize,
            review_passes_completed: state.metrics.review_passes_completed as usize,
            review_passes_total: state.metrics.max_review_passes as usize,
            review_runs: state.metrics.review_runs_total as usize,
            changes_detected: state.metrics.commits_created_total as usize,
            isolation_mode: false,
            verbose: false,
            review_summary: None,
        };

        // Verify summary matches reducer
        assert_eq!(summary.dev_runs_completed, 1);
        assert_eq!(summary.dev_runs_total, 1);
        assert_eq!(summary.review_passes_completed, 1);
        assert_eq!(summary.review_passes_total, 1);
        assert_eq!(summary.review_runs, 1);
        assert_eq!(summary.changes_detected, 2); // Dev commit + fix commit

        // Verify detailed metrics
        assert_eq!(state.metrics.dev_iterations_started, 1);
        assert_eq!(state.metrics.dev_iterations_completed, 1); // Completed via ContinuationSucceeded
        assert_eq!(state.metrics.dev_attempts_total, 2); // Initial + 1 continuation
        assert_eq!(state.metrics.fix_runs_total, 1);
        assert_eq!(state.metrics.review_passes_completed, 1); // Fix completed the pass
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

        // Construct summary
        let summary = PipelineSummary {
            total_time: "3m 45s".to_string(),
            dev_runs_completed: state.metrics.dev_iterations_completed as usize,
            dev_runs_total: state.metrics.max_dev_iterations as usize,
            review_passes_completed: state.metrics.review_passes_completed as usize,
            review_passes_total: state.metrics.max_review_passes as usize,
            review_runs: state.metrics.review_runs_total as usize,
            changes_detected: state.metrics.commits_created_total as usize,
            isolation_mode: false,
            verbose: false,
            review_summary: None,
        };

        // Verify all 3 passes completed
        assert_eq!(summary.review_passes_completed, 3);
        assert_eq!(summary.review_passes_total, 3);
        assert_eq!(summary.review_runs, 3);
        // Note: review_passes_started only increments when reviewer_pass != pass in PassStarted
        // After Completed/PassCompletedClean, reviewer_pass auto-advances to next_pass
        // So subsequent PassStarted calls don't increment (pass == reviewer_pass)
        // This is current behavior - only first PassStarted(1) increments from initial 0
        // The "completed" counter is what matters for showing progress
        assert_eq!(state.metrics.review_passes_completed, 3);
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

        // No events, no work done
        let summary = PipelineSummary {
            total_time: "0s".to_string(),
            dev_runs_completed: state.metrics.dev_iterations_completed as usize,
            dev_runs_total: state.metrics.max_dev_iterations as usize,
            review_passes_completed: state.metrics.review_passes_completed as usize,
            review_passes_total: state.metrics.max_review_passes as usize,
            review_runs: state.metrics.review_runs_total as usize,
            changes_detected: state.metrics.commits_created_total as usize,
            isolation_mode: false,
            verbose: false,
            review_summary: None,
        };

        assert_eq!(summary.dev_runs_completed, 0);
        assert_eq!(summary.dev_runs_total, 5);
        assert_eq!(summary.review_passes_completed, 0);
        assert_eq!(summary.review_passes_total, 3);
        assert_eq!(summary.review_runs, 0);
        assert_eq!(summary.changes_detected, 0);
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
#[test]
fn test_xsd_retry_attribution_across_phases() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::event::{PlanningEvent, CommitEvent, PipelinePhase};
        
        // Start with initial state
        let mut state = PipelineState::initial(1, 1);
        
        // Simulate planning XSD retry (1 attempt)
        state.phase = PipelinePhase::Planning;
        let event = PipelineEvent::Planning(PlanningEvent::OutputValidationFailed {
            iteration: 0,
            attempt: 0,
        });
        state = reduce(state, event);
        
        // Verify planning XSD retry was tracked
        assert_eq!(state.metrics.xsd_retry_planning, 1);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 1);
        
        // Simulate development XSD retry (2 attempts)
        state.phase = PipelinePhase::Development;
        state.iteration = 0;
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
        
        // Verify development XSD retries were tracked
        assert_eq!(state.metrics.xsd_retry_development, 2);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 3);
        
        // Simulate review XSD retry (1 attempt)
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;
        let event = PipelineEvent::Review(ReviewEvent::OutputValidationFailed {
            pass: 0,
            attempt: 0,
            error_detail: None,
        });
        state = reduce(state, event);
        
        // Verify review XSD retry was tracked
        assert_eq!(state.metrics.xsd_retry_review, 1);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 4);
        
        // Simulate fix XSD retry (1 attempt)
        let event = PipelineEvent::Review(ReviewEvent::FixOutputValidationFailed {
            pass: 0,
            attempt: 0,
            error_detail: None,
        });
        state = reduce(state, event);
        
        // Verify fix XSD retry was tracked
        assert_eq!(state.metrics.xsd_retry_fix, 1);
        assert_eq!(state.metrics.xsd_retry_attempts_total, 5);
        
        // Final assertions: verify total and per-phase attribution
        assert_eq!(
            state.metrics.xsd_retry_attempts_total, 5,
            "Total XSD retry attempts should sum all phases"
        );
        assert_eq!(
            state.metrics.xsd_retry_planning, 1,
            "Planning XSD retries should be 1"
        );
        assert_eq!(
            state.metrics.xsd_retry_development, 2,
            "Development XSD retries should be 2"
        );
        assert_eq!(
            state.metrics.xsd_retry_review, 1,
            "Review XSD retries should be 1"
        );
        assert_eq!(
            state.metrics.xsd_retry_fix, 1,
            "Fix XSD retries should be 1"
        );
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
