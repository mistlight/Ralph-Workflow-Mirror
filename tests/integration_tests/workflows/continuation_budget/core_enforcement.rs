//! Core continuation budget enforcement tests.
//!
//! Tests in this module verify:
//! - Dev and fix continuation budgets are enforced
//! - Continuation state resets across iterations
//! - Budget exhaustion transitions correctly
//! - Metrics track continuation attempts accurately

use ralph_workflow::reducer::event::{DevelopmentEvent, PipelineEvent, ReviewEvent};
use ralph_workflow::reducer::state::{
    ContinuationState, DevelopmentStatus, FixStatus, PipelineState,
};
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

#[test]
fn test_dev_continuation_budget_enforced() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(5, 0, &continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Attempt 0 (initial) - already happened, continuation_attempt = 0

        // Trigger continuation 2 times (attempts 1, 2)
        for attempt in 1..=2 {
            state = reduce(
                state,
                PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                    iteration: 0,
                    status: DevelopmentStatus::Partial,
                    summary: format!("Attempt {attempt}"),
                    files_changed: None,
                    next_steps: None,
                }),
            );
            assert_eq!(state.continuation.continuation_attempt, attempt);
        }

        // Attempt to trigger third continuation (would be attempt 3, but defensive check prevents it)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "Attempt 3".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        // AFTER FIX: trigger_continuation defensive check prevents increment to 3, counter stays at 2
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "Defensive check should prevent increment past attempt 2"
        );

        // continuations_exhausted() returns false (2 < 3), but the system won't schedule another
        // continuation because the defensive check cleared continue_pending flag.
        // In normal flow, OutcomeApplied detects exhaustion via (attempt + 1 >= max) check.
        assert!(!state.continuation.continuations_exhausted());
        assert!(!state.continuation.continue_pending);
    });
}

#[test]
fn test_fix_continuation_budget_enforced() {
    with_default_timeout(|| {
        let mut continuation = ContinuationState::with_limits(99, 3, 2);
        continuation.max_fix_continue_count = 2; // Set fix budget explicitly to 2
        let mut state = PipelineState::initial_with_continuation(0, 3, &continuation);

        // Simulate fix continuations (max_fix_continue_count = 2)
        // Attempt 0 is initial, attempts 1 and 2 are continuations
        for i in 1..=2 {
            state = reduce(
                state,
                PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                    pass: 0,
                    status: FixStatus::IssuesRemain,
                    summary: Some(format!("Fix continuation {i}")),
                }),
            );
            assert_eq!(state.continuation.fix_continuation_attempt, i);
            assert_eq!(state.metrics.fix_continuations_total, i);
        }

        // After fix_continuation_attempt = 2, which >= max_fix_continue_count (2), exhausted
        assert!(state.continuation.fix_continuations_exhausted());
    });
}

#[test]
fn test_continuation_state_resets_across_iterations() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);

        // Iteration 0 with continuation
        state = reduce(state, PipelineEvent::development_iteration_started(0));
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
        assert_eq!(state.continuation.continuation_attempt, 1);

        // New iteration should reset continuation state
        state = reduce(state, PipelineEvent::development_iteration_started(1));
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert!(!state.continuation.continue_pending);
    });
}

#[test]
fn test_continuation_budget_exhaustion_completes_iteration_and_resets_continuation() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 2, 2); // max_continue_count = 2
        let mut state = PipelineState::initial_with_continuation(5, 0, &continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Trigger 2 continuations (reach budget)
        for _ in 1..=2 {
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
        }

        // Now trigger budget exhaustion
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationBudgetExhausted {
                iteration: 0,
                total_attempts: 3,
                last_status: DevelopmentStatus::Partial,
            }),
        );

        // New reducer semantics: exhaustion completes the iteration and transitions to commit flow.
        assert_eq!(
            state.phase,
            ralph_workflow::reducer::event::PipelinePhase::CommitMessage
        );

        // Continuation state should be reset for the next iteration.
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert!(!state.continuation.is_continuation());
        assert!(state.continuation.context_cleanup_pending);
    });
}

#[test]
fn test_fix_continuation_budget_exhaustion_proceeds_to_commit() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 1); // max_fix_continue_count = 1
        let mut state = PipelineState::initial_with_continuation(0, 3, &continuation);
        state.phase = ralph_workflow::reducer::event::PipelinePhase::Review;
        state.reviewer_pass = 1;

        // Trigger 1 fix continuation (reach budget)
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                pass: 1,
                status: FixStatus::IssuesRemain,
                summary: Some("partial fix".to_string()),
            }),
        );
        assert_eq!(state.continuation.fix_continuation_attempt, 1);

        // Now trigger budget exhaustion
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationBudgetExhausted {
                pass: 1,
                total_attempts: 2,
                last_status: FixStatus::IssuesRemain,
            }),
        );

        // Should proceed to CommitMessage (accept partial fixes)
        assert_eq!(
            state.phase,
            ralph_workflow::reducer::event::PipelinePhase::CommitMessage
        );
    });
}

#[test]
fn test_continuation_metrics_track_correctly() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Initial attempt
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        assert_eq!(state.metrics.dev_attempts_total, 1);

        // First continuation
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
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        assert_eq!(state.metrics.dev_attempts_total, 2);

        // Second continuation
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "more work".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        assert_eq!(state.metrics.dev_attempts_total, 3);

        // Continuation succeeds
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationSucceeded {
                iteration: 0,
                total_continuation_attempts: 2,
            }),
        );

        // Verify: 1 iteration started, 1 iteration completed, 3 attempts total
        assert_eq!(state.metrics.dev_iterations_started, 1);
        assert_eq!(state.metrics.dev_iterations_completed, 1);
        assert_eq!(state.metrics.dev_attempts_total, 3);
    });
}

// ============================================================================
// Step 18: Verify metrics match continuation state
// ============================================================================

/// Test that `dev_continuation_attempt` metric stays in sync with `ContinuationState`.
///
/// CRITICAL: Metrics must be consistent with the source-of-truth continuation state.
#[test]
fn test_dev_continuation_metrics_match_state() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(3, 0, &continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Initial: no continuations yet
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert_eq!(state.metrics.dev_continuation_attempt, 0);

        // After first continuation
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

        assert_eq!(
            state.metrics.dev_continuation_attempt, state.continuation.continuation_attempt,
            "Metrics should match continuation state after first continuation"
        );

        // After second continuation
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "more partial".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );

        assert_eq!(
            state.metrics.dev_continuation_attempt, state.continuation.continuation_attempt,
            "Metrics should match continuation state after second continuation"
        );
    });
}

/// Test that `fix_continuation_attempt` metric stays in sync with `ContinuationState`.
#[test]
fn test_fix_continuation_metrics_match_state() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(0, 1, &continuation);
        state.phase = ralph_workflow::reducer::event::PipelinePhase::Review;
        state.reviewer_pass = 0;

        // Initial: no fix continuations yet
        assert_eq!(state.continuation.fix_continuation_attempt, 0);
        assert_eq!(state.metrics.fix_continuation_attempt, 0);

        // After first fix continuation
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                pass: 0,
                status: FixStatus::IssuesRemain,
                summary: Some("partial fix".to_string()),
            }),
        );

        assert_eq!(
            state.metrics.fix_continuation_attempt, state.continuation.fix_continuation_attempt,
            "Metrics should match continuation state after first fix continuation"
        );

        // After second fix continuation
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                pass: 0,
                status: FixStatus::IssuesRemain,
                summary: Some("more partial fix".to_string()),
            }),
        );

        assert_eq!(
            state.metrics.fix_continuation_attempt, state.continuation.fix_continuation_attempt,
            "Metrics should match continuation state after second fix continuation"
        );
    });
}

/// Test that continuation exhaustion is correctly detected in both state and metrics.
#[test]
fn test_continuation_exhaustion_matches_metrics() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(3, 0, &continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Trigger 2 continuations (attempts 1, 2)
        for i in 1..=2 {
            state = reduce(
                state,
                PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                    iteration: 0,
                    status: DevelopmentStatus::Partial,
                    summary: format!("attempt {i}"),
                    files_changed: None,
                    next_steps: None,
                }),
            );
        }

        // Attempt third continuation (defensive check prevents increment)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "attempt 3".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );

        // AFTER FIX: Counter stays at 2, exhaustion is detected by OutcomeApplied's (attempt + 1 >= max) check
        // continuations_exhausted() returns false because counter didn't reach max_continue_count
        assert_eq!(state.continuation.continuation_attempt, 2);
        assert!(!state.continuation.continuations_exhausted());
        // But the defensive check cleared continue_pending to prevent further scheduling
        assert!(!state.continuation.continue_pending);
        // Metrics should match state
        assert_eq!(
            state.metrics.dev_continuation_attempt,
            state.continuation.continuation_attempt
        );
    });
}
